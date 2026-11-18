
use rusqlite::{Connection, OptionalExtension, params};

use super::now;

/// Get or create the project row and return its project_id.
pub fn upsert_project(conn: &Connection, path: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "INSERT INTO project (path, created_at) VALUES (?1, ?2)
         ON CONFLICT(path) DO UPDATE SET path = excluded.path
         RETURNING id",
        params![path, now()],
        |row| row.get(0),
    )
}

/// Drop a project's symbols, calls and side tables; called before a rescan.
pub fn clear_project(conn: &Connection, project_id: i64) -> rusqlite::Result<()> {
    const SUB_SYMBOLS: &str = "SELECT id FROM symbol WHERE project_id = ?1";
    const SUB_CALLS: &str = "SELECT id FROM call_site WHERE project_id = ?1";

    for sql in [
        format!("DELETE FROM call_arg WHERE call_id IN ({SUB_CALLS})"),
        format!("DELETE FROM call_candidate WHERE call_id IN ({SUB_CALLS})"),
        "DELETE FROM call_site WHERE project_id = ?1".to_string(),
        format!("DELETE FROM card_line WHERE symbol_id IN ({SUB_SYMBOLS})"),
        format!("DELETE FROM card_param WHERE symbol_id IN ({SUB_SYMBOLS})"),
        format!("DELETE FROM local_type WHERE symbol_id IN ({SUB_SYMBOLS})"),
        format!("DELETE FROM class_base WHERE symbol_id IN ({SUB_SYMBOLS})"),
        format!("DELETE FROM class_meta WHERE symbol_id IN ({SUB_SYMBOLS})"),
        "DELETE FROM module WHERE project_id = ?1".to_string(),
        "DELETE FROM import WHERE project_id = ?1".to_string(),
        "DELETE FROM symbol WHERE project_id = ?1".to_string(),
    ] {
        conn.execute(&sql, params![project_id])?;
    }
    Ok(())
}

/// Upsert one symbol. Kept as a constant so the scan can prepare it once and
/// reuse the statement for a hundred thousand rows.

pub fn count(conn: &Connection, table: &str, project_id: i64) -> rusqlite::Result<i64> {
    let sql = format!("SELECT count(*) FROM {table} WHERE project_id = ?1");
    conn.query_row(&sql, params![project_id], |row| row.get(0))
}

/// Mark a symbol as an entry point.
pub fn mark_entry(conn: &Connection, symbol_id: i64, confidence: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE symbol SET is_entry = 1, entry_confidence = ?2 WHERE id = ?1",
        params![symbol_id, confidence],
    )?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct EntryRow {
    pub module: String,
    pub qualname: String,
    pub kind: String,
    pub confidence: String,
}

/// List every entry point of a project.
pub fn list_entries(conn: &Connection, project_id: i64) -> rusqlite::Result<Vec<EntryRow>> {
    let mut stmt = conn.prepare(
        "SELECT module, qualname, kind, COALESCE(entry_confidence, '')
           FROM symbol
          WHERE project_id = ?1 AND is_entry = 1
          ORDER BY module, qualname",
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok(EntryRow {
            module: row.get(0)?,
            qualname: row.get(1)?,
            kind: row.get(2)?,
            confidence: row.get(3)?,
        })
    })?;
    rows.collect()
}

/// Look up project_id by the project root path.
pub fn project_id_by_path(conn: &Connection, path: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM project WHERE path = ?1",
        params![path],
        |row| row.get(0),
    )
    .optional()
}

// ── notes ─────────────────────────────────────────────────────────

