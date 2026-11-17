use std::path::Path;

use rusqlite::{Connection, params};
use serde::Serialize;


#[derive(Debug, Clone, Serialize)]
pub struct FileRow {
    pub module: String,
    pub path: String,
    pub is_third_party: bool,
    pub has_main_guard: bool,
    pub symbols: i64,
}

pub fn rel_path(root: &Path, path: &str) -> String {
    fn norm(s: &str) -> String {
        s.trim_start_matches(r"\\?\").replace('\\', "/")
    }
    let root = norm(&root.to_string_lossy());
    let p = norm(path);
    let rest = p
        .strip_prefix(root.trim_end_matches('/'))
        .unwrap_or(&p)
        .trim_start_matches('/');
    if rest.is_empty() {
        p.trim_start_matches('/').to_string()
    } else {
        rest.to_string()
    }
}

/// Every indexed file of a project, ordered by path.
pub fn files(conn: &Connection, project_id: i64) -> rusqlite::Result<Vec<FileRow>> {
    // 定义条数走一次 GROUP BY，不给每个模块各跑一条子查询：五万个模块上
    // 逐条 count 是几十万次索引查找，光这一步就能让面板开不出来。
    let mut stmt = conn.prepare(
        "SELECT m.module, m.path, m.is_third_party, m.has_main_guard, COALESCE(c.n, 0)
           FROM module m
           LEFT JOIN (
                SELECT module, count(*) AS n FROM symbol
                 WHERE project_id = ?1 GROUP BY module
           ) c ON c.module = m.module
          WHERE m.project_id = ?1
          ORDER BY m.path",
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        let path: String = row.get(1)?;
        Ok(FileRow {
            module: row.get(0)?,
            path: path.trim_start_matches(r"\\?\").to_string(),
            is_third_party: row.get::<_, i64>(2)? != 0,
            has_main_guard: row.get::<_, i64>(3)? != 0,
            symbols: row.get(4)?,
        })
    })?;
    rows.collect()
}
