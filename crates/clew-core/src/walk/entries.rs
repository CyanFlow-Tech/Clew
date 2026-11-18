
use rusqlite::{Connection, params};

use crate::card::CardId;

pub fn entry_points(
    conn: &Connection,
    project_id: i64,
) -> rusqlite::Result<Vec<(i64, String, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, module, qualname, COALESCE(entry_confidence, '')
           FROM symbol
          WHERE project_id = ?1 AND is_entry = 1
          ORDER BY module, qualname",
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    rows.collect()
}

/// Resolve `[module:]qualname` to a symbol id.
pub fn find(conn: &Connection, project_id: i64, needle: &str) -> rusqlite::Result<Vec<i64>> {
    let (module, qualname) = match CardId::parts(needle) {
        Some((module, qualname)) => (Some(module), qualname),
        None => (None, needle),
    };
    let mut stmt = conn.prepare(
        "SELECT id FROM symbol
          WHERE project_id = ?1 AND qualname = ?2 AND (?3 IS NULL OR module = ?3)
          ORDER BY id",
    )?;
    let rows = stmt.query_map(params![project_id, qualname, module], |row| {
        row.get::<_, i64>(0)
    })?;
    rows.collect()
}
