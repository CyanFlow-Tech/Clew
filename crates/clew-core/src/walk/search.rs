use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use crate::card::CardId;
use super::files::rel_path;

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
pub(crate) const SCAN_CAP: usize = 20_000;

/// 路径输入框的两种写法。
///
/// - 直接输入（`agent`、`reference.dotsandboxes`）：匹配任意一级目录或文件名，
///   不跨级 —— `age` 命中 `agent/`，但 `sts/age` 不会命中 `tests/agent/`。
/// - 以 `/` 开头（`/tests/agent`）：相对项目根的路径前缀，只认这条路径下面。
#[derive(Debug, Clone, Serialize)]
pub struct PathFilter {
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
            card_id: CardId::new(&module, &qualname).into_string(),
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
pub fn symbol_score(hit: &SymbolHit, rel_path: &str, path_q: &str, name_q: &str) -> u32 {
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
