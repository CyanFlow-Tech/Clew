//! Read-side views for interactive descent.
//!
//! Everything here is a plain query over the index DB: `walk` and the future GUI
//! both read the same shapes, so resolution never has to run twice.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

/// One call on the caller's line, with what it bound.
#[derive(Debug, Clone, Serialize)]
pub struct CallHit {
    pub call_id: String,
    pub ln: i64,
    pub confidence: String,
    pub args: Vec<(String, String)>,
    /// Candidate implementations when dispatch is dynamic
    pub candidates: Vec<String>,
}

/// A resolved callee plus every call site that reaches it.
#[derive(Debug, Clone, Serialize)]
pub struct EdgeView {
    pub dst: i64,
    pub module: String,
    pub qualname: String,
    pub kind: String,
    pub is_third_party: bool,
    pub is_entry: bool,
    pub hits: Vec<CallHit>,
}

impl EdgeView {
    /// Last path segment of the qualname.
    pub fn short(&self) -> &str {
        self.qualname.rsplit('.').next().unwrap_or(&self.qualname)
    }
}

/// A node as the tree view needs it.
#[derive(Debug, Clone, Serialize)]
pub struct NodeView {
    pub id: i64,
    pub module: String,
    pub qualname: String,
    pub kind: String,
    pub params: Vec<String>,
    pub is_third_party: bool,
    pub is_entry: bool,
    pub entry_confidence: Option<String>,
    /// Resolved outgoing edges, in first-call order
    pub edges: Vec<EdgeView>,
    /// Calls that reached a builtin or a library function
    pub builtin_calls: usize,
    /// Calls nothing could be said about
    pub unresolved_calls: usize,
}

impl NodeView {
    /// `Class.method(a, b)` style heading.
    pub fn title(&self) -> String {
        format!("{}({})", self.qualname, self.params.join(", "))
    }

    pub fn short(&self) -> &str {
        self.qualname.rsplit('.').next().unwrap_or(&self.qualname)
    }

    /// Total calls counted at this node, resolved or not.
    pub fn total_calls(&self) -> usize {
        self.edges.iter().map(|e| e.hits.len()).sum::<usize>()
            + self.builtin_calls
            + self.unresolved_calls
    }
}

/// Load one node with its resolved outgoing edges.
pub fn node(conn: &Connection, id: i64) -> rusqlite::Result<Option<NodeView>> {
    let head = conn
        .query_row(
            "SELECT module, qualname, kind, is_third_party, is_entry, entry_confidence
               FROM symbol WHERE id = ?1",
            params![id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)? != 0,
                    row.get::<_, i64>(4)? != 0,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .optional()?;

    let Some((module, qualname, kind, is_third_party, is_entry, entry_confidence)) = head else {
        return Ok(None);
    };

    let mut stmt = conn.prepare("SELECT name FROM card_param WHERE symbol_id = ?1 ORDER BY ord")?;
    let params_list: Vec<String> = stmt
        .query_map(params![id], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;

    let mut args: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT a.call_id, a.param, a.expr
           FROM call_arg a JOIN call_site c ON c.id = a.call_id
          WHERE c.symbol_id = ?1
          ORDER BY a.call_id, a.param",
    )?;
    for row in stmt.query_map(params![id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })? {
        let (call_id, param, expr) = row?;
        args.entry(call_id).or_default().push((param, expr));
    }

    let mut candidates: HashMap<String, Vec<String>> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT cc.call_id, cc.card_id
           FROM call_candidate cc JOIN call_site c ON c.id = cc.call_id
          WHERE c.symbol_id = ?1
          ORDER BY cc.call_id, cc.ord",
    )?;
    for row in stmt.query_map(params![id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })? {
        let (call_id, card_id) = row?;
        candidates.entry(call_id).or_default().push(card_id);
    }

    let mut stmt = conn.prepare(
        "SELECT c.id, c.dst_symbol, s.module, s.qualname, s.kind, s.is_third_party,
                s.is_entry, c.confidence, c.call_line
           FROM call_site c JOIN symbol s ON s.id = c.dst_symbol
          WHERE c.symbol_id = ?1
          ORDER BY c.call_line, c.id",
    )?;
    let rows = stmt.query_map(params![id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)? != 0,
            row.get::<_, i64>(6)? != 0,
            row.get::<_, String>(7)?,
            row.get::<_, i64>(8)?,
        ))
    })?;

    let mut edges: Vec<EdgeView> = Vec::new();
    let mut slot: HashMap<i64, usize> = HashMap::new();
    for row in rows {
        let (call_id, dst, m, q, k, third, dst_entry, conf, ln) = row?;
        let hit = CallHit {
            candidates: candidates.remove(&call_id).unwrap_or_default(),
            args: args.remove(&call_id).unwrap_or_default(),
            call_id,
            ln,
            confidence: conf,
        };
        match slot.get(&dst) {
            Some(&i) => edges[i].hits.push(hit),
            None => {
                slot.insert(dst, edges.len());
                edges.push(EdgeView {
                    dst,
                    module: m,
                    qualname: q,
                    kind: k,
                    is_third_party: third,
                    is_entry: dst_entry,
                    hits: vec![hit],
                });
            }
        }
    }

    let mut builtin_calls = 0usize;
    let mut unresolved_calls = 0usize;
    let mut stmt = conn.prepare(
        "SELECT confidence, count(*) FROM call_site
          WHERE symbol_id = ?1 AND dst_symbol IS NULL GROUP BY confidence",
    )?;
    for row in stmt.query_map(params![id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })? {
        let (conf, n) = row?;
        if conf == "builtin" {
            builtin_calls += n as usize;
        } else {
            unresolved_calls += n as usize;
        }
    }

    Ok(Some(NodeView {
        id,
        module,
        qualname,
        kind,
        params: params_list,
        is_third_party,
        is_entry,
        entry_confidence,
        edges,
        builtin_calls,
        unresolved_calls,
    }))
}

