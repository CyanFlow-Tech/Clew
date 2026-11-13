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
    // 定义条数走一次 GROUP BY，不给每个模块各跑一条子查询：五万个模块上
    // 逐条 count 是几十万次索引查找，光这一步就能让面板开不出来。
    let mut stmt = conn.prepare(
        "SELECT m.module, m.path, m.is_third_party, m.has_main_guard, COALESCE(c.n, 0)
           FROM module m
           LEFT JOIN (
                SELECT module, count(*) AS n FROM symbol
                 WHERE project_id = ?1 GROUP BY module
           ) c ON c.module = m.module
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
pub fn symbols_in(conn: &Connection, project_id: i64, module: &str) -> rusqlite::Result<Vec<SymbolRow>> {
    symbols_of(conn, project_id, module, true)
}

/// 全项目的入口定义，侧栏「入口函数」视图一次取完：通常几十条，比按文件
/// 一个一个问过去便宜得多。
pub fn entry_symbols(conn: &Connection, project_id: i64) -> rusqlite::Result<Vec<SymbolRow>> {
    let mut stmt = conn.prepare(
        "SELECT s.module, s.qualname, s.kind, s.def_line, s.entry_confidence
           FROM symbol s
          WHERE s.project_id = ?1 AND s.is_entry = 1
          ORDER BY s.module, s.def_line, s.qualname",
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        let module: String = row.get(0)?;
        let qualname: String = row.get(1)?;
        Ok(SymbolRow {
            card_id: format!("{module}:{qualname}"),
            module,
            qualname,
            kind: row.get(2)?,
            def_line: row.get(3)?,
            is_entry: true,
            entry_confidence: row.get(4)?,
            params: Vec::new(),
            fanin: 0,
        })
    })?;
    rows.collect()
}

