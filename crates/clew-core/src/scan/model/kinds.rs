use std::fmt;
use std::str::FromStr;

use serde::Serialize;


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownKind(pub String);

impl fmt::Display for UnknownKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "库里出现了不认识的枚举值：{}", self.0)
    }
}

impl std::error::Error for UnknownKind {}

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
