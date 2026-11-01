//! Extract symbols and call sites with tree-sitter.

use blake3::Hasher;
use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
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
}

#[derive(Debug, Clone)]
pub struct SymbolDef {
    pub module: String,
    pub qualname: String,
    pub kind: SymbolKind,
    pub src_hash: String,
    pub def_line: u32,
    pub def_col: u32,
    pub body_start: usize,
    pub body_end: usize,
}

#[derive(Debug, Clone)]
pub struct CallSite {
    /// Qualname of the function that contains the call
    pub owner: String,
    /// Direct name of the callee
    pub callee: String,
    pub line: u32,
}

#[derive(Debug, Default)]
pub struct FileIndex {
    pub symbols: Vec<SymbolDef>,
    pub calls: Vec<CallSite>,
    pub unresolved_attribute: usize,
    pub unresolved_other: usize,
    /// The module has an `if __name__ == "__main__"` guard
    pub has_main_guard: bool,
}

pub struct Extractor {
    parser: Parser,
}

impl Extractor {
    pub fn new() -> Self {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .expect("failed to load tree-sitter-python");
        Self { parser }
    }

    pub fn extract(&mut self, module: &str, source: &str) -> FileIndex {
        let tree = match self.parser.parse(source, None) {
            Some(t) => t,
            None => return FileIndex::default(),
        };
        let mut st = State {
            src: source.as_bytes(),
            module,
            index: FileIndex::default(),
        };
        let mut class_stack: Vec<String> = Vec::new();
        visit(&mut st, tree.root_node(), &mut class_stack, "");
        st.index
    }
}

struct State<'a> {
    src: &'a [u8],
    module: &'a str,
    index: FileIndex,
}

fn node_text<'a>(st: &State<'a>, node: Node) -> &'a str {
    node.utf8_text(st.src).unwrap_or("")
}

fn child_text<'a>(st: &State<'a>, node: Node, field: &str) -> &'a str {
    node.child_by_field_name(field)
        .map(|n| node_text(st, n))
        .unwrap_or("")
}

fn qualify(class_stack: &[String], name: &str) -> String {
    if class_stack.is_empty() {
        name.to_string()
    } else {
        format!("{}.{}", class_stack.join("."), name)
    }
}

fn hash_body(st: &State, node: Node) -> String {
    let range = match node.child_by_field_name("body") {
        Some(body) => body.byte_range(),
        None => node.byte_range(),
    };
    let mut hasher = Hasher::new();
    hasher.update(&st.src[range]);
    hasher.finalize().to_hex().to_string()
}

fn visit(st: &mut State, node: Node, class_stack: &mut Vec<String>, owner: &str) {
    match node.kind() {
        "decorated_definition" => {
            if let Some(inner) = node.child_by_field_name("definition") {
                visit(st, inner, class_stack, owner);
            }
            return;
        }
        "class_definition" => {
            let name = child_text(st, node, "name").to_string();
            let qualname = qualify(class_stack, &name);
            st.index.symbols.push(SymbolDef {
                module: st.module.to_string(),
                qualname: qualname.clone(),
                kind: SymbolKind::Class,
                src_hash: hash_body(st, node),
                def_line: node.start_position().row as u32 + 1,
                def_col: node.start_position().column as u32,
                body_start: node.start_byte(),
                body_end: node.end_byte(),
            });
            class_stack.push(name);
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    visit(st, child, class_stack, owner);
                }
            }
            class_stack.pop();
            return;
        }
        "function_definition" => {
            let name = child_text(st, node, "name").to_string();
            let qualname = qualify(class_stack, &name);
            let kind = if class_stack.is_empty() {
                SymbolKind::Function
            } else {
                SymbolKind::Method
            };
            st.index.symbols.push(SymbolDef {
                module: st.module.to_string(),
                qualname: qualname.clone(),
                kind,
                src_hash: hash_body(st, node),
                def_line: node.start_position().row as u32 + 1,
                def_col: node.start_position().column as u32,
                body_start: node.start_byte(),
                body_end: node.end_byte(),
            });
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    visit(st, child, class_stack, &qualname);
                }
            }
            return;
        }
        "call" => {
            if let Some(func) = node.child_by_field_name("function") {
                match func.kind() {
                    "identifier" => {
                        if !owner.is_empty() {
                            st.index.calls.push(CallSite {
                                owner: owner.to_string(),
                                callee: node_text(st, func).to_string(),
                                line: node.start_position().row as u32 + 1,
                            });
                        }
                    }
                    "attribute" => st.index.unresolved_attribute += 1,
                    _ => st.index.unresolved_other += 1,
                }
            }
        }
        "if_statement" => {
            if owner.is_empty() {
                if let Some(cond) = node.child_by_field_name("condition") {
                    let text = node_text(st, cond);
                    if text.contains("__name__") && text.contains("__main__") {
                        st.index.has_main_guard = true;
                    }
                }
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit(st, child, class_stack, owner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"
import os

class A:
    def f(self):
        return g()

def g():
    return 1

def h():
    obj.method()
    return g()
"#;

    #[test]
    fn extracts_symbols_and_calls() {
        let mut ex = Extractor::new();
        let idx = ex.extract("pkg.m", SRC);

        let quals: Vec<&str> = idx.symbols.iter().map(|s| s.qualname.as_str()).collect();
        assert!(quals.contains(&"A"), "missing class A: {quals:?}");
        assert!(quals.contains(&"A.f"), "missing method A.f: {quals:?}");
        assert!(quals.contains(&"g"));
        assert!(quals.contains(&"h"));

        let ok: Vec<(&str, &str)> = idx
            .calls
            .iter()
            .map(|c| (c.owner.as_str(), c.callee.as_str()))
            .collect();
        assert_eq!(ok, vec![("A.f", "g"), ("h", "g")]);
        assert_eq!(idx.unresolved_attribute, 1);
    }
}
