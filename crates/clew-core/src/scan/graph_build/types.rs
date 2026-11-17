use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::super::model::*;

pub(crate) const BUILTINS: &[&str] = &[
    "print", "len", "range", "str", "int", "float", "bool", "list", "dict", "set", "tuple",
    "sum", "min", "max", "abs", "sorted", "enumerate", "zip", "map", "filter", "open",
    "isinstance", "getattr", "setattr", "hasattr", "super", "type", "round", "repr", "any",
    "all", "iter", "next", "input", "format", "id", "hash", "callable", "vars", "dir",
    "object", "frozenset", "bytes",
];


#[derive(Debug, Clone)]
pub struct ModuleMeta {
    pub module: String,
    pub path: PathBuf,
    pub src_hash: String,
    pub is_third_party: bool,
    pub has_main_guard: bool,
}

/// Definition facts that go into the `symbol` table.
#[derive(Debug, Clone)]
pub struct SymbolMeta {
    pub module: String,
    pub qualname: String,
    pub kind: SymbolKind,
    pub src_hash: String,
    pub def_line: u32,
    pub def_col: u32,
    pub body_start: usize,
    pub body_end: usize,
}

/// One entry of a module's import table.
#[derive(Debug, Clone)]
pub struct ImportMeta {
    /// Module that contains the import statement
    pub module: String,
    /// Name bound in that module
    pub local: String,
    /// Original name: module for `import a.b`, attribute for `from m import a`
    pub orig: String,
    /// Imported-from module; for `import a.b` this is `a.b` as well
    pub source_module: String,
    pub alias: bool,
    /// `import` form (`module_import`) instead of `from ... import`
    pub module_import: bool,
}

/// Class-level facts: bases and the `__init__` reference.
#[derive(Debug, Clone)]
pub struct ClassMeta {
    pub card_id: String,
    pub module: String,
    pub qualname: String,
    pub bases: Vec<String>,
    pub ctor: Option<String>,
}

/// Everything one tree-sitter pass produces: the graph plus the rows to persist.
#[derive(Debug, Default)]
pub struct Built {
    pub graph: Graph,
    pub modules: Vec<ModuleMeta>,
    pub symbols: Vec<SymbolMeta>,
    pub imports: Vec<ImportMeta>,
    pub classes: Vec<ClassMeta>,
    /// Body of the `if __name__ == "__main__":` guard, keyed by module
    pub module_cards: BTreeMap<String, Vec<Line>>,
}


/// Maximum characters per line before truncation.

#[derive(Debug, Clone)]
pub(crate) struct ImportInfo {
    pub(crate) module: String,
    pub(crate) orig: String,
    pub(crate) local: String,
    pub(crate) alias: bool,
    pub(crate) module_import: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct RawCall {
    pub(crate) target: String,
    pub(crate) kind: CallKind,
    pub(crate) recv: Option<String>,
    pub(crate) ctor: Option<String>,
    pub(crate) pos: Vec<String>,
    pub(crate) kwargs: Vec<(String, String)>,
    pub(crate) line: u32,
}

#[derive(Clone, Default)]
pub(crate) struct RawGraph {
    pub(crate) cards: BTreeMap<String, Card>,
    pub(crate) calls: BTreeMap<String, Call>,
    pub(crate) raw_calls: BTreeMap<String, RawCall>,
    pub(crate) imports: BTreeMap<String, BTreeMap<String, ImportInfo>>,
    pub(crate) card_imports: BTreeMap<String, BTreeMap<String, ImportInfo>>,
    pub(crate) methods: BTreeMap<(String, String), BTreeMap<String, String>>,
    pub(crate) class_bases: BTreeMap<String, (String, Vec<String>)>,
    pub(crate) class_attrs: BTreeMap<String, BTreeMap<String, String>>,
    pub(crate) card_locals: BTreeMap<String, BTreeMap<String, String>>,
    pub(crate) class_ids: BTreeMap<(String, String), String>,
    pub(crate) class_by_qualname: BTreeMap<String, String>,
}

#[derive(Clone, Default)]
pub(crate) struct GlobalIndex {
    pub(crate) by_name: Option<Arc<BTreeMap<String, Vec<String>>>>,
    pub(crate) by_method: Option<Arc<BTreeMap<String, Vec<String>>>>,
    pub(crate) by_class: Option<Arc<BTreeMap<String, Vec<String>>>>,
    pub(crate) by_qualname: Option<Arc<BTreeMap<String, Vec<String>>>>,
}
