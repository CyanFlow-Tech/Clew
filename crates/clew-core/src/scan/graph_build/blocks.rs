use std::collections::BTreeMap;

use tree_sitter::Node;

use super::super::model::*;
use crate::utils::syntax::{
    class_name_of_call, collect_calls, definition_of, is_clause, is_compound,
};
use crate::utils::text;
use super::builder::Builder;

impl Builder {
    pub(crate) fn emit_block(
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
                    text: text::first_line(&self.src, stmt),
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
                            self.graph.card_imports
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
                            text: text::first_line(&self.src, stmt),
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
                                    locals.insert(text::of(&self.src, left), callee);
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
    pub(crate) fn emit_compound(
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
                text: text::first_line(&self.src, node),
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

}
