
use rusqlite::{Connection, params};

use super::now;

pub fn insert_note(conn: &Connection, symbol_id: i64, text: &str) -> rusqlite::Result<i64> {
    let ts = now();
    conn.query_row(
        "INSERT INTO note (symbol_id, text, created_at, updated_at)
         VALUES (?1,?2,?3,?3) RETURNING id",
        params![symbol_id, text, ts],
        |row| row.get(0),
    )
}

// ── graph rows ────────────────────────────────────────────────────


pub fn call_confidence_counts(
    conn: &Connection,
    project_id: i64,
) -> rusqlite::Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT confidence, count(*) FROM call_site
          WHERE project_id = ?1 GROUP BY confidence ORDER BY confidence",
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    rows.collect()
}

/// Symbol counters grouped by kind.
pub fn symbol_kind_counts(
    conn: &Connection,
    project_id: i64,
) -> rusqlite::Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT kind, count(*) FROM symbol
          WHERE project_id = ?1 GROUP BY kind ORDER BY kind",
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    rows.collect()
}
