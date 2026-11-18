
use rusqlite::{Connection, params};
use serde::Serialize;


#[derive(Debug, Clone, Serialize)]
pub struct CodeLine {
    pub kind: String,
    pub text: String,
    pub indent: i64,
    pub ln: i64,
    pub call_target: Option<String>,
    pub confidence: Option<String>,
    /// Resolved callee of the anchor, so the UI can jump to it
    pub dst_symbol: Option<i64>,
}

/// The card body of a node, in display order.
pub fn card_lines(conn: &Connection, symbol_id: i64) -> rusqlite::Result<Vec<CodeLine>> {
    let mut stmt = conn.prepare(
        "SELECT l.kind, l.text, l.indent, l.ln, c.target, c.confidence, c.dst_symbol
           FROM card_line l LEFT JOIN call_site c ON c.id = l.call_id
          WHERE l.symbol_id = ?1
          ORDER BY l.ord",
    )?;
    let rows = stmt.query_map(params![symbol_id], |row| {
        Ok(CodeLine {
            kind: row.get(0)?,
            text: row.get(1)?,
            indent: row.get(2)?,
            ln: row.get(3)?,
            call_target: row.get(4)?,
            confidence: row.get(5)?,
            dst_symbol: row.get(6)?,
        })
    })?;
    rows.collect()
}
