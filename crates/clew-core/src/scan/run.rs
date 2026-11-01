//! Chain walker → extract → persist.

use std::path::Path;
use std::time::Instant;

use rusqlite::Connection;

use super::entry::{self, MODULE_ENTRY_QUALNAME};
use super::extract::{Extractor, SymbolDef, SymbolKind};
use super::walker;
use crate::store::repo;

#[derive(Debug, Default)]
pub struct ScanStats {
    pub files: usize,
    pub third_party_files: usize,
    pub symbols: usize,
    pub call_sites: usize,
    pub edges: usize,
    pub unresolved_attribute: usize,
    pub unresolved_other: usize,
    pub unresolved_callee: usize,
    pub entries_main_guard: usize,
    pub entries_pyproject: usize,
    pub entries_unresolved: usize,
    pub elapsed_ms: u128,
}

pub fn scan(
    conn: &mut Connection,
    root: &Path,
    include_venv: bool,
) -> anyhow::Result<ScanStats> {
    let started = Instant::now();
    let mut stats = ScanStats::default();

    let root = std::fs::canonicalize(root)?;
    let project_id = repo::upsert_project(conn, &root.to_string_lossy())?;
    repo::clear_project(conn, project_id)?;

    let files = walker::collect_with(&root, include_venv);
    stats.files = files.len();
    stats.third_party_files = files.iter().filter(|f| f.is_third_party).count();

    let mut extractor = Extractor::new();
    let mut pending: Vec<(String, super::extract::CallSite)> = Vec::new();

    // First pass: persist symbols and buffer the call sites
    let tx = conn.transaction()?;
    for file in &files {
        let Ok(source) = std::fs::read_to_string(&file.path) else {
            continue;
        };
        let index = extractor.extract(&file.module, &source);
        stats.unresolved_attribute += index.unresolved_attribute;
        stats.unresolved_other += index.unresolved_other;

        for sym in &index.symbols {
            repo::insert_symbol(&tx, project_id, sym, file.is_third_party)?;
            stats.symbols += 1;
        }

        // `if __name__ == "__main__"` → the module itself becomes an entry point
        if index.has_main_guard {
            let module_symbol = SymbolDef {
                module: file.module.clone(),
                qualname: MODULE_ENTRY_QUALNAME.to_string(),
                kind: SymbolKind::Module,
                src_hash: String::new(),
                def_line: 1,
                def_col: 0,
                body_start: 0,
                body_end: 0,
            };
            let id = repo::insert_symbol(&tx, project_id, &module_symbol, file.is_third_party)?;
            repo::mark_entry(&tx, id, "high")?;
            stats.entries_main_guard += 1;
        }
        for call in index.calls {
            pending.push((file.module.clone(), call));
        }
    }
    tx.commit()?;

    // `[project.scripts]` → mark the matching functions as entry points
    let tx = conn.transaction()?;
    for (module, func) in entry::scripts_from_pyproject(&root) {
        match repo::find_symbol(&tx, project_id, &module, &func)? {
            Some(id) => {
                repo::mark_entry(&tx, id, "high")?;
                stats.entries_pyproject += 1;
            }
            None => stats.entries_unresolved += 1,
        }
    }
    tx.commit()?;

    // Second pass: resolve call sites into edges
    stats.call_sites = pending.len();
    let tx = conn.transaction()?;
    for (module, call) in &pending {
        let Some(dst) = repo::find_symbol(&tx, project_id, module, &call.callee)? else {
            stats.unresolved_callee += 1;
            continue;
        };
        let Some(src) = repo::find_symbol(&tx, project_id, module, &call.owner)? else {
            continue;
        };
        repo::insert_edge(
            &tx,
            src,
            dst,
            "call",
            "inferred",
            "static",
            Some(call.line as i64),
        )?;
        stats.edges += 1;
    }
    tx.commit()?;

    stats.elapsed_ms = started.elapsed().as_millis();
    Ok(stats)
}
