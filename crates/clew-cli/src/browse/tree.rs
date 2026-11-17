use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};

use clew_core::store::Conn;
use clew_core::walk::{self, NodeView};
use super::labels::*;
use super::HELP;


pub(crate) struct Tree<'a> {
    conn: &'a Conn,
    root: i64,
    focus: i64,
    /// Nodes whose children are shown
    opened: BTreeSet<i64>,
    parent: BTreeMap<i64, i64>,
    cache: BTreeMap<i64, NodeView>,
    /// Enterable nodes of this redraw, in display order; number = index + 1
    index: Vec<i64>,
}

impl<'a> Tree<'a> {
    pub(crate) fn new(conn: &'a Conn, root: i64) -> anyhow::Result<Self> {
        let Some(view) = walk::node(conn, root)? else {
            anyhow::bail!("symbol {root} not found in the index");
        };
        Ok(Self {
            conn,
            root,
            focus: root,
            opened: BTreeSet::from([root]),
            parent: BTreeMap::new(),
            cache: BTreeMap::from([(root, view)]),
            index: Vec::new(),
        })
    }

    pub(crate) fn repl(mut self) -> anyhow::Result<()> {
        let stdin = io::stdin();
        loop {
            self.render();
            print!("> ");
            io::stdout().flush()?;

            let mut line = String::new();
            if stdin.read_line(&mut line)? == 0 {
                println!();
                return Ok(());
            }
            match line.trim() {
                "" => {}
                "q" | "quit" | "exit" => return Ok(()),
                "b" => self.up(),
                "a" => self.print_args(),
                "c" => self.print_code(),
                "h" | "?" => println!("{HELP}"),
                "n" => self.add_note()?,
                other => match other.parse::<usize>() {
                    Ok(n) => self.descend(n),
                    Err(_) => println!("未知命令 `{other}`；{HELP}"),
                },
            }
        }
    }

    pub(crate) fn ensure(&mut self, id: i64) {
        if self.cache.contains_key(&id) {
            return;
        }
        if let Ok(Some(view)) = walk::node(self.conn, id) {
            self.cache.insert(id, view);
        }
    }

    pub(crate) fn descend(&mut self, n: usize) {
        let Some(&dst) = self.index.get(n.wrapping_sub(1)) else {
            println!("没有编号 {n}");
            return;
        };
        self.ensure(dst);
        self.opened.insert(self.focus);
        self.parent.insert(dst, self.focus);
        self.opened.insert(dst);
        self.focus = dst;
    }

    pub(crate) fn up(&mut self) {
        match self.parent.get(&self.focus) {
            Some(&p) => self.focus = p,
            None => println!("已在起点"),
        }
    }

    pub(crate) fn add_note(&mut self) -> anyhow::Result<()> {
        let Some(view) = self.cache.get(&self.focus) else {
            return Ok(());
        };
        print!("笔记 → {}: ", view.qualname);
        io::stdout().flush()?;

        let mut text = String::new();
        if io::stdin().read_line(&mut text)? == 0 {
            return Ok(());
        }
        let text = text.trim();
        if text.is_empty() {
            return Ok(());
        }
        let id = clew_core::store::repo::insert_note(self.conn, self.focus, text)?;
        println!("已记下 note #{id}");
        Ok(())
    }

    pub(crate) fn render(&mut self) {
        let mut index = Vec::new();
        self.collect(self.root, &mut index);
        self.index = index;

        let Some(view) = self.cache.get(&self.focus) else {
            return;
        };
        let path = self.path();
        println!();
        println!("{}", "─".repeat(72));
        println!("{}  ({})", view.title(), view.kind);
        println!(
            "{}  ·  调用 {} 处（解析 {}）  ·  {}",
            path,
            view.total_calls(),
            view.edges.iter().map(|e| e.hits.len()).sum::<usize>(),
            view.module
        );
        if view.is_entry {
            println!(
                "入口点  {}",
                view.entry_confidence.as_deref().unwrap_or("unknown")
            );
        }
        let notes = walk::notes(self.conn, self.focus).unwrap_or_default();
        if !notes.is_empty() {
            println!("📝 {} 条笔记 · {}", notes.len(), notes[0]);
        }
        println!("{}", "─".repeat(72));

        let mut lines = Vec::new();
        self.draw(self.root, "", &mut lines);
        for line in lines {
            println!("{line}");
        }

        let hidden = view.builtin_calls + view.unresolved_calls;
        if hidden > 0 {
            println!("另有 {hidden} 处 builtin / 未解析");
        }
        println!("{HELP}");
    }

