use std::collections::HashMap;

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::card::CardId;
use super::symbols::SymbolRow;

#[derive(Debug, Clone, Serialize)]
pub struct FileNode {
    pub module: String,
    pub path: String,
    pub is_third_party: bool,
    pub has_main_guard: bool,
    pub symbols: Vec<SymbolRow>,
}

/// The whole project as a tree: every file in path order, every definition in
/// source order inside it.
///
/// 一次查询取全部定义行，参数表和 fanin 都不带：树里不画它们，而按符号各跑
/// 一条子查询，几千个符号就是几千次往返，切视图会卡住。
pub fn tree(conn: &Connection, project_id: i64) -> rusqlite::Result<Vec<FileNode>> {
    let mut files_stmt = conn.prepare(
        "SELECT module, path, is_third_party, has_main_guard
           FROM module
          WHERE project_id = ?1
          ORDER BY path",
    )?;
    let mut files: Vec<FileNode> = files_stmt
        .query_map(params![project_id], |row| {
            let path: String = row.get(1)?;
            Ok(FileNode {
                module: row.get(0)?,
                path: path.trim_start_matches(r"\\?\").to_string(),
                is_third_party: row.get::<_, i64>(2)? != 0,
                has_main_guard: row.get::<_, i64>(3)? != 0,
                symbols: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut slot: HashMap<String, usize> = files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.module.clone(), i))
        .collect();

    let mut sym_stmt = conn.prepare(
        "SELECT module, qualname, kind, def_line, is_entry, entry_confidence
           FROM symbol
          WHERE project_id = ?1
          ORDER BY module, def_line, qualname",
    )?;
    let rows = sym_stmt.query_map(params![project_id], |row| {
        let module: String = row.get(0)?;
        Ok((
            module,
            SymbolRow {
                card_id: String::new(),
                module: String::new(),
                qualname: row.get(1)?,
                kind: row.get(2)?,
                def_line: row.get(3)?,
                is_entry: row.get::<_, i64>(4)? != 0,
                entry_confidence: row.get(5)?,
                params: Vec::new(),
                fanin: 0,
            },
        ))
    })?;
    for row in rows {
        let (module, mut sym) = row?;
        // 符号的 module 在库里一定有对应文件行，除非库被外改过；遇到就补一行
        let idx = match slot.get(&module) {
            Some(i) => *i,
            None => {
                files.push(FileNode {
                    module: module.clone(),
                    path: module.replace('.', "/") + ".py",
                    is_third_party: false,
                    has_main_guard: false,
                    symbols: Vec::new(),
                });
                slot.insert(module.clone(), files.len() - 1);
                files.len() - 1
            }
        };
        sym.card_id = CardId::new(&module, &sym.qualname).into_string();
        sym.module = module;
        files[idx].symbols.push(sym);
    }

    Ok(files)
}
