use std::collections::BTreeMap;

use tree_sitter::Node;

use super::super::model::*;
use crate::utils::syntax::{
    call_parents, classify, postorder,
};
use crate::utils::text;
use super::builder::Builder;
use super::types::*;

impl Builder {
    pub(crate) fn emit_call_sequence(
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
    pub(crate) fn emit_calls_span(
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

        let mut name_of: BTreeMap<usize, String> = BTreeMap::new();
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
            let body = text::replacing(&self.src, calls[i].start_byte(), calls[i].end_byte(), repls);
            let cs_id = format!("{owner}#{}", out.len());
            self.record_call(cs_id.clone(), owner, calls[i]);
            out.push(Line {
                k: LineKind::Call,
                text: format!("{name} = {}", text::clipped(&body)),
                indent,
                ln,
                cs: Some(cs_id),
            });
        }

        let repls = repls_within(calls, &name_of, None, start, end);
        let body = text::replacing(&self.src, start, end, repls);
        let text = text::clipped(&body);
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

    pub(crate) fn record_call(&mut self, cs_id: String, owner: &str, call: Node) {
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
                            kwargs.push((text::of(&self.src, k), text::first_line(&self.src, v)));
                        }
                    }
                    "(" | ")" | "," => {}
                    _ => pos.push(text::first_line(&self.src, ch)),
                }
            }
        }

        self.graph.raw_calls.insert(
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
        self.graph.calls.insert(
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
}

fn repls_within(
    calls: &[Node],
    name_of: &BTreeMap<usize, String>,
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
    out.sort_by_key(|(s, e, _)| (*s, std::cmp::Reverse(*e)));
    out
}
