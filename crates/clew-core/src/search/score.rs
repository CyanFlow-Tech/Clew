
use super::query::{Query, Row};
use super::{browse, Hit, Index, Page};



pub fn slot(c: char) -> Option<u32> {
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


pub fn mask_of(s: &str) -> u64 {
    let mut m = 0u64;
    for c in s.chars() {
        if let Some(b) = slot(c) {
            m |= 1 << b;
        }
    }
    m
}

pub fn needle_mask(needle: &str) -> Option<u64> {
    let mut m = 0u64;
    for c in needle.chars() {
        m |= 1 << slot(c)?;
    }
    Some(m)
}


pub(crate) struct Match {
    pub(crate) tier: u32,
    pub(crate) spans: Vec<(u32, u32)>,
}

pub(crate) fn is_boundary(s: &str, at: usize) -> bool {
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
pub(crate) fn match_field(hay: &str, needle: &str) -> Option<Match> {
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

pub(crate) fn match_symbol(row: &Row, needle: &str) -> Option<(u32, &'static str, Vec<(u32, u32)>)> {
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
pub(crate) fn at_segment_start(text: &str, at: usize) -> bool {
    at == 0 || text.as_bytes()[at - 1] == b'.'
}

pub(crate) fn module_match(module_l: &str, needle: &str) -> Option<(u32, Vec<(u32, u32)>)> {
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