    pub(crate) fn path(&self) -> String {
        let mut chain = vec![self.focus];
        let mut cur = self.focus;
        while let Some(&p) = self.parent.get(&cur) {
            chain.push(p);
            cur = p;
        }
        chain.reverse();
        chain
            .iter()
            .filter_map(|id| self.cache.get(id).map(|v| v.short().to_string()))
            .collect::<Vec<_>>()
            .join(" › ")
    }

    pub(crate) fn collect(&self, id: i64, index: &mut Vec<i64>) {
        let Some(view) = self.cache.get(&id) else {
            return;
        };
        for edge in &view.edges {
            if !enterable(edge) {
                continue;
            }
            if self.opened.contains(&edge.dst) {
                self.collect(edge.dst, index);
            } else {
                index.push(edge.dst);
            }
        }
    }

    pub(crate) fn draw(&self, id: i64, prefix: &str, out: &mut Vec<String>) {
        let Some(view) = self.cache.get(&id) else {
            return;
        };
        let total = view.edges.len();
        for (i, edge) in view.edges.iter().enumerate() {
            let last = i + 1 == total;
            let branch = if last { "└─ " } else { "├─ " };
            let child_prefix = format!("{prefix}{}", if last { "   " } else { "│  " });

            let opened = self.opened.contains(&edge.dst);
            let mark = if !enterable(edge) {
                "··".to_string()
            } else if opened {
                "▾ ".to_string()
            } else {
                match self.index.iter().position(|d| *d == edge.dst) {
                    Some(p) => format!("[{:<2}]", p + 1),
                    None => "   ".to_string(),
                }
            };

            let mut line = format!("{prefix}{branch}{mark} {}", label(view, edge));
            line.push_str(&format!("  {}", sites(edge)));
            if !enterable(edge) {
                line.push_str("  (third-party)");
            }
            if let Some(note) = poly_note(edge) {
                line.push_str(&note);
            }
            if edge.dst == self.focus {
                line.push_str("   ← 当前");
            }
            out.push(line);

            if opened {
                let has_children = self
                    .cache
                    .get(&edge.dst)
                    .map(|v| !v.edges.is_empty())
                    .unwrap_or(false);
                if has_children {
                    self.draw(edge.dst, &child_prefix, out);
                } else {
                    out.push(format!("{child_prefix}(无调用)"));
                }
            }
        }
    }

    pub(crate) fn print_code(&self) {
        let Some(view) = self.cache.get(&self.focus) else {
            return;
        };
        let lines = walk::card_lines(self.conn, self.focus).unwrap_or_default();
        if lines.is_empty() {
            println!("{} 没有可显示的行", view.qualname);
            return;
        }
        println!("{}  ·  {} 行", view.title(), lines.len());
        let width = lines.iter().map(|l| l.ln.to_string().len()).max().unwrap_or(3);
        for line in &lines {
            let pad = "  ".repeat(line.indent.max(0) as usize);
            let anchor = match (&line.call_target, line.confidence.as_deref()) {
                (Some(target), Some(conf)) => match conf {
                    "exact" => format!("  ◆ {target}"),
                    "poly" => format!("  ◆ {target}  (多态)"),
                    "heur" => format!("  ◆ {target}  (同名)"),
                    "builtin" => format!("  ◆ {target}  (builtin)"),
                    _ => format!("  ◆ {target}  (未解析)"),
                },
                _ => String::new(),
            };
            println!("  {:>width$}  {pad}{}{anchor}", line.ln, line.text, width = width);
        }
    }

    pub(crate) fn print_args(&self) {
        let Some(view) = self.cache.get(&self.focus) else {
            return;
        };
        if view.edges.is_empty() {
            println!("{} 没有解析出的调用", view.qualname);
            return;
        }
        println!("{} 的调用点：", view.qualname);
        for edge in &view.edges {
            println!("  → {}  ({})", edge.qualname, edge.module);
            for (i, hit) in edge.hits.iter().enumerate() {
                if i >= 5 {
                    println!("       … 另有 {} 处调用", edge.hits.len() - 5);
                    break;
                }
                if hit.args.is_empty() {
                    println!("    L{:<5} {}", hit.ln, hit.confidence);
                } else {
                    println!("    L{:<5} {}", hit.ln, hit.confidence);
                    for (param, expr) in &hit.args {
                        println!("        {param} = {expr}");
                    }
                }
            }
        }
        if view.builtin_calls + view.unresolved_calls > 0 {
            println!(
                "  另有 {} 处 builtin / 未解析",
                view.builtin_calls + view.unresolved_calls
            );
        }
    }
}
