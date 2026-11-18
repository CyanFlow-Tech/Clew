
use rusqlite::{Connection, params};

use crate::scan::graph_build::ImportMeta;

pub fn insert_import(
    conn: &Connection,
    project_id: i64,
    imp: &ImportMeta,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO import (project_id, module, local, orig, source_module, is_alias, is_module_import)
         VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT DO NOTHING",
        params![
            project_id,
            imp.module,
            imp.local,
            imp.orig,
            imp.source_module,
            imp.alias as i64,
            imp.module_import as i64,
        ],
    )?;
    Ok(())
}

pub fn insert_class_meta(
    conn: &Connection,
    symbol_id: i64,
    module: &str,
    qualname: &str,
    ctor_symbol: Option<i64>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO class_meta (symbol_id, module, qualname, ctor_symbol) VALUES (?1,?2,?3,?4)
         ON CONFLICT(symbol_id) DO UPDATE SET ctor_symbol = excluded.ctor_symbol",
        params![symbol_id, module, qualname, ctor_symbol],
    )?;
    Ok(())
}

pub fn insert_class_base(
    conn: &Connection,
    symbol_id: i64,
    ord: usize,
    base_name: &str,
    base_symbol: Option<i64>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO class_base (symbol_id, ord, base_name, base_symbol) VALUES (?1,?2,?3,?4)
         ON CONFLICT DO NOTHING",
        params![symbol_id, ord as i64, base_name, base_symbol],
    )?;
    Ok(())
}