/// One file of the project, as the entry panel lists it.
#[derive(Debug, Clone, Serialize)]
pub struct FileRow {
    pub module: String,
    pub path: String,
    pub is_third_party: bool,
    pub has_main_guard: bool,
    pub symbols: i64,
}

/// One definition inside a file, as the entry panel lists it.
#[derive(Debug, Clone, Serialize)]
pub struct SymbolRow {
    pub card_id: String,
    pub module: String,
    pub qualname: String,
    pub kind: String,
    pub def_line: i64,
    pub is_entry: bool,
    pub entry_confidence: Option<String>,
    pub params: Vec<String>,
    /// How many call sites in the project reach this symbol.
    pub fanin: i64,
}

/// One hit of the entry panel search, grouped by file.
#[derive(Debug, Clone, Serialize)]
pub struct SearchFile {
    pub module: String,
    pub path: String,
    /// 文件名命中了，所以这里列的是整个文件的定义，不是只列名字命中的那些
    pub by_file: bool,
    pub symbols: Vec<SymbolRow>,
}

/// 面板里显示的路径一律相对项目根，用正斜杠。root 和 path 都可能带 Windows
/// 的 `\\?\` 前缀，先统一剥掉再比，否则 strip_prefix 永远失败，树就从盘符开始了。
pub fn rel_path(root: &Path, path: &str) -> String {
    fn norm(s: &str) -> String {
        s.trim_start_matches(r"\\?\").replace('\\', "/")
    }
    let root = norm(&root.to_string_lossy());
    let p = norm(path);
    let rest = p
        .strip_prefix(root.trim_end_matches('/'))
        .unwrap_or(&p)
        .trim_start_matches('/');
    if rest.is_empty() {
        p.trim_start_matches('/').to_string()
    } else {
        rest.to_string()
    }
}

/// Every indexed file of a project, ordered by path.
pub fn files(conn: &Connection, project_id: i64) -> rusqlite::Result<Vec<FileRow>> {
    let mut stmt = conn.prepare(
        "SELECT m.module, m.path, m.is_third_party, m.has_main_guard,
                (SELECT count(*) FROM symbol s
                  WHERE s.project_id = m.project_id AND s.module = m.module)
           FROM module m
          WHERE m.project_id = ?1
          ORDER BY m.path",
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        let path: String = row.get(1)?;
        Ok(FileRow {
            module: row.get(0)?,
            path: path.trim_start_matches(r"\\?\").to_string(),
            is_third_party: row.get::<_, i64>(2)? != 0,
            has_main_guard: row.get::<_, i64>(3)? != 0,
            symbols: row.get(4)?,
        })
    })?;
    rows.collect()
}

/// Every definition of one file, in source order.
pub fn symbols_in(conn: &Connection, module: &str) -> rusqlite::Result<Vec<SymbolRow>> {
    symbols_of(conn, module, true)
}

/// 搜索时不需要参数表，少一次 per-symbol 的查询，不然大项目上每次按键都要
/// 对上万个符号各跑一条 SQL。
fn symbols_of(
    conn: &Connection,
    module: &str,
    with_params: bool,
) -> rusqlite::Result<Vec<SymbolRow>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.qualname, s.kind, s.def_line, s.is_entry, s.entry_confidence,
                (SELECT count(*) FROM call_site c WHERE c.dst_symbol = s.id)
           FROM symbol s
          WHERE s.module = ?1
          ORDER BY s.def_line, s.qualname",
    )?;
    let mut out = Vec::new();
    for row in stmt.query_map(params![module], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)? != 0,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, i64>(6)?,
        ))
    })? {
        let (id, qualname, kind, def_line, is_entry, entry_confidence, fanin) = row?;
        let params_list = if with_params {
            let mut pstmt =
                conn.prepare("SELECT name FROM card_param WHERE symbol_id = ?1 ORDER BY ord")?;
            pstmt
                .query_map(params![id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        } else {
            Vec::new()
        };
        out.push(SymbolRow {
            card_id: format!("{module}:{qualname}"),
            module: module.to_string(),
            qualname,
            kind,
            def_line,
            is_entry,
            entry_confidence,
            params: params_list,
            fanin,
        });
    }
    Ok(out)
}

/// Search the entry panel: a file name match lists every definition in that
/// file, a definition name match lists just those definitions.
pub fn search(
    conn: &Connection,
    project_id: i64,
    needle: &str,
    limit: usize,
) -> rusqlite::Result<Vec<SearchFile>> {
    let needle = needle.trim();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let like = format!("%{needle}%");
    let low = needle.to_lowercase();
    let limit = limit.max(1);

    // 名字命中的上限，防止一个短词把整个项目的符号都拉进来
    const NAME_LIMIT: usize = 300;
    // 文件命中最多展开这么多个文件，每个文件都要查一遍它的定义
    const FILE_LIMIT: usize = 30;

    // 先用名字命中建组，这样它排在前面，而且同一个文件里没命中的函数不会被带进来
    let mut groups: Vec<SearchFile> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();

    let mut name_stmt = conn.prepare(
        "SELECT s.module, COALESCE(m.path, ''), s.qualname, s.kind, s.def_line,
                s.is_entry, s.entry_confidence,
                (SELECT count(*) FROM call_site c WHERE c.dst_symbol = s.id)
           FROM symbol s
           LEFT JOIN module m ON m.project_id = s.project_id AND m.module = s.module
          WHERE s.project_id = ?1 AND s.qualname LIKE ?2
          ORDER BY s.def_line, s.qualname
          LIMIT ?3",
    )?;
    let rows = name_stmt.query_map(params![project_id, like, NAME_LIMIT as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)? != 0,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, i64>(7)?,
        ))
    })?;
    for row in rows {
        let (module, path, qualname, kind, def_line, is_entry, confidence, fanin) = row?;
        let symbol = SymbolRow {
            card_id: format!("{module}:{qualname}"),
            module: module.clone(),
            qualname,
            kind,
            def_line,
            is_entry,
            entry_confidence: confidence,
            params: Vec::new(),
            fanin,
        };
        let path = path.trim_start_matches(r"\\?\").to_string();
        match index.get(&module) {
            Some(&i) => groups[i].symbols.push(symbol),
            None => {
                index.insert(module.clone(), groups.len());
                groups.push(SearchFile {
                    module,
                    path,
                    by_file: false,
                    symbols: vec![symbol],
                });
            }
        }
    }

    // 文件命中只比文件名和模块最后一段。目录名不算，否则搜 man 会把 command、
    // manager 这些目录下的文件全部拉出来，一次按键就是上千个文件。
    let mut file_stmt = conn.prepare(
        "SELECT module, path FROM module WHERE project_id = ?1 ORDER BY path",
    )?;
    let mut opened = 0usize;
    for row in file_stmt.query_map(params![project_id], |row| {
        let path: String = row.get(1)?;
        Ok((
            row.get::<_, String>(0)?,
            path.trim_start_matches(r"\\?\").to_string(),
        ))
    })? {
        if opened >= FILE_LIMIT {
            break;
        }
        let (module, path) = row?;
        let base = path
            .rsplit(|c| c == '/' || c == '\\')
            .next()
            .unwrap_or(&path)
            .to_lowercase();
        let last = module.rsplit('.').next().unwrap_or(&module).to_lowercase();
        if !base.contains(&low) && !last.contains(&low) {
            continue;
        }
        let syms = symbols_of(conn, &module, false)?;
        // 一条定义都没有的文件不能当入口，直接跳过
        if syms.is_empty() {
            continue;
        }
        opened += 1;
        match index.get(&module) {
            Some(&i) => {
                groups[i].symbols = syms;
                groups[i].by_file = true;
            }
            None => {
                index.insert(module.clone(), groups.len());
                groups.push(SearchFile {
                    module,
                    path,
                    by_file: true,
                    symbols: syms,
                });
            }
        }
    }

    // 只命中函数名的组排在前面，整份带出来的排后面
    groups.sort_by_key(|g| g.by_file);

    let mut out: Vec<SearchFile> = Vec::new();
    let mut total = 0usize;
    for mut g in groups {
        if total >= limit {
            break;
        }
        if total + g.symbols.len() > limit {
            g.symbols.truncate(limit - total);
        }
        total += g.symbols.len();
        out.push(g);
    }

    Ok(out)
}

/// Entry points of a project as `(symbol_id, module, qualname, confidence)`.
pub fn entry_points(
    conn: &Connection,
    project_id: i64,
) -> rusqlite::Result<Vec<(i64, String, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, module, qualname, COALESCE(entry_confidence, '')
           FROM symbol
          WHERE project_id = ?1 AND is_entry = 1
          ORDER BY module, qualname",
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    rows.collect()
}

/// Resolve `[module:]qualname` to a symbol id.
pub fn find(conn: &Connection, project_id: i64, needle: &str) -> rusqlite::Result<Vec<i64>> {
    let (module, qualname) = match needle.split_once(':') {
        Some((m, q)) => (Some(m), q),
        None => (None, needle),
    };
    let mut stmt = conn.prepare(
        "SELECT id FROM symbol
          WHERE project_id = ?1 AND qualname = ?2 AND (?3 IS NULL OR module = ?3)
          ORDER BY id",
    )?;
    let rows = stmt.query_map(params![project_id, qualname, module], |row| {
        row.get::<_, i64>(0)
    })?;
    rows.collect()
}

/// All notes attached to a node, newest first.
pub fn notes(conn: &Connection, symbol_id: i64) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT text FROM note WHERE symbol_id = ?1 ORDER BY updated_at DESC, id DESC",
    )?;
    let rows = stmt.query_map(params![symbol_id], |row| row.get::<_, String>(0))?;
    rows.collect()
}

/// One rendered source line of a card, with the call it anchors.
#[derive(Debug, Clone, Serialize)]
pub struct CodeLine {
    pub kind: String,
    pub text: String,
    pub indent: i64,
    pub ln: i64,
    pub call_target: Option<String>,
    pub confidence: Option<String>,
    /// Resolved callee of the anchor, so the UI can jump to it
    pub dst_symbol: Option<i64>,
}

/// The card body of a node, in display order.
pub fn card_lines(conn: &Connection, symbol_id: i64) -> rusqlite::Result<Vec<CodeLine>> {
    let mut stmt = conn.prepare(
        "SELECT l.kind, l.text, l.indent, l.ln, c.target, c.confidence, c.dst_symbol
           FROM card_line l LEFT JOIN call_site c ON c.id = l.call_id
          WHERE l.symbol_id = ?1
          ORDER BY l.ord",
    )?;
    let rows = stmt.query_map(params![symbol_id], |row| {
        Ok(CodeLine {
            kind: row.get(0)?,
            text: row.get(1)?,
            indent: row.get(2)?,
            ln: row.get(3)?,
            call_target: row.get(4)?,
            confidence: row.get(5)?,
            dst_symbol: row.get(6)?,
        })
    })?;
    rows.collect()
}
