use std::collections::BTreeMap;

use serde::Serialize;

use crate::scan::model::{Call, Card};

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub cards: BTreeMap<String, Card>,
    pub calls: BTreeMap<String, Call>,
    pub entries: Vec<Entry>,
}

/// What one expansion step needs: the requested cards, their outgoing call
/// sites, and enough global counts for the "hub" badge.
///
/// A class card brings its effective `__init__` along, because expanding a class
/// that has been instantiated also places the constructor card.
#[derive(Debug, Clone, Serialize)]
pub struct CardsPayload {
    pub cards: BTreeMap<String, Card>,
    pub calls: BTreeMap<String, Call>,
    /// card id -> call sites across the whole project that resolve to it
    pub fanin: BTreeMap<String, i64>,
    /// id -> name, for candidate targets the reader has not opened yet
    pub refs: BTreeMap<String, CardRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardRef {
    pub module: String,
    pub qualname: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub id: String,
    pub label: String,
}

