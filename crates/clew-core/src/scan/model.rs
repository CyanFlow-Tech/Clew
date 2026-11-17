//! Graph data model: cards (function/method/class) and call sites.
//!
//! This is the contract shape shared by the CLI and the future frontend, matching the prototype JSON one to one.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::Serialize;

use crate::card::CardId;

/// 库里出现了代码不认识的枚举字符串。
///
/// 读侧以前的写法是 `match raw { "call" => .., _ => 默认值 }`——一个拼错的
/// 字符串会被静默解释成默认值，永远查不出来。现在解析会带出这个错误，
/// 调用方决定是「报错」还是「按默认值继续」，选择写在调用点而不是藏在 match 末尾。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownKind(pub String);

impl fmt::Display for UnknownKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "库里出现了不认识的枚举值：{}", self.0)
    }
}

impl std::error::Error for UnknownKind {}

/// Kind of a definition as stored in the `symbol` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    /// Pseudo symbol standing for the module body (`<module>`)
    Module,
}

impl SymbolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SymbolKind::Function => "function",
            SymbolKind::Method => "method",
            SymbolKind::Class => "class",
            SymbolKind::Module => "module",
        }
    }

    /// Map a card kind to the symbol kind it is persisted as.
    pub fn of_card(kind: CardKind) -> Self {
        match kind {
            CardKind::Class => SymbolKind::Class,
            CardKind::Method => SymbolKind::Method,
            CardKind::Function => SymbolKind::Function,
        }
    }
}

