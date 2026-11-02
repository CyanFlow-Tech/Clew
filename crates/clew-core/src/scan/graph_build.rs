//! Build the graph (cards + call sites) with tree-sitter for `clew export`.
//!
//! Resolution order: same module, then the import table, then local type
//! inference, then project-wide uniqueness.
//! Deliberately no type-system analysis here (that is ty's job): syntax level only.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use blake3::Hasher;
use tree_sitter::{Node, Parser};

use super::model::{
    Call, CallKind, Card, CardKind, Confidence, Graph, Line, LineKind, MethodRef, SymbolKind,
};
use super::walker;

/// Per-file facts that go into the `module` table.
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
}

const BUILTINS: &[&str] = &[
    "print", "len", "range", "str", "int", "float", "bool", "list", "dict", "set", "tuple",
    "sum", "min", "max", "abs", "sorted", "enumerate", "zip", "map", "filter", "open",
    "isinstance", "getattr", "setattr", "hasattr", "super", "type", "round", "repr", "any",
    "all", "iter", "next", "input", "format", "id", "hash", "callable", "vars", "dir",
    "object", "frozenset", "bytes",
];

const SUMMARY_MAX_CALLS: usize = 3;

/// Maximum characters per line before truncation.
const LINE_LIMIT: usize = 96;

#[derive(Debug, Clone)]
struct ImportInfo {
    module: String,
    orig: String,
    local: String,
    alias: bool,
    module_import: bool,
}

#[derive(Debug)]
struct RawCall {
    target: String,
    kind: CallKind,
    recv: Option<String>,
    pos: Vec<String>,
    kwargs: Vec<(String, String)>,
    line: u32,
}

pub struct Builder {
    src: Vec<u8>,
    module: String,
    cards: BTreeMap<String, Card>,
    calls: BTreeMap<String, Call>,
    raw_calls: BTreeMap<String, RawCall>,
    /// module -> local name -> import info
    imports: BTreeMap<String, BTreeMap<String, ImportInfo>>,
    /// (module, class) -> method name -> card id
    methods: BTreeMap<(String, String), BTreeMap<String, String>>,
    /// class card id -> (module, base class names)
    class_bases: BTreeMap<String, (String, Vec<String>)>,
    /// class card id -> attribute name -> raw class name reference
    class_attrs: BTreeMap<String, BTreeMap<String, String>>,
    /// card id -> local variable -> raw class name reference
    card_locals: BTreeMap<String, BTreeMap<String, String>>,
    class_ids: BTreeMap<(String, String), String>,
    /// rows to persist
    out_modules: Vec<ModuleMeta>,
    out_symbols: Vec<SymbolMeta>,
    out_classes: Vec<ClassMeta>,
}

pub fn build(root: &Path) -> anyhow::Result<Built> {
    build_with(root, false)
}

pub fn build_with(root: &Path, include_venv: bool) -> anyhow::Result<Built> {
    let root = std::fs::canonicalize(root)?;
    let files = walker::collect_with(&root, include_venv);

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .expect("failed to load tree-sitter-python");

    let mut builder = Builder::default();
    for file in &files {
        let Ok(source) = std::fs::read_to_string(&file.path) else {
            continue;
        };
        let tree = match parser.parse(&source, None) {
            Some(t) => t,
            None => continue,
        };
        let mut hasher = Hasher::new();
        hasher.update(source.as_bytes());
        let src_hash = hasher.finalize().to_hex().to_string();
        builder.file(file, source.as_bytes(), tree.root_node(), src_hash);
    }
    builder.resolve();

    let mut imports = Vec::new();
    for (module, table) in &builder.imports {
        for info in table.values() {
            imports.push(ImportMeta {
                module: module.clone(),
                local: info.local.clone(),
                orig: info.orig.clone(),
                source_module: info.module.clone(),
                alias: info.alias,
                module_import: info.module_import,
            });
        }
    }

    Ok(Built {
        graph: Graph {
            cards: builder.cards,
            calls: builder.calls,
        },
        modules: builder.out_modules,
        symbols: builder.out_symbols,
        imports,
        classes: builder.out_classes,
    })
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            src: Vec::new(),
            module: String::new(),
            cards: BTreeMap::new(),
            calls: BTreeMap::new(),
            raw_calls: BTreeMap::new(),
            imports: BTreeMap::new(),
            methods: BTreeMap::new(),
            class_bases: BTreeMap::new(),
            class_attrs: BTreeMap::new(),
            card_locals: BTreeMap::new(),
            class_ids: BTreeMap::new(),
            out_modules: Vec::new(),
            out_symbols: Vec::new(),
            out_classes: Vec::new(),
        }
    }
}

