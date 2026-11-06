//! One tree-sitter pass (`graph_build`) persisted straight into the index DB.
//!
//! The database is the only source of truth: no intermediate JSON.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use rusqlite::Connection;

use super::entry::{self, MODULE_ENTRY_QUALNAME};
use super::graph_build::{self, SymbolMeta};
use super::model::{Confidence, SymbolKind};
use super::progress::weight;
use crate::store::repo;

#[derive(Debug, Default)]
pub struct ScanStats {
    pub modules: usize,
    pub third_party_modules: usize,
    pub symbols: usize,
    pub classes: usize,
    pub functions: usize,
    pub call_sites: usize,
    pub calls_resolved: usize,
    pub call_args: usize,
    pub call_candidates: usize,
    pub imports: usize,
    pub exact: usize,
    pub poly: usize,
    pub heur: usize,
    pub builtin: usize,
    pub none: usize,
    pub entries_main_guard: usize,
    pub entries_pyproject: usize,
    pub entries_unresolved: usize,
    pub elapsed_ms: u128,
}

/// Split a card id (`module:qualname`) into its two symbol key columns.
fn card_parts(card_id: &str) -> (String, String) {
    match card_id.split_once(':') {
        Some((module, qualname)) => (module.to_string(), qualname.to_string()),
        None => (String::new(), card_id.to_string()),
    }
}

pub fn scan(conn: &mut Connection, root: &Path, include_venv: bool) -> anyhow::Result<ScanStats> {
    scan_with(conn, root, include_venv, &mut |_, _, _, _| {})
}

