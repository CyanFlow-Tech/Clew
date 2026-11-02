//! One-shot read of the whole graph, shaped exactly like the frontend expects.
//!
//! This is the read-side twin of `scan::graph_build`: same `Card` / `Call` shapes,
//! rebuilt from the index instead of from source.

use std::collections::BTreeMap;

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::scan::model::{
    Call, CallKind, Card, CardKind, Confidence, Line, LineKind, MethodRef, SymbolKind,
};

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub cards: BTreeMap<String, Card>,
    pub calls: BTreeMap<String, Call>,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Serialize)]
pub struct Entry {
    pub id: String,
    pub label: String,
}

fn line_kind(raw: &str) -> LineKind {
    match raw {
        "call" => LineKind::Call,
        "branch" => LineKind::Branch,
        _ => LineKind::Stmt,
    }
}

fn call_kind(raw: &str) -> CallKind {
    match raw {
        "name" => CallKind::Name,
        "self" => CallKind::SelfRef,
        "selfattr" => CallKind::SelfAttr,
        "attr" => CallKind::Attr,
        _ => CallKind::Other,
    }
}

fn confidence(raw: &str) -> Confidence {
    match raw {
        "exact" => Confidence::Exact,
        "poly" => Confidence::Poly,
        "heur" => Confidence::Heur,
        "builtin" => Confidence::Builtin,
        _ => Confidence::None,
    }
}

fn card_kind(raw: &str) -> CardKind {
    match raw {
        "class" => CardKind::Class,
        "method" => CardKind::Method,
        _ => CardKind::Function,
    }
}

fn card_id(module: &str, qualname: &str) -> String {
    format!("{module}:{qualname}")
}

/// Effective constructor along the MRO.
///
/// Returns `(ctor card id, class that defines it, first base outside the project)`.
/// An inherited `__init__` keeps its defining class so the UI can say where it came
/// from; when the chain leaves the project (e.g. `dict`) there is nothing to open.
fn resolve_ctor(
    class_card: &str,
    classes: &BTreeMap<String, (String, String)>,
    methods_of: &BTreeMap<String, Vec<MethodRef>>,
    bases_of: &BTreeMap<String, Vec<String>>,
    depth: usize,
) -> (Option<String>, Option<String>, Option<String>) {
    if depth > 8 {
        return (None, None, None);
    }
    let Some((module, qualname)) = classes.get(class_card) else {
        return (None, None, None);
    };

    if let Some(init) = methods_of
        .get(class_card)
        .and_then(|ms| ms.iter().find(|m| m.name == "__init__"))
    {
        return (Some(init.target.clone()), Some(qualname.clone()), None);
    }

    for base in bases_of.get(class_card).map(Vec::as_slice).unwrap_or(&[]) {
        let base_card = card_id(module, base);
        if classes.contains_key(&base_card) {
            let found = resolve_ctor(&base_card, classes, methods_of, bases_of, depth + 1);
            if found.0.is_some() {
                return found;
            }
        } else {
            return (None, None, Some(base.clone()));
        }
    }

    // no declared base and no `__init__` of its own: `object` is implicit
    (None, None, Some("object".to_string()))
}

