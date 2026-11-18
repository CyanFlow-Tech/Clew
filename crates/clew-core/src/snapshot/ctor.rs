use std::collections::BTreeMap;

use rusqlite::Connection;

use crate::scan::model::MethodRef;

use super::card_id;
use super::rows::*;

pub(crate) fn resolve_ctor(
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

pub(crate) fn effective_ctor(
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
