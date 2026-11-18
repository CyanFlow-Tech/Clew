use std::path::PathBuf;


use super::super::{display, open_index};

pub fn entries(project: PathBuf, db: Option<PathBuf>) -> anyhow::Result<()> {
            let root = project.canonicalize()?;
            let (conn, project_id) = open_index(&root, db)?;

            let rows = clew_core::store::repo::list_entries(&conn, project_id)?;
            if rows.is_empty() {
                println!("no entry points found ({})", display(&root));
                return Ok(());
            }

            let width = rows.iter().map(|r| r.module.len()).max().unwrap_or(0);
            println!("{:<width$}  {:<28}  {:<8}  {}", "module", "qualname", "kind", "conf", width = width);
            for r in &rows {
                println!(
                    "{:<width$}  {:<28}  {:<8}  {}",
                    r.module,
                    r.qualname,
                    r.kind,
                    r.confidence,
                    width = width
                );
            }
            println!("\n{} entry points", rows.len());
    Ok(())
}

pub fn files(project: PathBuf, db: Option<PathBuf>) -> anyhow::Result<()> {
            let root = project.canonicalize()?;
            let (conn, project_id) = open_index(&root, db)?;
            let t = std::time::Instant::now();
            let mut rows = clew_core::walk::files(&conn, project_id)?;
            let took = t.elapsed().as_secs_f64() * 1000.0;
            for r in rows.iter_mut() {
                r.path = clew_core::walk::rel_path(&root, &r.path);
            }

            let width = rows.iter().map(|r| r.path.len()).max().unwrap_or(0);
            println!("{:<width$}  {:>6}  {:<4} {}", "path", "syms", "main", "module", width = width);
            for r in &rows {
                println!(
                    "{:<width$}  {:>6}  {:<4} {}",
                    r.path,
                    r.symbols,
                    if r.has_main_guard { "yes" } else { "" },
                    r.module,
                    width = width
                );
            }
            let third = rows.iter().filter(|r| r.is_third_party).count();
            println!("\n{} files ({third} third party)", rows.len());
            eprintln!("[t] files query took {took:.1}ms");
    Ok(())
}

pub fn symbols(project: PathBuf, module: String, db: Option<PathBuf>) -> anyhow::Result<()> {
            let root = project.canonicalize()?;
            let (conn, project_id) = open_index(&root, db)?;
            let rows = clew_core::walk::symbols_in(&conn, project_id, &module)?;
            if rows.is_empty() {
                println!("no symbols in {module}");
                return Ok(());
            }
            println!("{:<5}  {:<8}  {:<40}  {:>5}  {}", "line", "kind", "qualname", "fanin", "entry");
            for r in &rows {
                println!(
                    "{:<5}  {:<8}  {:<40}  {:>5}  {}",
                    r.def_line,
                    r.kind,
                    r.qualname,
                    r.fanin,
                    r.entry_confidence.clone().unwrap_or_default()
                );
            }
            println!("\n{} symbols in {module}", rows.len());
    Ok(())
}

pub fn tree(project: PathBuf, json: bool, rows: usize, db: Option<PathBuf>) -> anyhow::Result<()> {
            let root = project.canonicalize()?;
            let (conn, project_id) = open_index(&root, db)?;
            let t = std::time::Instant::now();
            let mut files = clew_core::walk::tree(&conn, project_id)?;
            for f in files.iter_mut() {
                f.path = clew_core::walk::rel_path(&root, &f.path);
            }
            let took = t.elapsed().as_secs_f64() * 1000.0;
            if json {
                println!("{}", serde_json::to_string(&files)?);
                eprintln!(
                    "[t] tree files={} symbols={} took {took:.1}ms",
                    files.len(),
                    files.iter().map(|f| f.symbols.len()).sum::<usize>()
                );
                return Ok(());
            }
            let mut printed = 0usize;
            for f in &files {
                if printed >= rows {
                    break;
                }
                println!("{}  ({})", f.path, f.module);
                printed += 1;
                for s in &f.symbols {
                    if printed >= rows {
                        break;
                    }
                    println!(
                        "  {:<5} {:<7} {}{}",
                        s.def_line,
                        s.kind,
                        s.qualname,
                        if s.is_entry { "  [entry]" } else { "" }
                    );
                    printed += 1;
                }
            }
            let symbols: usize = files.iter().map(|f| f.symbols.len()).sum();
            let third = files.iter().filter(|f| f.is_third_party).count();
            println!(
                "\n{} files ({third} third party), {symbols} symbols, took {took:.1}ms",
                files.len()
            );
    Ok(())
}

pub fn stats(project: PathBuf, db: Option<PathBuf>) -> anyhow::Result<()> {
            let root = project.canonicalize()?;
            let (conn, project_id) = open_index(&root, db)?;

            let mut classes = 0;
            let mut functions = 0;
            for (kind, n) in clew_core::store::repo::symbol_kind_counts(&conn, project_id)? {
                match kind.as_str() {
                    "class" => classes += n,
                    "module" => {}
                    _ => functions += n,
                }
            }
            println!(
                "cards      {} ({} classes, {} functions/methods)",
                classes + functions,
                classes,
                functions
            );

            let confs = clew_core::store::repo::call_confidence_counts(&conn, project_id)?;
            let total: i64 = confs.iter().map(|(_, n)| n).sum();
            println!("calls      {total}");
            for (name, n) in &confs {
                println!("  {name:<8} {n}");
            }
    Ok(())
}

pub fn args(project: PathBuf, qualname: String, line: i64, db: Option<PathBuf>) -> anyhow::Result<()> {
            let root = project.canonicalize()?;
            let (conn, project_id) = open_index(&root, db)?;

            let rows =
                clew_core::store::repo::call_args_on_line(&conn, project_id, &qualname, line)?;
            if rows.is_empty() {
                println!("no call site on {qualname}:{line}");
                return Ok(());
            }

            let mut last = String::new();
            for r in &rows {
                if r.call_id != last {
                    println!("{}  →  {}  [{}]", r.call_id, r.target, r.confidence);
                    last = r.call_id.clone();
                }
                match (&r.param, &r.expr) {
                    (Some(p), Some(e)) => println!("    {p} = {e}"),
                    _ => println!("    (no argument binding)"),
                }
            }
    Ok(())
}
