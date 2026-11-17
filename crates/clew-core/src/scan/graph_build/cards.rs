use std::collections::BTreeMap;

use blake3::Hasher;
use tree_sitter::Node;

use super::super::model::*;
use super::super::entry;
use super::super::walker;
use crate::utils::syntax::{
    class_name_of_call,
    first_block, is_main_guard, scope_items,
};
use crate::utils::text;
use super::builder::Builder;
use super::types::*;

impl Builder {
    pub(crate) fn body_hash(&self, node: Node) -> String {
        let range = match node.child_by_field_name("body") {
            Some(body) => body.byte_range(),
            None => node.byte_range(),
        };
        let mut hasher = Hasher::new();
        hasher.update(&self.src[range]);
        hasher.finalize().to_hex().to_string()
    }

    pub(crate) fn push_symbol(&mut self, node: Node, qualname: &str, kind: SymbolKind) {
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

    pub(crate) fn file(&mut self, file: &walker::SourceFile, src: &[u8], root: Node, src_hash: String) {
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
                    self.graph.imports
                        .entry(self.module.clone())
                        .or_default()
                        .extend(found);
                }
                "import_from_statement" => {
                    let found = self.collect_import_from(item);
                    self.graph.imports
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
    pub(crate) fn module_body(&mut self, guard: Node) {
        let owner = Graph::card_id(&self.module, entry::MODULE_ENTRY_QUALNAME);
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
            self.graph.card_locals.insert(owner, locals);
        }
    }

    // ── imports ───────────────────────────────────────────────────
    /// 相对 import 的基址。`from .x import y` 在普通模块里先退掉自己的模块名，
    /// 在包的 `__init__.py` 里则停在包本身：`pkg/__init__.py` 的 `.session`
    /// 是 `pkg.session`，少算一层会指到一个并不存在的模块。

    pub(crate) fn params_of(&self, node: Node) -> Vec<String> {
        let mut out = Vec::new();
        let Some(params) = node.child_by_field_name("parameters") else {
            return out;
        };
        let mut cursor = params.walk();
        for ch in params.children(&mut cursor) {
            match ch.kind() {
                "identifier" => out.push(text::of(&self.src, ch)),
                "typed_parameter" => {
                    if let Some(id) = ch.child_by_field_name("identifier") {
                        out.push(text::of(&self.src, id));
                    }
                }
                "default_parameter" | "typed_default_parameter" => {
                    if let Some(id) = ch.child_by_field_name("name") {
                        out.push(text::of(&self.src, id));
                    }
                }
                "list_splat_pattern" | "dictionary_splat_pattern" => {
                    let t = text::of(&self.src, ch);
                    out.push(t);
                }
                _ => {}
            }
        }
        out.retain(|p| p != "self" && p != "cls");
        out
    }

    pub(crate) fn collect_self_attrs(&mut self, init: Node, attrs: &mut BTreeMap<String, String>) {
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
                                if text::of(&self.src, obj) == "self" {
                                    if let Some(callee) = class_name_of_call(&self.src, right) {
                                        attrs.insert(text::of(&self.src, attr), callee);
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
}