/// Same as [`scan`], but reports coarse progress for a UI progress bar.
pub fn scan_with(
    conn: &mut Connection,
    root: &Path,
    include_venv: bool,
    report: &mut dyn FnMut(&'static str, u64, u64, f32),
) -> anyhow::Result<ScanStats> {
    let started = Instant::now();
    let mut stats = ScanStats::default();

    report("枚举文件", 0, 0, weight::WALK.0);
    let root = std::fs::canonicalize(root)?;

    let t_build = Instant::now();
    let built = graph_build::build_with(&root, include_venv, report)?;
    let d_build = t_build.elapsed();

    let t_db = Instant::now();
    let project_id = repo::upsert_project(conn, &root.to_string_lossy())?;
    let repo_clear = repo::clear_project(conn, project_id);
    let d_prepare = t_db.elapsed();
    repo_clear?;
    report("准备写入", 0, 1, weight::SYMBOLS.0);

    stats.modules = built.modules.len();
    stats.third_party_modules = built.modules.iter().filter(|m| m.is_third_party).count();

    let tx = conn.transaction()?;

    let mut third_party: HashMap<&str, bool> = HashMap::new();
    for m in &built.modules {
        repo::insert_module(&tx, project_id, m)?;
        third_party.insert(m.module.as_str(), m.is_third_party);
    }

    // symbols, keyed so card ids can be resolved without touching SQL again
    let t_syms = Instant::now();
    let mut ids: HashMap<(String, String), i64> = HashMap::new();
    for sym in &built.symbols {
        let tp = *third_party.get(sym.module.as_str()).unwrap_or(&false);
        let id = repo::insert_symbol(&tx, project_id, sym, tp)?;
        ids.insert((sym.module.clone(), sym.qualname.clone()), id);
        stats.symbols += 1;
        match sym.kind {
            SymbolKind::Class => stats.classes += 1,
            _ => stats.functions += 1,
        }
    }

    // module-level `if __name__ == "__main__"` guards become entry pseudo symbols
    for m in &built.modules {
        if !m.has_main_guard {
            continue;
        }
        let module_symbol = SymbolMeta {
            module: m.module.clone(),
            qualname: MODULE_ENTRY_QUALNAME.to_string(),
            kind: SymbolKind::Module,
            src_hash: String::new(),
            def_line: 1,
            def_col: 0,
            body_start: 0,
            body_end: 0,
        };
        let id = repo::insert_symbol(&tx, project_id, &module_symbol, m.is_third_party)?;
        repo::mark_entry(&tx, id, "high")?;
        ids.insert(
            (m.module.clone(), MODULE_ENTRY_QUALNAME.to_string()),
            id,
        );
        stats.entries_main_guard += 1;
    }
    let d_syms = t_syms.elapsed();
    report("写入符号", 1, 1, weight::SYMBOLS.1);

    // cards: parameters, rendered lines, inferred local types
    let t_cards = Instant::now();
    for card in built.graph.cards.values() {
        let Some(&sym_id) = ids.get(&(card.module.clone(), card.qualname.clone())) else {
            continue;
        };
        for (ord, name) in card.params.iter().enumerate() {
            repo::insert_card_param(&tx, sym_id, ord, name)?;
        }
        for (ord, line) in card.lines.iter().enumerate() {
            repo::insert_card_line(&tx, sym_id, ord, line)?;
        }
        for (var, class_card) in &card.locals {
            let class_symbol = ids.get(&card_parts(class_card)).copied();
            repo::insert_local_type(&tx, sym_id, var, class_card, class_symbol)?;
        }
    }

    // `__main__` guard bodies become the card of the module entry symbol
    for (module, lines) in &built.module_cards {
        let Some(&sym_id) = ids.get(&(module.clone(), MODULE_ENTRY_QUALNAME.to_string())) else {
            continue;
        };
        for (ord, line) in lines.iter().enumerate() {
            repo::insert_card_line(&tx, sym_id, ord, line)?;
        }
    }
    let d_cards = t_cards.elapsed();
    report("写入卡片", 1, 1, weight::CARDS.1);

    // call sites: every one of them, resolved or not
    let t_calls = Instant::now();
    for (cs_id, call) in &built.graph.calls {
        let module = call.from.split_once(':').map(|(m, _)| m).unwrap_or("");
        let owner_id = ids.get(&card_parts(&call.from)).copied();
        let dst_id = call
            .to
            .as_deref()
            .and_then(|to| ids.get(&card_parts(to)).copied());

        repo::insert_call_site(&tx, project_id, cs_id, module, owner_id, dst_id, call)?;
        stats.call_sites += 1;
        if dst_id.is_some() {
            stats.calls_resolved += 1;
        }
        match call.conf {
            Confidence::Exact => stats.exact += 1,
            Confidence::Poly => stats.poly += 1,
            Confidence::Heur => stats.heur += 1,
            Confidence::Builtin => stats.builtin += 1,
            Confidence::None => stats.none += 1,
        }

        for (param, expr) in &call.args {
            repo::insert_call_arg(&tx, cs_id, param, expr)?;
            stats.call_args += 1;
        }
        for (ord, candidate) in call.candidates.iter().enumerate() {
            let sym = ids.get(&card_parts(candidate)).copied();
            repo::insert_call_candidate(&tx, cs_id, ord, candidate, sym)?;
            stats.call_candidates += 1;
        }
    }
    let d_calls = t_calls.elapsed();
    report("写入调用", 1, 1, weight::CALLS.1);

    let t_misc = Instant::now();
    for imp in &built.imports {
        repo::insert_import(&tx, project_id, imp)?;
        stats.imports += 1;
    }

    // class bases and the `__init__` reference
    for class in &built.classes {
        let Some(&sym_id) = ids.get(&(class.module.clone(), class.qualname.clone())) else {
            continue;
        };
        let ctor = class
            .ctor
            .as_deref()
            .and_then(|id| ids.get(&card_parts(id)).copied());
        repo::insert_class_meta(&tx, sym_id, &class.module, &class.qualname, ctor)?;
        for (ord, base) in class.bases.iter().enumerate() {
            let base_id = ids.get(&(class.module.clone(), base.clone())).copied();
            repo::insert_class_base(&tx, sym_id, ord, base, base_id)?;
        }
    }

    // `[project.scripts]` → mark the matching functions as entry points
    for (module, func) in entry::scripts_from_pyproject(&root) {
        match repo::find_symbol(&tx, project_id, &module, &func)? {
            Some(id) => {
                repo::mark_entry(&tx, id, "high")?;
                stats.entries_pyproject += 1;
            }
            None => stats.entries_unresolved += 1,
        }
    }

    let d_misc = t_misc.elapsed();
    report("写入其他表", 1, 1, weight::MISC.1);

    report("提交", 0, 1, weight::COMMIT.0);
    let t_commit = Instant::now();
    tx.commit()?;
    let d_commit = t_commit.elapsed();
    report("完成", 1, 1, 1.0);

    let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
    eprintln!(
        "[t] build {:.1}ms prepare {:.1}ms symbols {:.1}ms cards {:.1}ms calls {:.1}ms misc {:.1}ms commit {:.1}ms",
        ms(d_build),
        ms(d_prepare),
        ms(d_syms),
        ms(d_cards),
        ms(d_calls),
        ms(d_misc),
        ms(d_commit)
    );

    stats.elapsed_ms = started.elapsed().as_millis();
    Ok(stats)
}
