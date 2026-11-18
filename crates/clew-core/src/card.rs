//! 卡片的身份。
//!
//! 索引库、查询层、前端都靠一个字符串认卡片：`模块名:限定名`，例如
//! `cli.main:main`、`parser.core:Parser.parse`。以前这个格式靠各处手写
//! `format!("{module}:{qualname}")` 拼、靠各处 `split(':')` 拆，拼法有 4 份、
//! 拆法有 6 份，谁也不知道对方是不是同一套规则。这里把它收成一个类型：
//! 构造只有一个入口，拆解只有一个入口，格式写在 `Display` 里。

use std::fmt;
use std::str::FromStr;

/// 模块级定义的限定名。库里存的就是这个字符串，界面上显示成 `__main__`。
pub const MODULE_ENTRY_QUALNAME: &str = "<module>";

/// 一张卡片的身份：`模块名:限定名`。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CardId {
    module: String,
    qualname: String,
}

impl CardId {
    pub fn new(module: impl Into<String>, qualname: impl Into<String>) -> Self {
        Self {
            module: module.into(),
            qualname: qualname.into(),
        }
    }

    /// 模块级定义（`if __name__ == "__main__"` 那一块）的卡片身份。
    pub fn module_entry(module: impl Into<String>) -> Self {
        Self::new(module, MODULE_ENTRY_QUALNAME)
    }

    /// 拆一个 `模块:限定名` 字符串。没有冒号的输入不是卡片身份。
    pub fn parse(raw: &str) -> Option<Self> {
        Self::parts(raw).map(|(module, qualname)| Self::new(module, qualname))
    }

    /// 只借出两段、不造 `CardId` 时用它。分隔符规则只写在这里。
    pub fn parts(raw: &str) -> Option<(&str, &str)> {
        raw.split_once(':')
    }

    /// 只要模块名、不要整个身份时用它，省掉一次分配。
    pub fn module_of(raw: &str) -> Option<&str> {
        Self::parts(raw).map(|(module, _)| module)
    }

    /// 只要限定名、不要整个身份时用它，省掉一次分配。
    pub fn qualname_of(raw: &str) -> Option<&str> {
        Self::parts(raw).map(|(_, qualname)| qualname)
    }

    pub fn module(&self) -> &str {
        &self.module
    }

    pub fn qualname(&self) -> &str {
        &self.qualname
    }

    /// 是不是模块级定义那张伪卡片。
    pub fn is_module_entry(&self) -> bool {
        self.qualname == MODULE_ENTRY_QUALNAME
    }

    /// 限定名的最后一段：`Parser.parse` -> `parse`。
    pub fn short(&self) -> &str {
        self.qualname.rsplit('.').next().unwrap_or(&self.qualname)
    }

    pub fn into_string(self) -> String {
        self.to_string()
    }
}

impl fmt::Display for CardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.module, self.qualname)
    }
}

impl FromStr for CardId {
    type Err = MalformedCardId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| MalformedCardId(s.to_string()))
    }
}

/// 一个不是 `模块:限定名` 形状的字符串。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MalformedCardId(pub String);

impl fmt::Display for MalformedCardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "不是卡片身份（缺少冒号）：{}", self.0)
    }
}

impl std::error::Error for MalformedCardId {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_parse_round_trip() {
        let id = CardId::new("parser.core", "Parser.parse");
        assert_eq!(id.to_string(), "parser.core:Parser.parse");
        assert_eq!(CardId::parse(&id.to_string()), Some(id));
    }

    #[test]
    fn module_and_qualname_come_back_apart() {
        let id = CardId::parse("cli.main:main").expect("parses");
        assert_eq!(id.module(), "cli.main");
        assert_eq!(id.qualname(), "main");
        assert_eq!(id.short(), "main");
    }

    #[test]
    fn qualname_may_contain_dots_but_only_one_colon_splits() {
        let id = CardId::parse("pkg.outer:Outer.Inner.run").expect("parses");
        assert_eq!(id.module(), "pkg.outer");
        assert_eq!(id.qualname(), "Outer.Inner.run");
        assert_eq!(id.short(), "run");
    }

    #[test]
    fn a_string_without_a_colon_is_not_an_id() {
        assert_eq!(CardId::parse("pkg.outer"), None);
        assert_eq!("pkg.outer".parse::<CardId>().unwrap_err().0, "pkg.outer");
    }

    #[test]
    fn the_module_entry_is_recognised() {
        assert!(CardId::module_entry("cli.main").is_module_entry());
        assert!(!CardId::new("cli.main", "main").is_module_entry());
        assert_eq!(
            CardId::module_entry("cli.main").to_string(),
            "cli.main:<module>"
        );
    }
}
