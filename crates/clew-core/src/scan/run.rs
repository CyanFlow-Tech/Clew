//! One tree-sitter pass (`graph_build`) persisted straight into the index DB.
//!
//! The database is the only source of truth: no intermediate JSON.
//!
//! This pass only builds the directory: modules, symbols, imports and entry
//! points. Card bodies and call sites are written later, per module, by
//! [`super::materialize`].

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use rusqlite::Connection;

use super::entry::{self, MODULE_ENTRY_QUALNAME};
use super::graph_build::{self, SymbolMeta};
use super::model::SymbolKind;
use super::progress::weight;
use super::walker;
use crate::store::repo;

#[derive(Debug, Default)]
pub struct ScanStats {
    pub modules: usize,
    pub third_party_modules: usize,
    pub symbols: usize,
    pub classes: usize,
    pub functions: usize,
    pub imports: usize,
    pub entries_main_guard: usize,
    pub entries_pyproject: usize,
    pub entries_unresolved: usize,
    pub elapsed_ms: u128,
}

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
    let built = graph_build::build_with(walker::python(), &root, include_venv, report)?;
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

    let t_syms = Instant::now();
    {
        let mut stmt = tx.prepare(repo::SYMBOL_INSERT)?;
        for sym in &built.symbols {
            let tp = *third_party.get(sym.module.as_str()).unwrap_or(&false);
            stmt.query_row(
                rusqlite::params![
                    project_id,
                    sym.module,
                    sym.qualname,
                    sym.kind.as_str(),
                    sym.src_hash,
                    sym.def_line,
                    sym.def_col,
                    sym.body_start as i64,
                    sym.body_end as i64,
                    tp as i64,
                ],
                |row| row.get::<_, i64>(0),
            )?;
            stats.symbols += 1;
            match sym.kind {
                SymbolKind::Class => stats.classes += 1,
                _ => stats.functions += 1,
            }
        }
    }

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
        stats.entries_main_guard += 1;
    }
    let d_syms = t_syms.elapsed();
    report("写入符号", 1, 1, weight::SYMBOLS.1);

    let t_misc = Instant::now();
    for imp in &built.imports {
        repo::insert_import(&tx, project_id, imp)?;
        stats.imports += 1;
    }

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
        "[t] build {:.1}ms prepare {:.1}ms symbols {:.1}ms misc {:.1}ms commit {:.1}ms",
        ms(d_build),
        ms(d_prepare),
        ms(d_syms),
        ms(d_misc),
        ms(d_commit)
    );

    stats.elapsed_ms = started.elapsed().as_millis();
    Ok(stats)
}
