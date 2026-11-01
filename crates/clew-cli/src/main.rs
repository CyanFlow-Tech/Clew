use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

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
    /// Export the graph (cards + call sites) as JSON
    Export {
        /// Project root
        project: PathBuf,
        /// Output JSON path
        #[arg(long, default_value = "clew-graph.json")]
        out: PathBuf,
    },
    /// List the entry points discovered for a project
    Entries {
        /// Project root
        project: PathBuf,
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
            println!("files      {}", stats.files);
            println!("  3rd      {}", stats.third_party_files);
            println!("symbols    {}", stats.symbols);
            println!("calls      {}", stats.call_sites);
            println!("edges      {}", stats.edges);
            println!(
                "entries    {} (main guard {}, pyproject {}, unresolved {})",
                stats.entries_main_guard + stats.entries_pyproject,
                stats.entries_main_guard,
                stats.entries_pyproject,
                stats.entries_unresolved
            );
            println!("unresolved attr   {}", stats.unresolved_attribute);
            println!("unresolved other  {}", stats.unresolved_other);
            println!("unresolved callee {}", stats.unresolved_callee);
            println!("elapsed    {} ms", stats.elapsed_ms);
        }
        Command::Export { project, out } => {
            let root = project.canonicalize()?;
            let graph = clew_core::scan::graph_build::build(&root)?;
            let stats = graph.stats();
            let json = serde_json::to_string(&graph)?;
            std::fs::write(&out, json)?;

            println!("cards      {} ({} classes, {} functions/methods)", stats.cards, stats.classes, stats.functions);
            println!("calls      {}", stats.calls);
            println!("  exact    {}", stats.exact);
            println!("  poly     {}", stats.poly);
            println!("  heur     {}", stats.heur);
            println!("  builtin  {}", stats.builtin);
            println!("  none     {}", stats.none);
            println!("out        {}", display(&out));
        }
        Command::Entries { project, db } => {
            let root = project.canonicalize()?;
            let db_path = resolve(&root, db);
            let conn = clew_core::store::open(&db_path)?;

            let project_id = clew_core::store::repo::project_id_by_path(
                &conn,
                &root.to_string_lossy(),
            )?
            .ok_or_else(|| anyhow::anyhow!("project not indexed yet, run scan first: {}", display(&root)))?;

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
    }

    Ok(())
}
