//! Build the graph (cards + call sites) with tree-sitter for `clew export`.
//!
//! Resolution order: same module, then the import table, then local type
//! inference, then project-wide uniqueness.
//! Deliberately no type-system analysis here (that is ty's job): syntax level only.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use blake3::Hasher;
use rayon::prelude::*;
use tree_sitter::{Node, Parser};

use super::model::{
    Call, CallKind, Card, CardKind, Confidence, Graph, Line, LineKind, MethodRef, SymbolKind,
};
use super::progress::weight;
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
    /// Body of the `if __name__ == "__main__":` guard, keyed by module
    pub module_cards: BTreeMap<String, Vec<Line>>,
}

const BUILTINS: &[&str] = &[
    "print", "len", "range", "str", "int", "float", "bool", "list", "dict", "set", "tuple",
    "sum", "min", "max", "abs", "sorted", "enumerate", "zip", "map", "filter", "open",
    "isinstance", "getattr", "setattr", "hasattr", "super", "type", "round", "repr", "any",
    "all", "iter", "next", "input", "format", "id", "hash", "callable", "vars", "dir",
    "object", "frozenset", "bytes",
];

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

#[derive(Debug, Clone)]
struct RawCall {
    target: String,
    kind: CallKind,
    recv: Option<String>,
    ctor: Option<String>,
    pos: Vec<String>,
    kwargs: Vec<(String, String)>,
    line: u32,
}

#[derive(Clone)]
pub struct Builder {
    src: Vec<u8>,
    module: String,
    /// 这个文件是包的 `__init__.py`。相对 import 的基址要按包算，不能先退一层。
    is_package: bool,
    cards: BTreeMap<String, Card>,
    calls: BTreeMap<String, Call>,
    raw_calls: BTreeMap<String, RawCall>,
    /// module -> local name -> import info
    imports: BTreeMap<String, BTreeMap<String, ImportInfo>>,
    /// card id -> local name -> import info. 函数体里的 import 只对那个函数生效，
    /// 但它同样决定 `skill_ledger.get_entry(...)` 指向哪里。
    card_imports: BTreeMap<String, BTreeMap<String, ImportInfo>>,
    /// (module, class) -> method name -> card id
    methods: BTreeMap<(String, String), BTreeMap<String, String>>,
    /// class card id -> (module, base class names)
    class_bases: BTreeMap<String, (String, Vec<String>)>,
    /// class card id -> attribute name -> raw class name reference
    class_attrs: BTreeMap<String, BTreeMap<String, String>>,
    /// card id -> local variable -> raw class name reference
    card_locals: BTreeMap<String, BTreeMap<String, String>>,
    class_ids: BTreeMap<(String, String), String>,
    /// class qualname -> card id, for the last-resort match in `resolve_class`.
    /// Scanning `class_ids` per lookup is quadratic on a large project.
    class_by_qualname: BTreeMap<String, String>,
    /// module -> body lines of its `__main__` guard
    module_cards: BTreeMap<String, Vec<Line>>,
    /// Counter for the `%N` temporaries, unique within one card
    temp_counter: usize,
    /// Skip card bodies and call sites, keep only definitions and imports.
    /// The GUI uses this for a fast first index and materialises cards later.
    symbols_only: bool,
    /// 外部注入的全项目索引。只解析一个文件时，靠它判断「这个名字全项目唯一」
    /// 与「同名方法有哪些候选」，否则这些调用只能落成未解析。
    pub(crate) global_by_name: Option<Arc<BTreeMap<String, Vec<String>>>>,
    pub(crate) global_by_method: Option<Arc<BTreeMap<String, Vec<String>>>>,
    /// 全项目 bare 类名 -> 类卡 id。按需物化时目标类不在场，靠它判断类型。
    pub(crate) global_by_class: Option<Arc<BTreeMap<String, Vec<String>>>>,
    /// 全项目 qualname -> 卡 id，只收类与方法。类卡不在场时查方法用。
    pub(crate) global_by_qualname: Option<Arc<BTreeMap<String, Vec<String>>>>,
    /// rows to persist
    out_modules: Vec<ModuleMeta>,
    out_symbols: Vec<SymbolMeta>,
    out_classes: Vec<ClassMeta>,
}

pub fn build(root: &Path) -> anyhow::Result<Built> {
    build_with(root, false, true, &mut |_, _, _, _| {})
}

