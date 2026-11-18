
use serde::Serialize;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
pub struct ModuleRef {
    pub module: String,
    pub path: String,
    pub(crate) module_l: String,
    /// 出现过的字符集合，给查询做快速排除
    pub(crate) mask: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Row {
    pub kind: Kind,
    pub module: u32,
    pub qualname: String,
    pub def_line: u32,
    pub card_id: String,
    pub is_entry: bool,
    /// 名字最后一段，小写
    pub(crate) name_l: String,
    /// 完整 qualname，小写
    pub(crate) qual_l: String,
    pub(crate) mask: u64,
}

/// 结果范围。自动识别只收扫描器认的入口与用户标记的，全部函数不限。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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

    pub(crate) fn is_empty(&self) -> bool {
        self.prefix.is_empty() && self.name.is_empty()
    }
}


