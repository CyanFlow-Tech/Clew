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
}