use std::collections::{BTreeMap, BTreeSet};

use super::super::model::*;
use super::types::*;

#[derive(Clone)]
pub struct Builder {
    pub(crate) src: Vec<u8>,
    pub(crate) module: String,
    pub(crate) is_package: bool,
    pub(crate) graph: RawGraph,
    pub(crate) module_cards: BTreeMap<String, Vec<Line>>,
    pub(crate) temp_counter: usize,
    pub(crate) symbols_only: bool,
    pub(crate) global: GlobalIndex,
    pub(crate) out_modules: Vec<ModuleMeta>,
    pub(crate) out_symbols: Vec<SymbolMeta>,
    pub(crate) out_classes: Vec<ClassMeta>,
}


impl Default for Builder {
    fn default() -> Self {
        Self {
            src: Vec::new(),
            module: String::new(),
            is_package: false,
            graph: RawGraph::default(),
            module_cards: BTreeMap::new(),
            temp_counter: 0,
            symbols_only: false,
            global: GlobalIndex::default(),
            out_modules: Vec::new(),
            out_symbols: Vec::new(),
            out_classes: Vec::new(),
        }
    }
}

impl Builder {
    /// A builder that emits full cards, used by on-demand materialisation.
    pub(crate) fn for_cards() -> Self {
        Self::default()
    }

    pub(crate) fn cards(&self) -> &BTreeMap<String, Card> {
        &self.graph.cards
    }

    pub(crate) fn calls(&self) -> &BTreeMap<String, Call> {
        &self.graph.calls
    }

    /// `__main__` guard bodies, keyed by module. They belong to the module entry
    /// pseudo symbol rather than to a card of their own.
    pub(crate) fn module_cards(&self) -> &BTreeMap<String, Vec<Line>> {
        &self.module_cards
    }

    pub(crate) fn classes(&self) -> &[ClassMeta] {
        &self.out_classes
    }

    /// Modules this file imports from, used to decide what to parse alongside it.
    pub(crate) fn imported_modules(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for table in self.graph.imports.values() {
            for info in table.values() {
                out.insert(info.module.clone());
            }
        }
        out
    }

    /// Fold another builder's per-file results into this one.
    ///
    /// Files are parsed in parallel, each into its own builder, then merged
    /// here before resolution. Every cross-file map keys on module names, so
    /// extending them is enough.
    pub(crate) fn merge(&mut self, other: Builder) {
        self.graph.cards.extend(other.graph.cards);
        self.graph.calls.extend(other.graph.calls);
        self.graph.raw_calls.extend(other.graph.raw_calls);
        for (k, v) in other.graph.imports {
            self.graph.imports.entry(k).or_default().extend(v);
        }
        for (k, v) in other.graph.methods {
            self.graph.methods.entry(k).or_default().extend(v);
        }
        self.graph.class_bases.extend(other.graph.class_bases);
        for (k, v) in other.graph.class_attrs {
            self.graph.class_attrs.entry(k).or_default().extend(v);
        }
        for (k, v) in other.graph.card_locals {
            self.graph.card_locals.entry(k).or_default().extend(v);
        }
        for (k, v) in other.graph.card_imports {
            self.graph.card_imports.entry(k).or_default().extend(v);
        }
        self.graph.class_ids.extend(other.graph.class_ids);
        for (k, v) in other.graph.class_by_qualname {
            self.graph.class_by_qualname.entry(k).or_insert(v);
        }
        for (k, v) in other.module_cards {
            self.module_cards.entry(k).or_default().extend(v);
        }
        self.out_modules.extend(other.out_modules);
        self.out_symbols.extend(other.out_symbols);
        self.out_classes.extend(other.out_classes);
    }
}