impl Builder {
    fn text(&self, node: Node) -> String {
        node.utf8_text(&self.src).unwrap_or("").to_string()
    }

    fn first_line(&self, node: Node) -> String {
        let raw = self.text(node);
        let line = raw.lines().next().unwrap_or("").trim();
        if line.chars().count() > LINE_LIMIT {
            let mut s: String = line.chars().take(LINE_LIMIT - 1).collect();
            s.push('…');
            s
        } else {
            line.to_string()
        }
    }

    /// blake3 of the definition body, used to spot edits on rescan.
    fn body_hash(&self, node: Node) -> String {
        let range = match node.child_by_field_name("body") {
            Some(body) => body.byte_range(),
            None => node.byte_range(),
        };
        let mut hasher = Hasher::new();
        hasher.update(&self.src[range]);
        hasher.finalize().to_hex().to_string()
    }

    fn push_symbol(&mut self, node: Node, qualname: &str, kind: SymbolKind) {
        self.out_symbols.push(SymbolMeta {
            module: self.module.clone(),
            qualname: qualname.to_string(),
            kind,
            src_hash: self.body_hash(node),
            def_line: node.start_position().row as u32 + 1,
            def_col: node.start_position().column as u32,
            body_start: node.start_byte(),
            body_end: node.end_byte(),
        });
    }

