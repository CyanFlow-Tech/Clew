//! Query and write wrappers.

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::scan::graph_build::{ImportMeta, ModuleMeta, SymbolMeta};
use crate::scan::model::{Call, Line};

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

pub fn insert_symbol(
    conn: &Connection,
    project_id: i64,
    sym: &SymbolMeta,
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

// ── graph rows ────────────────────────────────────────────────────

pub fn insert_module(
    conn: &Connection,
    project_id: i64,
    m: &ModuleMeta,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO module (project_id, module, path, src_hash, is_third_party, has_main_guard)
         VALUES (?1,?2,?3,?4,?5,?6)
         ON CONFLICT(project_id, module) DO UPDATE SET
             path           = excluded.path,
             src_hash       = excluded.src_hash,
             is_third_party = excluded.is_third_party,
             has_main_guard = excluded.has_main_guard",
        params![
            project_id,
            m.module,
            m.path.to_string_lossy(),
            m.src_hash,
            m.is_third_party as i64,
            m.has_main_guard as i64,
        ],
    )?;
    Ok(())
}

pub fn insert_card_param(
    conn: &Connection,
    symbol_id: i64,
    ord: usize,
    name: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO card_param (symbol_id, ord, name) VALUES (?1,?2,?3)
         ON CONFLICT DO NOTHING",
        params![symbol_id, ord as i64, name],
    )?;
    Ok(())
}

pub fn insert_card_line(
    conn: &Connection,
    symbol_id: i64,
    ord: usize,
    line: &Line,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO card_line (symbol_id, ord, kind, text, indent, ln, call_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT DO NOTHING",
        params![
            symbol_id,
            ord as i64,
            line.k.as_str(),
            line.text,
            line.indent as i64,
            line.ln as i64,
            line.cs,
        ],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn insert_call_site(
    conn: &Connection,
    project_id: i64,
    id: &str,
    module: &str,
    symbol_id: Option<i64>,
    dst_symbol: Option<i64>,
    call: &Call,
) -> rusqlite::Result<()> {
    // the caller is stored as a bare qualname so it matches the symbol table
    let owner = call.from.split_once(':').map(|(_, q)| q).unwrap_or(&call.from);
    conn.execute(
        "INSERT INTO call_site
             (id, project_id, symbol_id, module, owner, target, kind, recv,
              confidence, dst_symbol, imported, typed, call_line)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
         ON CONFLICT(id) DO UPDATE SET
             symbol_id  = excluded.symbol_id,
             dst_symbol = excluded.dst_symbol,
             confidence = excluded.confidence,
             imported   = excluded.imported,
             typed      = excluded.typed",
        params![
            id,
            project_id,
            symbol_id,
            module,
            owner,
            call.target,
            call.kind.as_str(),
            call.recv,
            call.conf.as_str(),
            dst_symbol,
            call.imported,
            call.typed,
            call.ln as i64,
        ],
    )?;
    Ok(())
}

pub fn insert_call_arg(
    conn: &Connection,
    call_id: &str,
    param: &str,
    expr: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO call_arg (call_id, param, expr) VALUES (?1,?2,?3)
         ON CONFLICT DO NOTHING",
        params![call_id, param, expr],
    )?;
    Ok(())
}

pub fn insert_call_candidate(
    conn: &Connection,
    call_id: &str,
    ord: usize,
    card_id: &str,
    symbol_id: Option<i64>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO call_candidate (call_id, ord, card_id, symbol_id) VALUES (?1,?2,?3,?4)
         ON CONFLICT DO NOTHING",
        params![call_id, ord as i64, card_id, symbol_id],
    )?;
    Ok(())
}

pub fn insert_import(
    conn: &Connection,
    project_id: i64,
    imp: &ImportMeta,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO import (project_id, module, local, orig, source_module, is_alias, is_module_import)
         VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT DO NOTHING",
        params![
            project_id,
            imp.module,
            imp.local,
            imp.orig,
            imp.source_module,
            imp.alias as i64,
            imp.module_import as i64,
        ],
    )?;
    Ok(())
}

pub fn insert_class_meta(
    conn: &Connection,
    symbol_id: i64,
    module: &str,
    qualname: &str,
    ctor_symbol: Option<i64>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO class_meta (symbol_id, module, qualname, ctor_symbol) VALUES (?1,?2,?3,?4)
         ON CONFLICT(symbol_id) DO UPDATE SET ctor_symbol = excluded.ctor_symbol",
        params![symbol_id, module, qualname, ctor_symbol],
    )?;
    Ok(())
}

pub fn insert_class_base(
    conn: &Connection,
    symbol_id: i64,
    ord: usize,
    base_name: &str,
    base_symbol: Option<i64>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO class_base (symbol_id, ord, base_name, base_symbol) VALUES (?1,?2,?3,?4)
         ON CONFLICT DO NOTHING",
        params![symbol_id, ord as i64, base_name, base_symbol],
    )?;
    Ok(())
}

pub fn insert_local_type(
    conn: &Connection,
    symbol_id: i64,
    var: &str,
    class_card: &str,
    class_symbol: Option<i64>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO local_type (symbol_id, var, class_card, class_symbol) VALUES (?1,?2,?3,?4)
         ON CONFLICT DO NOTHING",
        params![symbol_id, var, class_card, class_symbol],
    )?;
    Ok(())
}

/// Resolve a card id (`module:qualname`) to a symbol row id.
pub fn symbol_id_by_card(
    conn: &Connection,
    project_id: i64,
    card_id: &str,
) -> rusqlite::Result<Option<i64>> {
    let Some((module, qualname)) = card_id.split_once(':') else {
        return Ok(None);
    };
    find_symbol(conn, project_id, module, qualname)
}

/// Call sites on one line of one function, with their argument bindings.
pub struct CallArgRow {
    pub call_id: String,
    pub target: String,
    pub confidence: String,
    pub param: Option<String>,
    pub expr: Option<String>,
}

pub fn call_args_on_line(
    conn: &Connection,
    project_id: i64,
    qualname: &str,
    line: i64,
) -> rusqlite::Result<Vec<CallArgRow>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.target, c.confidence, a.param, a.expr
           FROM call_site c
           LEFT JOIN call_arg a ON a.call_id = c.id
          WHERE c.project_id = ?1 AND c.owner = ?2 AND c.call_line = ?3
          ORDER BY c.id, a.param",
    )?;
    let rows = stmt.query_map(params![project_id, qualname, line], |row| {
        Ok(CallArgRow {
            call_id: row.get(0)?,
            target: row.get(1)?,
            confidence: row.get(2)?,
            param: row.get(3)?,
            expr: row.get(4)?,
        })
    })?;
    rows.collect()
}

/// Per-confidence call counters, for the scan/verify report.
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
