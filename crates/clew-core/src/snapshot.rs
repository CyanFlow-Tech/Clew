//! One-shot read of the whole graph, shaped exactly like the frontend expects.
//!
//! This is the read-side twin of `scan::graph_build`: same `Card` / `Call` shapes,
//! rebuilt from the index instead of from source.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension, params};
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

/// What one expansion step needs: the requested cards, their outgoing call
/// sites, and enough global counts for the "hub" badge.
///
/// A class card brings its effective `__init__` along, because expanding a class
/// that has been instantiated also places the constructor card.
#[derive(Debug, Serialize)]
pub struct CardsPayload {
    pub cards: BTreeMap<String, Card>,
    pub calls: BTreeMap<String, Call>,
    /// card id -> call sites across the whole project that resolve to it
    pub fanin: BTreeMap<String, i64>,
    /// id -> name, for candidate targets the reader has not opened yet
    pub refs: BTreeMap<String, CardRef>,
}

#[derive(Debug, Serialize)]
pub struct CardRef {
    pub module: String,
    pub qualname: String,
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

/// Entry points as card ids, in the order the UI lists them.
pub fn entries(conn: &Connection, project_id: i64) -> rusqlite::Result<Vec<Entry>> {
    let mut stmt = conn.prepare(
        "SELECT module, qualname FROM symbol
          WHERE project_id = ?1 AND is_entry = 1
          ORDER BY module, qualname",
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (module, qualname) = row?;
        let id = card_id(&module, &qualname);
        out.push(Entry {
            label: id.clone(),
            id,
        });
    }
    Ok(out)
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
        let title = format!("{}({})", crate::display_qual(qualname), params.join(", "));
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

    let entries = entries(conn, project_id)?;

    Ok(Snapshot {
        cards,
        calls,
        entries,
    })
}

// ─── on-demand card read ──────────────────────────────────────────
//
// The GUI asks for one card at a time as the reader walks the graph. Pulling
// the whole snapshot means serialising every card and every call of the
// project, which on a large code base is hundreds of megabytes for a screen
// that shows a few dozen nodes.

fn symbol_row(
    conn: &Connection,
    project_id: i64,
    module: &str,
    qualname: &str,
) -> rusqlite::Result<Option<(i64, String)>> {
    conn.query_row(
        "SELECT id, kind FROM symbol
          WHERE project_id = ?1 AND module = ?2 AND qualname = ?3",
        params![project_id, module, qualname],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
    )
    .optional()
}

fn card_lines(conn: &Connection, symbol_id: i64) -> rusqlite::Result<Vec<Line>> {
    let mut stmt = conn.prepare(
        "SELECT kind, text, indent, ln, call_id
           FROM card_line WHERE symbol_id = ?1 ORDER BY ord",
    )?;
    let rows = stmt.query_map(params![symbol_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (kind, text, indent, ln, cs) = row?;
        out.push(Line {
            k: line_kind(&kind),
            text,
            indent: indent.clamp(0, 255) as u8,
            ln: u32::try_from(ln).unwrap_or(0),
            cs,
        });
    }
    Ok(out)
}

fn card_params(conn: &Connection, symbol_id: i64) -> rusqlite::Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT name FROM card_param WHERE symbol_id = ?1 ORDER BY ord")?;
    let rows = stmt.query_map(params![symbol_id], |row| row.get::<_, String>(0))?;
    rows.collect()
}

fn card_locals(conn: &Connection, symbol_id: i64) -> rusqlite::Result<BTreeMap<String, String>> {
    let mut stmt =
        conn.prepare("SELECT var, class_card FROM local_type WHERE symbol_id = ?1")?;
    let rows = stmt.query_map(params![symbol_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (var, class_card) = row?;
        out.insert(var, class_card);
    }
    Ok(out)
}

fn class_bases(conn: &Connection, symbol_id: i64) -> rusqlite::Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT base_name FROM class_base WHERE symbol_id = ?1 ORDER BY ord")?;
    let rows = stmt.query_map(params![symbol_id], |row| row.get::<_, String>(0))?;
    rows.collect()
}

/// Methods declared directly on this class, not on a nested one.
fn class_methods(
    conn: &Connection,
    project_id: i64,
    module: &str,
    qualname: &str,
) -> rusqlite::Result<Vec<MethodRef>> {
    let prefix = format!("{qualname}.");
    let mut stmt = conn.prepare(
        "SELECT qualname FROM symbol
          WHERE project_id = ?1 AND module = ?2 AND kind = 'method' AND qualname LIKE ?3
          ORDER BY qualname",
    )?;
    let rows = stmt.query_map(params![project_id, module, &format!("{prefix}%")], |row| {
        row.get::<_, String>(0)
    })?;
    let mut out = Vec::new();
    for row in rows {
        let full = row?;
        let rest = &full[prefix.len()..];
        if rest.contains('.') {
            continue;
        }
        out.push(MethodRef {
            name: rest.to_string(),
            target: card_id(module, &full),
        });
    }
    Ok(out)
}

/// Effective constructor along the MRO, same rule as `resolve_ctor` above.
fn effective_ctor(
    conn: &Connection,
    project_id: i64,
    module: &str,
    qualname: &str,
    symbol_id: i64,
    depth: usize,
) -> rusqlite::Result<(Option<String>, Option<String>, Option<String>)> {
    if depth > 8 {
        return Ok((None, None, None));
    }
    let init = format!("{qualname}.__init__");
    if symbol_row(conn, project_id, module, &init)?.is_some() {
        return Ok((
            Some(card_id(module, &init)),
            Some(qualname.to_string()),
            None,
        ));
    }

    for base in class_bases(conn, symbol_id)? {
        let found_class = symbol_row(conn, project_id, module, &base)?
            .filter(|(_, kind)| kind == "class");
        match found_class {
            Some((base_id, _)) => {
                let found = effective_ctor(conn, project_id, module, &base, base_id, depth + 1)?;
                if found.0.is_some() {
                    return Ok(found);
                }
            }
            None => return Ok((None, None, Some(base))),
        }
    }

    Ok((None, None, Some("object".to_string())))
}

fn one_card(
    conn: &Connection,
    project_id: i64,
    module: &str,
    qualname: &str,
    symbol_id: i64,
    kind: &str,
) -> rusqlite::Result<Card> {
    let id = card_id(module, qualname);
    let params = card_params(conn, symbol_id)?;
    let title = format!("{}({})", crate::display_qual(qualname), params.join(", "));
    let mut card = Card {
        id,
        module: module.to_string(),
        qualname: qualname.to_string(),
        title,
        params,
        kind: card_kind(kind),
        lines: card_lines(conn, symbol_id)?,
        methods: Vec::new(),
        bases: Vec::new(),
        ctor: None,
        ctor_owner: None,
        external_base: None,
        locals: card_locals(conn, symbol_id)?,
    };
    if kind == "class" {
        card.methods = class_methods(conn, project_id, module, qualname)?;
        card.bases = class_bases(conn, symbol_id)?;
        let (ctor, owner, external) =
            effective_ctor(conn, project_id, module, qualname, symbol_id, 0)?;
        card.ctor = ctor;
        card.ctor_owner = owner;
        card.external_base = external;
    }
    Ok(card)
}

fn read_calls(
    conn: &Connection,
    project_id: i64,
    symbol_id: i64,
    from_id: &str,
) -> rusqlite::Result<BTreeMap<String, Call>> {
    let mut stmt = conn.prepare(
        "SELECT id, target, kind, recv, confidence, dst_symbol, imported, typed, call_line
           FROM call_site WHERE project_id = ?1 AND symbol_id = ?2
          ORDER BY id",
    )?;
    let rows = stmt.query_map(params![project_id, symbol_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<i64>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, i64>(8)?,
        ))
    })?;
    let mut prepared = Vec::new();
    for row in rows {
        prepared.push(row?);
    }
    drop(stmt);

    let mut out = BTreeMap::new();
    for (id, target, kind, recv, conf, dst, imported, typed, ln) in prepared {
        let mut args = BTreeMap::new();
        let mut stmt =
            conn.prepare("SELECT param, expr FROM call_arg WHERE call_id = ?1 ORDER BY param")?;
        let rows = stmt.query_map(params![id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for r in rows {
            let (p, e) = r?;
            args.insert(p, e);
        }
        drop(stmt);

        let mut candidates = Vec::new();
        let mut stmt =
            conn.prepare("SELECT card_id FROM call_candidate WHERE call_id = ?1 ORDER BY ord")?;
        let rows = stmt.query_map(params![id], |r| r.get::<_, String>(0))?;
        for r in rows {
            candidates.push(r?);
        }
        drop(stmt);

        let to = match dst {
            Some(d) => conn
                .query_row(
                    "SELECT module, qualname FROM symbol WHERE id = ?1 AND project_id = ?2",
                    params![d, project_id],
                    |r| Ok(card_id(&r.get::<_, String>(0)?, &r.get::<_, String>(1)?)),
                )
                .optional()?,
            None => None,
        };

        out.insert(
            id,
            Call {
                from: from_id.to_string(),
                target,
                kind: call_kind(&kind),
                recv,
                args,
                ln: u32::try_from(ln).unwrap_or(0),
                conf: confidence(&conf),
                to,
                candidates,
                imported,
                typed,
            },
        );
    }
    Ok(out)
}

/// Read the requested cards plus what one expansion step needs.
///
/// A class card carries its effective `__init__` along, because expanding a
/// class that has been instantiated also places the constructor card.
pub fn cards_of(
    conn: &Connection,
    project_id: i64,
    ids: &[String],
) -> rusqlite::Result<CardsPayload> {
    let mut cards: BTreeMap<String, Card> = BTreeMap::new();
    let mut calls: BTreeMap<String, Call> = BTreeMap::new();
    let mut fanin: BTreeMap<String, i64> = BTreeMap::new();
    let mut refs: BTreeMap<String, CardRef> = BTreeMap::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut queue: Vec<String> = ids.to_vec();

    while let Some(id) = queue.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let Some((module, qualname)) = id.split_once(':') else {
            continue;
        };
        let Some((symbol_id, kind)) = symbol_row(conn, project_id, module, qualname)? else {
            continue;
        };

        let card = one_card(conn, project_id, module, qualname, symbol_id, &kind)?;
        if let Some(ctor) = card.ctor.clone() {
            queue.push(ctor);
        }
        cards.insert(id.clone(), card);

        for (cs_id, call) in read_calls(conn, project_id, symbol_id, &id)? {
            calls.insert(cs_id, call);
        }

        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM call_site WHERE project_id = ?1 AND dst_symbol = ?2",
            params![project_id, symbol_id],
            |r| r.get(0),
        )?;
        fanin.insert(id.clone(), n);
    }

    // Names for every target we mentioned but did not send in full: the bubble
    // lists polymorphic candidates by name before they are opened.
    let mut need: BTreeSet<String> = BTreeSet::new();
    for call in calls.values() {
        if let Some(to) = &call.to {
            need.insert(to.clone());
        }
        for cand in &call.candidates {
            need.insert(cand.clone());
        }
    }
    for id in need {
        if cards.contains_key(&id) || refs.contains_key(&id) {
            continue;
        }
        if let Some((module, qualname)) = id.split_once(':') {
            let module = module.to_string();
            let qualname = qualname.to_string();
            if symbol_row(conn, project_id, &module, &qualname)?.is_some() {
                refs.insert(id, CardRef { module, qualname });
            }
        }
    }

    Ok(CardsPayload {
        cards,
        calls,
        fanin,
        refs,
    })
}
