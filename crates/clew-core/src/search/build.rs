
use rusqlite::{params, Connection};

use crate::card::CardId;
use super::query::{Kind, ModuleRef, Row};
use super::Index;
use super::score::mask_of;


pub fn build_index(conn: &Connection, project_id: i64) -> rusqlite::Result<Index> {
    let mut modules: Vec<ModuleRef> = Vec::new();
    let mut by_name: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT module, path FROM module WHERE project_id = ?1 ORDER BY module",
        )?;
        let rows = stmt.query_map(params![project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (module, path) = row?;
            by_name.insert(module.clone(), modules.len() as u32);
            let module_l = module.to_lowercase();
            let mask = mask_of(&module_l);
            modules.push(ModuleRef {
                module_l,
                mask,
                module,
                path,
            });
        }
    }

    let mut rows: Vec<Row> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT module, qualname, kind, def_line, is_entry
               FROM symbol WHERE project_id = ?1",
        )?;
        let found = stmt.query_map(params![project_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?;
        for row in found {
            let (module, qualname, kind, def_line, is_entry) = row?;
            let Some(&idx) = by_name.get(&module) else {
                continue;
            };
            let kind = match kind.as_str() {
                "function" => Kind::Func,
                "method" => Kind::Method,
                "class" => Kind::Class,
                // 模块入口是 <module> 卡片，只在自动识别里出现，不参与文字搜索
                "module" => Kind::Mod,
                _ => continue,
            };
            let name = if qualname == "<module>" {
                module.rsplit('.').next().unwrap_or(&module).to_string()
            } else {
                qualname.rsplit('.').next().unwrap_or(&qualname).to_string()
            };
            let name_l = name.to_lowercase();
            let qual_l = qualname.to_lowercase();
            let mask = mask_of(&name_l) | mask_of(&qual_l);
            rows.push(Row {
                kind,
                module: idx,
                card_id: CardId::new(module, &qualname).into_string(),
                name_l,
                qual_l,
                mask,
                qualname,
                def_line: def_line.max(0) as u32,
                is_entry: is_entry != 0,
            });
        }
    }

    Ok(Index { modules, rows })
}

