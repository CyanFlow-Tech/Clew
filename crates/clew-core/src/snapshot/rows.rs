use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, params};

use crate::scan::model::{Line, MethodRef};

use super::card_id;

pub(crate) fn symbol_row(
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

pub(crate) fn card_lines(conn: &Connection, symbol_id: i64) -> rusqlite::Result<Vec<Line>> {
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
            k: kind.parse().unwrap_or_default(),
            text,
            indent: indent.clamp(0, 255) as u8,
            ln: u32::try_from(ln).unwrap_or(0),
            cs,
        });
    }
    Ok(out)
}

pub(crate) fn card_params(conn: &Connection, symbol_id: i64) -> rusqlite::Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT name FROM card_param WHERE symbol_id = ?1 ORDER BY ord")?;
    let rows = stmt.query_map(params![symbol_id], |row| row.get::<_, String>(0))?;
    rows.collect()
}

pub(crate) fn card_locals(conn: &Connection, symbol_id: i64) -> rusqlite::Result<BTreeMap<String, String>> {
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

pub(crate) fn class_bases(conn: &Connection, symbol_id: i64) -> rusqlite::Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT base_name FROM class_base WHERE symbol_id = ?1 ORDER BY ord")?;
    let rows = stmt.query_map(params![symbol_id], |row| row.get::<_, String>(0))?;
    rows.collect()
}

/// Methods declared directly on this class, not on a nested one.
pub(crate) fn class_methods(
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
