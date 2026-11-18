use std::collections::BTreeMap;

use tree_sitter::Node;

use super::super::model::*;
use crate::utils::syntax::scope_items;
use crate::utils::text;
use super::builder::Builder;
use super::types::*;

impl Builder {
    pub(crate) fn add_function(&mut self, node: Node, scope: &str, in_class: bool) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = text::of(&self.src, name_node);
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
                self.graph.methods
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
        let title = format!("{}({})", crate::display_qual(&qualname), params.join(", "));

        self.graph.cards.insert(
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
            self.graph.methods
                .entry((self.module.clone(), scope.to_string()))
                .or_default()
                .insert(name, id.clone());
        }
        if !locals.is_empty() {
            self.graph.card_locals.insert(id, locals);
        }
    }

    pub(crate) fn add_class(&mut self, node: Node, scope: &str) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = text::of(&self.src, name_node);
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
                    bases.push(text::of(&self.src, ch));
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
            self.graph.class_ids
                .insert((self.module.clone(), qualname.clone()), id.clone());
            self.graph.class_by_qualname
                .entry(qualname.clone())
                .or_insert_with(|| id.clone());
            self.graph.class_bases
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
            let mname = text::of(&self.src, fname);
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
        self.graph.cards.insert(
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
        self.graph.class_ids
            .insert((self.module.clone(), qualname.clone()), id.clone());
        self.graph.class_by_qualname
            .entry(qualname.clone())
            .or_insert_with(|| id.clone());
        self.graph.class_bases
            .insert(id.clone(), (self.module.clone(), bases));
        self.graph.class_attrs.insert(id.clone(), ctor_attrs);
        if !ctor_locals.is_empty() {
            self.graph.card_locals.insert(id.clone(), ctor_locals);
        }

        // method bodies inside a class become cards of their own
        for def in &method_defs {
            self.add_function(*def, &qualname, true);
        }
        for def in &nested_classes {
            self.add_class(*def, &qualname);
        }
    }

}
