//! 按需把单个文件的卡片与调用点物化进索引。
//!
//! 打开项目时只建了目录（module / symbol / import），卡片等点到再算。
//! 解析一个文件时把它 import 的模块也一起解一遍，这样 `obj.method()` 的
//! 方法签名、`self.x = Foo()` 的类型推断都还查得到。多出来的这份视野只留在
//! 内存里，不入库，也不展开到第二层依赖之外。
//!
//! 这里刻意做成一个持有连接与缓存的实体：将来加后台预解析时，只需在外面
//! 套一层调度器，把 `ensure` 交给工作线程，命中缓存就直接返回。

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use rusqlite::{params, Connection, OptionalExtension};

use super::graph_build::{self, Builder};
use super::model::SymbolKind;
use super::walker;
use crate::store::repo;

/// 单个文件最多顺带解析这么多个依赖模块，防止一个函数拽进半个项目。
const MAX_DEPS: usize = 100;

pub struct Materializer {
    conn: Connection,
    project_id: i64,
    /// module -> 源文件路径
    paths: HashMap<String, PathBuf>,
    /// 已解析但未入库的原始结果，resolve 之前的状态，可反复合并
    parsed: HashMap<String, Builder>,
    /// 已经物化进库的模块
    done: BTreeSet<String>,
    /// 全项目索引，第一次物化时从 symbol 表建一次
    by_name: Option<Arc<BTreeMap<String, Vec<String>>>>,
    by_method: Option<Arc<BTreeMap<String, Vec<String>>>>,
}

