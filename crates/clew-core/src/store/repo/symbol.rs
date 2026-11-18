
use rusqlite::{Connection, OptionalExtension, params};

use crate::card::CardId;
use crate::scan::graph_build::SymbolMeta;

pub const SYMBOL_INSERT: &str = "\
INSERT INTO symbol (project_id, module, qualname, kind, src_hash, def_line, def_col,
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
RETURNING id";

pub fn insert_symbol(
    conn: &Connection,
    project_id: i64,
    sym: &SymbolMeta,
    is_third_party: bool,
) -> rusqlite::Result<i64> {
    conn.query_row(
        SYMBOL_INSERT,
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


pub fn symbol_id_by_card(
    conn: &Connection,
    project_id: i64,
    card_id: &str,
) -> rusqlite::Result<Option<i64>> {
    let Some(cid) = CardId::parse(card_id) else {
        return Ok(None);
    };
    find_symbol(conn, project_id, cid.module(), cid.qualname())
}
