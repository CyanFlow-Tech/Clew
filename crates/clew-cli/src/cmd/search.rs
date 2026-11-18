use std::path::PathBuf;


use crate::browse;
use super::super::open_index;

pub fn findsyms(project: PathBuf, path: Option<String>, name: Option<String>, page: usize, size: usize, db: Option<PathBuf>) -> anyhow::Result<()> {
            let root = project.canonicalize()?;
            let (conn, project_id) = open_index(&root, db)?;
            let t = std::time::Instant::now();
            let out = clew_core::walk::search_symbols(
                &conn,
                project_id,
                &root,
                path.as_deref().unwrap_or(""),
                name.as_deref().unwrap_or(""),
            )?;
            let took = t.elapsed().as_secs_f64() * 1000.0;
            let size = size.max(1);
            let start = page.saturating_sub(1) * size;
            let slice: Vec<_> = out
                .hits
                .iter()
                .skip(start)
                .take(size)
                .map(|h| {
                    let mut h = h.clone();
                    h.path = clew_core::walk::rel_path(&root, &h.path);
                    h
                })
                .collect();
            println!(
                "{:<5}  {:<6}  {:<44}  {}",
                "score", "kind", "qualname", "file:line"
            );
            for h in &slice {
                println!(
                    "{:<5}  {:<6}  {:<44}  {}:{}",
                    h.score, h.kind, h.qualname, h.path, h.def_line
                );
            }
            println!(
                "\n{} matches{} · page {} · showing {} · took {took:.1}ms",
                out.total,
                if out.truncated { " (capped)" } else { "" },
                page,
                slice.len()
            );
    Ok(())
}

pub fn search(project: PathBuf, q: String, scope: String, ctx: Option<String>, db: Option<PathBuf>, json: bool) -> anyhow::Result<()> {
            let root = project.canonicalize()?;
            let (conn, project_id) = open_index(&root, db)?;
            let t = std::time::Instant::now();
            let index = clew_core::search::build_index(&conn, project_id)?;
            let built = t.elapsed();
            let t = std::time::Instant::now();
            let query = clew_core::search::Query::parse(
                &q,
                clew_core::search::Scope::parse(&scope),
                ctx.clone(),
            );
            let page = clew_core::search::run(&index, &query);
            let took = t.elapsed();
            if json {
                println!("{}", serde_json::to_string_pretty(&page)?);
                return Ok(());
            }
            println!(
                "{:<6}  {:<7}  {:<8}  {:<44}  {}",
                "score", "kind", "field", "qualname", "file:line"
            );
            for h in &page.hits {
                let mut name = if h.qualname.is_empty() {
                    h.module.clone()
                } else {
                    h.qualname.clone()
                };
                if h.kind == clew_core::search::Kind::Mod && !h.card_id.is_empty() {
                    name.push_str("  <entry>");
                }
                println!(
                    "{:<6}  {:<7}  {:<8}  {:<44}  {}:{}",
                    h.score,
                    h.kind.as_str(),
                    h.field,
                    name,
                    clew_core::walk::rel_path(&root, &h.path),
                    h.def_line
                );
            }
            println!(
                "\n{} hits, index {} rows in {:.1}ms, query {:.1}ms",
                page.hits.len(),
                index.rows.len(),
                built.as_secs_f64() * 1000.0,
                took.as_secs_f64() * 1000.0
            );
    Ok(())
}

pub fn find(project: PathBuf, q: String, db: Option<PathBuf>) -> anyhow::Result<()> {
            let root = project.canonicalize()?;
            let (conn, project_id) = open_index(&root, db)?;
            let mut rows = clew_core::walk::search(&conn, project_id, &q, 200)?;
            for g in rows.iter_mut() {
                g.path = clew_core::walk::rel_path(&root, &g.path);
            }
            if rows.is_empty() {
                println!("no hits for {q}");
                return Ok(());
            }
            println!("{:<5}  {:<8}  {:<44}  {}", "line", "kind", "qualname", "file");
            for g in &rows {
                println!("{}  ({})", g.path, if g.by_file { "文件命中" } else { "名字命中" });
                for r in &g.symbols {
                    println!(
                        "{:<5}  {:<8}  {:<44}  {}",
                        r.def_line,
                        r.kind,
                        r.qualname,
                        g.module
                    );
                }
            }
            let n: usize = rows.iter().map(|g| g.symbols.len()).sum();
            println!("\n{} hits in {} files for {q}", n, rows.len());
    Ok(())
}

pub fn walk(project: PathBuf, entry: Option<String>, db: Option<PathBuf>) -> anyhow::Result<()> {
            let root = project.canonicalize()?;
            let (conn, project_id) = open_index(&root, db)?;
            browse::run(&conn, project_id, browse::Options { entry })?;
    Ok(())
}
