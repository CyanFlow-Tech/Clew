use std::collections::BTreeMap;

use crate::scan::model::*;
use crate::utils::syntax::class_of_qualname;
use crate::utils::text;
use crate::card::CardId;
use super::super::builder::Builder;
use super::super::types::*;


const MAX_CANDIDATES: usize = 8;

pub(crate) fn import_label(imp: &ImportInfo) -> String {
    let suffix = if imp.alias {
        format!(" as {}", imp.local)
    } else {
        String::new()
    };
    if imp.module_import {
        format!("import {}{}", imp.module, suffix)
    } else {
        format!("from {} import {}{}", imp.module, imp.orig, suffix)
    }
}

#[derive(Default)]

pub(crate) struct Resolution {
    pub chosen: Option<String>,
    pub conf: Confidence,
    pub candidates: Vec<String>,
    pub imported: Option<String>,
    pub typed: Option<String>,
}

pub(crate) struct Caller<'a> {
    from: &'a str,
    module: String,
    qualname: Option<String>,
}

impl<'a> Caller<'a> {
    pub(crate) fn new(from: &'a str, cards: &BTreeMap<String, Card>) -> Self {
        Self {
            from,
            module: CardId::module_of(from).unwrap_or("").to_string(),
            qualname: cards.get(from).map(|c| c.qualname.clone()),
        }
    }
}

pub(crate) fn bind_args(
    cards: &BTreeMap<String, Card>,
    raw: &RawCall,
    to: &str,
) -> BTreeMap<String, String> {
    let mut args = BTreeMap::new();
    let Some(target_card) = cards.get(to) else {
        return args;
    };
    for (i, expr) in raw.pos.iter().enumerate() {
        if let Some(p) = target_card.params.get(i) {
            args.insert(p.clone(), expr.clone());
        }
    }
    for (k, v) in &raw.kwargs {
        if target_card.params.contains(k) {
            args.insert(k.clone(), v.clone());
        }
    }
    args
}

pub(crate) struct Resolver<'a> {
    pub(crate) b: &'a Builder,
    pub(crate) by_name: &'a BTreeMap<String, Vec<String>>,
    pub(crate) by_module_method: &'a BTreeMap<(String, String), Vec<String>>,
    pub(crate) by_method: &'a BTreeMap<String, Vec<String>>,
    pub(crate) card_locals: &'a BTreeMap<String, BTreeMap<String, String>>,
    pub(crate) class_attrs: &'a BTreeMap<String, BTreeMap<String, String>>,
}