/// Rebuild every card and call site of a project.
pub fn snapshot(conn: &Connection, project_id: i64) -> rusqlite::Result<Snapshot> {
    let mut card_of_symbol: BTreeMap<i64, String> = BTreeMap::new();
    let mut symbols: Vec<(i64, String, String, String)> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, module, qualname, kind FROM symbol WHERE project_id = ?1 ORDER BY id",
        )?;
        for row in stmt.query_map(params![project_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })? {
            let (id, module, qualname, kind) = row?;
            card_of_symbol.insert(id, card_id(&module, &qualname));
            symbols.push((id, module, qualname, kind));
        }
    }

    let mut params_of: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT symbol_id, name FROM card_param ORDER BY symbol_id, ord",
        )?;
        for row in stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })? {
            let (sid, name) = row?;
            params_of.entry(sid).or_default().push(name);
        }
    }

    let mut lines_of: BTreeMap<i64, Vec<Line>> = BTreeMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT symbol_id, kind, text, indent, ln, call_id
               FROM card_line ORDER BY symbol_id, ord",
        )?;
        for row in stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })? {
            let (sid, kind, text, indent, ln, call_id) = row?;
            lines_of.entry(sid).or_default().push(Line {
                k: line_kind(&kind),
                text,
                indent: indent.clamp(0, 255) as u8,
                ln: u32::try_from(ln).unwrap_or(0),
                cs: call_id,
            });
        }
    }

    let mut locals_of: BTreeMap<i64, BTreeMap<String, String>> = BTreeMap::new();
    {
        let mut stmt = conn.prepare("SELECT symbol_id, var, class_card FROM local_type")?;
        for row in stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })? {
            let (sid, var, class_card) = row?;
            locals_of.entry(sid).or_default().insert(var, class_card);
        }
    }

    // methods grouped by owning class card id
    let mut methods_of: BTreeMap<String, Vec<MethodRef>> = BTreeMap::new();
    for (_, module, qualname, kind) in &symbols {
        if SymbolKind::Method.as_str() != kind {
            continue;
        }
        let Some((owner, name)) = qualname.rsplit_once('.') else {
            continue;
        };
        methods_of
            .entry(card_id(module, owner))
            .or_default()
            .push(MethodRef {
                name: name.to_string(),
                target: card_id(module, qualname),
            });
    }

    // base class names, keyed by class card id
    let mut bases_of: BTreeMap<String, Vec<String>> = BTreeMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT s.module, s.qualname, cb.base_name
               FROM class_base cb JOIN symbol s ON s.id = cb.symbol_id
              ORDER BY s.module, s.qualname, cb.ord",
        )?;
        for row in stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })? {
            let (module, qualname, base) = row?;
            bases_of.entry(card_id(&module, &qualname)).or_default().push(base);
        }
    }

    // effective constructors, walked along the MRO before anything is consumed
    let mut classes: BTreeMap<String, (String, String)> = BTreeMap::new();
    for (_, module, qualname, kind) in &symbols {
        if kind == SymbolKind::Class.as_str() {
            classes.insert(card_id(module, qualname), (module.clone(), qualname.clone()));
        }
    }
    let mut ctor_of: BTreeMap<String, (Option<String>, Option<String>, Option<String>)> =
        BTreeMap::new();
    for id in classes.keys() {
        ctor_of.insert(
            id.clone(),
            resolve_ctor(id, &classes, &methods_of, &bases_of, 0),
        );
    }

    let mut cards: BTreeMap<String, Card> = BTreeMap::new();
    for (sid, module, qualname, kind) in &symbols {
        let id = card_id(module, qualname);
        let params = params_of.remove(sid).unwrap_or_default();
        let title = format!("{qualname}({})", params.join(", "));
        let (ctor, ctor_owner, external_base) =
            ctor_of.remove(&id).unwrap_or((None, None, None));
        let card = Card {
            id: id.clone(),
            module: module.clone(),
            qualname: qualname.clone(),
            title,
            params,
            kind: card_kind(kind),
            lines: lines_of.remove(sid).unwrap_or_default(),
            methods: methods_of.remove(&id).unwrap_or_default(),
            bases: bases_of.remove(&id).unwrap_or_default(),
            ctor,
            ctor_owner,
            external_base,
            locals: locals_of.remove(sid).unwrap_or_default(),
        };
        cards.insert(id, card);
    }

    let mut args_of: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT a.call_id, a.param, a.expr
               FROM call_arg a JOIN call_site c ON c.id = a.call_id
              WHERE c.project_id = ?1
              ORDER BY a.call_id, a.param",
        )?;
        for row in stmt.query_map(params![project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })? {
            let (call_id, param, expr) = row?;
            args_of.entry(call_id).or_default().insert(param, expr);
        }
    }

    let mut candidates_of: BTreeMap<String, Vec<String>> = BTreeMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT cc.call_id, cc.card_id
               FROM call_candidate cc JOIN call_site c ON c.id = cc.call_id
              WHERE c.project_id = ?1
              ORDER BY cc.call_id, cc.ord",
        )?;
        for row in stmt.query_map(params![project_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            let (call_id, cand) = row?;
            candidates_of.entry(call_id).or_default().push(cand);
        }
    }

    let mut calls: BTreeMap<String, Call> = BTreeMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, module, owner, target, kind, recv, confidence, dst_symbol,
                    imported, typed, call_line
               FROM call_site WHERE project_id = ?1 ORDER BY id",
        )?;
        for row in stmt.query_map(params![project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, i64>(10)?,
            ))
        })? {
            let (id, module, owner, target, kind, recv, conf, dst, imported, typed, ln) = row?;
            let call = Call {
                from: card_id(&module, &owner),
                target,
                kind: call_kind(&kind),
                recv,
                args: args_of.remove(&id).unwrap_or_default(),
                ln: u32::try_from(ln).unwrap_or(0),
                conf: confidence(&conf),
                to: dst.and_then(|d| card_of_symbol.get(&d).cloned()),
                candidates: candidates_of.remove(&id).unwrap_or_default(),
                imported,
                typed,
            };
            calls.insert(id, call);
        }
    }

    let mut entries = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT module, qualname FROM symbol
              WHERE project_id = ?1 AND is_entry = 1
              ORDER BY module, qualname",
        )?;
        for row in stmt.query_map(params![project_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            let (module, qualname) = row?;
            let id = card_id(&module, &qualname);
            entries.push(Entry {
                label: id.clone(),
                id,
            });
        }
    }

    Ok(Snapshot {
        cards,
        calls,
        entries,
    })
}
