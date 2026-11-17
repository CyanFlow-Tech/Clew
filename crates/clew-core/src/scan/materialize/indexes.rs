use std::collections::BTreeMap;
use std::sync::Arc;

use rusqlite::params;

use crate::card::CardId;
use super::Materializer;

impl Materializer {
    /// 第一次物化时建一次全项目索引，之后常驻。
    ///
    /// 只有几十张卡在手时，「这个名字全项目唯一」「同名方法有哪些候选」这类
    /// 判断必须看全局，否则大半调用只能落成未解析。
    pub(crate) fn load_global_indexes(&mut self) -> anyhow::Result<()> {
        if self.by_name.is_some() {
            return Ok(());
        }
        let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut by_method: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut by_class: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut by_qualname: BTreeMap<String, Vec<String>> = BTreeMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT module, qualname, kind FROM symbol
                  WHERE project_id = ?1 AND kind <> 'module'",
            )?;
            let rows = stmt.query_map(params![self.project_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?;
            for row in rows {
                let (module, qualname, kind) = row?;
                let id = CardId::new(&module, &qualname).into_string();
                let bare = qualname
                    .rsplit('.')
                    .next()
                    .unwrap_or(&qualname)
                    .to_string();
                by_name.entry(bare.clone()).or_default().push(id.clone());
                if kind == "method" {
                    by_method.entry(bare).or_default().push(id.clone());
                }
                if kind == "class" || kind == "method" {
                    by_qualname
                        .entry(qualname.clone())
                        .or_default()
                        .push(id.clone());
                }
                if kind == "class" {
                    by_class
                        .entry(qualname.rsplit('.').next().unwrap_or(&qualname).to_string())
                        .or_default()
                        .push(id);
                }
            }
        }
        for v in by_name.values_mut() {
            v.sort();
            v.dedup();
        }
        for v in by_method.values_mut() {
            v.sort();
            v.dedup();
        }
        for v in by_class.values_mut() {
            v.sort();
            v.dedup();
        }
        for v in by_qualname.values_mut() {
            v.sort();
            v.dedup();
        }
        self.by_name = Some(Arc::new(by_name));
        self.by_method = Some(Arc::new(by_method));
        self.by_class = Some(Arc::new(by_class));
        self.by_qualname = Some(Arc::new(by_qualname));
        Ok(())
    }
}