    fn file(&mut self, file: &walker::PyFile, src: &[u8], root: Node, src_hash: String) {
        self.src = src.to_vec();
        self.module = file.module.clone();

        let mut cursor = root.walk();
        for node in root.children(&mut cursor) {
            match node.kind() {
                "import_statement" => self.collect_import(node),
                "import_from_statement" => self.collect_import_from(node),
                "function_definition" => self.add_function(node, None),
                "class_definition" => self.add_class(node),
                "decorated_definition" => {
                    if let Some(inner) = node.child_by_field_name("definition") {
                        match inner.kind() {
                            "function_definition" => self.add_function(inner, None),
                            "class_definition" => self.add_class(inner),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        self.out_modules.push(ModuleMeta {
            module: file.module.clone(),
            path: file.path.clone(),
            src_hash,
            is_third_party: file.is_third_party,
            has_main_guard: has_main_guard(&self.src, root),
        });
    }

    // ── imports ───────────────────────────────────────────────────
    fn collect_import(&mut self, node: Node) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "dotted_name" => {
                    let full = self.text(child);
                    let local = full.split('.').next().unwrap_or(&full).to_string();
                    self.imports.entry(self.module.clone()).or_default().insert(
                        local.clone(),
                        ImportInfo {
                            module: full.clone(),
                            orig: local.clone(),
                            local,
                            alias: false,
                            module_import: true,
                        },
                    );
                }
                "aliased_import" => {
                    if let (Some(name), Some(alias)) = (
                        child.child_by_field_name("name"),
                        child.child_by_field_name("alias"),
                    ) {
                        let full = self.text(name);
                        let local = self.text(alias);
                        self.imports.entry(self.module.clone()).or_default().insert(
                            local.clone(),
                            ImportInfo {
                                module: full,
                                orig: local.clone(),
                                local,
                                alias: true,
                                module_import: true,
                            },
                        );
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_import_from(&mut self, node: Node) {
        let module_node = node.child_by_field_name("module_name");
        let mut base = module_node.map(|n| self.text(n)).unwrap_or_default();

        // module_name itself is relative (from .Board import Board)
        if base.starts_with('.') {
            let dots = base.chars().take_while(|c| *c == '.').count();
            let rest = base.trim_start_matches('.').to_string();
            let mut parts: Vec<&str> = self.module.split('.').collect();
            if !parts.is_empty() {
                parts.pop();
            }
            for _ in 1..dots {
                parts.pop();
            }
            base = if rest.is_empty() {
                parts.join(".")
            } else if parts.is_empty() {
                rest
            } else {
                format!("{}.{}", parts.join("."), rest)
            };
        }

        // relative import: from . import mod
        if base.is_empty() {
            let mut cursor = node.walk();
            for ch in node.children(&mut cursor) {
                if ch.kind() == "relative_import" {
                    let raw = self.text(ch);
                    let dots = raw.chars().take_while(|c| *c == '.').count();
                    let rest = raw.trim_start_matches('.').to_string();
                    let mut parts: Vec<&str> = self.module.split('.').collect();
                    if parts.len() > 0 {
                        parts.pop();                       // drop the current module name
                    }
                    for _ in 1..dots {
                        parts.pop();                       // one level up per extra dot
                    }
                    if !rest.is_empty() {
                        base = if parts.is_empty() {
                            rest
                        } else {
                            format!("{}.{}", parts.join("."), rest)
                        };
                    } else {
                        base = parts.join(".");
                    }
                }
            }
        }
        // skip the dots of a relative import; handled above
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "dotted_name" if module_node.map(|m| m.id()) == Some(child.id()) => {}
                "dotted_name" => {
                    let orig = self.text(child);
                    self.imports.entry(self.module.clone()).or_default().insert(
                        orig.clone(),
                        ImportInfo {
                            module: base.clone(),
                            orig: orig.clone(),
                            local: orig,
                            alias: false,
                            module_import: false,
                        },
                    );
                }
                "aliased_import" => {
                    if let (Some(name), Some(alias)) = (
                        child.child_by_field_name("name"),
                        child.child_by_field_name("alias"),
                    ) {
                        let orig = self.text(name);
                        let local = self.text(alias);
                        self.imports.entry(self.module.clone()).or_default().insert(
                            local.clone(),
                            ImportInfo {
                                module: base.clone(),
                                orig,
                                local,
                                alias: true,
                                module_import: false,
                            },
                        );
                    }
                }
                _ => {}
            }
        }
    }

    // ── functions and methods ─────────────────────────────────────
    fn add_function(&mut self, node: Node, class: Option<&str>) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node);
        let qualname = match class {
            Some(c) => format!("{c}.{name}"),
            None => name.clone(),
        };
        let id = Graph::card_id(&self.module, &qualname);
        let params = self.params_of(node);
        let mut lines = Vec::new();
        let mut locals = BTreeMap::new();
        if let Some(body) = node.child_by_field_name("body") {
            self.emit_block(body, &id, 0, &mut lines, &mut locals);
        }
        let title = format!("{qualname}({})", params.join(", "));
        let kind = if class.is_some() {
            CardKind::Method
        } else {
            CardKind::Function
        };

        self.cards.insert(
            id.clone(),
            Card {
                id: id.clone(),
                module: self.module.clone(),
                qualname: qualname.clone(),
                title,
                params,
                kind,
                lines,
                methods: Vec::new(),
                locals: BTreeMap::new(),
            },
        );
        self.push_symbol(node, &qualname, SymbolKind::of_card(kind));
        if let Some(c) = class {
            self.methods
                .entry((self.module.clone(), c.to_string()))
                .or_default()
                .insert(name, id.clone());
        }
        if !locals.is_empty() {
            self.card_locals.insert(id, locals);
        }
    }

    fn add_class(&mut self, node: Node) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node);
        let id = Graph::card_id(&self.module, &name);

        let mut bases = Vec::new();
        if let Some(supers) = node.child_by_field_name("superclasses") {
            let mut cursor = supers.walk();
            for ch in supers.children(&mut cursor) {
                if ch.kind() == "identifier" {
                    bases.push(self.text(ch));
                }
            }
        }

        let mut methods = Vec::new();
        let mut ctor_params: Vec<String> = Vec::new();
        let mut ctor_lines = Vec::new();
        let mut ctor_locals = BTreeMap::new();
        let mut ctor_attrs: BTreeMap<String, String> = BTreeMap::new();
        let mut ctor_id: Option<String> = None;

        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                let def = if child.kind() == "decorated_definition" {
                    child.child_by_field_name("definition")
                } else if child.kind() == "function_definition" {
                    Some(child)
                } else {
                    None
                };
                let Some(def) = def else { continue };
                let Some(fname) = def.child_by_field_name("name") else {
                    continue;
                };
                let mname = self.text(fname);
                methods.push(MethodRef {
                    name: mname.clone(),
                    target: format!("{id}.{mname}"),
                });
                if mname == "__init__" {
                    ctor_id = Some(format!("{id}.{mname}"));
                    ctor_params = self.params_of(def);
                    if let Some(b) = def.child_by_field_name("body") {
                        self.emit_block(b, &id, 0, &mut ctor_lines, &mut ctor_locals);
                    }
                    self.collect_self_attrs(def, &mut ctor_attrs);
                }
            }
        }

