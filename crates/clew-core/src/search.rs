//! 搜索只有两级：前缀与函数名。
//!
//! 输入用冒号分隔，左边是模块前缀（字面匹配，不做模糊），右边是函数名（全词、
//! 前缀、词界、子序列四级）。没有冒号时整串同时试两边，函数名命中优先。结果
//! 一律是定义，格式固定为 `模块:qualname`，文件与模块本身不出现在结果里。

use rusqlite::{params, Connection};
use serde::Serialize;

/// 搜索结果的种类。前三个是符号，可作根，后两个是路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Func,
    Method,
    Class,
    File,
    Mod,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Func => "func",
            Kind::Method => "method",
            Kind::Class => "class",
            Kind::File => "file",
            Kind::Mod => "mod",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "func" | "function" => Some(Kind::Func),
            "method" => Some(Kind::Method),
            "class" => Some(Kind::Class),
            "file" => Some(Kind::File),
            "mod" | "module" => Some(Kind::Mod),
            _ => None,
        }
    }

    pub fn is_symbol(self) -> bool {
        matches!(self, Kind::Func | Kind::Method | Kind::Class)
    }
}

/// 一个模块的文件信息。行里只存下标，16 万行摊下来省几十兆。
pub struct ModuleRef {
    pub module: String,
    pub path: String,
    module_l: String,
    /// 出现过的字符集合，给查询做快速排除
    mask: u64,
}

pub struct Row {
    pub kind: Kind,
    pub module: u32,
    pub qualname: String,
    pub def_line: u32,
    pub card_id: String,
    pub is_entry: bool,
    /// 名字最后一段，小写
    name_l: String,
    /// 完整 qualname，小写
    qual_l: String,
    mask: u64,
}

/// 一个字符在掩码里的位置。字母、数字、下划线、点、斜杠之外一律返回 None，
/// 调用方据此决定这次查询能不能用掩码提前排除。
fn slot(c: char) -> Option<u32> {
    match c {
        'a'..='z' => Some(c as u32 - 'a' as u32),
        '0'..='9' => Some(26 + c as u32 - '0' as u32),
        '_' => Some(36),
        '.' => Some(37),
        '/' => Some(38),
        '-' => Some(39),
        _ => None,
    }
}

fn mask_of(s: &str) -> u64 {
    let mut m = 0u64;
    for c in s.chars() {
        if let Some(b) = slot(c) {
            m |= 1 << b;
        }
    }
    m
}

fn needle_mask(needle: &str) -> Option<u64> {
    let mut m = 0u64;
    for c in needle.chars() {
        m |= 1 << slot(c)?;
    }
    Some(m)
}

pub struct Index {
    pub modules: Vec<ModuleRef>,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub kind: Kind,
    pub module: String,
    pub path: String,
    pub qualname: String,
    pub def_line: u32,
    pub card_id: String,
    /// 扫描器认定的入口。左栏只收这一类，外加用户自己标记的。
    pub is_entry: bool,
    /// 命中的字段，前端据此决定在哪一段上色
    pub field: &'static str,
    /// 命中字符在展示文本里的字节区间，成对出现
    pub spans: Vec<(u32, u32)>,
    pub score: u32,
}

#[derive(serde::Serialize)]
pub struct Page {
    pub hits: Vec<Hit>,
}

/// 结果范围。自动识别只收扫描器认的入口与用户标记的，全部函数不限。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Entry,
    All,
}

impl Scope {
    pub fn parse(s: &str) -> Scope {
        if s == "all" {
            Scope::All
        } else {
            Scope::Entry
        }
    }
}

/// 两级查询。左边是模块，右边是函数名。
///
/// 只写左边就是模块查询，列出这些模块下的定义。写了冒号再加右边，才在名字上收窄。
pub struct Query {
    pub prefix: String,
    pub name: String,
    pub scope: Scope,
    pub ctx_module: Option<String>,
}

impl Query {
    pub fn parse(text: &str, scope: Scope, ctx_module: Option<String>) -> Query {
        let lower = text.trim().to_lowercase();
        match lower.find(':') {
            Some(at) => Query {
                prefix: lower[..at].trim().to_string(),
                name: lower[at + 1..].trim().to_string(),
                scope,
                ctx_module,
            },
            // 没有冒号就只查模块，末尾补一个冒号与不补完全等价
            None => Query {
                prefix: lower.trim_end_matches(':').trim().to_string(),
                name: String::new(),
                scope,
                ctx_module,
            },
        }
    }

    fn is_empty(&self) -> bool {
        self.prefix.is_empty() && self.name.is_empty()
    }
}