impl Resolver<'_> {
    pub(crate) fn resolve(&self, raw: &RawCall, caller: &Caller) -> Resolution {
        if super::super::types::BUILTINS.contains(&raw.target.as_str()) {
            return Resolution {
                conf: Confidence::Builtin,
                ..Default::default()
            };
        }
        match raw.kind {
            CallKind::SelfRef => self.self_ref(raw, caller),
            CallKind::SelfAttr => self.self_attr(raw, caller),
            CallKind::Attr => self.attr(raw, caller),
            CallKind::Name => self.name(raw, caller),
            CallKind::Other => Resolution::default(),
        }
    }

    pub(crate) fn self_ref(&self, raw: &RawCall, caller: &Caller) -> Resolution {
        let mut r = Resolution::default();
        let Some(qualname) = &caller.qualname else {
            return r;
        };
        let Some(cls) = class_of_qualname(qualname) else {
            return r;
        };
        let Some(mid) = self
            .b
            .graph
            .methods
            .get(&(caller.module.clone(), cls))
            .and_then(|m| m.get(&raw.target))
        else {
            return r;
        };
        r.chosen = Some(mid.clone());
        let cands = self
            .by_module_method
            .get(&(caller.module.clone(), raw.target.clone()));
        match cands.map(|c| c.len()).unwrap_or(0) {
            n if n > 1 => {
                r.conf = Confidence::Poly;
                r.candidates = cands
                    .map(|c| c.iter().take(MAX_CANDIDATES).cloned().collect())
                    .unwrap_or_default();
            }
            _ => r.conf = Confidence::Exact,
        }
        r
    }

    pub(crate) fn self_attr(&self, raw: &RawCall, caller: &Caller) -> Resolution {
        let mut r = Resolution::default();
        let (Some(qualname), Some(attr)) = (&caller.qualname, &raw.recv) else {
            return r;
        };
        let Some(cls) = class_of_qualname(qualname) else {
            return r;
        };
        let Some(cls_id) = self
            .b
            .graph
            .class_ids
            .get(&(caller.module.clone(), cls))
            .cloned()
        else {
            return r;
        };
        let Some(attr_cls) = self
            .class_attrs
            .get(&cls_id)
            .and_then(|a| a.get(attr))
            .cloned()
        else {
            return r;
        };
        let Some(mid) = self.b.find_method(&attr_cls, &raw.target, 0) else {
            return r;
        };
        r.chosen = Some(mid);
        r.conf = Confidence::Exact;
        r.typed = Some(format!("{attr}: {}", text::short_name(&attr_cls)));
        r
    }

    pub(crate) fn attr(&self, raw: &RawCall, caller: &Caller) -> Resolution {
        let mut r = Resolution::default();
        self.attr_from_locals(raw, caller, &mut r);
        if r.chosen.is_none() {
            self.attr_from_import(raw, caller, &mut r);
        }
        if r.chosen.is_none() {
            self.attr_from_ctor(raw, caller, &mut r);
        }
        if r.chosen.is_none() {
            self.attr_by_method(raw, &mut r);
        }
        r
    }

    pub(crate) fn attr_from_locals(&self, raw: &RawCall, caller: &Caller, r: &mut Resolution) {
        let Some(recv) = &raw.recv else {
            return;
        };
        let Some(target_cls) = self
            .card_locals
            .get(caller.from)
            .and_then(|m| m.get(recv))
            .cloned()
        else {
            return;
        };
        let Some(mid) = self.b.find_method(&target_cls, &raw.target, 0) else {
            return;
        };
        r.chosen = Some(mid);
        r.conf = Confidence::Exact;
        r.typed = Some(format!("{recv}: {}", text::short_name(&target_cls)));
    }

    pub(crate) fn attr_from_import(&self, raw: &RawCall, caller: &Caller, r: &mut Resolution) {
        let Some(recv) = &raw.recv else {
            return;
        };
        let Some(imp) = self.b.import_of(caller.from, &caller.module, recv).cloned() else {
            return;
        };
        let owners: Vec<String> = if imp.module_import {
            vec![imp.module.clone()]
        } else {
            vec![
                format!("{}.{}", imp.module, imp.orig),
                imp.module.clone(),
            ]
        };
        for owner in &owners {
            let want = Graph::card_id(owner, &raw.target);
            if self.b.card_known(&want, &raw.target, self.by_name) {
                r.chosen = Some(want);
                r.conf = Confidence::Exact;
                r.imported = Some(import_label(&imp));
                return;
            }
        }
        let cls_id = Graph::card_id(&imp.module, &imp.orig);
        if let Some(mid) = self.b.find_method(&cls_id, &raw.target, 0) {
            r.chosen = Some(mid);
            r.conf = Confidence::Exact;
            r.imported = Some(import_label(&imp));
        }
    }

    pub(crate) fn attr_from_ctor(&self, raw: &RawCall, caller: &Caller, r: &mut Resolution) {
        let Some(ctor) = &raw.ctor else {
            return;
        };
        match self.b.resolve_class(caller.from, ctor) {
            Some(cls_id) => {
                if let Some(mid) = self.b.find_method(&cls_id, &raw.target, 0) {
                    r.chosen = Some(mid);
                    r.conf = Confidence::Exact;
                    r.typed = Some(format!("{ctor}() → {}", text::short_name(&cls_id)));
                }
            }
            None => {
                if let Some(imp) = self.b.import_of(caller.from, &caller.module, ctor) {
                    r.typed = Some(format!("{ctor}() → 外部（{}）", imp.module));
                    r.imported = Some(import_label(imp));
                }
            }
        }
    }

    pub(crate) fn attr_by_method(&self, raw: &RawCall, r: &mut Resolution) {
        let Some(list) = self.by_method.get(&raw.target) else {
            return;
        };
        if list.len() == 1 {
            r.chosen = Some(list[0].clone());
            r.conf = Confidence::Heur;
        } else if list.len() > 1 {
            r.candidates = list.iter().take(MAX_CANDIDATES).cloned().collect();
            r.conf = Confidence::Poly;
        }
    }

    pub(crate) fn name(&self, raw: &RawCall, caller: &Caller) -> Resolution {
        let mut r = Resolution::default();
        let same = Graph::card_id(&caller.module, &raw.target);
        let mut label_from_import = None;
        if self.b.graph.cards.contains_key(&same) {
            r.chosen = Some(same);
            r.conf = Confidence::Exact;
        } else if let Some(imp) = self.b.import_of(caller.from, &caller.module, &raw.target) {
            let target_id = Graph::card_id(&imp.module, &imp.orig);
            if self.b.card_known(&target_id, &raw.target, self.by_name) {
                r.chosen = Some(target_id);
                r.conf = Confidence::Exact;
            }
            label_from_import = Some(import_label(imp));
        }
        if r.chosen.is_none() {
            if let Some(hit) = self.by_name.get(&raw.target) {
                if hit.len() == 1 {
                    r.chosen = Some(hit[0].clone());
                    r.conf = Confidence::Heur;
                }
            }
        }
        if r.imported.is_none() {
            r.imported = label_from_import;
        }
        r
    }
}