/// 搜索时不需要参数表，少一次 per-symbol 的查询，不然大项目上每次按键都要
/// 对上万个符号各跑一条 SQL。
fn symbols_of(
    conn: &Connection,
    project_id: i64,
    module: &str,
    with_params: bool,
) -> rusqlite::Result<Vec<SymbolRow>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.qualname, s.kind, s.def_line, s.is_entry, s.entry_confidence,
                (SELECT count(*) FROM call_site c WHERE c.dst_symbol = s.id)
           FROM symbol s
          WHERE s.project_id = ?1 AND s.module = ?2
          ORDER BY s.def_line, s.qualname",
    )?;
    let mut out = Vec::new();
    for row in stmt.query_map(params![project_id, module], |row| {
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

/// One file with all of its definitions, as the sidebar's symbol tree shows it.
#[derive(Debug, Clone, Serialize)]
pub struct FileNode {
    pub module: String,
    pub path: String,
    pub is_third_party: bool,
    pub has_main_guard: bool,
    pub symbols: Vec<SymbolRow>,
}

/// The whole project as a tree: every file in path order, every definition in
/// source order inside it.
///
/// 一次查询取全部定义行，参数表和 fanin 都不带：树里不画它们，而按符号各跑
/// 一条子查询，几千个符号就是几千次往返，切视图会卡住。
pub fn tree(conn: &Connection, project_id: i64) -> rusqlite::Result<Vec<FileNode>> {
    let mut files_stmt = conn.prepare(
        "SELECT module, path, is_third_party, has_main_guard
           FROM module
          WHERE project_id = ?1
          ORDER BY path",
    )?;
    let mut files: Vec<FileNode> = files_stmt
        .query_map(params![project_id], |row| {
            let path: String = row.get(1)?;
            Ok(FileNode {
                module: row.get(0)?,
                path: path.trim_start_matches(r"\\?\").to_string(),
                is_third_party: row.get::<_, i64>(2)? != 0,
                has_main_guard: row.get::<_, i64>(3)? != 0,
                symbols: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut slot: HashMap<String, usize> = files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.module.clone(), i))
        .collect();

    let mut sym_stmt = conn.prepare(
        "SELECT module, qualname, kind, def_line, is_entry, entry_confidence
           FROM symbol
          WHERE project_id = ?1
          ORDER BY module, def_line, qualname",
    )?;
    let rows = sym_stmt.query_map(params![project_id], |row| {
        let module: String = row.get(0)?;
        Ok((
            module,
            SymbolRow {
                card_id: String::new(),
                module: String::new(),
                qualname: row.get(1)?,
                kind: row.get(2)?,
                def_line: row.get(3)?,
                is_entry: row.get::<_, i64>(4)? != 0,
                entry_confidence: row.get(5)?,
                params: Vec::new(),
                fanin: 0,
            },
        ))
    })?;
    for row in rows {
        let (module, mut sym) = row?;
        // 符号的 module 在库里一定有对应文件行，除非库被外改过；遇到就补一行
        let idx = match slot.get(&module) {
            Some(i) => *i,
            None => {
                files.push(FileNode {
                    module: module.clone(),
                    path: module.replace('.', "/") + ".py",
                    is_third_party: false,
                    has_main_guard: false,
                    symbols: Vec::new(),
                });
                slot.insert(module.clone(), files.len() - 1);
                files.len() - 1
            }
        };
        sym.card_id = format!("{module}:{}", sym.qualname);
        sym.module = module;
        files[idx].symbols.push(sym);
    }

    Ok(files)
}

/// One row of the sidebar search: a definition, the file it lives in, and why
/// it ranked where it did.
#[derive(Debug, Clone, Serialize)]
pub struct SymbolHit {
    pub card_id: String,
    pub module: String,
    pub path: String,
    pub qualname: String,
    pub kind: String,
    pub def_line: i64,
    pub is_entry: bool,
    pub score: u32,
}

/// One page of the sidebar search, plus how many rows matched in total.
#[derive(Debug, Clone, Serialize)]
pub struct SymbolPage {
    /// Matches found, capped at `SCAN_CAP`; compare with `truncated`.
    pub total: usize,
    /// True when even the scan stopped early, so `total` is a lower bound.
    pub truncated: bool,
    pub hits: Vec<SymbolHit>,
}

/// 一次扫描最多收这么多行：子串匹配一定走全表，上限是给最坏情况兜底。
const SCAN_CAP: usize = 20_000;

/// 路径输入框的两种写法。
///
/// - 直接输入（`agent`、`reference.dotsandboxes`）：匹配任意一级目录或文件名，
///   不跨级 —— `age` 命中 `agent/`，但 `sts/age` 不会命中 `tests/agent/`。
/// - 以 `/` 开头（`/tests/agent`）：相对项目根的路径前缀，只认这条路径下面。
struct PathFilter {
    prefix: bool,
    /// 用户写的原串（点没动过），用来兜住 `config.py` 这种文件名
    raw: String,
    /// 点当分隔符处理后的串，用来兜住 `reference.dotsandboxes` 这种包路径
    query: String,
}

impl PathFilter {
    fn parse(q: &str) -> Option<PathFilter> {
        let prefix = q.starts_with('/');
        let body = q.trim_start_matches('/').trim();
        if body.is_empty() {
            return None;
        }
        Some(PathFilter {
            prefix,
            raw: body.to_string(),
            query: body.replace('.', "/"),
        })
    }

    /// 粗筛只能用“连续子串”匹配，多段查询的整串在路径里并不连续，
    /// 所以只拿最长的那一段去筛，剩下的交给逐段精筛。
    fn coarse(&self) -> &str {
        Self::longest_seg(&self.query)
    }

    fn coarse_alt(&self) -> &str {
        Self::longest_seg(&self.raw)
    }

    fn longest_seg(s: &str) -> &str {
        s.split('/')
            .filter(|x| !x.is_empty())
            .max_by_key(|x| x.len())
            .unwrap_or(s)
    }

    fn matches(&self, rel: &str) -> bool {
        let segs: Vec<&str> = rel.split('/').filter(|s| !s.is_empty()).collect();
        // 原串兜住 `config.py`（点属于文件名），转成分隔符的串兜住 `pkg.mod`
        Self::seg_hit(&segs, &self.query, self.prefix)
            || Self::seg_hit(&segs, &self.raw, self.prefix)
    }

    /// 逐级匹配：查询按 `/` 切段，每段只要是对应路径段的**子串**就算命中，
    /// 段与段要落在相邻的路径层级上。`/` 开头的查询必须从第一段开始。
    fn seg_hit(rel_segs: &[&str], q: &str, from_root: bool) -> bool {
        let qsegs: Vec<&str> = q.split('/').filter(|s| !s.is_empty()).collect();
        if qsegs.is_empty() || qsegs.len() > rel_segs.len() {
            return false;
        }
        let last = rel_segs.len() - qsegs.len();
        let starts: Vec<usize> = if from_root {
            vec![0]
        } else {
            (0..=last).collect()
        };
        starts.into_iter().any(|i| {
            qsegs
                .iter()
                .enumerate()
                .all(|(j, qs)| rel_segs[i + j].contains(qs))
        })
    }
}

/// Two independent filters over the project: one against the file path, one
/// against the definition name. Both are plain substring matches, case
/// insensitive, and they combine with AND.
///
/// 排序只做一件事：函数名越接近“整个名字”越靠前，路径命中排在后面。
/// 分页在调用方做，这里一次给出整段排序结果。
pub fn search_symbols(
    conn: &Connection,
    project_id: i64,
    root: &Path,
    path_q: &str,
    name_q: &str,
) -> rusqlite::Result<SymbolPage> {
    let path_q = path_q.trim().to_lowercase();
    let name_q = name_q.trim().to_lowercase();
    let path_filter = PathFilter::parse(&path_q);

    let mut sql = String::from(
        "SELECT s.module, s.qualname, s.kind, s.def_line, s.is_entry, m.path
           FROM symbol s
           JOIN module m ON m.project_id = s.project_id AND m.module = s.module
          WHERE s.project_id = :pid",
    );
    if path_filter.is_some() {
        // 粗筛在 SQL 里做，路径先归一成斜杠；精确到「哪一级」的判定在下面用相对路径做
        sql.push_str(
            " AND (lower(replace(m.path, '\\', '/')) LIKE :path
                   OR lower(replace(m.path, '\\', '/')) LIKE :path_alt)",
        );
    }
    if !name_q.is_empty() {
        // 模块级定义在库里叫 <module>，界面上叫 __main__，搜索要按界面上的名字认
        if "__main__".contains(&name_q) {
            sql.push_str(" AND (lower(s.qualname) LIKE :name OR s.qualname = '<module>')");
        } else {
            sql.push_str(" AND lower(s.qualname) LIKE :name");
        }
    }
    sql.push_str(&format!(" LIMIT {SCAN_CAP}"));

    let mut stmt = conn.prepare(&sql)?;
    let like_path = path_filter
        .as_ref()
        .map(|pf| format!("%{}%", pf.coarse()))
        .unwrap_or_else(|| "%%".to_string());
    let like_alt = path_filter
        .as_ref()
        .map(|pf| format!("%{}%", pf.coarse_alt()))
        .unwrap_or_else(|| "%%".to_string());
    let like_name = format!("%{name_q}%");
    let mut args: Vec<(&str, &dyn rusqlite::ToSql)> = vec![(":pid", &project_id)];
    if path_filter.is_some() {
        args.push((":path", &like_path));
        args.push((":path_alt", &like_alt));
    }
    if !name_q.is_empty() {
        args.push((":name", &like_name));
    }

    let mut hits: Vec<SymbolHit> = Vec::new();
    let rows = stmt.query_map(args.as_slice(), |row| {
        let module: String = row.get(0)?;
        let qualname: String = row.get(1)?;
        let path: String = row.get(5)?;
        Ok(SymbolHit {
            card_id: format!("{module}:{qualname}"),
            module,
            path: path.trim_start_matches(r"\\?\").to_string(),
            qualname,
            kind: row.get(2)?,
            def_line: row.get(3)?,
            is_entry: row.get::<_, i64>(4)? != 0,
            score: 0,
        })
    })?;
    for row in rows {
        let mut hit = row?;
        // 粗筛过了还得按相对路径判一次：直接输入只认单个层级，/ 开头才认前缀
        let rel = rel_path(root, &hit.path).to_lowercase();
        if let Some(pf) = path_filter.as_ref() {
            if !pf.matches(&rel) {
                continue;
            }
        }
        hit.score = symbol_score(&hit, &rel, &path_q, &name_q);
        hits.push(hit);
    }

    let truncated = hits.len() >= SCAN_CAP;
    // 分数相同也要有确定的次序，否则翻页会看到重复或漏行
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.path.to_lowercase().cmp(&b.path.to_lowercase()))
            .then_with(|| a.def_line.cmp(&b.def_line))
            .then_with(|| a.qualname.cmp(&b.qualname))
    });

    Ok(SymbolPage {
        total: hits.len(),
        truncated,
        hits,
    })
}

/// 名字命中永远压过只命中路径；名字里越靠前、越接近整词的排得越高。
fn symbol_score(hit: &SymbolHit, rel_path: &str, path_q: &str, name_q: &str) -> u32 {
    let qual = if hit.qualname == "<module>" {
        "__main__".to_string()
    } else {
        hit.qualname.to_lowercase()
    };
    let last = qual.rsplit('.').next().unwrap_or(&qual);
    let mut score = 0u32;

    if !name_q.is_empty() {
        if last == name_q {
            score += 1000;
        } else if last.starts_with(name_q) {
            score += 800;
        } else if last.contains(name_q) {
            score += 600;
        } else if qual.contains(name_q) {
            // 命中在外层（类名、包名）上，比直接命中函数名低
            score += 400;
        }
        // 子串越靠前越相关
        if let Some(pos) = qual.find(name_q) {
            score = score.saturating_sub(pos.min(50) as u32);
        }
    }

    if !path_q.is_empty() {
        if let Some(pos) = rel_path.find(path_q) {
            // 路径过滤是主条件，命中位置只用来在同一档里排序
            score += 100u32.saturating_sub(pos.min(100) as u32);
        }
    }

    if hit.is_entry {
        score += 20;
    }
    score
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
        let syms = symbols_of(conn, project_id, &module, false)?;
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