pub fn build_with(
    root: &Path,
    include_venv: bool,
    emit_cards: bool,
    report: &mut dyn FnMut(&'static str, u64, u64, f32),
) -> anyhow::Result<Built> {
    let t_all = Instant::now();
    let root = std::fs::canonicalize(root)?;

    let t_walk = Instant::now();
    let files = walker::collect_with(&root, include_venv);
    let d_walk = t_walk.elapsed();
    report(
        "枚举文件",
        files.len() as u64,
        files.len() as u64,
        weight::WALK.1,
    );

    // 每个文件互相独立，解析与提取按核并行，之后合并再统一解析调用。
    // 每块 500 个文件报告一次进度，并行期间不至于让进度条停住。
    let d_read = AtomicU64::new(0);
    let d_parse = AtomicU64::new(0);
    let d_emit = AtomicU64::new(0);
    let bytes = AtomicU64::new(0);

    let mut builder = Builder::default();
    builder.symbols_only = !emit_cards;

    const CHUNK: usize = 500;
    let mut done = 0usize;
    for batch in files.chunks(CHUNK) {
        let parts: Vec<Builder> = batch
            .par_iter()
            .map_init(new_parser, |parser, file| {
                let mut part = Builder::default();
                part.symbols_only = !emit_cards;

                let t = Instant::now();
                let Ok(source) = std::fs::read_to_string(&file.path) else {
                    return part;
                };
                bytes.fetch_add(source.len() as u64, Ordering::Relaxed);
                d_read.fetch_add(nanos(t.elapsed()), Ordering::Relaxed);

                let t = Instant::now();
                let tree = match parser.parse(&source, None) {
                    Some(t) => t,
                    None => return part,
                };
                d_parse.fetch_add(nanos(t.elapsed()), Ordering::Relaxed);

                let mut hasher = Hasher::new();
                hasher.update(source.as_bytes());
                let src_hash = hasher.finalize().to_hex().to_string();

                let t = Instant::now();
                part.file(file, source.as_bytes(), tree.root_node(), src_hash);
                d_emit.fetch_add(nanos(t.elapsed()), Ordering::Relaxed);
                part
            })
            .collect();

        for part in parts {
            builder.merge(part);
        }
        done += batch.len();
        report(
            "解析源码",
            done as u64,
            files.len() as u64,
            weight::lerp(weight::PARSE, done as u64, files.len() as u64),
        );
    }

    let (d_read, d_parse, d_emit, bytes) = (
        Duration::from_nanos(d_read.load(Ordering::Relaxed)),
        Duration::from_nanos(d_parse.load(Ordering::Relaxed)),
        Duration::from_nanos(d_emit.load(Ordering::Relaxed)),
        bytes.load(Ordering::Relaxed) as usize,
    );

    let t = Instant::now();
    report("解析调用", 0, 1, weight::RESOLVE.0);
    builder.resolve();
    let d_resolve = t.elapsed();
    report("解析调用", 1, 1, weight::RESOLVE.1);
    eprintln!(
        "[t] stages files={} bytes={} walk {:.1}ms read {:.1}ms parse {:.1}ms emit {:.1}ms resolve {:.1}ms total {:.1}ms",
        files.len(),
        bytes,
        d_walk.as_secs_f64() * 1000.0,
        d_read.as_secs_f64() * 1000.0,
        d_parse.as_secs_f64() * 1000.0,
        d_emit.as_secs_f64() * 1000.0,
        d_resolve.as_secs_f64() * 1000.0,
        t_all.elapsed().as_secs_f64() * 1000.0
    );

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
        module_cards: builder.module_cards,
    })
}

impl Default for Builder {    fn default() -> Self {
        Self {
            src: Vec::new(),
            module: String::new(),
            is_package: false,
            cards: BTreeMap::new(),
            calls: BTreeMap::new(),
            raw_calls: BTreeMap::new(),
            imports: BTreeMap::new(),
            card_imports: BTreeMap::new(),
            methods: BTreeMap::new(),
            class_bases: BTreeMap::new(),
            class_attrs: BTreeMap::new(),
            card_locals: BTreeMap::new(),
            class_ids: BTreeMap::new(),
            class_by_qualname: BTreeMap::new(),
            module_cards: BTreeMap::new(),
            temp_counter: 0,
            symbols_only: false,
            global_by_name: None,
            global_by_method: None,
            global_by_class: None,
            global_by_qualname: None,
            out_modules: Vec::new(),
            out_symbols: Vec::new(),
            out_classes: Vec::new(),
        }
    }
}

impl Builder {
    /// A builder that emits full cards, used by on-demand materialisation.
    pub(crate) fn for_cards() -> Self {
        Self::default()
    }

    pub(crate) fn cards(&self) -> &BTreeMap<String, Card> {
        &self.cards
    }

    pub(crate) fn calls(&self) -> &BTreeMap<String, Call> {
        &self.calls
    }

    /// `__main__` guard bodies, keyed by module. They belong to the module entry
    /// pseudo symbol rather than to a card of their own.
    pub(crate) fn module_cards(&self) -> &BTreeMap<String, Vec<Line>> {
        &self.module_cards
    }

    /// Modules this file imports from, used to decide what to parse alongside it.
    pub(crate) fn imported_modules(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for table in self.imports.values() {
            for info in table.values() {
                out.insert(info.module.clone());
            }
        }
        out
    }

    /// Fold another builder's per-file results into this one.
    ///
    /// Files are parsed in parallel, each into its own builder, then merged
    /// here before resolution. Every cross-file map keys on module names, so
    /// extending them is enough.
    pub(crate) fn merge(&mut self, other: Builder) {
        self.cards.extend(other.cards);
        self.calls.extend(other.calls);
        self.raw_calls.extend(other.raw_calls);
        for (k, v) in other.imports {
            self.imports.entry(k).or_default().extend(v);
        }
        for (k, v) in other.methods {
            self.methods.entry(k).or_default().extend(v);
        }
        self.class_bases.extend(other.class_bases);
        for (k, v) in other.class_attrs {
            self.class_attrs.entry(k).or_default().extend(v);
        }
        for (k, v) in other.card_locals {
            self.card_locals.entry(k).or_default().extend(v);
        }
        for (k, v) in other.card_imports {
            self.card_imports.entry(k).or_default().extend(v);
        }
        self.class_ids.extend(other.class_ids);
        for (k, v) in other.class_by_qualname {
            self.class_by_qualname.entry(k).or_insert(v);
        }
        for (k, v) in other.module_cards {
            self.module_cards.entry(k).or_default().extend(v);
        }
        self.out_modules.extend(other.out_modules);
        self.out_symbols.extend(other.out_symbols);
        self.out_classes.extend(other.out_classes);
    }

    fn text(&self, node: Node) -> String {
        node.utf8_text(&self.src).unwrap_or("").to_string()
    }

    fn clipped(&self, raw: &str) -> String {
        let line = raw.lines().next().unwrap_or("").trim();
        if line.chars().count() > LINE_LIMIT {
            let mut s: String = line.chars().take(LINE_LIMIT - 1).collect();
            s.push('…');
            s
        } else {
            line.to_string()
        }
    }

    fn first_line(&self, node: Node) -> String {
        self.clipped(&self.text(node))
    }

