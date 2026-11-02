//! Read-side views for interactive descent.
//!
//! Everything here is a plain query over the index DB: `walk` and the future GUI
//! both read the same shapes, so resolution never has to run twice.

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};

/// One call on the caller's line, with what it bound.
#[derive(Debug, Clone)]
pub struct CallHit {
    pub call_id: String,
    pub ln: i64,
    pub confidence: String,
    pub args: Vec<(String, String)>,
    /// Candidate implementations when dispatch is dynamic
    pub candidates: Vec<String>,
}

/// A resolved callee plus every call site that reaches it.
#[derive(Debug, Clone)]
pub struct EdgeView {
    pub dst: i64,
    pub module: String,
    pub qualname: String,
    pub kind: String,
    pub is_third_party: bool,
    pub is_entry: bool,
    pub hits: Vec<CallHit>,
}

impl EdgeView {
    /// Last path segment of the qualname.
    pub fn short(&self) -> &str {
        self.qualname.rsplit('.').next().unwrap_or(&self.qualname)
    }
}

/// A node as the tree view needs it.
#[derive(Debug, Clone)]
pub struct NodeView {
    pub id: i64,
    pub module: String,
    pub qualname: String,
    pub kind: String,
    pub params: Vec<String>,
    pub is_third_party: bool,
    pub is_entry: bool,
    pub entry_confidence: Option<String>,
    /// Resolved outgoing edges, in first-call order
    pub edges: Vec<EdgeView>,
    /// Calls that reached a builtin or a library function
    pub builtin_calls: usize,
    /// Calls nothing could be said about
    pub unresolved_calls: usize,
}

impl NodeView {
    /// `Class.method(a, b)` style heading.
    pub fn title(&self) -> String {
        format!("{}({})", self.qualname, self.params.join(", "))
    }

    pub fn short(&self) -> &str {
        self.qualname.rsplit('.').next().unwrap_or(&self.qualname)
    }

    /// Total calls counted at this node, resolved or not.
    pub fn total_calls(&self) -> usize {
        self.edges.iter().map(|e| e.hits.len()).sum::<usize>()
            + self.builtin_calls
            + self.unresolved_calls
    }
}

/// Load one node with its resolved outgoing edges.
pub fn node(conn: &Connection, id: i64) -> rusqlite::Result<Option<NodeView>> {
    let head = conn
        .query_row(
            "SELECT module, qualname, kind, is_third_party, is_entry, entry_confidence
               FROM symbol WHERE id = ?1",
            params![id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)? != 0,
                    row.get::<_, i64>(4)? != 0,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .optional()?;

    let Some((module, qualname, kind, is_third_party, is_entry, entry_confidence)) = head else {
        return Ok(None);
    };

    let mut stmt = conn.prepare("SELECT name FROM card_param WHERE symbol_id = ?1 ORDER BY ord")?;
    let params_list: Vec<String> = stmt
        .query_map(params![id], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;

    let mut args: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT a.call_id, a.param, a.expr
           FROM call_arg a JOIN call_site c ON c.id = a.call_id
          WHERE c.symbol_id = ?1
          ORDER BY a.call_id, a.param",
    )?;
    for row in stmt.query_map(params![id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })? {
        let (call_id, param, expr) = row?;
        args.entry(call_id).or_default().push((param, expr));
    }

    let mut candidates: HashMap<String, Vec<String>> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT cc.call_id, cc.card_id
           FROM call_candidate cc JOIN call_site c ON c.id = cc.call_id
          WHERE c.symbol_id = ?1
          ORDER BY cc.call_id, cc.ord",
    )?;
    for row in stmt.query_map(params![id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })? {
        let (call_id, card_id) = row?;
        candidates.entry(call_id).or_default().push(card_id);
    }

    let mut stmt = conn.prepare(
        "SELECT c.id, c.dst_symbol, s.module, s.qualname, s.kind, s.is_third_party,
                s.is_entry, c.confidence, c.call_line
           FROM call_site c JOIN symbol s ON s.id = c.dst_symbol
          WHERE c.symbol_id = ?1
          ORDER BY c.call_line, c.id",
    )?;
    let rows = stmt.query_map(params![id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)? != 0,
            row.get::<_, i64>(6)? != 0,
            row.get::<_, String>(7)?,
            row.get::<_, i64>(8)?,
        ))
    })?;

    let mut edges: Vec<EdgeView> = Vec::new();
    let mut slot: HashMap<i64, usize> = HashMap::new();
    for row in rows {
        let (call_id, dst, m, q, k, third, dst_entry, conf, ln) = row?;
        let hit = CallHit {
            candidates: candidates.remove(&call_id).unwrap_or_default(),
            args: args.remove(&call_id).unwrap_or_default(),
            call_id,
            ln,
            confidence: conf,
        };
        match slot.get(&dst) {
            Some(&i) => edges[i].hits.push(hit),
            None => {
                slot.insert(dst, edges.len());
                edges.push(EdgeView {
                    dst,
                    module: m,
                    qualname: q,
                    kind: k,
                    is_third_party: third,
                    is_entry: dst_entry,
                    hits: vec![hit],
                });
            }
        }
    }

    let mut builtin_calls = 0usize;
    let mut unresolved_calls = 0usize;
    let mut stmt = conn.prepare(
        "SELECT confidence, count(*) FROM call_site
          WHERE symbol_id = ?1 AND dst_symbol IS NULL GROUP BY confidence",
    )?;
    for row in stmt.query_map(params![id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })? {
        let (conf, n) = row?;
        if conf == "builtin" {
            builtin_calls += n as usize;
        } else {
            unresolved_calls += n as usize;
        }
    }

    Ok(Some(NodeView {
        id,
        module,
        qualname,
        kind,
        params: params_list,
        is_third_party,
        is_entry,
        entry_confidence,
        edges,
        builtin_calls,
        unresolved_calls,
    }))
}

