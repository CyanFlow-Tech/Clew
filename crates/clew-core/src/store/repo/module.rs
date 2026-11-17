
use rusqlite::{Connection, params};

use crate::scan::graph_build::ModuleMeta;

pub fn insert_module(
    conn: &Connection,
    project_id: i64,
    m: &ModuleMeta,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO module (project_id, module, path, src_hash, is_third_party, has_main_guard)
         VALUES (?1,?2,?3,?4,?5,?6)
         ON CONFLICT(project_id, module) DO UPDATE SET
             path           = excluded.path,
             src_hash       = excluded.src_hash,
             is_third_party = excluded.is_third_party,
             has_main_guard = excluded.has_main_guard",
        params![
            project_id,
            m.module,
            m.path.to_string_lossy(),
            m.src_hash,
            m.is_third_party as i64,
            m.has_main_guard as i64,
        ],
    )?;
    Ok(())
}
