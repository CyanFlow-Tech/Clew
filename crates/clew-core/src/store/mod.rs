//! Opening and initialising the index database.

pub mod repo;
pub mod schema;

use std::path::Path;

use rusqlite::Connection;

use crate::error::ClewError;

/// Connection type alias so callers need not depend on rusqlite directly.
pub type Conn = Connection;

/// Open (creating if needed) the index database and bring the schema up to date.
pub fn open(path: &Path) -> Result<Connection, ClewError> {
    let conn = Connection::open(path).map_err(|source| ClewError::Sql {
        at: Some(path.to_path_buf()),
        source,
    })?;
    init(&conn)?;
    Ok(conn)
}

/// Open the index of an already-indexed project, if it exists.
pub fn open_index(db_path: &Path, root: &Path) -> Result<Option<(Conn, i64)>, ClewError> {
    if !db_path.exists() {
        return Ok(None);
    }
    let conn = open(db_path)?;
    let project_id = repo::project_id_by_path(&conn, &root.to_string_lossy())?;
    Ok(project_id.map(|id| (conn, id)))
}

/// 建表或把老库升到 [`schema::VERSION`]。
///
/// 三种库都要照顾到：
///
/// - 全新的库 —— `user_version` 是 0，表也不存在，跑一遍 `DDL`。
/// - 这个版本之前建的库 —— 表都在，但那时还没有版本号，`user_version` 也是 0。
///   `DDL` 全是 `IF NOT EXISTS`，重跑一遍无害，然后把版本号补上。
/// - 更新版 Clew 建的库 —— 版本号比本程序高，直接停：拿老代码跑新结构
///   会在很远的地方报一句看不懂的 SQL 错误。
fn init(conn: &Connection) -> Result<(), ClewError> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;",
    )?;

    let found = user_version(conn)?;
    if found > schema::VERSION {
        return Err(ClewError::refused(format!(
            "索引库的结构版本是 v{found}，这个版本的 Clew 只认到 v{}。\
             换用更新的 Clew，或删掉索引重新扫一遍。",
            schema::VERSION
        )));
    }

    if found == 0 {
        conn.execute_batch(schema::DDL)?;
    } else {
        for from in found..schema::VERSION {
            let Some(sql) = schema::MIGRATIONS.get((from - 1) as usize) else {
                return Err(ClewError::refused(format!(
                    "缺少 v{from} → v{} 的迁移脚本",
                    from + 1
                )));
            };
            conn.execute_batch(sql)?;
        }
    }

    set_user_version(conn, schema::VERSION)?;
    Ok(())
}

fn user_version(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

fn set_user_version(conn: &Connection, version: i64) -> rusqlite::Result<()> {
    // PRAGMA 不吃参数绑定；版本号来自编译期常量，没有注入面。
    conn.execute_batch(&format!("PRAGMA user_version = {version};"))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_all_tables() {
        let path = std::env::temp_dir().join(format!("clew-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let conn = open(&path).expect("open");
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap();
        let names: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        for expected in [
            "call_arg",
            "call_candidate",
            "call_site",
            "card_line",
            "card_param",
            "class_base",
            "class_meta",
            "import",
            "local_type",
            "module",
            "note",
            "project",
            "session",
            "symbol",
            "symbol_alias",
            "visit",
        ] {
            assert!(names.iter().any(|n| n == expected), "missing table {expected}");
        }

        let _ = std::fs::remove_file(&path);
    }

    /// 迁移是「库还开着的时候复制文件」：先确认 SQLite 的句柄不挡复制，
    /// 再确认复制出来那份能打开、表还在。WAL 要先收进主库，否则 -wal 里的
    /// 数据不跟着走。
    #[test]
    fn the_index_can_be_copied_while_open() {
        let dir = std::env::temp_dir().join(format!("clew-copy-open-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let db = dir.join("index.db");
        let conn = open(&db).expect("open");
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();

        let copy = dir.join("copy.db");
        std::fs::copy(&db, &copy).expect("copy while the connection is still open");

        let opened = open(&copy).expect("open the copy");
        let tables: i64 = opened
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(tables >= 16, "copy lost tables: {tables}");

        drop(opened);
        drop(conn);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("clew-store-{name}-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn a_fresh_database_is_stamped_with_the_current_version() {
        let path = scratch("fresh");
        let conn = open(&path).expect("open");
        assert_eq!(user_version(&conn).unwrap(), schema::VERSION);
        drop(conn);
        let _ = std::fs::remove_file(&path);
    }

    /// 这个版本之前建的库：表都在，但没有 `user_version`。重开一次要能认领它，
    /// 把版本号补上，而且不能把已有数据清掉。
    #[test]
    fn a_database_from_before_versioning_is_adopted() {
        let path = scratch("adopt");
        let conn = open(&path).expect("open");
        conn.execute_batch("PRAGMA user_version = 0;").unwrap();
        conn.execute(
            "INSERT INTO project (id, path, created_at) VALUES (1, 'x', 0)",
            [],
        )
        .unwrap();
        drop(conn);

        let conn = open(&path).expect("reopen the older database");
        assert_eq!(user_version(&conn).unwrap(), schema::VERSION);
        let projects: i64 = conn
            .query_row("SELECT count(*) FROM project", [], |row| row.get(0))
            .unwrap();
        assert_eq!(projects, 1, "认领老库时丢了数据");
        drop(conn);
        let _ = std::fs::remove_file(&path);
    }

    /// 更新版 Clew 建的库：宁可停下，也不要拿老代码去跑新结构。
    #[test]
    fn a_database_from_a_newer_version_is_refused() {
        let path = scratch("newer");
        let conn = open(&path).expect("open");
        set_user_version(&conn, schema::VERSION + 1).unwrap();
        drop(conn);

        let err = open(&path).expect_err("必须拒绝");
        let text = err.to_string();
        assert!(text.contains("结构版本"), "报错要说清是版本问题：{text}");
        let _ = std::fs::remove_file(&path);
    }

    /// 迁移脚本的条数必须正好覆盖 VERSION - 1 级。加版本忘了加脚本，
    /// 这个测试会先炸，而不是等用户的老库升到一半卡住。
    #[test]
    fn migrations_cover_every_step() {
        assert_eq!(
            schema::MIGRATIONS.len() as i64,
            schema::VERSION - 1,
            "VERSION 与 MIGRATIONS 条数对不上"
        );
    }
}