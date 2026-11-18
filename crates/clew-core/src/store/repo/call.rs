
use rusqlite::{Connection, params};

use crate::card::CardId;
use crate::scan::model::Call;

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
    let owner = CardId::qualname_of(&call.from).unwrap_or(call.from.as_str());
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