impl Materializer {
    pub fn open(project_id: i64, conn: Connection) -> rusqlite::Result<Self> {
        let mut paths = HashMap::new();
        {
            let mut stmt = conn
                .prepare("SELECT module, path FROM module WHERE project_id = ?1")?;
            let rows = stmt.query_map(params![project_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (module, path) = row?;
                paths.insert(module, PathBuf::from(path));
            }
        }
        Ok(Self {
            conn,
            project_id,
            paths,
            parsed: HashMap::new(),
            done: BTreeSet::new(),
            by_name: None,
            by_method: None,
        })
    }

    /// 提前建好全项目索引。放在打开项目之后、第一次点击之前，
    /// 免得第一次展开多等两百毫秒。
    pub fn preload(&mut self) -> anyhow::Result<()> {
        self.load_global_indexes()
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn root_module_ready(&self, module: &str) -> bool {
        self.done.contains(module)
    }

    /// 保证某个模块的卡片与调用点已在库里，返回写入的卡片数。
    pub fn ensure(&mut self, module: &str) -> anyhow::Result<usize> {
        if self.done.contains(module) {
            return Ok(0);
        }
        // 老库或已经物化过的模块直接记下，不重复解析
        if self.already_materialized(module)? {
            self.done.insert(module.to_string());
            return Ok(0);
        }
        let Some(main) = self.parse_module(module) else {
            return Ok(0);
        };

        let mut acc = main;
        let mut taken = 0usize;
        for dep in acc.clone().imported_modules() {
            if dep.as_str() == module || taken >= MAX_DEPS {
                continue;
            }
            if let Some(extra) = self.parse_module(&dep) {
                acc.merge(extra);
                taken += 1;
            }
        }
        self.load_global_indexes()?;
        acc.global_by_name = self.by_name.clone();
        acc.global_by_method = self.by_method.clone();
        acc.resolve();

        let written = self.persist(module, &acc)?;
        self.done.insert(module.to_string());
        Ok(written)
    }

    /// 第一次物化时建一次全项目索引，之后常驻。
    ///
    /// 只有几十张卡在手时，「这个名字全项目唯一」「同名方法有哪些候选」这类
    /// 判断必须看全局，否则大半调用只能落成未解析。
    fn load_global_indexes(&mut self) -> anyhow::Result<()> {
        if self.by_name.is_some() {
            return Ok(());
        }
        let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut by_method: BTreeMap<String, Vec<String>> = BTreeMap::new();
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
                let id = format!("{module}:{qualname}");
                let bare = qualname
                    .rsplit('.')
                    .next()
                    .unwrap_or(&qualname)
                    .to_string();
                by_name.entry(bare.clone()).or_default().push(id.clone());
                if kind == "method" {
                    by_method.entry(bare).or_default().push(id);
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
        self.by_name = Some(Arc::new(by_name));
        self.by_method = Some(Arc::new(by_method));
        Ok(())
    }

    fn already_materialized(&self, module: &str) -> rusqlite::Result<bool> {
        let hit: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM card_line l
                   JOIN symbol s ON s.id = l.symbol_id
                  WHERE s.project_id = ?1 AND s.module = ?2
                  LIMIT 1",
                params![self.project_id, module],
                |r| r.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(hit.is_some())
    }

    fn parse_module(&mut self, module: &str) -> Option<Builder> {
        if let Some(cached) = self.parsed.get(module) {
            return Some(cached.clone());
        }
        let path = self.paths.get(module)?.clone();
        let source = std::fs::read_to_string(&path).ok()?;
        let mut parser = graph_build::new_parser();
        let tree = parser.parse(&source, None)?;

        let mut hasher = blake3::Hasher::new();
        hasher.update(source.as_bytes());
        let src_hash = hasher.finalize().to_hex().to_string();

        let file = walker::PyFile {
            path,
            module: module.to_string(),
            is_third_party: false,
        };
        let mut builder = Builder::for_cards();
        builder.file(&file, source.as_bytes(), tree.root_node(), src_hash);

        self.parsed.insert(module.to_string(), builder.clone());
        Some(builder)
    }

    /// 只把这一个模块的卡片、类结构与调用点写进库。
    fn persist(&self, module: &str, built: &Builder) -> anyhow::Result<usize> {
        let tx = self.conn.unchecked_transaction()?;

        let mut ids: HashMap<String, i64> = HashMap::new();
        {
            let mut stmt = tx.prepare(
                "SELECT qualname, id FROM symbol WHERE project_id = ?1 AND module = ?2",
            )?;
            let rows = stmt.query_map(params![self.project_id, module], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (qualname, id) = row?;
                ids.insert(qualname, id);
            }
        }

        let prefix = format!("{module}:");
        let mut written = 0usize;

        // 调用目标多半在别的模块里，本模块的 symbol id 表查不到。
        // 把这一模块引用到的外部卡片 id 收齐，一次查完再写。
        let mut external: HashMap<String, i64> = HashMap::new();
        {
            let mut need: BTreeSet<String> = BTreeSet::new();
            for call in built.calls().values() {
                if !call.from.starts_with(&prefix) {
                    continue;
                }
                if let Some(to) = &call.to {
                    if !to.starts_with(&prefix) {
                        need.insert(to.clone());
                    }
                }
                for cand in &call.candidates {
                    if !cand.starts_with(&prefix) {
                        need.insert(cand.clone());
                    }
                }
            }
            for card in built.cards().values() {
                if !card.id.starts_with(&prefix) {
                    continue;
                }
                for class_card in card.locals.values() {
                    if !class_card.starts_with(&prefix) {
                        need.insert(class_card.clone());
                    }
                }
                if let Some(ctor) = &card.ctor {
                    if !ctor.starts_with(&prefix) {
                        need.insert(ctor.clone());
                    }
                }
            }

            let mut stmt = tx.prepare(
                "SELECT id FROM symbol
                  WHERE project_id = ?1 AND module = ?2 AND qualname = ?3",
            )?;
            for card_id in need {
                let Some((m, q)) = card_id.split_once(':') else {
                    continue;
                };
                if let Some(id) = stmt
                    .query_row(params![self.project_id, m, q], |r| r.get::<_, i64>(0))
                    .optional()?
                {
                    external.insert(card_id, id);
                }
            }
        }

        /// 先查本模块，再查外部收集到的映射。
        fn pick(
            card_id: &str,
            prefix: &str,
            ids: &HashMap<String, i64>,
            external: &HashMap<String, i64>,
        ) -> Option<i64> {
            if let Some(rest) = card_id.strip_prefix(prefix) {
                return ids.get(rest).copied();
            }
            external.get(card_id).copied()
        }

        for (card_id, card) in built.cards() {
            if !card_id.starts_with(&prefix) {
                continue;
            }
            let Some(&sym_id) = ids.get(&card.qualname) else {
                continue;
            };
            for (ord, name) in card.params.iter().enumerate() {
                repo::insert_card_param(&tx, sym_id, ord, name)?;
            }
            for (ord, line) in card.lines.iter().enumerate() {
                repo::insert_card_line(&tx, sym_id, ord, line)?;
            }
            for (var, class_card) in &card.locals {
                let class_symbol = pick(class_card, &prefix, &ids, &external);
                repo::insert_local_type(&tx, sym_id, var, class_card, class_symbol)?;
            }
            if card.kind == super::model::CardKind::Class {
                let ctor = card
                    .ctor
                    .as_deref()
                    .and_then(|id| pick(id, &prefix, &ids, &external));
                repo::insert_class_meta(&tx, sym_id, &card.module, &card.qualname, ctor)?;
                for (ord, base) in card.bases.iter().enumerate() {
                    let base_id = ids.get(base).copied();
                    repo::insert_class_base(&tx, sym_id, ord, base, base_id)?;
                }
            }
            written += 1;
        }

        // `if __name__ == "__main__"` 的正文属于模块入口那个伪符号，
        // 不写进去入口卡就是一张空卡
        if let Some(lines) = built.module_cards().get(module) {
            if let Some(&sym_id) = ids.get(super::entry::MODULE_ENTRY_QUALNAME) {
                for (ord, line) in lines.iter().enumerate() {
                    repo::insert_card_line(&tx, sym_id, ord, line)?;
                }
            }
        }

        for (cs_id, call) in built.calls() {
            if !call.from.starts_with(&prefix) {
                continue;
            }
            let owner_id = call
                .from
                .split_once(':')
                .and_then(|(_, q)| ids.get(q))
                .copied();
            // 目标可能在别的模块，pick 会先查本模块再查外部映射
            let dst_id = call
                .to
                .as_deref()
                .and_then(|to| pick(to, &prefix, &ids, &external));
            repo::insert_call_site(&tx, self.project_id, cs_id, module, owner_id, dst_id, call)?;
            for (param, expr) in &call.args {
                repo::insert_call_arg(&tx, cs_id, param, expr)?;
            }
            for (ord, candidate) in call.candidates.iter().enumerate() {
                let sym = pick(candidate, &prefix, &ids, &external);
                repo::insert_call_candidate(&tx, cs_id, ord, candidate, sym)?;
            }
        }

        tx.commit()?;
        Ok(written)
    }
}

/// 模块名到卡片 id 的映射工具，物化入口用它从一张卡找到所属文件。
pub fn module_of_card(card_id: &str) -> Option<&str> {
    card_id.split_once(':').map(|(m, _)| m)
}

#[allow(dead_code)]
fn kind_is_class(kind: SymbolKind) -> bool {
    kind == SymbolKind::Class
}