struct Match {
    tier: u32,
    spans: Vec<(u32, u32)>,
}

fn is_boundary(s: &str, at: usize) -> bool {
    if at == 0 {
        return true;
    }
    match s[..at].chars().next_back() {
        Some(c) => !c.is_alphanumeric() && c != '_',
        None => true,
    }
}

/// 在一段小写文本里找 needle。needle 也是小写。
///
/// 先试子串（全词、前缀、词界、任意位置），不中再退到子序列。子序列只认
/// 字符顺序，跨度越大扣分越多。
fn match_field(hay: &str, needle: &str) -> Option<Match> {
    if needle.is_empty() {
        return None;
    }
    if let Some(pos) = hay.find(needle) {
        let tier = if hay.len() == needle.len() {
            1000
        } else if pos == 0 {
            800
        } else if is_boundary(hay, pos) {
            700
        } else {
            250
        };
        let end = (pos + needle.len()) as u32;
        return Some(Match {
            tier,
            spans: vec![(pos as u32, end)],
        });
    }
    // 单个字符的子序列等于「包含任意一个该字符」，太吵，不做
    if needle.chars().count() < 2 {
        return None;
    }
    let mut want = needle.chars();
    let mut current = want.next()?;
    let mut first: Option<usize> = None;
    let mut marks: Vec<(u32, u32)> = Vec::with_capacity(needle.len());
    for (i, ch) in hay.char_indices() {
        if ch != current {
            continue;
        }
        if first.is_none() {
            first = Some(i);
        }
        marks.push((i as u32, (i + ch.len_utf8()) as u32));
        match want.next() {
            Some(next) => current = next,
            None => {
                let start = first.unwrap_or(0);
                let tier = if is_boundary(hay, start) { 500 } else { 300 };
                return Some(Match {
                    tier,
                    spans: marks,
                });
            }
        }
    }
    None
}

/// 从库里读出全部可搜的行。符号按 kind 分，另外补上文件与模块两行。
pub fn build_index(conn: &Connection, project_id: i64) -> rusqlite::Result<Index> {
    let mut modules: Vec<ModuleRef> = Vec::new();
    let mut by_name: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT module, path FROM module WHERE project_id = ?1 ORDER BY module",
        )?;
        let rows = stmt.query_map(params![project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (module, path) = row?;
            by_name.insert(module.clone(), modules.len() as u32);
            let module_l = module.to_lowercase();
            let mask = mask_of(&module_l);
            modules.push(ModuleRef {
                module_l,
                mask,
                module,
                path,
            });
        }
    }

    let mut rows: Vec<Row> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT module, qualname, kind, def_line, is_entry
               FROM symbol WHERE project_id = ?1",
        )?;
        let found = stmt.query_map(params![project_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?;
        for row in found {
            let (module, qualname, kind, def_line, is_entry) = row?;
            let Some(&idx) = by_name.get(&module) else {
                continue;
            };
            let kind = match kind.as_str() {
                "function" => Kind::Func,
                "method" => Kind::Method,
                "class" => Kind::Class,
                // 模块入口是 <module> 卡片，只在自动识别里出现，不参与文字搜索
                "module" => Kind::Mod,
                _ => continue,
            };
            let name = if qualname == "<module>" {
                module.rsplit('.').next().unwrap_or(&module).to_string()
            } else {
                qualname.rsplit('.').next().unwrap_or(&qualname).to_string()
            };
            let name_l = name.to_lowercase();
            let qual_l = qualname.to_lowercase();
            let mask = mask_of(&name_l) | mask_of(&qual_l);
            rows.push(Row {
                kind,
                module: idx,
                card_id: format!("{module}:{qualname}"),
                name_l,
                qual_l,
                mask,
                qualname,
                def_line: def_line.max(0) as u32,
                is_entry: is_entry != 0,
            });
        }
    }

    Ok(Index { modules, rows })
}

/// 一行符号在名字这一侧的最好命中。qualname 也算，`Runner.main` 这种能直接搜。
fn match_symbol(row: &Row, needle: &str) -> Option<(u32, &'static str, Vec<(u32, u32)>)> {
    let by_name = match_field(&row.name_l, needle);
    let by_qual = match_field(&row.qual_l, needle);
    match (by_name, by_qual) {
        (Some(a), Some(b)) => {
            // 整串更长，同级别时让末段命中排前面
            let sa = a.tier;
            let sb = b.tier * 90 / 100;
            if sa >= sb {
                Some((sa, "name", a.spans))
            } else {
                Some((sb, "qualname", b.spans))
            }
        }
        (Some(a), None) => Some((a.tier, "name", a.spans)),
        (None, Some(b)) => Some((b.tier * 90 / 100, "qualname", b.spans)),
        (None, None) => None,
    }
}

