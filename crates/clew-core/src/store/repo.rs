//! Query and write wrappers.

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::scan::extract::SymbolDef;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

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

/// Drop a project's symbols and edges; called before a rescan.
pub fn clear_project(conn: &Connection, project_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM edge
          WHERE src_symbol IN (SELECT id FROM symbol WHERE project_id = ?1)
             OR dst_symbol IN (SELECT id FROM symbol WHERE project_id = ?1)",
        params![project_id],
    )?;
    conn.execute(
        "DELETE FROM symbol WHERE project_id = ?1",
        params![project_id],
    )?;
    Ok(())
}

pub fn insert_symbol(
    conn: &Connection,
    project_id: i64,
    sym: &SymbolDef,
    is_third_party: bool,
) -> rusqlite::Result<i64> {
    conn.query_row(
        "INSERT INTO symbol (project_id, module, qualname, kind, src_hash, def_line, def_col,
                             body_start, body_end, is_third_party, is_entry, entry_confidence)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,0,NULL)
         ON CONFLICT(project_id, module, qualname) DO UPDATE SET
             kind          = excluded.kind,
             src_hash      = excluded.src_hash,
             def_line      = excluded.def_line,
             def_col       = excluded.def_col,
             body_start    = excluded.body_start,
             body_end      = excluded.body_end,
             is_third_party = excluded.is_third_party
         RETURNING id",
        params![
            project_id,
            sym.module,
            sym.qualname,
            sym.kind.as_str(),
            sym.src_hash,
            sym.def_line,
            sym.def_col,
            sym.body_start as i64,
            sym.body_end as i64,
            is_third_party as i64,
        ],
        |row| row.get(0),
    )
}

/// Look up a symbol by qualname, preferring the same module.
pub fn find_symbol(
    conn: &Connection,
    project_id: i64,
    module: &str,
    qualname: &str,
) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM symbol
          WHERE project_id = ?1 AND qualname = ?2
          ORDER BY CASE WHEN module = ?3 THEN 0 ELSE 1 END, id
          LIMIT 1",
        params![project_id, qualname, module],
        |row| row.get(0),
    )
    .optional()
}

#[allow(clippy::too_many_arguments)]
pub fn insert_edge(
    conn: &Connection,
    src: i64,
    dst: i64,
    kind: &str,
    confidence: &str,
    source: &str,
    call_line: Option<i64>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO edge (src_symbol, dst_symbol, kind, confidence, source, call_line)
         VALUES (?1,?2,?3,?4,?5,?6)
         ON CONFLICT DO NOTHING",
        params![src, dst, kind, confidence, source, call_line],
    )?;
    Ok(())
}

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
