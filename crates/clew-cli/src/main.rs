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
        /// Index path; relative paths resolve against the project root
        #[arg(long, default_value = ".clew/index.db")]
        db: PathBuf,
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
        /// Index path; relative paths resolve against the project root
        #[arg(long, default_value = ".clew/index.db")]
        db: PathBuf,
    },
    /// Print index counters read back from the database
    Stats {
        /// Project root
        project: PathBuf,
        /// Index path; relative paths resolve against the project root
        #[arg(long, default_value = ".clew/index.db")]
        db: PathBuf,
    },
    /// Show the argument bindings of the calls on one line of one function
    Args {
        /// Project root
        project: PathBuf,
        /// Caller qualname, e.g. `Board.__init__`
        qualname: String,
        /// 1-based line number
        line: i64,
        /// Index path; relative paths resolve against the project root
        #[arg(long, default_value = ".clew/index.db")]
        db: PathBuf,
    },
    /// List every indexed file, the way the entry panel's file tree sees them
    Files {
        /// Project root
        project: PathBuf,
        /// Index path; relative paths resolve against the project root
        #[arg(long, default_value = ".clew/index.db")]
        db: PathBuf,
    },
    /// List the definitions of one file
    Symbols {
        /// Project root
        project: PathBuf,
        /// Module name, e.g. `pkg.sub.mod`
        module: String,
        /// Index path; relative paths resolve against the project root
        #[arg(long, default_value = ".clew/index.db")]
        db: PathBuf,
    },
    /// Search definitions; a file-name hit lists that file's definitions
    Find {
        /// Project root
        project: PathBuf,
        /// Search text
        q: String,
        /// Index path; relative paths resolve against the project root
        #[arg(long, default_value = ".clew/index.db")]
        db: PathBuf,
    },
    /// Walk the call graph from an entry point
    Walk {
        /// Project root
        project: PathBuf,
        /// Start from this symbol (`qualname` or `module:qualname`) instead of
        /// picking an entry point
        #[arg(long)]
        entry: Option<String>,
        /// Index path; relative paths resolve against the project root
        #[arg(long, default_value = ".clew/index.db")]
        db: PathBuf,
    },
    /// Write the whole graph as JSON, for opening the UI in a plain browser
    Export {
        /// Project root
        project: PathBuf,
        /// Output path; relative paths resolve against the current directory
        #[arg(long, short)]
        out: PathBuf,
        /// Index path; relative paths resolve against the project root
        #[arg(long, default_value = ".clew/index.db")]
        db: PathBuf,
    },
    /// Materialise module cards on demand (what a click does in the GUI)
    Materialize {
        /// Project root
        project: PathBuf,
        /// Module names, e.g. `pkg.sub.mod`
        modules: Vec<String>,
        /// Index path; relative paths resolve against the project root
        #[arg(long, default_value = ".clew/index.db")]
        db: PathBuf,
    },
    /// Measure what opening a project costs when cards are read on demand
    Bench {
        /// Project root
        project: PathBuf,
        /// How many expansions to simulate along the first resolvable call
        #[arg(long, default_value = "100")]
        steps: usize,
        /// Index path; relative paths resolve against the project root
        #[arg(long, default_value = ".clew/index.db")]
        db: PathBuf,
    },
}

fn resolve(root: &Path, db: PathBuf) -> PathBuf {
    if db.is_absolute() {
        db
    } else {
        root.join(db)
    }
}

fn display(path: &Path) -> String {
    path.display().to_string().trim_start_matches(r"\\?\").to_string()
}

/// Open the index of an already-scanned project.
fn open_index(root: &Path, db: PathBuf) -> anyhow::Result<(clew_core::store::Conn, i64)> {
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
            let mut rows = clew_core::walk::files(&conn, project_id)?;
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
        }
        Command::Symbols { project, module, db } => {
            let root = project.canonicalize()?;
            let (conn, _project_id) = open_index(&root, db)?;
            let rows = clew_core::walk::symbols_in(&conn, &module)?;
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
