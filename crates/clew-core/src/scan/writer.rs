use std::collections::{BTreeMap, HashMap};

use super::graph_build::ClassMeta;
use super::model::{Call, Card, Line};
use crate::card::{CardId, MODULE_ENTRY_QUALNAME};
use crate::store::repo;

pub struct SymbolIds {
    module: String,
    local: HashMap<String, i64>,
    external: HashMap<(String, String), i64>,
}

impl SymbolIds {
    pub fn new(
        module: impl Into<String>,
        local: HashMap<String, i64>,
        external: HashMap<(String, String), i64>,
    ) -> Self {
        Self {
            module: module.into(),
            local,
            external,
        }
    }

    pub fn get(&self, module: &str, qualname: &str) -> Option<i64> {
        if module == self.module.as_str() {
            self.local.get(qualname).copied()
        } else {
            self.external
                .get(&(module.to_string(), qualname.to_string()))
                .copied()
        }
    }

    pub fn get_card(&self, card_id: &str) -> Option<i64> {
        match CardId::parse(card_id) {
            Some(id) => self.get(id.module(), id.qualname()),
            None => None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CallCounts {
    pub sites: usize,
    pub resolved: usize,
    pub args: usize,
    pub candidates: usize,
}

pub struct IndexWriter<'a> {
    tx: &'a rusqlite::Connection,
    project_id: i64,
    ids: &'a SymbolIds,
}

impl<'a> IndexWriter<'a> {
    pub fn new(tx: &'a rusqlite::Connection, project_id: i64, ids: &'a SymbolIds) -> Self {
        Self {
            tx,
            project_id,
            ids,
        }
    }

    pub fn cards<K>(&self, cards: &BTreeMap<String, Card>, keep: K) -> anyhow::Result<usize>
    where
        K: Fn(&str) -> bool,
    {
        let mut written = 0;
        for (card_id, card) in cards {
            if !keep(card_id) {
                continue;
            }
            let Some(sym_id) = self.ids.get(&card.module, &card.qualname) else {
                continue;
            };
            for (ord, name) in card.params.iter().enumerate() {
                repo::insert_card_param(self.tx, sym_id, ord, name)?;
            }
            for (ord, line) in card.lines.iter().enumerate() {
                repo::insert_card_line(self.tx, sym_id, ord, line)?;
            }
            for (var, class_card) in &card.locals {
                let class_symbol = self.ids.get_card(class_card);
                repo::insert_local_type(self.tx, sym_id, var, class_card, class_symbol)?;
            }
            written += 1;
        }
        Ok(written)
    }

    pub fn module_entry_body(&self, module: &str, lines: &[Line]) -> anyhow::Result<()> {
        let Some(sym_id) = self.ids.get(module, MODULE_ENTRY_QUALNAME) else {
            return Ok(());
        };
        for (ord, line) in lines.iter().enumerate() {
            repo::insert_card_line(self.tx, sym_id, ord, line)?;
        }
        Ok(())
    }

    pub fn classes<K>(&self, classes: &[ClassMeta], keep: K) -> anyhow::Result<()>
    where
        K: Fn(&str) -> bool,
    {
        for class in classes {
            if !keep(&class.card_id) {
                continue;
            }
            let Some(sym_id) = self.ids.get(&class.module, &class.qualname) else {
                continue;
            };
            let ctor = class.ctor.as_deref().and_then(|id| self.ids.get_card(id));
            repo::insert_class_meta(self.tx, sym_id, &class.module, &class.qualname, ctor)?;
            for (ord, base) in class.bases.iter().enumerate() {
                let base_id = self.ids.get(&class.module, base);
                repo::insert_class_base(self.tx, sym_id, ord, base, base_id)?;
            }
        }
        Ok(())
    }

    pub fn calls<K>(&self, calls: &BTreeMap<String, Call>, keep: K) -> anyhow::Result<CallCounts>
    where
        K: Fn(&str) -> bool,
    {
        let mut counts = CallCounts::default();
        for (cs_id, call) in calls {
            if !keep(&call.from) {
                continue;
            }
            let module = CardId::module_of(&call.from).unwrap_or("");
            let owner_id = self.ids.get_card(&call.from);
            let dst_id = call.to.as_deref().and_then(|to| self.ids.get_card(to));
            repo::insert_call_site(self.tx, self.project_id, cs_id, module, owner_id, dst_id, call)?;
            counts.sites += 1;
            if dst_id.is_some() {
                counts.resolved += 1;
            }
            for (param, expr) in &call.args {
                repo::insert_call_arg(self.tx, cs_id, param, expr)?;
                counts.args += 1;
            }
            for (ord, candidate) in call.candidates.iter().enumerate() {
                let sym = self.ids.get_card(candidate);
                repo::insert_call_candidate(self.tx, cs_id, ord, candidate, sym)?;
                counts.candidates += 1;
            }
        }
        Ok(counts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> SymbolIds {
        let mut local = HashMap::new();
        local.insert("f".to_string(), 1);
        let mut external = HashMap::new();
        external.insert(("b".to_string(), "g".to_string()), 2);
        SymbolIds::new("a", local, external)
    }

    #[test]
    fn the_local_module_wins() {
        assert_eq!(ids().get("a", "f"), Some(1));
    }

    #[test]
    fn another_module_goes_through_the_external_map() {
        assert_eq!(ids().get("b", "g"), Some(2));
        assert_eq!(ids().get("c", "g"), None);
    }

    #[test]
    fn a_card_id_resolves_through_both_maps() {
        assert_eq!(ids().get_card("a:f"), Some(1));
        assert_eq!(ids().get_card("b:g"), Some(2));
        assert_eq!(ids().get_card("no-colon"), None);
    }

    #[test]
    fn a_name_missing_from_the_local_module_does_not_fall_back() {
        assert_eq!(ids().get("a", "g"), None);
    }
}
