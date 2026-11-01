//! Graph data model: cards (function/method/class) and call sites.
//!
//! This is the contract shape shared by the CLI and the future frontend, matching the prototype JSON one to one.

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CardKind {
    Function,
    Method,
    Class,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LineKind {
    Stmt,
    Call,
    Branch,
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

#[derive(Debug, Clone, Serialize)]
pub struct Call {
    pub from: String,
    pub target: String,
    pub kind: CallKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recv: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
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
    pub fn card_id(module: &str, qualname: &str) -> String {
        format!("{module}:{qualname}")
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
