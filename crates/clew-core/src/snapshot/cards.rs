use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension, params};

use crate::card::CardId;
use crate::scan::model::{Call, Card};

use super::card_id;
use super::ctor::*;
use super::rows::*;
use super::types::*;

pub(crate) fn one_card(
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
        kind: kind.parse().unwrap_or_default(),
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

pub(crate) fn read_calls(
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
                kind: kind.parse().unwrap_or_default(),
                recv,
                args,
                ln: u32::try_from(ln).unwrap_or(0),
                conf: conf.parse().unwrap_or_default(),
                to,
                candidates,
                imported,
                typed,
            },
        );
    }
    Ok(out)
}

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
        let Some(cid) = CardId::parse(&id) else {
            continue;
        };
        let (module, qualname) = (cid.module(), cid.qualname());
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
        if let Some(cid) = CardId::parse(&id) {
            let module = cid.module().to_string();
            let qualname = cid.qualname().to_string();
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
