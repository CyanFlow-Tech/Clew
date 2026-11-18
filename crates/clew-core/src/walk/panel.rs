use std::collections::HashMap;

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::card::CardId;
use super::symbols::{symbols_of, SymbolRow};

#[derive(Debug, Clone, Serialize)]
pub struct SearchFile {
    pub module: String,
    pub path: String,
    /// 文件名命中了，所以这里列的是整个文件的定义，不是只列名字命中的那些
    pub by_file: bool,
    pub symbols: Vec<SymbolRow>,
}

/// 面板里显示的路径一律相对项目根，用正斜杠。root 和 path 都可能带 Windows
/// 的 `\\?\` 前缀，先统一剥掉再比，否则 strip_prefix 永远失败，树就从盘符开始了。

pub fn search(
    conn: &Connection,
    project_id: i64,
    needle: &str,
    limit: usize,
) -> rusqlite::Result<Vec<SearchFile>> {
    let needle = needle.trim();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let like = format!("%{needle}%");
    let low = needle.to_lowercase();
    let limit = limit.max(1);

    // 名字命中的上限，防止一个短词把整个项目的符号都拉进来
    const NAME_LIMIT: usize = 300;
    // 文件命中最多展开这么多个文件，每个文件都要查一遍它的定义
    const FILE_LIMIT: usize = 30;

    // 先用名字命中建组，这样它排在前面，而且同一个文件里没命中的函数不会被带进来
    let mut groups: Vec<SearchFile> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();

    let mut name_stmt = conn.prepare(
        "SELECT s.module, COALESCE(m.path, ''), s.qualname, s.kind, s.def_line,
                s.is_entry, s.entry_confidence,
                (SELECT count(*) FROM call_site c WHERE c.dst_symbol = s.id)
           FROM symbol s
           LEFT JOIN module m ON m.project_id = s.project_id AND m.module = s.module
          WHERE s.project_id = ?1 AND s.qualname LIKE ?2
          ORDER BY s.def_line, s.qualname
          LIMIT ?3",
    )?;
    let rows = name_stmt.query_map(params![project_id, like, NAME_LIMIT as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)? != 0,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, i64>(7)?,
        ))
    })?;
    for row in rows {
        let (module, path, qualname, kind, def_line, is_entry, confidence, fanin) = row?;
        let symbol = SymbolRow {
            card_id: CardId::new(&module, &qualname).into_string(),
            module: module.clone(),
            qualname,
            kind,
            def_line,
            is_entry,
            entry_confidence: confidence,
            params: Vec::new(),
            fanin,
        };
        let path = path.trim_start_matches(r"\\?\").to_string();
        match index.get(&module) {
            Some(&i) => groups[i].symbols.push(symbol),
            None => {
                index.insert(module.clone(), groups.len());
                groups.push(SearchFile {
                    module,
                    path,
                    by_file: false,
                    symbols: vec![symbol],
                });
            }
        }
    }

    // 文件命中只比文件名和模块最后一段。目录名不算，否则搜 man 会把 command、
    // manager 这些目录下的文件全部拉出来，一次按键就是上千个文件。
    let mut file_stmt = conn.prepare(
        "SELECT module, path FROM module WHERE project_id = ?1 ORDER BY path",
    )?;
    let mut opened = 0usize;
    for row in file_stmt.query_map(params![project_id], |row| {
        let path: String = row.get(1)?;
        Ok((
            row.get::<_, String>(0)?,
            path.trim_start_matches(r"\\?\").to_string(),
        ))
    })? {
        if opened >= FILE_LIMIT {
            break;
        }
        let (module, path) = row?;
        let base = path
            .rsplit(|c| c == '/' || c == '\\')
            .next()
            .unwrap_or(&path)
            .to_lowercase();
        let last = module.rsplit('.').next().unwrap_or(&module).to_lowercase();
        if !base.contains(&low) && !last.contains(&low) {
            continue;
        }
        let syms = symbols_of(conn, project_id, &module, false)?;
        // 一条定义都没有的文件不能当入口，直接跳过
        if syms.is_empty() {
            continue;
        }
        opened += 1;
        match index.get(&module) {
            Some(&i) => {
                groups[i].symbols = syms;
                groups[i].by_file = true;
            }
            None => {
                index.insert(module.clone(), groups.len());
                groups.push(SearchFile {
                    module,
                    path,
                    by_file: true,
                    symbols: syms,
                });
            }
        }
    }

    // 只命中函数名的组排在前面，整份带出来的排后面
    groups.sort_by_key(|g| g.by_file);

    let mut out: Vec<SearchFile> = Vec::new();
    let mut total = 0usize;
    for mut g in groups {
        if total >= limit {
            break;
        }
        if total + g.symbols.len() > limit {
            g.symbols.truncate(limit - total);
        }
        total += g.symbols.len();
        out.push(g);
    }

    Ok(out)
}
