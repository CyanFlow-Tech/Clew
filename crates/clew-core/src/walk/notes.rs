
use rusqlite::{Connection, params};


pub fn notes(conn: &Connection, symbol_id: i64) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT text FROM note WHERE symbol_id = ?1 ORDER BY updated_at DESC, id DESC",
    )?;
    let rows = stmt.query_map(params![symbol_id], |row| row.get::<_, String>(0))?;
    rows.collect()
}
