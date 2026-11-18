use tree_sitter::Node;

use crate::scan::model::CallKind;

pub(crate) fn first_block(node: Node) -> Option<Node> {    let mut cursor = node.walk();
    node.children(&mut cursor).find(|c| c.kind() == "block")
}

/// The `def`/`class` node a statement defines, decorated or not.
pub(crate) fn definition_of(node: Node) -> Option<Node> {
    match node.kind() {
        "function_definition" | "class_definition" => Some(node),
        "decorated_definition" => node.child_by_field_name("definition"),
        _ => None,
    }
}

/// Statements that keep the enclosing scope instead of opening a new one.
pub(crate) fn is_scope_container(kind: &str) -> bool {
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
pub(crate) fn is_clause(kind: &str) -> bool {
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
pub(crate) fn is_compound(kind: &str) -> bool {
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
pub(crate) fn scope_items<'a>(node: Node<'a>, want: &[&str]) -> Vec<Node<'a>> {
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
pub(crate) fn is_main_guard(src: &[u8], node: Node) -> bool {
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
pub(crate) struct Callee {
    pub target: Option<String>,
    pub kind: CallKind,
    pub recv: Option<String>,
    /// `C(...).m()`：接收者是构造调用，类型就是那个类名
    pub ctor: Option<String>,
}

pub(crate) fn classify(src: &[u8], func: Node) -> Callee {
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

pub(crate) fn collect_calls(stmt: Node) -> Vec<Node> {
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
pub(crate) fn call_parents(calls: &[Node]) -> Vec<Option<usize>> {
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
pub(crate) fn postorder(idx: usize, children: &[Vec<usize>], out: &mut Vec<usize>) {
    for &child in &children[idx] {
        postorder(child, children, out);
    }
    out.push(idx);
}

/// Call spans inside `start..end` that got a temporary name, minus `exclude`.
pub(crate) fn class_name_of_call(src: &[u8], call: Node) -> Option<String> {
    let func = call.child_by_field_name("function")?;
    match func.kind() {
        "identifier" => Some(func.utf8_text(src).unwrap_or("").to_string()),
        "attribute" => func
            .child_by_field_name("attribute")
            .map(|n| n.utf8_text(src).unwrap_or("").to_string()),
        _ => None,
    }
}

pub(crate) fn class_of_qualname(qualname: &str) -> Option<String> {
    let mut parts: Vec<&str> = qualname.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    parts.pop();
    Some(parts.join("."))
}
