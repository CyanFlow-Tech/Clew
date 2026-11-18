#[allow(dead_code)]
mod build;
mod query;
#[allow(dead_code)]
mod score;

use query::{ModuleRef, Row};

pub use build::build_index;
pub use score::run;
pub use query::{Kind, Query, Scope};


use serde::Serialize;

#[allow(dead_code)]
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

pub(crate) fn browse(index: &Index, scope: Scope) -> Page {
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
    use super::score::mask_of;
    use super::score::match_field;
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