/// 模块侧的命中阶梯。先按点分段找子串，找不到再退到子序列，
/// 这样 `cli` 能命中 `hermes_cli`，`hercli` 也能命中 `hermes_cli`。
///
/// 900 段全等，800 段开头，700 段内包含，600 段首起的子序列，
/// 500 跨段包含，400 别处的子序列。层级越具体越靠前。
fn at_segment_start(text: &str, at: usize) -> bool {
    at == 0 || text.as_bytes()[at - 1] == b'.'
}

fn module_match(module_l: &str, needle: &str) -> Option<(u32, Vec<(u32, u32)>)> {
    if needle.is_empty() {
        return None;
    }

    let mut best: Option<(u32, Vec<(u32, u32)>)> = None;
    let mut offset = 0usize;
    for seg in module_l.split('.') {
        let start = offset;
        offset += seg.len() + 1;
        if let Some(pos) = seg.find(needle) {
            let tier = if pos == 0 && seg.len() == needle.len() {
                900
            } else if pos == 0 {
                800
            } else {
                700
            };
            let at = (start + pos) as u32;
            let candidate = (tier, vec![(at, at + needle.len() as u32)]);
            let better = match best {
                Some((t, _)) => tier > t,
                None => true,
            };
            if better {
                best = Some(candidate);
            }
        }
    }
    if best.is_some() {
        return best;
    }

    // 跨段的字面命中，比如 `cli.cur` 之于 `cli.curator`
    if let Some(pos) = module_l.find(needle) {
        let at = pos as u32;
        return Some((500, vec![(at, at + needle.len() as u32)]));
    }

    // 子序列。单个字符不做，等于「含这个字母」，太吵
    if needle.chars().count() < 2 {
        return None;
    }
    let mut want = needle.chars();
    let mut current = want.next()?;
    let mut spans: Vec<(u32, u32)> = Vec::new();
    let mut first: Option<usize> = None;
    for (i, ch) in module_l.char_indices() {
        if ch != current {
            continue;
        }
        if first.is_none() {
            first = Some(i);
        }
        spans.push((i as u32, (i + ch.len_utf8()) as u32));
        match want.next() {
            Some(next) => current = next,
            None => {
                let start = first.unwrap_or(0);
                let tier = if at_segment_start(module_l, start) { 600 } else { 400 };
                return Some((tier, spans));
            }
        }
    }
    None
}

/// 跑一次查询。左边是模块，右边是函数名。
///
/// 只写左边就列出这些模块下的定义，按模块命中的三级排序。写了右边就用函数名
/// 定强弱，模块命中只作为门槛与少量加分。范围（自动识别 / 全部函数）只管空查询。
pub fn run(index: &Index, query: &Query) -> Page {
    if query.is_empty() || (query.prefix.is_empty() && query.name.is_empty()) {
        return browse(index, query.scope);
    }
    let mut hits: Vec<Hit> = Vec::new();
    let mask = {
        let joined = format!("{}{}", query.prefix, query.name);
        needle_mask(&joined)
    };

    for row in &index.rows {
        if !row.kind.is_symbol() {
            continue;
        }
        let module = &index.modules[row.module as usize];
        if let Some(m) = mask {
            if (row.mask | module.mask) & m != m {
                continue;
            }
        }

        let mod_hit = if query.prefix.is_empty() {
            None
        } else {
            match module_match(&module.module_l, &query.prefix) {
                Some(found) => Some(found),
                None => continue,
            }
        };

        let (mut score, field, spans) = if query.name.is_empty() {
            let (tier, spans) = mod_hit.clone().expect("空函数名时模块命中必然存在");
            (tier, "module", spans)
        } else {
            match match_symbol(row, &query.name) {
                Some((tier, field, spans)) => {
                    let bonus = mod_hit.map(|(t, _)| t / 4).unwrap_or(0);
                    (tier + bonus, field, spans)
                }
                None => continue,
            }
        };
        if row.is_entry {
            score += 50;
        }
        if query.ctx_module.as_deref() == Some(module.module.as_str()) {
            score += 30;
        }
        hits.push(Hit {
            kind: row.kind,
            module: module.module.clone(),
            path: module.path.clone(),
            qualname: row.qualname.clone(),
            def_line: row.def_line,
            card_id: row.card_id.clone(),
            is_entry: row.is_entry,
            field,
            spans,
            score,
        });
    }

    // 同分时先按模块名，再按定义名字典序，这条规则固定
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.module.to_lowercase().cmp(&b.module.to_lowercase()))
            .then_with(|| a.qualname.to_lowercase().cmp(&b.qualname.to_lowercase()))
            .then_with(|| a.def_line.cmp(&b.def_line))
    });
    Page { hits }
}

