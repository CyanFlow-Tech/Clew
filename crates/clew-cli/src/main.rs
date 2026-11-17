use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

mod browse;
mod cmd;

#[derive(Parser)]
#[command(name = "clew", version, about = "Entry-point map for Python projects")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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
pub(crate) fn resolve(root: &Path, db: Option<PathBuf>) -> PathBuf {
    match db {
        Some(path) if path.is_absolute() => path,
        Some(path) => root.join(path),
        None => clew_core::space::db_path(root),
    }
}

pub(crate) fn display(path: &Path) -> String {
    path.display().to_string().trim_start_matches(r"\\?\").to_string()
}

/// Open the index of an already-scanned project.
pub(crate) fn open_index(root: &Path, db: Option<PathBuf>) -> anyhow::Result<(clew_core::store::Conn, i64)> {
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
        Command::Entries { project, db } => cmd::entries(project, db)?,
        Command::Files { project, db } => cmd::files(project, db)?,
        Command::Symbols { project, module, db } => cmd::symbols(project, module, db)?,
        Command::Tree { project, json, rows, db } => cmd::tree(project, json, rows, db)?,
        Command::FindSyms { project, path, name, page, size, db } => cmd::findsyms(project, path, name, page, size, db)?,
        Command::Search { project, q, scope, ctx, db, json } => cmd::search(project, q, scope, ctx, db, json)?,
        Command::Find { project, q, db } => cmd::find(project, q, db)?,
        Command::Stats { project, db } => cmd::stats(project, db)?,
        Command::Args { project, qualname, line, db } => cmd::args(project, qualname, line, db)?,
        Command::Walk { project, entry, db } => cmd::walk(project, entry, db)?,
        Command::Export { project, out, db } => cmd::export(project, out, db)?,
        Command::Materialize { project, modules, db } => cmd::materialize(project, modules, db)?,
        Command::Bench { project, steps, db } => cmd::bench(project, steps, db)?,
    }

    Ok(())
}

pub(crate) fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
