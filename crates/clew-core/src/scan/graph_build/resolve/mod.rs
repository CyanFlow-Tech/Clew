use std::collections::BTreeMap;
use std::sync::Arc;

use super::super::model::*;
use crate::card::CardId;
use super::builder::Builder;
use super::types::*;
pub(crate) mod kinds;

use kinds::{bind_args, Caller, Resolver};

impl Builder {
    pub(crate) fn resolve(&mut self) {
        let by_name: Arc<BTreeMap<String, Vec<String>>> = match &self.global.by_name {
            Some(shared) => shared.clone(),
            None => Arc::new({
                let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for (id, card) in &self.graph.cards {
                    let bare = card
                        .qualname
                        .rsplit('.')
                        .next()
                        .unwrap_or(&card.qualname)
                        .to_string();
                    map.entry(bare).or_default().push(id.clone());
                }
                map
            }),
        };

        let (by_module_method, local_by_method) = {
            let mut local: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
            let mut global: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for ((module, _class), ms) in &self.graph.methods {
                for (name, id) in ms {
                    local
                        .entry((module.clone(), name.clone()))
                        .or_default()
                        .push(id.clone());
                    global.entry(name.clone()).or_default().push(id.clone());
                }
            }
            for v in local.values_mut() {
                v.sort();
                v.dedup();
            }
            for v in global.values_mut() {
                v.sort();
                v.dedup();
            }
            (local, global)
        };
        let by_method: Arc<BTreeMap<String, Vec<String>>> = match &self.global.by_method {
            Some(shared) => shared.clone(),
            None => Arc::new(local_by_method),
        };

        let mut card_locals: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for (card_id, vars) in self.graph.card_locals.clone() {
            let mut resolved = BTreeMap::new();
            for (var, raw) in vars {
                if let Some(cls) = self.resolve_class(&card_id, &raw) {
                    resolved.insert(var, cls);
                }
            }
            if !resolved.is_empty() {
                card_locals.insert(card_id, resolved);
            }
        }

        let mut class_attrs: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for (cls_id, attrs) in self.graph.class_attrs.clone() {
            let mut resolved = BTreeMap::new();
            for (attr, raw) in attrs {
                if let Some(target) = self.resolve_class(&cls_id, &raw) {
                    resolved.insert(attr, target);
                }
            }
            class_attrs.insert(cls_id, resolved);
        }

        let raw_calls = std::mem::take(&mut self.graph.raw_calls);
        for (cs_id, raw) in raw_calls {
            let mut call = self.graph.calls.remove(&cs_id).unwrap_or(Call {
                from: String::new(),
                target: String::new(),
                kind: CallKind::Other,
                recv: None,
                args: BTreeMap::new(),
                ln: 0,
                conf: Confidence::None,
                to: None,
                candidates: Vec::new(),
                imported: None,
                typed: None,
            });
            call.target = raw.target.clone();
            call.kind = raw.kind;
            call.recv = raw.recv.clone();
            call.ln = raw.line;

            let caller = Caller::new(&call.from, &self.graph.cards);
            let resolution = {
                let resolver = Resolver {
                    b: self,
                    by_name: by_name.as_ref(),
                    by_module_method: &by_module_method,
                    by_method: by_method.as_ref(),
                    card_locals: &card_locals,
                    class_attrs: &class_attrs,
                };
                resolver.resolve(&raw, &caller)
            };

            if let Some(to) = &resolution.chosen {
                call.args = bind_args(&self.graph.cards, &raw, to);
            }
            call.to = resolution.chosen;
            call.conf = resolution.conf;
            call.candidates = resolution.candidates;
            call.imported = resolution.imported;
            call.typed = resolution.typed;
            self.graph.calls.insert(cs_id, call);
        }

        for (card_id, vars) in card_locals {
            if let Some(card) = self.graph.cards.get_mut(&card_id) {
                card.locals = vars;
            }
        }
    }


    /// 这个 id 是不是一个类。类卡可能还没物化（按需模式下只读了几个文件），
    /// 那就查全项目的类表。
    pub(crate) fn is_class(&self, id: &str) -> bool {
        if let Some(c) = self.graph.cards.get(id) {
            return c.kind == CardKind::Class;
        }
        let qualname = CardId::qualname_of(id).unwrap_or(id);
        let bare = qualname.rsplit('.').next().unwrap_or(qualname);
        self.global.by_class
            .as_ref()
            .and_then(|m| m.get(bare))
            .map_or(false, |ids| ids.iter().any(|x| x == id))
    }

    /// bare 类名 -> 类卡 id，只接受唯一命中。
    pub(crate) fn global_class(&self, name: &str) -> Option<String> {
        let ids = self.global.by_class.as_ref()?.get(name)?;
        if ids.len() == 1 {
            return Some(ids[0].clone());
        }
        None
    }

    pub(crate) fn resolve_class(&self, from_card: &str, raw: &str) -> Option<String> {
        let module = CardId::module_of(from_card).unwrap_or("");
        let direct = Graph::card_id(module, raw);
        if self.is_class(&direct) {
            return Some(direct);
        }
        if let Some(imp) = self.import_of(from_card, module, raw) {
            let id = Graph::card_id(&imp.module, &imp.orig);
            if self.is_class(&id) {
                return Some(id);
            }
        }
        self.graph.class_by_qualname
            .get(raw)
            .cloned()
            .or_else(|| self.global_class(raw))
    }

    /// 名称在这个位置解析到哪个导入：先看所在卡片自己的 import，
    /// 再看模块的。函数体里 `from tools import skill_ledger` 不会出现在模块表里。
    pub(crate) fn import_of(&self, from_card: &str, module: &str, name: &str) -> Option<&ImportInfo> {
        self.graph.card_imports
            .get(from_card)
            .and_then(|m| m.get(name))
            .or_else(|| self.graph.imports.get(module).and_then(|m| m.get(name)))
    }

    /// 这张卡在项目里存在吗。按需物化时别的模块还没解析，只看 `self.graph.cards`
    /// 会把项目里确实存在的目标判成不存在，所以再查一次全项目的名字表。
    pub(crate) fn card_known(&self, id: &str, bare: &str, by_name: &BTreeMap<String, Vec<String>>) -> bool {
        if self.graph.cards.contains_key(id) {
            return true;
        }
        by_name
            .get(bare)
            .map_or(false, |ids| ids.iter().any(|x| x == id))
    }

    pub(crate) fn find_method(&self, class_id: &str, name: &str, depth: usize) -> Option<String> {
        if depth > 6 {
            return None;
        }
        let Some(card) = self.graph.cards.get(class_id) else {
            // 类卡还没物化：按全项目的 qualname 直接找这个方法，
            // 只接受唯一命中，继承链这时候查不到。
            let cid = CardId::parse(class_id)?;
            let want = format!("{}.{name}", cid.qualname());
            let ids = self.global.by_qualname.as_ref()?.get(&want)?;
            return if ids.len() == 1 {
                Some(ids[0].clone())
            } else {
                None
            };
        };
        if let Some(mid) = self
            .graph
            .methods
            .get(&(card.module.clone(), card.qualname.clone()))
            .and_then(|m| m.get(name))
        {
            return Some(mid.clone());
        }
        let bases = self.graph.class_bases.get(class_id)?.1.clone();
        for base in bases {
            if let Some(base_id) = self.graph.class_ids.get(&(card.module.clone(), base)) {
                if let Some(hit) = self.find_method(base_id, name, depth + 1) {
                    return Some(hit);
                }
            }
        }
        None
    }
}
