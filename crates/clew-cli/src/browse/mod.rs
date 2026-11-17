mod labels;
mod tree;

pub(crate) use tree::Tree;

use std::io::{self, Write};

use clew_core::store::Conn;
use clew_core::walk::{self};
pub struct Options {
    /// Entry to start from: `qualname` or `module:qualname`
    pub entry: Option<String>,
}

pub(crate) const HELP: &str = "编号 进入    b 返回    c 看代码    a 看实参    n 记笔记    q 退出";

pub fn run(conn: &Conn, project_id: i64, opts: Options) -> anyhow::Result<()> {
    let root = match opts.entry {
        Some(needle) => {
            let hits = walk::find(conn, project_id, &needle)?;
            match hits.len() {
                0 => anyhow::bail!("no symbol matches `{needle}`"),
                1 => hits[0],
                _ => match choose(conn, &hits, &format!("`{needle}` 命中多处"))? {
                    Some(id) => id,
                    None => return Ok(()),
                },
            }
        }
        None => match pick_entry(conn, project_id)? {
            Some(id) => id,
            None => return Ok(()),
        },
    };

    Tree::new(conn, root)?.repl()
}

fn pick_entry(conn: &Conn, project_id: i64) -> anyhow::Result<Option<i64>> {
    let entries = walk::entry_points(conn, project_id)?;
    if entries.is_empty() {
        println!("没有入口点，先跑 clew scan");
        return Ok(None);
    }
    println!("入口点：");
    for (i, (_, module, qualname, conf)) in entries.iter().enumerate() {
        println!("  [{:<2}] {:<8} {module}:{qualname}", i + 1, conf);
    }
    print!("选择 1-{} 或直接回车退出: ", entries.len());
    io::stdout().flush()?;

    let mut line = String::new();
    if io::stdin().read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let Ok(n) = line.trim().parse::<usize>() else {
        return Ok(None);
    };
    Ok(entries.get(n - 1).map(|e| e.0))
}

fn choose(conn: &Conn, ids: &[i64], prompt: &str) -> anyhow::Result<Option<i64>> {
    println!("{prompt}：");
    for (i, id) in ids.iter().enumerate() {
        if let Some(v) = walk::node(conn, *id)? {
            println!("  [{:<2}] {}:{}  ({})", i + 1, v.module, v.qualname, v.kind);
        }
    }
    print!("选择 1-{} 或直接回车退出: ", ids.len());
    io::stdout().flush()?;

    let mut line = String::new();
    if io::stdin().read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let Ok(n) = line.trim().parse::<usize>() else {
        return Ok(None);
    };
    Ok(ids.get(n - 1).copied())
}