/// Entry points of a project as `(symbol_id, module, qualname, confidence)`.
pub fn entry_points(
    conn: &Connection,
    project_id: i64,
) -> rusqlite::Result<Vec<(i64, String, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, module, qualname, COALESCE(entry_confidence, '')
           FROM symbol
          WHERE project_id = ?1 AND is_entry = 1
          ORDER BY module, qualname",
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    rows.collect()
}

/// Resolve `[module:]qualname` to a symbol id.
pub fn find(conn: &Connection, project_id: i64, needle: &str) -> rusqlite::Result<Vec<i64>> {
    let (module, qualname) = match needle.split_once(':') {
        Some((m, q)) => (Some(m), q),
        None => (None, needle),
    };
    let mut stmt = conn.prepare(
        "SELECT id FROM symbol
          WHERE project_id = ?1 AND qualname = ?2 AND (?3 IS NULL OR module = ?3)
          ORDER BY id",
    )?;
    let rows = stmt.query_map(params![project_id, qualname, module], |row| {
        row.get::<_, i64>(0)
    })?;
    rows.collect()
}

/// All notes attached to a node, newest first.
pub fn notes(conn: &Connection, symbol_id: i64) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT text FROM note WHERE symbol_id = ?1 ORDER BY updated_at DESC, id DESC",
    )?;
    let rows = stmt.query_map(params![symbol_id], |row| row.get::<_, String>(0))?;
    rows.collect()
}

/// One rendered source line of a card, with the call it anchors.
#[derive(Debug, Clone)]
pub struct CodeLine {
    pub kind: String,
    pub text: String,
    pub indent: i64,
    pub ln: i64,
    pub call_target: Option<String>,
    pub confidence: Option<String>,
}

/// The card body of a node, in display order.
pub fn card_lines(conn: &Connection, symbol_id: i64) -> rusqlite::Result<Vec<CodeLine>> {
    let mut stmt = conn.prepare(
        "SELECT l.kind, l.text, l.indent, l.ln, c.target, c.confidence
           FROM card_line l LEFT JOIN call_site c ON c.id = l.call_id
          WHERE l.symbol_id = ?1
          ORDER BY l.ord",
    )?;
    let rows = stmt.query_map(params![symbol_id], |row| {
        Ok(CodeLine {
            kind: row.get(0)?,
            text: row.get(1)?,
            indent: row.get(2)?,
            ln: row.get(3)?,
            call_target: row.get(4)?,
            confidence: row.get(5)?,
        })
    })?;
    rows.collect()
}
