use std::collections::BTreeMap;

use tree_sitter::Node;

use crate::utils::text;
use super::builder::Builder;
use super::types::*;

impl Builder {
    pub(crate) fn relative_base(&self, dots: usize) -> String {
        let mut parts: Vec<&str> = self.module.split('.').collect();
        if !self.is_package {
            parts.pop();
        }
        for _ in 1..dots {
            parts.pop();
        }
        parts.join(".")
    }

    pub(crate) fn collect_import(&mut self, node: Node) -> BTreeMap<String, ImportInfo> {
        let mut out = BTreeMap::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "dotted_name" => {
                    let full = text::of(&self.src, child);
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
                        let full = text::of(&self.src, name);
                        let local = text::of(&self.src, alias);
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

    pub(crate) fn collect_import_from(&mut self, node: Node) -> BTreeMap<String, ImportInfo> {
        let mut out = BTreeMap::new();
        let module_node = node.child_by_field_name("module_name");
        let mut base = module_node.map(|n| text::of(&self.src, n)).unwrap_or_default();

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
                    let raw = text::of(&self.src, ch);
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
                    let orig = text::of(&self.src, child);
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
                        let orig = text::of(&self.src, name);
                        let local = text::of(&self.src, alias);
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
}
