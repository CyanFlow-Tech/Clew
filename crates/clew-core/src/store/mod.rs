//! Opening and initialising the index database.

pub mod repo;
pub mod schema;

use std::path::Path;

use rusqlite::Connection;

/// Connection type alias so callers need not depend on rusqlite directly.
pub type Conn = Connection;

/// Open (creating if needed) the index database and ensure the schema exists.
pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    init(&conn)?;
    Ok(conn)
}

/// Open the index of an already-indexed project, if it exists.
pub fn open_index(db_path: &Path, root: &Path) -> rusqlite::Result<Option<(Conn, i64)>> {
    if !db_path.exists() {
        return Ok(None);
    }
    let conn = open(db_path)?;
    let project_id = repo::project_id_by_path(&conn, &root.to_string_lossy())?;
    Ok(project_id.map(|id| (conn, id)))
}

fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;",
    )?;
    conn.execute_batch(schema::DDL)?;
    Ok(())
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
}