
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::card::CardId;

#[derive(Debug, Clone, Serialize)]
pub struct SymbolRow {
    pub card_id: String,
    pub module: String,
    pub qualname: String,
    pub kind: String,
    pub def_line: i64,
    pub is_entry: bool,
    pub entry_confidence: Option<String>,
    pub params: Vec<String>,
    /// How many call sites in the project reach this symbol.
    pub fanin: i64,
}

/// One hit of the entry panel search, grouped by file.

pub fn symbols_in(conn: &Connection, project_id: i64, module: &str) -> rusqlite::Result<Vec<SymbolRow>> {
    symbols_of(conn, project_id, module, true)
}

/// 全项目的入口定义，侧栏「入口函数」视图一次取完：通常几十条，比按文件
/// 一个一个问过去便宜得多。
pub fn entry_symbols(conn: &Connection, project_id: i64) -> rusqlite::Result<Vec<SymbolRow>> {
    let mut stmt = conn.prepare(
        "SELECT s.module, s.qualname, s.kind, s.def_line, s.entry_confidence
           FROM symbol s
          WHERE s.project_id = ?1 AND s.is_entry = 1
          ORDER BY s.module, s.def_line, s.qualname",
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        let module: String = row.get(0)?;
        let qualname: String = row.get(1)?;
        Ok(SymbolRow {
            card_id: CardId::new(&module, &qualname).into_string(),
            module,
            qualname,
            kind: row.get(2)?,
            def_line: row.get(3)?,
            is_entry: true,
            entry_confidence: row.get(4)?,
            params: Vec::new(),
            fanin: 0,
        })
    })?;
    rows.collect()
}

/// 搜索时不需要参数表，少一次 per-symbol 的查询，不然大项目上每次按键都要
/// 对上万个符号各跑一条 SQL。
pub fn symbols_of(
    conn: &Connection,
    project_id: i64,
    module: &str,
    with_params: bool,
) -> rusqlite::Result<Vec<SymbolRow>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.qualname, s.kind, s.def_line, s.is_entry, s.entry_confidence,
                (SELECT count(*) FROM call_site c WHERE c.dst_symbol = s.id)
           FROM symbol s
          WHERE s.project_id = ?1 AND s.module = ?2
          ORDER BY s.def_line, s.qualname",
    )?;
    let mut out = Vec::new();
    for row in stmt.query_map(params![project_id, module], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)? != 0,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, i64>(6)?,
        ))
    })? {
        let (id, qualname, kind, def_line, is_entry, entry_confidence, fanin) = row?;
        let params_list = if with_params {
            let mut pstmt =
                conn.prepare("SELECT name FROM card_param WHERE symbol_id = ?1 ORDER BY ord")?;
            pstmt
                .query_map(params![id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        } else {
            Vec::new()
        };
        out.push(SymbolRow {
            card_id: CardId::new(module, &qualname).into_string(),
            module: module.to_string(),
            qualname,
            kind,
            def_line,
            is_entry,
            entry_confidence,
            params: params_list,
            fanin,
        });
    }
    Ok(out)
}
