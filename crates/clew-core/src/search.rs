//! 一套搜索。符号、文件、模块走同一份打分，前端只负责按 kind 分队渲染。
//!
//! 三层命中（全词相等、前缀、子序列）各自给基础分，再乘字段权重。命中位置
//! 一并返回，前端不再自己匹配一遍，两栏、CLI、将来的模型检索看到的是同一份判断。

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
    path_l: String,
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
    /// 命中的字段，前端据此决定在哪一段上色
    pub field: &'static str,
    /// 命中字符在展示文本里的字节区间，成对出现
    pub spans: Vec<(u32, u32)>,
    pub score: u32,
}

pub struct Page {
    pub hits: Vec<Hit>,
    /// 被种类过滤挡下的命中数，按 kind 记。界面用它提示「另有 N 个方法命中」
    pub suppressed: Vec<(&'static str, usize)>,
}

/// 查询。ctx_module 是当前打开的模块，命中它加一点分。
pub struct Query<'a> {
    pub text: &'a str,
    pub kinds: Vec<Kind>,
    pub ctx_module: Option<&'a str>,
    pub limit: usize,
}

struct Match {
    tier: u32,
    spans: Vec<(u32, u32)>,
    gaps: u32,
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
            gaps: 0,
        });
    }
    // 单个字符的子序列等于「包含任意一个该字符」，太吵，不做
    if needle.chars().count() < 2 {
        return None;
    }
    let mut want = needle.chars();
    let mut current = want.next()?;
    let mut first: Option<usize> = None;
    let mut prev_end = 0usize;
    let mut gaps = 0u32;
    let mut marks: Vec<(u32, u32)> = Vec::with_capacity(needle.len());
    for (i, ch) in hay.char_indices() {
        if ch != current {
            continue;
        }
        if first.is_none() {
            first = Some(i);
        } else {
            gaps += (i - prev_end) as u32;
        }
        prev_end = i + ch.len_utf8();
        marks.push((i as u32, prev_end as u32));
        match want.next() {
            Some(next) => current = next,
            None => {
                let start = first.unwrap_or(0);
                let tier = if is_boundary(hay, start) { 500 } else { 300 };
                return Some(Match {
                    tier,
                    spans: marks,
                    gaps,
                });
            }
        }
    }
    None
}

/// 字段权重，按百分比算，避免浮点。
fn weight(field: &str) -> u32 {
    match field {
        "name" => 100,
        "qualname" => 80,
        "module" => 60,
        "path" => 50,
        _ => 50,
    }
}

