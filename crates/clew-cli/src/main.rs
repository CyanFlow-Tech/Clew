use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

mod walk;

#[derive(Parser)]
#[command(name = "clew", version, about = "Entry-point map for Python projects")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan a project and build the index
    Scan {
        /// Project root
        project: PathBuf,
        /// Index path; defaults to the project space, relative paths resolve
        /// against the project root
        #[arg(long)]
        db: Option<PathBuf>,
        /// Also index .venv / venv / env
        #[arg(long)]
        include_venv: bool,
        /// Only index modules, symbols and imports, skipping card bodies
        #[arg(long)]
        symbols_only: bool,
    },
    /// List the entry points discovered for a project
    Entries {
        /// Project root
        project: PathBuf,
        /// Index path; defaults to the project space, relative paths resolve
        /// against the project root
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Print index counters read back from the database
    Stats {
        /// Project root
        project: PathBuf,
        /// Index path; defaults to the project space, relative paths resolve
        /// against the project root
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Show the argument bindings of the calls on one line of one function
    Args {
        /// Project root
        project: PathBuf,
        /// Caller qualname, e.g. `Board.__init__`
        qualname: String,
        /// 1-based line number
        line: i64,
        /// Index path; defaults to the project space, relative paths resolve
        /// against the project root
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// List every indexed file, the way the entry panel's file tree sees them
    Files {
        /// Project root
        project: PathBuf,
        /// Index path; defaults to the project space, relative paths resolve
        /// against the project root
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// List the definitions of one file
    Symbols {
        /// Project root
        project: PathBuf,
        /// Module name, e.g. `pkg.sub.mod`
        module: String,
        /// Index path; defaults to the project space, relative paths resolve
        /// against the project root
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Print the sidebar symbol tree: every file and the definitions inside it
    Tree {
        /// Project root
        project: PathBuf,
        /// Print the whole tree as JSON instead of an outline
        #[arg(long)]
        json: bool,
        /// How many outline rows to print
        #[arg(long, default_value = "60")]
        rows: usize,
        /// Index path; defaults to the project space, relative paths resolve
        /// against the project root
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Substring search over paths and definition names (the sidebar's search)
    FindSyms {
        /// Project root
        project: PathBuf,
        /// Filter against the file path, any substring, case insensitive
        #[arg(long)]
        path: Option<String>,
        /// Filter against the definition name, any substring, case insensitive
        #[arg(long)]
        name: Option<String>,
        /// 1-based page number
        #[arg(long, default_value = "1")]
        page: usize,
        /// Rows per page
        #[arg(long, default_value = "50")]
        size: usize,
        /// Index path; defaults to the project space, relative paths resolve
        /// against the project root
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Search definitions. Use `prefix:name`, `prefix:`, `:name` or bare text
    Search {
        /// Project root
        project: PathBuf,
        /// Query text. A colon splits the module prefix from the definition name
        q: String,
        /// Result scope: entry (default) or all
        #[arg(long, default_value = "entry")]
        scope: String,
        /// Module to boost, usually the one being read
        #[arg(long)]
        ctx: Option<String>,
        /// Index path; defaults to the project space, relative paths resolve
        /// against the project root
        #[arg(long)]
        db: Option<PathBuf>,
        /// Print the page as JSON instead of a table
        #[arg(long)]
        json: bool,
    },
    /// Search definitions; a file-name hit lists that file's definitions
    Find {
        /// Project root
        project: PathBuf,
        /// Search text
        q: String,
        /// Index path; defaults to the project space, relative paths resolve
        /// against the project root
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Walk the call graph from an entry point
    Walk {
        /// Project root
        project: PathBuf,
        /// Start from this symbol (`qualname` or `module:qualname`) instead of
        /// picking an entry point
        #[arg(long)]
        entry: Option<String>,
        /// Index path; defaults to the project space, relative paths resolve
        /// against the project root
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Write the whole graph as JSON, for opening the UI in a plain browser
    Export {
        /// Project root
        project: PathBuf,
        /// Output path; relative paths resolve against the current directory
        #[arg(long, short)]
        out: PathBuf,
        /// Index path; defaults to the project space, relative paths resolve
        /// against the project root
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Materialise module cards on demand (what a click does in the GUI)
    Materialize {
        /// Project root
        project: PathBuf,
        /// Module names, e.g. `pkg.sub.mod`
        modules: Vec<String>,
        /// Index path; defaults to the project space, relative paths resolve
        /// against the project root
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Measure what opening a project costs when cards are read on demand
    Bench {
        /// Project root
        project: PathBuf,
        /// How many expansions to simulate along the first resolvable call
        #[arg(long, default_value = "100")]
        steps: usize,
        /// Index path; defaults to the project space, relative paths resolve
        /// against the project root
        #[arg(long)]
        db: Option<PathBuf>,
    },
}

/// 没给 `--db` 就用项目空间里的库，给了就按项目根解析相对路径。
fn resolve(root: &Path, db: Option<PathBuf>) -> PathBuf {
    match db {
        Some(path) if path.is_absolute() => path,
        Some(path) => root.join(path),
        None => clew_core::space::db_path(root),
    }
}

fn display(path: &Path) -> String {
    path.display().to_string().trim_start_matches(r"\\?\").to_string()
}

/// Open the index of an already-scanned project.
fn open_index(root: &Path, db: Option<PathBuf>) -> anyhow::Result<(clew_core::store::Conn, i64)> {
    let db_path = resolve(root, db);
    match clew_core::store::open_index(&db_path, root)? {
        Some(pair) => Ok(pair),
        None => anyhow::bail!(
            "project not indexed yet, run scan first: {} (db {})",
            display(root),
            display(&db_path)
        ),
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Scan {
            project,
            db,
            include_venv,
            symbols_only,
        } => {
            let root = project.canonicalize()?;
            let db_path = resolve(&root, db);
            if let Some(dir) = db_path.parent() {
                std::fs::create_dir_all(dir)?;
            }

            let mut conn = clew_core::store::open(&db_path)?;
            let stats = clew_core::scan::run::scan_with(
                &mut conn,
                &root,
                include_venv,
                !symbols_only,
                &mut |_, _, _, _| {},
            )?;

            println!("db         {}", display(&db_path));
            println!("modules    {} (3rd {})", stats.modules, stats.third_party_modules);
            println!(
                "symbols    {} ({} classes, {} functions/methods)",
                stats.symbols, stats.classes, stats.functions
            );
            println!(
                "calls      {} (resolved {})",
                stats.call_sites, stats.calls_resolved
            );
            println!("  args     {}", stats.call_args);
            println!("  cands    {}", stats.call_candidates);
            println!("imports    {}", stats.imports);
            println!(
                "entries    {} (main guard {}, pyproject {}, unresolved {})",
                stats.entries_main_guard + stats.entries_pyproject,
                stats.entries_main_guard,
                stats.entries_pyproject,
                stats.entries_unresolved
            );
            println!("elapsed    {} ms", stats.elapsed_ms);
        }
        Command::Entries { project, db } => {
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
        }
        Command::Files { project, db } => {
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
        }
        Command::Symbols { project, module, db } => {
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
        }
        Command::Tree {
            project,
            json,
            rows: limit,
            db,
        } => {
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
                if printed >= limit {
                    break;
                }
                println!("{}  ({})", f.path, f.module);
                printed += 1;
                for s in &f.symbols {
                    if printed >= limit {
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
        }
        Command::FindSyms {
            project,
            path,
            name,
            page,
            size,
            db,
        } => {
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
        }
        Command::Search {
            project,
            q,
            scope,
            ctx,
            db,
            json,
        } => {
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
        }
        Command::Find { project, q, db } => {
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
        }
        Command::Stats { project, db } => {
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
        }
        Command::Args {
            project,
            qualname,
            line,
            db,
        } => {
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
        }
        Command::Walk { project, entry, db } => {
            let root = project.canonicalize()?;
            let (conn, project_id) = open_index(&root, db)?;
            walk::run(&conn, project_id, walk::Options { entry })?;
        }
        Command::Export { project, out, db } => {
            use std::io::Write;

            let root = project.canonicalize()?;
            let (conn, project_id) = open_index(&root, db)?;
            let snap = clew_core::snapshot::snapshot(&conn, project_id)?;

            if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(dir)?;
            }
            let mut file = std::io::BufWriter::new(std::fs::File::create(&out)?);
            serde_json::to_writer(&mut file, &snap)?;
            file.flush()?;
            drop(file);

            let bytes = std::fs::metadata(&out)?.len();
            println!("out        {}", display(&out));
            println!("cards      {}", snap.cards.len());
            println!("calls      {}", snap.calls.len());
            println!("entries    {}", snap.entries.len());
            println!("size       {} KiB", bytes / 1024);
        }
        Command::Materialize {
            project,
            modules,
            db,
        } => {
            let root = project.canonicalize()?;
            let (conn, project_id) = open_index(&root, db)?;
            let mut mat = clew_core::scan::materialize::Materializer::open(project_id, conn)?;
            // GUI 打开项目时就建好了，这里对齐一下计时口径
            let t_pre = std::time::Instant::now();
            mat.preload()?;
            println!("preload   {:.1} ms", ms(t_pre.elapsed()));
            let mut total = std::time::Duration::ZERO;
            for module in &modules {
                let t = std::time::Instant::now();
                let cards = mat.ensure(module)?;
                let d = t.elapsed();
                total += d;
                println!("{:<40} {:>4} cards  {:>8.1} ms", module, cards, ms(d));
            }
            if modules.len() > 1 {
                println!(
                    "total {:.1} ms  avg {:.1} ms",
                    ms(total),
                    ms(total) / modules.len() as f64
                );
            }
        }
        Command::Bench {
            project,
            steps,
            db,
        } => {
            let root = project.canonicalize()?;
            let (conn, project_id) = open_index(&root, db)?;

            // what the GUI does on open: read the entry list, then the first card
            let t = std::time::Instant::now();
            let entries = clew_core::snapshot::entries(&conn, project_id)?;
            let d_entries = t.elapsed();

            let first = entries
                .first()
                .map(|e| e.id.clone())
                .ok_or_else(|| anyhow::anyhow!("no entry points in this project"))?;
            let t = std::time::Instant::now();
            let payload = clew_core::snapshot::cards_of(&conn, project_id, &[first.clone()])?;
            let d_first = t.elapsed();
            let first_calls = payload.calls.len();
            let first_bytes = serde_json::to_string(&payload)?.len();

            // then one expansion per click, following resolved edges breadth first
            let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
            queue.push_back(first);
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut per_step = Vec::new();
            let mut total = std::time::Duration::ZERO;
            let t_all = std::time::Instant::now();
            while per_step.len() < steps {
                let Some(id) = queue.pop_front() else { break };
                if !seen.insert(id.clone()) {
                    continue;
                }
                let t = std::time::Instant::now();
                let payload = clew_core::snapshot::cards_of(&conn, project_id, &[id])?;
                let d = t.elapsed();
                per_step.push(d);
                total += d;
                for call in payload.calls.values() {
                    if let Some(to) = &call.to {
                        if !seen.contains(to) {
                            queue.push_back(to.clone());
                        }
                    }
                }
            }
            let d_all = t_all.elapsed();

            per_step.sort();
            let n = per_step.len();
            let avg = if n > 0 { total.as_secs_f64() / n as f64 } else { 0.0 };
            let min = per_step.first().map(|d| d.as_secs_f64()).unwrap_or(0.0);
            let max = per_step.last().map(|d| d.as_secs_f64()).unwrap_or(0.0);

            println!("entries      {:.2} ms  ({} entry points)", ms(d_entries), entries.len());
            println!(
                "first card   {:.2} ms  ({} call sites, {} bytes)",
                ms(d_first),
                first_calls,
                first_bytes
            );
            println!(
                "open session {:.2} ms  (entries + first card)",
                ms(d_entries + d_first)
            );
            println!("steps        {}", n);
            println!("step total   {:.2} ms", ms(d_all));
            println!(
                "step each    avg {:.2} ms  min {:.2}  max {:.2}",
                avg * 1000.0,
                min * 1000.0,
                max * 1000.0
            );

            // one line of CSV for the plotting script
            println!(
                "csv,{:.3},{:.3},{},{:.3},{:.3},{:.3}",
                ms(d_entries),
                ms(d_first),
                n,
                ms(d_all),
                avg * 1000.0,
                max * 1000.0
            );
        }
    }

    Ok(())
}

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