    /// Source text of `start..end` with the given call spans replaced.
    ///
    /// The spans may nest (`f(g(h()))`), so the text is assembled from the
    /// original bytes with a cursor instead of edited in place: replacing an
    /// inner span shortens the string and would leave every enclosing span on
    /// stale offsets, which panics as soon as the shift lands inside a
    /// multibyte character.
    fn text_replacing(
        &self,
        start: usize,
        end: usize,
        mut repls: Vec<(usize, usize, String)>,
    ) -> String {
        repls.sort_by_key(|(s, _, _)| *s);
        let mut out = String::new();
        let mut cursor = start;
        for (s, e, name) in repls {
            // an inner span covered by one already written out leaves nothing to do
            if s < cursor || e > end {
                continue;
            }
            out.push_str(&String::from_utf8_lossy(&self.src[cursor..s]));
            out.push_str(&name);
            cursor = e;
        }
        out.push_str(&String::from_utf8_lossy(&self.src[cursor..end]));
        out
    }

    /// Expand one statement into its evaluation order.
    ///
    /// Every nested call is lifted into `%0 = inner(…)` and substituted back into
    /// its parent, so the card reads top-down in the order Python evaluates it:
    /// innermost first, the enclosing call last.
    fn emit_call_sequence(
        &mut self,
        stmt: Node,
        calls: &[Node],
        owner: &str,
        indent: u8,
        ln: u32,
        out: &mut Vec<Line>,
    ) {
        self.emit_calls_span(
            stmt.start_byte(),
            stmt.end_byte(),
            LineKind::Call,
            LineKind::Stmt,
            calls,
            owner,
            indent,
            ln,
            out,
        );
    }