fn best_of(fields: &[(&'static str, &str)], needle: &str) -> Option<(&'static str, Match)> {
    let mut best: Option<(&'static str, Match)> = None;
    let mut best_score = 0u32;
    for (name, hay) in fields {
        if hay.is_empty() {
            continue;
        }
        if let Some(m) = match_field(hay, needle) {
            let score = m.tier * weight(name) / 100;
            if score > best_score {
                best_score = score;
                best = Some((name, m));
            }
        }
    }
    best
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
            let path_l = path.to_lowercase();
            let mask = mask_of(&module_l) | mask_of(&path_l);
            modules.push(ModuleRef {
                module_l,
                path_l,
                mask,
                module,
                path,
            });
        }
    }

    let mut rows: Vec<Row> = Vec::new();
    // 文件与模块各一行，名字用文件名或模块最后一段
    for (idx, m) in modules.iter().enumerate() {
        let file_name = m
            .path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&m.path)
            .to_string();
        rows.push(Row {
            kind: Kind::File,
            module: idx as u32,
            qualname: String::new(),
            def_line: 0,
            card_id: String::new(),
            is_entry: false,
            name_l: file_name.to_lowercase(),
            qual_l: String::new(),
            mask: mask_of(&file_name.to_lowercase()),
        });
        let last = m.module.rsplit('.').next().unwrap_or(&m.module).to_string();
        rows.push(Row {
            kind: Kind::Mod,
            module: idx as u32,
            qualname: String::new(),
            def_line: 0,
            card_id: String::new(),
            is_entry: false,
            name_l: last.to_lowercase(),
            qual_l: String::new(),
            mask: mask_of(&last.to_lowercase()),
        });
    }

    {
        let mut stmt = conn.prepare(
            "SELECT module, qualname, kind, def_line, is_entry
               FROM symbol WHERE project_id = ?1 AND kind <> 'module'",
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
                _ => continue,
            };
            let name = qualname.rsplit('.').next().unwrap_or(&qualname).to_string();
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

/// 跑一次查询。命中全部分种类算完再按 kinds 分流，被挡下的数量才报得出来。
pub fn run(index: &Index, query: &Query) -> Page {
    let needle = query.text.trim().to_lowercase();
    let mut hits: Vec<Hit> = Vec::new();
    let mut suppressed: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    if needle.len() < 2 && needle.is_empty() {
        return Page {
            hits,
            suppressed: Vec::new(),
        };
    }

    let wanted = needle_mask(&needle);
    for row in &index.rows {
        let module = &index.modules[row.module as usize];
        if let Some(mask) = wanted {
            if (row.mask | module.mask) & mask != mask {
                continue;
            }
        }
        let fields: [(&'static str, &str); 4] = [
            ("name", &row.name_l),
            ("qualname", &row.qual_l),
            ("module", &module.module_l),
            ("path", &module.path_l),
        ];
        let Some((field, m)) = best_of(&fields, &needle) else {
            continue;
        };
        if !query.kinds.contains(&row.kind) {
            *suppressed.entry(row.kind.as_str()).or_insert(0) += 1;
            continue;
        }
        let mut score = m.tier * weight(field) / 100;
        if row.is_entry {
            score += 50;
        }
        if query.ctx_module == Some(module.module.as_str()) {
            score += 30;
        }
        let penalty = m.gaps.min(100);
        score = score.saturating_sub(penalty);
        hits.push(Hit {
            kind: row.kind,
            module: module.module.clone(),
            path: module.path.clone(),
            qualname: row.qualname.clone(),
            def_line: row.def_line,
            card_id: row.card_id.clone(),
            field,
            spans: m.spans,
            score,
        });
    }

    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.qualname.len().cmp(&b.qualname.len()))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.def_line.cmp(&b.def_line))
    });

    // 同一个模块既有文件行又有模块行时只留文件行。两者是同一样东西的两种看法，
    // 并排出现就是两行一模一样的条目。
    if query.kinds.contains(&Kind::File) {
        let files: std::collections::HashSet<String> = hits
            .iter()
            .filter(|h| h.kind == Kind::File)
            .map(|h| h.module.clone())
            .collect();
        hits.retain(|h| !(h.kind == Kind::Mod && files.contains(&h.module)));
    }

    hits.truncate(query.limit.max(1));
    Page {
        hits,
        suppressed: suppressed.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(name: &str, path: &str) -> ModuleRef {
        let module_l = name.to_lowercase();
        let path_l = path.to_lowercase();
        ModuleRef {
            mask: mask_of(&module_l) | mask_of(&path_l),
            module_l,
            path_l,
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
            modules: vec![module("pkg.session", "pkg/session.py")],
            rows: vec![
                row(Kind::Func, 0, "get_entry", 10),
                row(Kind::Func, 0, "get_entry_name", 20),
                row(Kind::Method, 0, "SessionStore.get_entry", 30),
                row(Kind::Func, 0, "gather_entries", 40),
            ],
        }
    }

    fn all() -> Vec<Kind> {
        vec![
            Kind::Func,
            Kind::Method,
            Kind::Class,
            Kind::File,
            Kind::Mod,
        ]
    }

    #[test]
    fn exact_beats_prefix_beats_subsequence() {
        let idx = sample();
        let page = run(
            &idx,
            &Query {
                text: "get_entry",
                kinds: all(),
                ctx_module: None,
                limit: 10,
            },
        );
        let names: Vec<&str> = page.hits.iter().map(|h| h.qualname.as_str()).collect();
        assert_eq!(names[0], "get_entry");
        assert_eq!(names[1], "SessionStore.get_entry");
        assert_eq!(names[2], "get_entry_name");
        assert!(page.hits[2].score < page.hits[0].score);
    }

    #[test]
    fn subsequence_matches_across_gaps() {
        let idx = sample();
        let page = run(
            &idx,
            &Query {
                text: "gente",
                kinds: all(),
                ctx_module: None,
                limit: 10,
            },
        );
        assert!(
            page.hits.iter().any(|h| h.qualname == "gather_entries"),
            "subsequence should reach gather_entries"
        );
    }

    #[test]
    fn filtering_by_kind_reports_what_was_dropped() {
        let idx = sample();
        let page = run(
            &idx,
            &Query {
                text: "get_entry",
                kinds: vec![Kind::Method],
                ctx_module: None,
                limit: 10,
            },
        );
        assert_eq!(page.hits.len(), 1);
        assert_eq!(page.hits[0].kind, Kind::Method);
        let funcs = page
            .suppressed
            .iter()
            .find(|(k, _)| *k == "func")
            .map(|(_, n)| *n)
            .unwrap_or(0);
        assert_eq!(funcs, 2, "two funcs matched but were filtered out");
    }

    #[test]
    fn file_row_hides_its_module_row() {
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
            module: 0,
            qualname: String::new(),
            def_line: 0,
            card_id: String::new(),
            is_entry: false,
            name_l: "session".to_string(),
            qual_l: String::new(),
            mask: mask_of("session"),
        });

        // 两种都勾：只留文件行
        let both = run(
            &idx,
            &Query {
                text: "session",
                kinds: vec![Kind::File, Kind::Mod],
                ctx_module: None,
                limit: 10,
            },
        );
        assert_eq!(both.hits.len(), 1);
        assert_eq!(both.hits[0].kind, Kind::File);

        // 只勾模块：模块行照常出现
        let only_mod = run(
            &idx,
            &Query {
                text: "session",
                kinds: vec![Kind::Mod],
                ctx_module: None,
                limit: 10,
            },
        );
        assert_eq!(only_mod.hits.len(), 1);
        assert_eq!(only_mod.hits[0].kind, Kind::Mod);
    }

    #[test]
    fn single_letter_never_goes_fuzzy() {
        // 单个字符只按子串算，不参与子序列，否则一个字母能把整个项目拉出来
        assert_eq!(match_field("abc", "b").map(|m| m.tier), Some(250));
        assert!(match_field("abc", "z").is_none());
        // 两个字符才允许跨字符命中
        assert!(match_field("abc", "ac").is_some());
    }
}