/// 空查询时的列表。范围决定收不收非入口的定义，排序一律模块名在前。
pub fn browse(index: &Index, scope: Scope) -> Page {
    let mut hits: Vec<Hit> = Vec::new();
    for row in &index.rows {
        let want = match scope {
            // 自动识别：扫描器认的入口，函数与模块入口都收
            Scope::Entry => row.is_entry && (row.kind.is_symbol() || row.kind == Kind::Mod),
            // 全部函数：只要定义，模块入口不算函数
            Scope::All => row.kind.is_symbol(),
        };
        if !want {
            continue;
        }
        let module = &index.modules[row.module as usize];
        hits.push(Hit {
            kind: row.kind,
            module: module.module.clone(),
            path: module.path.clone(),
            qualname: row.qualname.clone(),
            def_line: row.def_line,
            card_id: row.card_id.clone(),
            is_entry: row.is_entry,
            field: "module",
            spans: Vec::new(),
            score: 0,
        });
    }
    hits.sort_by(|a, b| {
        a.module
            .to_lowercase()
            .cmp(&b.module.to_lowercase())
            .then_with(|| a.qualname.to_lowercase().cmp(&b.qualname.to_lowercase()))
            .then_with(|| a.def_line.cmp(&b.def_line))
    });
    Page { hits }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(name: &str, path: &str) -> ModuleRef {
        let module_l = name.to_lowercase();
        ModuleRef {
            mask: mask_of(&module_l),
            module_l,
            module: name.to_string(),
            path: path.to_string(),
        }
    }

    fn row(kind: Kind, module: u32, qualname: &str, def_line: u32) -> Row {
        let name = qualname.rsplit('.').next().unwrap_or(qualname).to_string();
        let name_l = name.to_lowercase();
        let qual_l = qualname.to_lowercase();
        Row {
            kind,
            module,
            card_id: if qualname.is_empty() {
                String::new()
            } else {
                format!("m{module}:{qualname}")
            },
            name_l: name_l.clone(),
            qual_l,
            mask: mask_of(&name_l),
            qualname: qualname.to_string(),
            def_line,
            is_entry: false,
        }
    }

    fn sample() -> Index {
        Index {
            modules: vec![
                module("pkg.session", "pkg/session.py"),
                module("pkg.store", "pkg/store.py"),
                module("hermes_cli.main", "hermes_cli/main.py"),
            ],
            rows: vec![
                row(Kind::Func, 0, "get_entry", 10),
                row(Kind::Func, 0, "get_entry_name", 20),
                row(Kind::Method, 0, "SessionStore.get_entry", 30),
                row(Kind::Func, 0, "gather_entries", 40),
                row(Kind::Func, 1, "delete", 50),
                row(Kind::Func, 1, "delete_all", 60),
                row(Kind::Func, 2, "bootstrap", 70),
            ],
        }
    }

    fn query(text: &str) -> Query {
        Query::parse(text, Scope::All, None)
    }

    fn names(page: &Page) -> Vec<&str> {
        page.hits.iter().map(|h| h.qualname.as_str()).collect()
    }

    #[test]
    fn exact_beats_prefix_beats_subsequence() {
        let page = run(&sample(), &query(":get_entry"));
        let got = names(&page);
        assert_eq!(got[0], "get_entry");
        assert_eq!(got[1], "SessionStore.get_entry");
        assert_eq!(got[2], "get_entry_name");
        assert!(page.hits[2].score < page.hits[0].score);
    }

    #[test]
    fn subsequence_matches_across_gaps() {
        let page = run(&sample(), &query(":gente"));
        assert!(page.hits.iter().any(|h| h.qualname == "gather_entries"));
    }

    #[test]
    fn prefix_alone_lists_every_definition_lexicographically() {
        let page = run(&sample(), &query("pkg.store:"));
        let got = names(&page);
        assert_eq!(got, vec!["delete", "delete_all"]);
        // 同分按字典序，这条规则固定
        assert_eq!(page.hits[0].score, page.hits[1].score);
    }

    #[test]
    fn prefix_and_name_narrow_together() {
        let page = run(&sample(), &query("pkg.store:delete_all"));
        assert_eq!(names(&page), vec!["delete_all"]);
    }

    #[test]
    fn module_query_needs_a_real_substring() {
        // 跨段的拼接不算命中，hermes.cur 不在 hermes_cli.main 里
        let page = run(&sample(), &query("hermes.cur"));
        assert!(page.hits.is_empty());
    }

    #[test]
    fn plain_text_searches_modules_by_segment() {
        let mut idx = sample();
        idx.modules.push(module("cli", "cli.py"));
        idx.modules.push(module("cli.main", "cli/main.py"));
        idx.rows.push(row(Kind::Func, 3, "run", 1));
        idx.rows.push(row(Kind::Func, 4, "boot", 2));

        let page = run(&idx, &query("cli"));
        let modules: Vec<&str> = page.hits.iter().map(|h| h.module.as_str()).collect();
        // 段全等 > 段开头 > 段内包含
        assert_eq!(modules[0], "cli");
        assert_eq!(modules[1], "cli.main");
        assert!(
            modules.iter().any(|m| *m == "hermes_cli.main"),
            "段内包含也要命中，hermes_cli 里有 cli 这一段"
        );
        // 末尾补冒号与不补完全等价
        assert_eq!(run(&idx, &query("cli:")).hits.len(), page.hits.len());
    }

    #[test]
    fn module_query_accepts_subsequences() {
        // hercli 命中 hermes_cli，段首起的子序列排在不那么规整的上面
        let page = run(&sample(), &query("hercli"));
        assert!(
            page.hits.iter().any(|h| h.module == "hermes_cli.main"),
            "hermes_cli 该被 hercli 命中"
        );
    }

    #[test]
    fn typed_query_ignores_scope() {
        // 打字时范围不参与过滤，两种范围给同一份结果
        let entry = Query::parse(":delete", Scope::Entry, None);
        let all = Query::parse(":delete", Scope::All, None);
        let a = run(&sample(), &entry);
        let b = run(&sample(), &all);
        assert_eq!(a.hits.len(), b.hits.len());
        assert!(a.hits.iter().any(|h| !h.is_entry));
    }

    #[test]
    fn results_are_definitions_only() {
        let mut idx = sample();
        idx.rows.push(Row {
            kind: Kind::File,
            module: 0,
            qualname: String::new(),
            def_line: 0,
            card_id: String::new(),
            is_entry: false,
            name_l: "session.py".to_string(),
            qual_l: String::new(),
            mask: mask_of("session.py"),
        });
        idx.rows.push(Row {
            kind: Kind::Mod,
            module: 2,
            qualname: String::new(),
            def_line: 0,
            card_id: "hermes_cli.main:<module>".to_string(),
            is_entry: true,
            name_l: "main".to_string(),
            qual_l: String::new(),
            mask: mask_of("main"),
        });
        let page = run(&idx, &query("main"));
        assert!(
            page.hits
                .iter()
                .all(|h| h.kind == Kind::Func || h.kind == Kind::Method || h.kind == Kind::Class),
            "文件与模块不该出现在结果里"
        );
    }

    #[test]
    fn empty_query_browses_instead_of_returning_nothing() {
        // 空查询按模块名字典序列出，与搜索结果的格式一致
        let page = run(&sample(), &query(""));
        assert!(!page.hits.is_empty());
        let modules: Vec<&str> = page.hits.iter().map(|h| h.module.as_str()).collect();
        let mut sorted = modules.clone();
        sorted.sort();
        assert_eq!(modules, sorted);
        // 只有一个冒号没有任何约束，等同于空查询
        assert_eq!(run(&sample(), &query(":")).hits.len(), page.hits.len());
    }

    #[test]
    fn entry_scope_keeps_only_entries() {
        let mut idx = sample();
        idx.rows[0].is_entry = true;
        let entry_page = run(
            &idx,
            &Query::parse("", Scope::Entry, None),
        );
        assert!(entry_page.hits.iter().all(|h| h.is_entry));
        assert_eq!(entry_page.hits.len(), 1);
        let all_page = run(&idx, &Query::parse("", Scope::All, None));
        assert!(all_page.hits.len() > entry_page.hits.len());
    }

    #[test]
    fn single_letter_never_goes_fuzzy() {
        // 单个字符只按子串算，不参与子序列，否则一个字母能把整个项目拉出来
        assert_eq!(match_field("abc", "b").map(|m| m.tier), Some(250));
        assert!(match_field("abc", "z").is_none());
        assert!(match_field("abc", "ac").is_some());
    }
}
