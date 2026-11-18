
use rusqlite::{Connection, params};

use crate::scan::model::Line;

pub fn insert_card_param(
    conn: &Connection,
    symbol_id: i64,
    ord: usize,
    name: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO card_param (symbol_id, ord, name) VALUES (?1,?2,?3)
         ON CONFLICT DO NOTHING",
        params![symbol_id, ord as i64, name],
    )?;
    Ok(())
}

pub fn insert_card_line(
    conn: &Connection,
    symbol_id: i64,
    ord: usize,
    line: &Line,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO card_line (symbol_id, ord, kind, text, indent, ln, call_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT DO NOTHING",
        params![
            symbol_id,
            ord as i64,
            line.k.as_str(),
            line.text,
            line.indent as i64,
            line.ln as i64,
            line.cs,
        ],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]

pub fn insert_local_type(
    conn: &Connection,
    symbol_id: i64,
    var: &str,
    class_card: &str,
    class_symbol: Option<i64>,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO local_type (symbol_id, var, class_card, class_symbol) VALUES (?1,?2,?3,?4)
         ON CONFLICT DO NOTHING",
        params![symbol_id, var, class_card, class_symbol],
    )?;
    Ok(())
}