    /// The same expansion over an arbitrary source span: a statement, or just
    /// the header of a compound statement.
    ///
    /// `own` is the kind of the last line when it carries a call itself,
    /// `plain` the kind when it only shows the substituted text.
    #[allow(clippy::too_many_arguments)]
    fn emit_calls_span(
        &mut self,
        start: usize,
        end: usize,
        own: LineKind,
        plain: LineKind,
        calls: &[Node],
        owner: &str,
        indent: u8,
        ln: u32,
        out: &mut Vec<Line>,
    ) {
        let parents = call_parents(calls);
        let mut roots = Vec::new();
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); calls.len()];
        for (i, parent) in parents.iter().enumerate() {
            match parent {
                Some(p) => children[*p].push(i),
                None => roots.push(i),
            }
        }

        let mut order = Vec::new();
        for &root in &roots {
            postorder(root, &children, &mut order);
        }

        // a lone top-level call keeps the statement line as its own row
        let statement_call = if roots.len() == 1 { Some(roots[0]) } else { None };

        let mut name_of: HashMap<usize, String> = HashMap::new();
        for &i in &order {
            if statement_call == Some(i) {
                continue;
            }
            name_of.insert(i, format!("%{}", self.temp_counter));
            self.temp_counter += 1;
        }

        for &i in &order {
            let Some(name) = name_of.get(&i).cloned() else {
                continue;
            };
            let repls = repls_within(
                calls,
                &name_of,
                Some(i),
                calls[i].start_byte(),
                calls[i].end_byte(),
            );
            let body = self.text_replacing(calls[i].start_byte(), calls[i].end_byte(), repls);
            let cs_id = format!("{owner}#{}", out.len());
            self.record_call(cs_id.clone(), owner, calls[i]);
            out.push(Line {
                k: LineKind::Call,
                text: format!("{name} = {}", self.clipped(&body)),
                indent,
                ln,
                cs: Some(cs_id),
            });
        }

        let repls = repls_within(calls, &name_of, None, start, end);
        let body = self.text_replacing(start, end, repls);
        let text = self.clipped(&body);
        match statement_call {
            Some(root) => {
                let cs_id = format!("{owner}#{}", out.len());
                self.record_call(cs_id.clone(), owner, calls[root]);
                out.push(Line {
                    k: own,
                    text,
                    indent,
                    ln,
                    cs: Some(cs_id),
                });
            }
            None => out.push(Line {
                k: plain,
                text,
                indent,
                ln,
                cs: None,
            }),
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

    pub(crate) fn file(&mut self, file: &walker::PyFile, src: &[u8], root: Node, src_hash: String) {
        self.src = src.to_vec();
        self.module = file.module.clone();
        self.is_package = file
            .path
            .file_name()
            .map(|n| n == "__init__.py")
            .unwrap_or(false);

        let mut cursor = root.walk();
        let mut main_guard = false;
        for node in root.children(&mut cursor) {
            if node.kind() == "if_statement" && is_main_guard(&self.src, node) {
                main_guard = true;
                if !self.symbols_only {
                    self.module_body(node);
                }
            }
        }

        // Imports and definitions anywhere the module scope reaches. A `def`
        // under `if`/`for`/`while`/`with`/`try` is still module level, so the
        // walk descends through those statements instead of reading the root's
        // children one by one.
        for item in scope_items(
            root,
            &[
                "import_statement",
                "import_from_statement",
                "function_definition",
                "class_definition",
            ],
        ) {
            match item.kind() {
                "import_statement" => {
                    let found = self.collect_import(item);
                    self.imports
                        .entry(self.module.clone())
                        .or_default()
                        .extend(found);
                }
                "import_from_statement" => {
                    let found = self.collect_import_from(item);
                    self.imports
                        .entry(self.module.clone())
                        .or_default()
                        .extend(found);
                }
                "function_definition" => self.add_function(item, "", false),
                "class_definition" => self.add_class(item, ""),
                _ => {}
            }
        }

        self.out_modules.push(ModuleMeta {
            module: file.module.clone(),
            path: file.path.clone(),
            src_hash,
            is_third_party: file.is_third_party,
            has_main_guard: main_guard,
        });
    }

    /// The `__main__` guard body becomes the card of the module entry symbol.
    fn module_body(&mut self, guard: Node) {
        let owner = Graph::card_id(&self.module, super::entry::MODULE_ENTRY_QUALNAME);
        let mut lines = Vec::new();
        let mut locals = BTreeMap::new();
        self.temp_counter = 0;
        // `if_statement` uses `consequence`, not a `body` field
        if let Some(body) = first_block(guard) {
            self.emit_block(body, &owner, 0, &mut lines, &mut locals, "");
        }
        if !lines.is_empty() {
            self.module_cards.insert(self.module.clone(), lines);
        }
        // module-level `n1 = NNetWrapper(g)` must feed call resolution too
        if !locals.is_empty() {
            self.card_locals.insert(owner, locals);
        }
    }

    // ── imports ───────────────────────────────────────────────────
    /// 相对 import 的基址。`from .x import y` 在普通模块里先退掉自己的模块名，
    /// 在包的 `__init__.py` 里则停在包本身：`pkg/__init__.py` 的 `.session`
    /// 是 `pkg.session`，少算一层会指到一个并不存在的模块。
    fn relative_base(&self, dots: usize) -> String {
        let mut parts: Vec<&str> = self.module.split('.').collect();
        if !self.is_package {
            parts.pop();
        }
        for _ in 1..dots {
            parts.pop();
        }
        parts.join(".")
    }

    fn collect_import(&mut self, node: Node) -> BTreeMap<String, ImportInfo> {
        let mut out = BTreeMap::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "dotted_name" => {
                    let full = self.text(child);
                    let local = full.split('.').next().unwrap_or(&full).to_string();
                    out.insert(
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
                        out.insert(
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
        out
    }

    fn collect_import_from(&mut self, node: Node) -> BTreeMap<String, ImportInfo> {
        let mut out = BTreeMap::new();
        let module_node = node.child_by_field_name("module_name");
        let mut base = module_node.map(|n| self.text(n)).unwrap_or_default();

        // module_name itself is relative (from .Board import Board)
        if base.starts_with('.') {
            let dots = base.chars().take_while(|c| *c == '.').count();
            let rest = base.trim_start_matches('.').to_string();
            let up = self.relative_base(dots);
            base = if rest.is_empty() {
                up
            } else if up.is_empty() {
                rest
            } else {
                format!("{}.{}", up, rest)
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
                    let up = self.relative_base(dots);
                    base = if rest.is_empty() {
                        up
                    } else if up.is_empty() {
                        rest
                    } else {
                        format!("{}.{}", up, rest)
                    };
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
                    out.insert(
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
                        out.insert(
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
        out
    }

    // ── functions and methods ─────────────────────────────────────
    /// `scope` is the qualname of the enclosing scope (empty at module level),
    /// so a nested `def` lands at `outer.inner` instead of `inner`.
    fn add_function(&mut self, node: Node, scope: &str, in_class: bool) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node);
        let qualname = if scope.is_empty() {
            name.clone()
        } else {
            format!("{scope}.{name}")
        };
        let id = Graph::card_id(&self.module, &qualname);
        let kind = if in_class {
            CardKind::Method
        } else {
            CardKind::Function
        };

        // 只建符号时到此为止：函数体留给按需物化
        if self.symbols_only {
            self.push_symbol(node, &qualname, SymbolKind::of_card(kind));
            if in_class {
                self.methods
                    .entry((self.module.clone(), scope.to_string()))
                    .or_default()
                    .insert(name, id);
            }
            // the fast scan never walks bodies, so a `def` nested inside this
            // one has to be picked up right here
            if let Some(body) = node.child_by_field_name("body") {
                for def in scope_items(body, &["function_definition", "class_definition"]) {
                    match def.kind() {
                        "function_definition" => self.add_function(def, &qualname, false),
                        "class_definition" => self.add_class(def, &qualname),
                        _ => {}
                    }
                }
            }
            return;
        }

        let params = self.params_of(node);
        self.temp_counter = 0;
        let mut lines = Vec::new();
        let mut locals = BTreeMap::new();
        if let Some(body) = node.child_by_field_name("body") {
            self.emit_block(body, &id, 0, &mut lines, &mut locals, &qualname);
        }
        let title = format!("{qualname}({})", params.join(", "));

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
                bases: Vec::new(),
                ctor: None,
                ctor_owner: None,
                external_base: None,
                locals: BTreeMap::new(),
            },
        );
        self.push_symbol(node, &qualname, SymbolKind::of_card(kind));
        if in_class {
            self.methods
                .entry((self.module.clone(), scope.to_string()))
                .or_default()
                .insert(name, id.clone());
        }
        if !locals.is_empty() {
            self.card_locals.insert(id, locals);
        }
    }

    fn add_class(&mut self, node: Node, scope: &str) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = self.text(name_node);
        let qualname = if scope.is_empty() {
            name.clone()
        } else {
            format!("{scope}.{name}")
        };
        let id = Graph::card_id(&self.module, &qualname);
        self.temp_counter = 0;

        let mut bases = Vec::new();
        if let Some(supers) = node.child_by_field_name("superclasses") {
            let mut cursor = supers.walk();
            for ch in supers.children(&mut cursor) {
                // `Base` is an identifier, `mod.Base` / `unittest.TestCase` an attribute
                if matches!(ch.kind(), "identifier" | "attribute") {
                    bases.push(self.text(ch));
                }
            }
        }

        // Methods and nested classes anywhere the class body reaches:
        // `if TYPE_CHECKING:` and `try: ... except ImportError:` do not leave
        // class scope, so their `def`s are still methods.
        let mut method_defs: Vec<Node> = Vec::new();
        let mut nested_classes: Vec<Node> = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            for def in scope_items(body, &["function_definition", "class_definition"]) {
                if def.kind() == "class_definition" {
                    nested_classes.push(def);
                } else {
                    method_defs.push(def);
                }
            }
        }

        // 只建符号时：类体不展开，方法只登记名字与位置
        if self.symbols_only {
            self.push_symbol(node, &qualname, SymbolKind::Class);
            self.class_ids
                .insert((self.module.clone(), qualname.clone()), id.clone());
            self.class_by_qualname
                .entry(qualname.clone())
                .or_insert_with(|| id.clone());
            self.class_bases
                .insert(id.clone(), (self.module.clone(), bases.clone()));
            for def in &method_defs {
                self.add_function(*def, &qualname, true);
            }
            for def in &nested_classes {
                self.add_class(*def, &qualname);
            }
            return;
        }

        let mut methods = Vec::new();
        let mut ctor_params: Vec<String> = Vec::new();
        let mut ctor_lines = Vec::new();
        let mut ctor_locals = BTreeMap::new();
        let mut ctor_attrs: BTreeMap<String, String> = BTreeMap::new();
        let mut ctor_id: Option<String> = None;

        for def in &method_defs {
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
                ctor_params = self.params_of(*def);
                if let Some(b) = def.child_by_field_name("body") {
                    self.emit_block(b, &id, 0, &mut ctor_lines, &mut ctor_locals, &qualname);
                }
                self.collect_self_attrs(*def, &mut ctor_attrs);
            }
        }

        let title = format!("{name}({})", ctor_params.join(", "));
        self.cards.insert(
            id.clone(),
            Card {
                id: id.clone(),
                module: self.module.clone(),
                qualname: qualname.clone(),
                title,
                params: ctor_params,
                kind: CardKind::Class,
                lines: ctor_lines,
                methods,
                bases: bases.clone(),
                ctor: None,
                ctor_owner: None,
                external_base: None,
                locals: BTreeMap::new(),
            },
        );
        self.push_symbol(node, &qualname, SymbolKind::Class);
        self.out_classes.push(ClassMeta {
            card_id: id.clone(),
            module: self.module.clone(),
            qualname: qualname.clone(),
            bases: bases.clone(),
            ctor: ctor_id,
        });
        self.class_ids
            .insert((self.module.clone(), qualname.clone()), id.clone());
        self.class_by_qualname
            .entry(qualname.clone())
            .or_insert_with(|| id.clone());
        self.class_bases
            .insert(id.clone(), (self.module.clone(), bases));
        self.class_attrs.insert(id.clone(), ctor_attrs);
        if !ctor_locals.is_empty() {
            self.card_locals.insert(id.clone(), ctor_locals);
        }

        // method bodies inside a class become cards of their own
        for def in &method_defs {
            self.add_function(*def, &qualname, true);
        }
        for def in &nested_classes {
            self.add_class(*def, &qualname);
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
        scope: &str,
    ) {
        let mut cursor = block.walk();
        for stmt in block.children(&mut cursor) {
            // A `def`/`class` inside the body owns its lines in a card of its
            // own; the enclosing card keeps just the definition line, and the
            // nested definition is registered under the enclosing qualname.
            if let Some(def) = definition_of(stmt) {
                let saved = self.temp_counter;
                out.push(Line {
                    k: LineKind::Branch,
                    text: self.first_line(stmt),
                    indent,
                    ln: stmt.start_position().row as u32 + 1,
                    cs: None,
                });
                match def.kind() {
                    "function_definition" => self.add_function(def, scope, false),
                    "class_definition" => self.add_class(def, scope),
                    _ => {}
                }
                // the nested card numbers its own `%N`, so the enclosing card
                // keeps counting where it left off
                self.temp_counter = saved;
                continue;
            }

            if is_compound(stmt.kind()) {
                self.emit_compound(stmt, owner, indent, out, locals, scope);
                continue;
            }

            match stmt.kind() {
                "block" => self.emit_block(stmt, owner, indent, out, locals, scope),
                _ => {
                    // 函数体里的 import 只对这个函数生效，但解析 `mod.func()` 要用到它
                    if stmt.kind() == "import_statement" || stmt.kind() == "import_from_statement" {
                        let found = if stmt.kind() == "import_statement" {
                            self.collect_import(stmt)
                        } else {
                            self.collect_import_from(stmt)
                        };
                        if !found.is_empty() {
                            self.card_imports
                                .entry(owner.to_string())
                                .or_default()
                                .extend(found);
                        }
                    }
                    let calls = collect_calls(stmt);
                    let ln = stmt.start_position().row as u32 + 1;
                    if calls.is_empty() {
                        out.push(Line {
                            k: LineKind::Stmt,
                            text: self.first_line(stmt),
                            indent,
                            ln,
                            cs: None,
                        });
                    } else {
                        self.emit_call_sequence(stmt, &calls, owner, indent, ln, out);
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

    /// One compound statement: its header line, then every body and clause.
    ///
    /// The calls in the header (`if check():`, `while gate():`,
    /// `with suppress(err):`, `for x in fetch():`) are call sites of the same
    /// card, so the header line can be anchored just like a plain statement.
    /// `indent` is the column of the header itself: `if`/`elif`/`else` share it
    /// and only their bodies go one level deeper.
    fn emit_compound(
        &mut self,
        node: Node,
        owner: &str,
        indent: u8,
        out: &mut Vec<Line>,
        locals: &mut BTreeMap<String, String>,
        scope: &str,
    ) {
        let ln = node.start_position().row as u32 + 1;
        // the header ends where the first body or clause begins
        let mut probe = node.walk();
        let body_start = node
            .children(&mut probe)
            .find(|c| c.kind() == "block" || is_clause(c.kind()))
            .map(|c| c.start_byte())
            .unwrap_or_else(|| node.end_byte());
        let header_calls: Vec<Node> = collect_calls(node)
            .into_iter()
            .filter(|c| c.end_byte() <= body_start)
            .collect();
        if header_calls.is_empty() {
            out.push(Line {
                k: LineKind::Branch,
                text: self.first_line(node),
                indent,
                ln,
                cs: None,
            });
        } else {
            self.emit_calls_span(
                node.start_byte(),
                body_start,
                LineKind::Branch,
                LineKind::Branch,
                &header_calls,
                owner,
                indent,
                ln,
                out,
            );
        }

        let mut cursor = node.walk();
        for ch in node.children(&mut cursor) {
            if ch.kind() == "block" {
                self.emit_block(ch, owner, indent + 1, out, locals, scope);
            } else if is_clause(ch.kind()) {
                // `elif`/`else`/`except` stay at the header's column
                self.emit_compound(ch, owner, indent, out, locals, scope);
            }
        }
    }

    fn record_call(&mut self, cs_id: String, owner: &str, call: Node) {
        // keep the call site even when the callee is unknown, so the anchor
        // never points at a missing record
        let (target, kind, recv, ctor) = match call.child_by_field_name("function") {
            Some(func) => {
                let c = classify(&self.src, func);
                (c.target, c.kind, c.recv, c.ctor)
            }
            None => (None, CallKind::Other, None, None),
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
                ctor,
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
    pub(crate) fn resolve(&mut self) {
        // 注入的全项目索引优先。按需物化一个文件时，本地的卡片只有几十张，
        // 靠它判断唯一性会把「全项目有十个同名函数」看成唯一。
        let by_name: Arc<BTreeMap<String, Vec<String>>> = match &self.global_by_name {
            Some(shared) => shared.clone(),
            None => Arc::new({
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
            }),
        };

        // Method lookup by name, built once.
        //
        // Walking `self.methods` per call site is what made resolution quadratic:
        // on a 220k-symbol project every `obj.x()` scanned the whole method table.
        // These two indexes turn each lookup into a single map hit.
        let (by_module_method, local_by_method) = {
            let mut local: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
            let mut global: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for ((module, _class), ms) in &self.methods {
                for (name, id) in ms {
                    local
                        .entry((module.clone(), name.clone()))
                        .or_default()
                        .push(id.clone());
                    global.entry(name.clone()).or_default().push(id.clone());
                }
            }
            for v in local.values_mut() {
                v.sort();
                v.dedup();
            }
            for v in global.values_mut() {
                v.sort();
                v.dedup();
            }
            (local, global)
        };
        let by_method: Arc<BTreeMap<String, Vec<String>>> = match &self.global_by_method {
            Some(shared) => shared.clone(),
            None => Arc::new(local_by_method),
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
            // Only the qualname is needed while resolving; cloning the whole card
            // (with its rendered lines) for every call site is wasted work.
            let card_qualname: Option<String> =
                self.cards.get(&call.from).map(|c| c.qualname.clone());

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
                        if let Some(qn) = &card_qualname {
                            if let Some(cls) = class_of_qualname(qn) {
                                if let Some(mid) = self
                                    .methods
                                    .get(&(module.clone(), cls.clone()))
                                    .and_then(|m| m.get(&raw.target))
                                {
                                    // methods may be overridden in subclasses: expose every candidate.
                                    // Copy at most MAX_CANDIDATES: a name like `run` is defined
                                    // thousands of times, and cloning the whole list per call site
                                    // costs more than the scan it replaced.
                                    let cands = by_module_method
                                        .get(&(module.clone(), raw.target.clone()));
                                    chosen = Some(mid.clone());
                                    match cands.map(|c| c.len()).unwrap_or(0) {
                                        n if n > 1 => {
                                            conf = Confidence::Poly;
                                            candidates = cands
                                                .map(|c| {
                                                    c.iter()
                                                        .take(MAX_CANDIDATES)
                                                        .cloned()
                                                        .collect()
                                                })
                                                .unwrap_or_default();
                                        }
                                        _ => conf = Confidence::Exact,
                                    }
                                }
                            }
                        }
                    }
                    CallKind::SelfAttr => {
                        if let (Some(qn), Some(attr)) = (&card_qualname, &raw.recv) {
                            if let Some(cls) = class_of_qualname(qn) {
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
                        // 接收者是导入进来的模块。`from tools import skill_ledger` 之后
                        // `skill_ledger.get_entry(...)` 指的是那个模块的顶层定义，不是某个
                        // 同名类的方法。只按方法名在全项目里挑，会挑到别人的测试替身。
                        if chosen.is_none() {
                            if let Some(recv) = &raw.recv {
                                if let Some(imp) = self.import_of(&call.from, &module, recv).cloned()
                                {
                                    // `import a.b` 之后 a.b.f，接收者就是模块 a.b。
                                    // `from a import b` 之后 b.f，b 可能是子模块 a.b，也可能是 a 里的名字。
                                    let owners: Vec<String> = if imp.module_import {
                                        vec![imp.module.clone()]
                                    } else {
                                        vec![
                                            format!("{}.{}", imp.module, imp.orig),
                                            imp.module.clone(),
                                        ]
                                    };
                                    for owner in &owners {
                                        let want = Graph::card_id(owner, &raw.target);
                                        if self.card_known(&want, &raw.target, by_name.as_ref()) {
                                            chosen = Some(want);
                                            conf = Confidence::Exact;
                                            imported = Some(import_label(&imp));
                                            break;
                                        }
                                    }
                                    if chosen.is_none() {
                                        let cls_id = Graph::card_id(&imp.module, &imp.orig);
                                        if let Some(mid) = self.find_method(&cls_id, &raw.target, 0)
                                        {
                                            chosen = Some(mid);
                                            conf = Confidence::Exact;
                                            imported = Some(import_label(&imp));
                                        }
                                    }
                                }
                            }
                        }
                        // 接收者是构造调用：`Path().resolve()` 的类型就是 Path。
                        // 项目里没有这个类（标准库或第三方）就到此为止，别去猜，
                        // 但要把它来自哪个模块记下来，界面上标成外部类型。
                        if chosen.is_none() {
                            if let Some(ctor) = &raw.ctor {
                                match self.resolve_class(&call.from, ctor) {
                                    Some(cls_id) => {
                                        if let Some(mid) = self.find_method(&cls_id, &raw.target, 0)
                                        {
                                            chosen = Some(mid);
                                            conf = Confidence::Exact;
                                            typed = Some(format!(
                                                "{ctor}() → {}",
                                                short_name(&cls_id)
                                            ));
                                        }
                                    }
                                    None => {
                                        if let Some(imp) =
                                            self.import_of(&call.from, &module, ctor)
                                        {
                                            typed = Some(format!(
                                                "{ctor}() → 外部（{}）",
                                                imp.module
                                            ));
                                            imported = Some(import_label(imp));
                                        }
                                    }
                                }
                            }
                        }
                        if chosen.is_none() {
                            if let Some(list) = by_method.get(&raw.target) {
                                if list.len() == 1 {
                                    chosen = Some(list[0].clone());
                                    conf = Confidence::Heur;
                                } else if list.len() > 1 {
                                    // 接收者类型未知，同名方法又不止一个：只给候选，
                                    // 不挑一个装作解析到了。挑第一个就是在瞎猜。
                                    candidates =
                                        list.iter().take(MAX_CANDIDATES).cloned().collect();
                                    conf = Confidence::Poly;
                                }
                            }
                        }
                    }
                    _ => {}
                }

                if chosen.is_none() && raw.kind == CallKind::Name {
                    let same = Graph::card_id(&module, &raw.target);
                    // import 命中但目标卡不在场（按需模式，或重导出）时不能就地收手，
                    // 下面还有按全项目名字唯一命中的兜底。
                    let mut label_from_import = None;
                    if self.cards.contains_key(&same) {
                        chosen = Some(same);
                        conf = Confidence::Exact;
                    } else if let Some(imp) = self.import_of(&call.from, &module, &raw.target) {
                        let target_id = Graph::card_id(&imp.module, &imp.orig);
                        if self.card_known(&target_id, &raw.target, by_name.as_ref()) {
                            chosen = Some(target_id);
                            conf = Confidence::Exact;
                        }
                        label_from_import = Some(import_label(imp));
                    }
                    if chosen.is_none() {
                        if let Some(hit) = by_name.get(&raw.target) {
                            if hit.len() == 1 {
                                chosen = Some(hit[0].clone());
                                conf = Confidence::Heur;
                            }
                        }
                    }
                    if imported.is_none() {
                        // 解析不到也要留下来源：`Path(__file__)` 这类名字至少让人看出
                        // 它来自外部模块，不是项目里丢了定义
                        imported = label_from_import;
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

    /// 这个 id 是不是一个类。类卡可能还没物化（按需模式下只读了几个文件），
    /// 那就查全项目的类表。
    fn is_class(&self, id: &str) -> bool {
        if let Some(c) = self.cards.get(id) {
            return c.kind == CardKind::Class;
        }
        let bare = id
            .split_once(':')
            .map(|(_, q)| q)
            .unwrap_or(id)
            .rsplit('.')
            .next()
            .unwrap_or(id);
        self.global_by_class
            .as_ref()
            .and_then(|m| m.get(bare))
            .map_or(false, |ids| ids.iter().any(|x| x == id))
    }

    /// bare 类名 -> 类卡 id，只接受唯一命中。
    fn global_class(&self, name: &str) -> Option<String> {
        let ids = self.global_by_class.as_ref()?.get(name)?;
        if ids.len() == 1 {
            return Some(ids[0].clone());
        }
        None
    }

    fn resolve_class(&self, from_card: &str, raw: &str) -> Option<String> {
        let module = from_card.split(':').next().unwrap_or("");
        let direct = Graph::card_id(module, raw);
        if self.is_class(&direct) {
            return Some(direct);
        }
        if let Some(imp) = self.import_of(from_card, module, raw) {
            let id = Graph::card_id(&imp.module, &imp.orig);
            if self.is_class(&id) {
                return Some(id);
            }
        }
        self.class_by_qualname
            .get(raw)
            .cloned()
            .or_else(|| self.global_class(raw))
    }

    /// 名称在这个位置解析到哪个导入：先看所在卡片自己的 import，
    /// 再看模块的。函数体里 `from tools import skill_ledger` 不会出现在模块表里。
    fn import_of(&self, from_card: &str, module: &str, name: &str) -> Option<&ImportInfo> {
        self.card_imports
            .get(from_card)
            .and_then(|m| m.get(name))
            .or_else(|| self.imports.get(module).and_then(|m| m.get(name)))
    }

    /// 这张卡在项目里存在吗。按需物化时别的模块还没解析，只看 `self.cards`
    /// 会把项目里确实存在的目标判成不存在，所以再查一次全项目的名字表。
    fn card_known(&self, id: &str, bare: &str, by_name: &BTreeMap<String, Vec<String>>) -> bool {
        if self.cards.contains_key(id) {
            return true;
        }
        by_name
            .get(bare)
            .map_or(false, |ids| ids.iter().any(|x| x == id))
    }

    fn find_method(&self, class_id: &str, name: &str, depth: usize) -> Option<String> {
        if depth > 6 {
            return None;
        }
        let Some(card) = self.cards.get(class_id) else {
            // 类卡还没物化：按全项目的 qualname 直接找这个方法，
            // 只接受唯一命中，继承链这时候查不到。
            let qualname = class_id.split_once(':').map(|(_, q)| q)?;
            let want = format!("{qualname}.{name}");
            let ids = self.global_by_qualname.as_ref()?.get(&want)?;
            return if ids.len() == 1 {
                Some(ids[0].clone())
            } else {
                None
            };
        };
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

/// First `block` child of a compound statement.
/// One parser per worker thread, built once by `map_init`.
pub(crate) fn new_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .expect("failed to load tree-sitter-python");
    parser
}

fn nanos(d: Duration) -> u64 {
    d.as_nanos().min(u64::MAX as u128) as u64
}

fn first_block(node: Node) -> Option<Node> {    let mut cursor = node.walk();
    node.children(&mut cursor).find(|c| c.kind() == "block")
}

/// The `def`/`class` node a statement defines, decorated or not.
fn definition_of(node: Node) -> Option<Node> {
    match node.kind() {
        "function_definition" | "class_definition" => Some(node),
        "decorated_definition" => node.child_by_field_name("definition"),
        _ => None,
    }
}

/// Statements that keep the enclosing scope instead of opening a new one.
fn is_scope_container(kind: &str) -> bool {
    matches!(
        kind,
        "module"
            | "block"
            | "if_statement"
            | "for_statement"
            | "while_statement"
            | "with_statement"
            | "try_statement"
            | "match_statement"
    ) || is_clause(kind)
}

/// The trailing parts of a compound statement: `else`, `elif`, `except`,
/// `finally`, `case`.
fn is_clause(kind: &str) -> bool {
    matches!(
        kind,
        "elif_clause"
            | "else_clause"
            | "except_clause"
            | "except_group_clause"
            | "finally_clause"
            | "case_clause"
    )
}

/// Compound statements and their clauses: `emit_compound` handles these.
fn is_compound(kind: &str) -> bool {
    matches!(
        kind,
        "if_statement"
            | "for_statement"
            | "while_statement"
            | "with_statement"
            | "try_statement"
            | "match_statement"
    ) || is_clause(kind)
}

/// Definitions and imports directly visible in one scope, in source order.
///
/// Descends through `if`/`for`/`while`/`with`/`try`/`match` and their clauses
/// because those do not open a scope, and stops at function bodies: a `def`
/// inside a `def` belongs to that def, not here.
fn scope_items<'a>(node: Node<'a>, want: &[&str]) -> Vec<Node<'a>> {
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        let mut cursor = n.walk();
        let children: Vec<Node> = n.children(&mut cursor).collect();
        for ch in children {
            let kind = ch.kind();
            if want.contains(&kind) {
                out.push(ch);
            } else if kind == "decorated_definition" {
                if let Some(def) = ch.child_by_field_name("definition") {
                    if want.contains(&def.kind()) {
                        out.push(def);
                    }
                }
            } else if is_scope_container(kind) {
                stack.push(ch);
            }
        }
    }
    out.sort_by_key(|n| n.start_byte());
    out
}

/// True for `if __name__ == "__main__":`.
fn is_main_guard(src: &[u8], node: Node) -> bool {
    if node.kind() != "if_statement" {
        return false;
    }
    match node.child_by_field_name("condition") {
        Some(cond) => {
            let text = cond.utf8_text(src).unwrap_or("");
            text.contains("__name__") && text.contains("__main__")
        }
        None => false,
    }
}

/// Classify the callee part of a call such as `def f():`
/// 一个调用点被调者的形状：叫什么、怎么调、接收者是什么。
struct Callee {
    target: Option<String>,
    kind: CallKind,
    recv: Option<String>,
    /// `C(...).m()`：接收者是构造调用，类型就是那个类名
    ctor: Option<String>,
}

fn classify(src: &[u8], func: Node) -> Callee {
    let text = |n: Node| n.utf8_text(src).unwrap_or("").to_string();
    let plain = |target: Option<String>, kind: CallKind, recv: Option<String>| Callee {
        target,
        kind,
        recv,
        ctor: None,
    };
    match func.kind() {
        "identifier" => plain(Some(text(func)), CallKind::Name, None),
        "attribute" => {
            let obj = func.child_by_field_name("object");
            let attr = func.child_by_field_name("attribute");
            let attr_name = attr.map(text);
            match obj {
                Some(o) if o.kind() == "identifier" => {
                    let name = text(o);
                    if name == "self" || name == "cls" {
                        plain(attr_name, CallKind::SelfRef, Some(name))
                    } else {
                        plain(attr_name, CallKind::Attr, Some(name))
                    }
                }
                Some(o) if o.kind() == "attribute" => {
                    let inner_obj = o.child_by_field_name("object");
                    let inner_attr = o.child_by_field_name("attribute");
                    if let (Some(io), Some(ia)) = (inner_obj, inner_attr) {
                        if text(io) == "self" {
                            return plain(attr_name, CallKind::SelfAttr, Some(text(ia)));
                        }
                    }
                    plain(attr_name, CallKind::Attr, None)
                }
                // `Path().resolve()`：接收者是构造调用，类型能直接读出来
                Some(o) if o.kind() == "call" => Callee {
                    target: attr_name,
                    kind: CallKind::Attr,
                    recv: None,
                    ctor: class_name_of_call(src, o),
                },
                _ => plain(attr_name, CallKind::Attr, None),
            }
        }
        _ => plain(None, CallKind::Other, None),
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

/// Index of the smallest enclosing call for each entry of `calls`.
fn call_parents(calls: &[Node]) -> Vec<Option<usize>> {
    let mut parents = vec![None; calls.len()];
    for i in 0..calls.len() {
        let (si, ei) = (calls[i].start_byte(), calls[i].end_byte());
        let mut best: Option<(usize, usize)> = None;
        for j in 0..calls.len() {
            if i == j {
                continue;
            }
            let (sj, ej) = (calls[j].start_byte(), calls[j].end_byte());
            if sj <= si && ei <= ej && (sj != si || ej != ei) {
                let span = ej - sj;
                if best.map_or(true, |(len, _)| span < len) {
                    best = Some((span, j));
                }
            }
        }
        parents[i] = best.map(|(_, j)| j);
    }
    parents
}

/// Children before parents: the order Python evaluates a nested expression.
fn postorder(idx: usize, children: &[Vec<usize>], out: &mut Vec<usize>) {
    for &child in &children[idx] {
        postorder(child, children, out);
    }
    out.push(idx);
}

/// Call spans inside `start..end` that got a temporary name, minus `exclude`.
fn repls_within(
    calls: &[Node],
    name_of: &HashMap<usize, String>,
    exclude: Option<usize>,
    start: usize,
    end: usize,
) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    for (&i, name) in name_of {
        if Some(i) == exclude {
            continue;
        }
        let (s, e) = (calls[i].start_byte(), calls[i].end_byte());
        if start <= s && e <= end {
            out.push((s, e, name.clone()));
        }
    }
    out.sort_by_key(|(s, _, _)| *s);
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

/// How many candidate targets a single call site may carry.
///
/// A name like `get` or `run` is defined hundreds or thousands of times in a
/// large project. Every candidate becomes a row in `call_candidate`, and the
/// anchor only ever shows a short list, so the tail is dropped.
const MAX_CANDIDATES: usize = 8;

/// 调用点来源的展示文本，工具提示与「来源：」那一行都用它。
fn import_label(imp: &ImportInfo) -> String {
    let suffix = if imp.alias {
        format!(" as {}", imp.local)
    } else {
        String::new()
    };
    if imp.module_import {
        format!("import {}{}", imp.module, suffix)
    } else {
        format!("from {} import {}{}", imp.module, imp.orig, suffix)
    }
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
