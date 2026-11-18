mod cards;
mod ctor;
mod rows;
mod types;

pub use cards::cards_of;
pub use types::{CardRef, CardsPayload, Entry, Snapshot};

pub(crate) fn card_id(module: &str, qualname: &str) -> String {
    CardId::new(module, qualname).into_string()
}

use std::collections::BTreeMap;

use rusqlite::{Connection, params};

use crate::card::CardId;
use crate::scan::model::{Call, Card, Line, MethodRef, SymbolKind};
use ctor::*;

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
                k: kind.parse().unwrap_or_default(),
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
            kind: kind.parse().unwrap_or_default(),
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
                kind: kind.parse().unwrap_or_default(),
                recv,
                args: args_of.remove(&id).unwrap_or_default(),
                ln: u32::try_from(ln).unwrap_or(0),
                conf: conf.parse().unwrap_or_default(),
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