impl Default for SymbolKind {
    fn default() -> Self {
        SymbolKind::Function
    }
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SymbolKind {
    type Err = UnknownKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "function" => Ok(SymbolKind::Function),
            "method" => Ok(SymbolKind::Method),
            "class" => Ok(SymbolKind::Class),
            "module" => Ok(SymbolKind::Module),
            other => Err(UnknownKind(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CardKind {
    Function,
    Method,
    Class,
}

impl CardKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CardKind::Function => "function",
            CardKind::Method => "method",
            CardKind::Class => "class",
        }
    }
}

impl Default for CardKind {
    fn default() -> Self {
        CardKind::Function
    }
}

impl fmt::Display for CardKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CardKind {
    type Err = UnknownKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "function" => Ok(CardKind::Function),
            "method" => Ok(CardKind::Method),
            "class" => Ok(CardKind::Class),
            other => Err(UnknownKind(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LineKind {
    Stmt,
    Call,
    Branch,
}

impl LineKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LineKind::Stmt => "stmt",
            LineKind::Call => "call",
            LineKind::Branch => "branch",
        }
    }
}

impl Default for LineKind {
    fn default() -> Self {
        LineKind::Stmt
    }
}

impl fmt::Display for LineKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for LineKind {
    type Err = UnknownKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "stmt" => Ok(LineKind::Stmt),
            "call" => Ok(LineKind::Call),
            "branch" => Ok(LineKind::Branch),
            other => Err(UnknownKind(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Line {
    pub k: LineKind,
    pub text: String,
    #[serde(default)]
    pub indent: u8,
    /// 1-based line number; the lines split out of one statement share the same value
    #[serde(default)]
    pub ln: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cs: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MethodRef {
    pub name: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Card {
    pub id: String,
    pub module: String,
    pub qualname: String,
    pub title: String,
    pub params: Vec<String>,
    pub kind: CardKind,
    pub lines: Vec<Line>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<MethodRef>,
    /// Base class names as written in the source
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bases: Vec<String>,
    /// Effective `__init__` card id, found along the MRO
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctor: Option<String>,
    /// Class that actually defines `ctor`; differs from this class when inherited
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctor_owner: Option<String>,
    /// Base class living outside the project (its `__init__` cannot be shown)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_base: Option<String>,
    /// Local variable -> class card id (inferred from constructor assignments)
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub locals: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CallKind {
    /// `foo()`
    Name,
    /// `self.foo()` / `cls.foo()`
    #[serde(rename = "self")]
    SelfRef,
    /// `self.attr.foo()`
    SelfAttr,
    /// `obj.foo()`
    Attr,
    /// Anything else (chained, subscripted, ...)
    Other,
}

impl CallKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CallKind::Name => "name",
            CallKind::SelfRef => "self",
            CallKind::SelfAttr => "selfattr",
            CallKind::Attr => "attr",
            CallKind::Other => "other",
        }
    }
}

impl Default for CallKind {
    fn default() -> Self {
        CallKind::Other
    }
}

impl fmt::Display for CallKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CallKind {
    type Err = UnknownKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "name" => Ok(CallKind::Name),
            "self" => Ok(CallKind::SelfRef),
            "selfattr" => Ok(CallKind::SelfAttr),
            "attr" => Ok(CallKind::Attr),
            "other" => Ok(CallKind::Other),
            other => Err(UnknownKind(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// Same module / import / type inference / self. method
    Exact,
    /// Several implementations share the name; all candidates are listed
    Poly,
    /// Single match by name
    Heur,
    /// Builtin or library function
    Builtin,
    /// Unresolved
    None,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::Exact => "exact",
            Confidence::Poly => "poly",
            Confidence::Heur => "heur",
            Confidence::Builtin => "builtin",
            Confidence::None => "none",
        }
    }
}

impl Default for Confidence {
    fn default() -> Self {
        Confidence::None
    }
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Confidence {
    type Err = UnknownKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "exact" => Ok(Confidence::Exact),
            "poly" => Ok(Confidence::Poly),
            "heur" => Ok(Confidence::Heur),
            "builtin" => Ok(Confidence::Builtin),
            "none" => Ok(Confidence::None),
            other => Err(UnknownKind(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Call {
    pub from: String,
    pub target: String,
    pub kind: CallKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recv: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
    /// 1-based line of the call expression itself
    #[serde(default)]
    pub ln: u32,
    pub conf: Confidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
    /// Shaped like `from mod import Name as Alias`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported: Option<String>,
    /// Shaped like `nnet: NNetWrapper`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typed: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct Graph {
    pub cards: BTreeMap<String, Card>,
    pub calls: BTreeMap<String, Call>,
}

impl Graph {
    /// 卡片身份的展示文本。构造只有 [`CardId`] 一处，这里保留旧签名方便调用点。
    pub fn card_id(module: &str, qualname: &str) -> String {
        CardId::new(module, qualname).into_string()
    }

    pub fn stats(&self) -> Stats {
        let mut s = Stats::default();
        s.cards = self.cards.len();
        for c in self.cards.values() {
            match c.kind {
                CardKind::Class => s.classes += 1,
                _ => s.functions += 1,
            }
        }
        s.calls = self.calls.len();
        for c in self.calls.values() {
            match c.conf {
                Confidence::Exact => s.exact += 1,
                Confidence::Poly => s.poly += 1,
                Confidence::Heur => s.heur += 1,
                Confidence::Builtin => s.builtin += 1,
                Confidence::None => s.none += 1,
            }
        }
        s
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub cards: usize,
    pub classes: usize,
    pub functions: usize,
    pub calls: usize,
    pub exact: usize,
    pub poly: usize,
    pub heur: usize,
    pub builtin: usize,
    pub none: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 写进库的字符串与从库里读回的值必须是同一套词：`as_str` 出去，
    /// `FromStr` 回来，中间不需要任何地方再手写一遍 match。
    macro_rules! round_trips {
        ($ty:ty, $($value:expr),+ $(,)?) => {
            $(
                let v: $ty = $value;
                let text = v.to_string();
                assert_eq!(text, v.as_str(), "Display 与 as_str 不一致");
                assert_eq!(text.parse::<$ty>(), Ok(v), "{} 读不回来", text);
            )+
        };
    }

    #[test]
    fn every_kind_round_trips_through_its_stored_text() {
        round_trips!(
            SymbolKind,
            SymbolKind::Function,
            SymbolKind::Method,
            SymbolKind::Class,
            SymbolKind::Module,
        );
        round_trips!(
            CardKind,
            CardKind::Function,
            CardKind::Method,
            CardKind::Class,
        );
        round_trips!(
            LineKind,
            LineKind::Stmt,
            LineKind::Call,
            LineKind::Branch,
        );
        round_trips!(
            CallKind,
            CallKind::Name,
            CallKind::SelfRef,
            CallKind::SelfAttr,
            CallKind::Attr,
            CallKind::Other,
        );
        round_trips!(
            Confidence,
            Confidence::Exact,
            Confidence::Poly,
            Confidence::Heur,
            Confidence::Builtin,
            Confidence::None,
        );
    }

    #[test]
    fn serde_writes_the_same_text_as_the_database() {
        assert_eq!(serde_json::to_string(&CallKind::SelfRef).unwrap(), "\"self\"");
        assert_eq!(serde_json::to_string(&Confidence::Heur).unwrap(), "\"heur\"");
        assert_eq!(serde_json::to_string(&SymbolKind::Module).unwrap(), "\"module\"");
    }

    #[test]
    fn an_unknown_kind_is_an_error_not_a_silent_default() {
        let err = "cal".parse::<LineKind>().unwrap_err();
        assert_eq!(err.0, "cal");
        assert_eq!("class".parse::<LineKind>().unwrap_err().0, "class");
    }

    #[test]
    fn the_defaults_match_what_the_old_match_arms_fell_through_to() {
        assert_eq!(LineKind::default(), LineKind::Stmt);
        assert_eq!(CallKind::default(), CallKind::Other);
        assert_eq!(Confidence::default(), Confidence::None);
        assert_eq!(CardKind::default(), CardKind::Function);
        assert_eq!(SymbolKind::default(), SymbolKind::Function);
    }
}
