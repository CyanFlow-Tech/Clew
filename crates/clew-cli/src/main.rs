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
        } => {
            let root = project.canonicalize()?;
            let db_path = resolve(&root, db);
            if let Some(dir) = db_path.parent() {
                std::fs::create_dir_all(dir)?;
            }

            let mut conn = clew_core::store::open(&db_path)?;
            let stats = clew_core::scan::run::scan(&mut conn, &root, include_venv)?;

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
    }

    Ok(())
}