        let title = format!("{name}({})", ctor_params.join(", "));
        self.cards.insert(
            id.clone(),
            Card {
                id: id.clone(),
                module: self.module.clone(),
                qualname: name.clone(),
                title,
                params: ctor_params,
                kind: CardKind::Class,
                lines: ctor_lines,
                methods,
                locals: BTreeMap::new(),
            },
        );
        self.push_symbol(node, &name, SymbolKind::Class);
        self.out_classes.push(ClassMeta {
            card_id: id.clone(),
            module: self.module.clone(),
            qualname: name.clone(),
            bases: bases.clone(),
            ctor: ctor_id,
        });
        self.class_ids
            .insert((self.module.clone(), name.clone()), id.clone());
        self.class_bases
            .insert(id.clone(), (self.module.clone(), bases));
        self.class_attrs.insert(id.clone(), ctor_attrs);
        if !ctor_locals.is_empty() {
            self.card_locals.insert(id.clone(), ctor_locals);
        }

        // method bodies inside a class become cards of their own
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                let def = if child.kind() == "decorated_definition" {
                    child.child_by_field_name("definition")
                } else if child.kind() == "function_definition" {
                    Some(child)
                } else {
                    None
                };
                if let Some(def) = def {
                    self.add_function(def, Some(&name));
                }
            }
        }
    }

    fn params_of(&self, node: Node) -> Vec<String> {
        let mut out = Vec::new();
        let Some(params) = node.child_by_field_name("parameters") else {
            return out;
        };
        let mut cursor = params.walk();
        for ch in params.children(&mut cursor) {
            match ch.kind() {
                "identifier" => out.push(self.text(ch)),
                "typed_parameter" => {
                    if let Some(id) = ch.child_by_field_name("identifier") {
                        out.push(self.text(id));
                    }
                }
                "default_parameter" | "typed_default_parameter" => {
                    if let Some(id) = ch.child_by_field_name("name") {
                        out.push(self.text(id));
                    }
                }
                "list_splat_pattern" | "dictionary_splat_pattern" => {
                    let t = self.text(ch);
                    out.push(t);
                }
                _ => {}
            }
        }
        out.retain(|p| p != "self" && p != "cls");
        out
    }

    fn collect_self_attrs(&mut self, init: Node, attrs: &mut BTreeMap<String, String>) {
        let body = match init.child_by_field_name("body") {
            Some(b) => b,
            None => return,
        };
        let mut stack = vec![body];
        while let Some(node) = stack.pop() {
            let mut cursor = node.walk();
            for ch in node.children(&mut cursor) {
                if ch.kind() == "assignment" {
                    if let (Some(left), Some(right)) = (
                        ch.child_by_field_name("left"),
                        ch.child_by_field_name("right"),
                    ) {
                        if left.kind() == "attribute" && right.kind() == "call" {
                            let obj = left.child_by_field_name("object");
                            let attr = left.child_by_field_name("attribute");
                            if let (Some(obj), Some(attr)) = (obj, attr) {
                                if self.text(obj) == "self" {
                                    if let Some(callee) = class_name_of_call(&self.src, right) {
                                        attrs.insert(self.text(attr), callee);
                                    }
                                }
                            }
                        }
                    }
                }
                stack.push(ch);
            }
        }
    }

    // ── statement lines ───────────────────────────────────────────
    fn emit_block(
        &mut self,
        block: Node,
        owner: &str,
        indent: u8,
        out: &mut Vec<Line>,
        locals: &mut BTreeMap<String, String>,
    ) {
        let mut cursor = block.walk();
        for stmt in block.children(&mut cursor) {
            match stmt.kind() {
                "if_statement" | "for_statement" | "while_statement" | "with_statement"
                | "try_statement" | "match_statement" => {
                    out.push(Line {
                        k: LineKind::Branch,
                        text: self.first_line(stmt),
                        indent,
                        ln: stmt.start_position().row as u32 + 1,
                        cs: None,
                    });
                    let mut inner = stmt.walk();
                    for ch in stmt.children(&mut inner) {
                        match ch.kind() {
                            "block" | "else_clause" | "elif_clause" | "except_clause"
                            | "finally_clause" | "case_clause" => {
                                self.emit_compound_body(ch, owner, indent + 1, out, locals);
                            }
                            _ => {}
                        }
                    }
                }
                "block" => self.emit_block(stmt, owner, indent, out, locals),
                _ => {
                    let calls = collect_calls(stmt);
                    let ln = stmt.start_position().row as u32 + 1;
                    if calls.len() >= 2 {
                        for (i, call) in calls.iter().take(SUMMARY_MAX_CALLS).enumerate() {
                            let cs_id = format!("{owner}#{}", out.len());
                            self.record_call(cs_id.clone(), owner, *call);
                            out.push(Line {
                                k: LineKind::Call,
                                text: if i == 0 {
                                    self.first_line(stmt)
                                } else {
                                    format!("↳ {}", self.first_line(*call))
                                },
                                indent: if i == 0 { indent } else { indent + 1 },
                                ln,
                                cs: Some(cs_id),
                            });
                        }
                    } else if calls.len() == 1 {
                        let cs_id = format!("{owner}#{}", out.len());
                        self.record_call(cs_id.clone(), owner, calls[0]);
                        out.push(Line {
                            k: LineKind::Call,
                            text: self.first_line(stmt),
                            indent,
                            ln,
                            cs: Some(cs_id),
                        });
                    } else {
                        out.push(Line {
                            k: LineKind::Stmt,
                            text: self.first_line(stmt),
                            indent,
                            ln,
                            cs: None,
                        });
                    }
                    // local variable type: x = Class(...), wrapped in expression_statement
                    let assign = if stmt.kind() == "assignment" {
                        Some(stmt)
                    } else if stmt.kind() == "expression_statement" {
                        stmt.child(0).filter(|n| n.kind() == "assignment")
                    } else {
                        None
                    };
                    if let Some(stmt) = assign {
                        if let (Some(left), Some(right)) =
                            (stmt.child_by_field_name("left"), stmt.child_by_field_name("right"))
                        {
                            if left.kind() == "identifier" && right.kind() == "call" {
                                if let Some(callee) = class_name_of_call(&self.src, right) {
                                    locals.insert(self.text(left), callee);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn emit_compound_body(
        &mut self,
        node: Node,
        owner: &str,
        indent: u8,
        out: &mut Vec<Line>,
        locals: &mut BTreeMap<String, String>,
    ) {
        if node.kind() == "block" {
            self.emit_block(node, owner, indent, out, locals);
            return;
        }
        let mut cursor = node.walk();
        for ch in node.children(&mut cursor) {
            if ch.kind() == "block" {
                self.emit_block(ch, owner, indent, out, locals);
            }
        }
    }

    fn record_call(&mut self, cs_id: String, owner: &str, call: Node) {
        // keep the call site even when the callee is unknown, so the anchor
        // never points at a missing record
        let (target, kind, recv) = match call.child_by_field_name("function") {
            Some(func) => classify(&self.src, func),
            None => (None, CallKind::Other, None),
        };
        let target = target.unwrap_or_default();

        let mut pos = Vec::new();
        let mut kwargs = Vec::new();
        if let Some(args) = call.child_by_field_name("arguments") {
            let mut cursor = args.walk();
            for ch in args.children(&mut cursor) {
                match ch.kind() {
                    "keyword_argument" => {
                        if let (Some(k), Some(v)) = (
                            ch.child_by_field_name("name"),
                            ch.child_by_field_name("value"),
                        ) {
                            kwargs.push((self.text(k), self.first_line(v)));
                        }
                    }
                    "(" | ")" | "," => {}
                    _ => pos.push(self.first_line(ch)),
                }
            }
        }

        self.raw_calls.insert(
            cs_id.clone(),
            RawCall {
                target,
                kind,
                recv,
                pos,
                kwargs,
                line: call.start_position().row as u32 + 1,
            },
        );
        self.calls.insert(
            cs_id,
            Call {
                from: owner.to_string(),
                target: String::new(),
                kind: CallKind::Other,
                recv: None,
                args: BTreeMap::new(),
                ln: 0,
                conf: Confidence::None,
                to: None,
                candidates: Vec::new(),
                imported: None,
                typed: None,
            },
        );
    }

    // ── resolution ────────────────────────────────────────────────
    fn resolve(&mut self) {
        let by_name: BTreeMap<String, Vec<String>> = {
            let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for (id, card) in &self.cards {
                let bare = card
                    .qualname
                    .rsplit('.')
                    .next()
                    .unwrap_or(&card.qualname)
                    .to_string();
                map.entry(bare).or_default().push(id.clone());
            }
            map
        };

        // local variables -> class cards
        let mut card_locals: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for (card_id, vars) in self.card_locals.clone() {
            let mut resolved = BTreeMap::new();
            for (var, raw) in vars {
                if let Some(cls) = self.resolve_class(&card_id, &raw) {
                    resolved.insert(var, cls);
                }
            }
            if !resolved.is_empty() {
                card_locals.insert(card_id, resolved);
            }
        }
        // class attributes -> class cards
        let mut class_attrs: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for (cls_id, attrs) in self.class_attrs.clone() {
            let mut resolved = BTreeMap::new();
            for (attr, raw) in attrs {
                if let Some(target) = self.resolve_class(&cls_id, &raw) {
                    resolved.insert(attr, target);
                }
            }
            class_attrs.insert(cls_id, resolved);
        }

        let raw_calls = std::mem::take(&mut self.raw_calls);
        for (cs_id, raw) in raw_calls {
            let mut call = self.calls.remove(&cs_id).unwrap_or(Call {
                from: String::new(),
                target: String::new(),
                kind: CallKind::Other,
                recv: None,
                args: BTreeMap::new(),
                ln: 0,
                conf: Confidence::None,
                to: None,
                candidates: Vec::new(),
                imported: None,
                typed: None,
            });
            call.target = raw.target.clone();
            call.kind = raw.kind;
            call.recv = raw.recv.clone();
            call.ln = raw.line;

            let module = call.from.split(':').next().unwrap_or("").to_string();
            let card = self.cards.get(&call.from).cloned();

            let mut chosen: Option<String> = None;
            let mut conf = Confidence::None;
            let mut candidates: Vec<String> = Vec::new();
            let mut imported = None;
            let mut typed = None;

            if BUILTINS.contains(&raw.target.as_str()) {
                conf = Confidence::Builtin;
            } else {
                match raw.kind {
                    CallKind::SelfRef => {
                        if let Some(card) = &card {
                            if let Some(cls) = class_of_qualname(&card.qualname) {
                                if let Some(mid) = self
                                    .methods
                                    .get(&(module.clone(), cls.clone()))
                                    .and_then(|m| m.get(&raw.target))
                                {
                                    // methods may be overridden in subclasses: expose every candidate
                                    let mut cands: Vec<String> = self
                                        .methods
                                        .iter()
                                        .filter(|((m, _), _)| m == &module)
                                        .filter_map(|(_, ms)| ms.get(&raw.target).cloned())
                                        .collect();
                                    cands.sort();
                                    cands.dedup();
                                    chosen = Some(mid.clone());
                                    if cands.len() > 1 {
                                        conf = Confidence::Poly;
                                        candidates = cands;
                                    } else {
                                        conf = Confidence::Exact;
                                    }
                                }
                            }
                        }
                    }
                    CallKind::SelfAttr => {
                        if let (Some(card), Some(attr)) = (&card, &raw.recv) {
                            if let Some(cls) = class_of_qualname(&card.qualname) {
                                let cls_id = self
                                    .class_ids
                                    .get(&(module.clone(), cls.clone()))
                                    .cloned();
                                if let Some(cls_id) = cls_id {
                                    if let Some(attr_cls) = class_attrs
                                        .get(&cls_id)
                                        .and_then(|a| a.get(attr))
                                        .cloned()
                                    {
                                        if let Some(mid) =
                                            self.find_method(&attr_cls, &raw.target, 0)
                                        {
                                            chosen = Some(mid);
                                            conf = Confidence::Exact;
                                            typed = Some(format!("{attr}: {}", short_name(&attr_cls)));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    CallKind::Attr => {
                        if let Some(recv) = &raw.recv {
                            if let Some(target_cls) =
                                card_locals.get(&call.from).and_then(|m| m.get(recv)).cloned()
                            {
                                if let Some(mid) = self.find_method(&target_cls, &raw.target, 0) {
                                    chosen = Some(mid);
                                    conf = Confidence::Exact;
                                    typed = Some(format!("{recv}: {}", short_name(&target_cls)));
                                }
                            }
                        }
                        if chosen.is_none() {
                            let mut cands: Vec<String> = self
                                .methods
                                .values()
                                .filter_map(|m| m.get(&raw.target).cloned())
                                .collect();
                            cands.sort();
                            cands.dedup();
                            if cands.len() >= 2 {
                                chosen = Some(cands[0].clone());
                                candidates = cands;
                                conf = Confidence::Poly;
                            } else if cands.len() == 1 {
                                chosen = Some(cands[0].clone());
                                conf = Confidence::Heur;
                            }
                        }
                    }
                    _ => {}
                }

                if chosen.is_none() && raw.kind == CallKind::Name {
                    let same = Graph::card_id(&module, &raw.target);
                    if self.cards.contains_key(&same) {
                        chosen = Some(same);
                        conf = Confidence::Exact;
                    } else if let Some(imp) = self.imports.get(&module).and_then(|m| m.get(&raw.target)) {
                        let target_id = Graph::card_id(&imp.module, &imp.orig);
                        if self.cards.contains_key(&target_id) {
                            chosen = Some(target_id);
                            conf = Confidence::Exact;
                            imported = Some(if imp.module_import {
                                format!("import {}{}", imp.module, if imp.alias { format!(" as {}", imp.local) } else { String::new() })
                            } else {
                                format!(
                                    "from {} import {}{}",
                                    imp.module,
                                    imp.orig,
                                    if imp.alias { format!(" as {}", imp.local) } else { String::new() }
                                )
                            });
                        }
                    } else if let Some(hit) = by_name.get(&raw.target) {
                        if hit.len() == 1 {
                            chosen = Some(hit[0].clone());
                            conf = Confidence::Heur;
                        }
                    }
                }
            }

            if let Some(to) = &chosen {
                if let Some(target_card) = self.cards.get(to) {
                    let params = target_card.params.clone();
                    let mut args = BTreeMap::new();
                    for (i, expr) in raw.pos.iter().enumerate() {
                        if let Some(p) = params.get(i) {
                            args.insert(p.clone(), expr.clone());
                        }
                    }
                    for (k, v) in &raw.kwargs {
                        if params.contains(k) {
                            args.insert(k.clone(), v.clone());
                        }
                    }
                    call.args = args;
                }
            }
            call.to = chosen;
            call.conf = conf;
            call.candidates = candidates;
            call.imported = imported;
            call.typed = typed;
            self.calls.insert(cs_id, call);
        }

        for (card_id, vars) in card_locals {
            if let Some(card) = self.cards.get_mut(&card_id) {
                card.locals = vars;
            }
        }
    }

    fn resolve_class(&self, from_card: &str, raw: &str) -> Option<String> {
        let module = from_card.split(':').next().unwrap_or("");
        let direct = Graph::card_id(module, raw);
        if let Some(c) = self.cards.get(&direct) {
            if c.kind == CardKind::Class {
                return Some(direct);
            }
        }
        if let Some(imp) = self.imports.get(module).and_then(|m| m.get(raw)) {
            let id = Graph::card_id(&imp.module, &imp.orig);
            if let Some(c) = self.cards.get(&id) {
                if c.kind == CardKind::Class {
                    return Some(id);
                }
            }
        }
        self.class_ids
            .values()
            .find(|id| {
                self.cards
                    .get(*id)
                    .map(|c| c.qualname == raw)
                    .unwrap_or(false)
            })
            .cloned()
    }

    fn find_method(&self, class_id: &str, name: &str, depth: usize) -> Option<String> {
        if depth > 6 {
            return None;
        }
        let card = self.cards.get(class_id)?;
        if let Some(mid) = self
            .methods
            .get(&(card.module.clone(), card.qualname.clone()))
            .and_then(|m| m.get(name))
        {
            return Some(mid.clone());
        }
        let bases = self.class_bases.get(class_id)?.1.clone();
        for base in bases {
            if let Some(base_id) = self.class_ids.get(&(card.module.clone(), base)) {
                if let Some(hit) = self.find_method(base_id, name, depth + 1) {
                    return Some(hit);
                }
            }
        }
        None
    }
}

/// True when the module has a top-level `if __name__ == "__main__":` guard.
fn has_main_guard(src: &[u8], root: Node) -> bool {
    let mut cursor = root.walk();
    for node in root.children(&mut cursor) {
        if node.kind() != "if_statement" {
            continue;
        }
        if let Some(cond) = node.child_by_field_name("condition") {
            let text = cond.utf8_text(src).unwrap_or("");
            if text.contains("__name__") && text.contains("__main__") {
                return true;
            }
        }
    }
    false
}

/// Classify the callee part of a call such as `def f():`
fn classify(src: &[u8], func: Node) -> (Option<String>, CallKind, Option<String>) {
    let text = |n: Node| n.utf8_text(src).unwrap_or("").to_string();
    match func.kind() {
        "identifier" => (Some(text(func)), CallKind::Name, None),
        "attribute" => {
            let obj = func.child_by_field_name("object");
            let attr = func.child_by_field_name("attribute");
            let attr_name = attr.map(text);
            match obj {
                Some(o) if o.kind() == "identifier" => {
                    let name = text(o);
                    if name == "self" || name == "cls" {
                        (attr_name, CallKind::SelfRef, Some(name))
                    } else {
                        (attr_name, CallKind::Attr, Some(name))
                    }
                }
                Some(o) if o.kind() == "attribute" => {
                    let inner_obj = o.child_by_field_name("object");
                    let inner_attr = o.child_by_field_name("attribute");
                    if let (Some(io), Some(ia)) = (inner_obj, inner_attr) {
                        if text(io) == "self" {
                            return (attr_name, CallKind::SelfAttr, Some(text(ia)));
                        }
                    }
                    (attr_name, CallKind::Attr, None)
                }
                _ => (attr_name, CallKind::Attr, None),
            }
        }
        _ => (None, CallKind::Other, None),
    }
}

fn collect_calls(stmt: Node) -> Vec<Node> {
    let mut out = Vec::new();
    let mut stack = vec![stmt];
    while let Some(node) = stack.pop() {
        if node.kind() == "call" {
            out.push(node);
        }
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        for ch in children.into_iter().rev() {
            stack.push(ch);
        }
    }
    out.sort_by_key(|n| (n.start_position().row, n.start_position().column));
    out
}

/// Class name inside `x = Class(...)` / `x = mod.Class(...)`
fn class_name_of_call(src: &[u8], call: Node) -> Option<String> {
    let func = call.child_by_field_name("function")?;
    match func.kind() {
        "identifier" => Some(func.utf8_text(src).unwrap_or("").to_string()),
        "attribute" => func
            .child_by_field_name("attribute")
            .map(|n| n.utf8_text(src).unwrap_or("").to_string()),
        _ => None,
    }
}

fn class_of_qualname(qualname: &str) -> Option<String> {
    let mut parts: Vec<&str> = qualname.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    parts.pop();
    Some(parts.join("."))
}

fn short_name(card_id: &str) -> String {
    card_id
        .split(':')
        .nth(1)
        .unwrap_or(card_id)
        .rsplit('.')
        .next()
        .unwrap_or(card_id)
        .to_string()
}
