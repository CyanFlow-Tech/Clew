//! DDL for the index database.

pub const DDL: &str = r#"
CREATE TABLE IF NOT EXISTS project (
  id INTEGER PRIMARY KEY,
  path TEXT NOT NULL UNIQUE,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS symbol (
  id INTEGER PRIMARY KEY,
  project_id INTEGER NOT NULL,
  module TEXT NOT NULL,
  qualname TEXT NOT NULL,
  kind TEXT NOT NULL,
  src_hash TEXT,
  def_line INTEGER NOT NULL,
  def_col INTEGER NOT NULL,
  body_start INTEGER NOT NULL,
  body_end INTEGER NOT NULL,
  is_third_party INTEGER NOT NULL DEFAULT 0,
  is_entry INTEGER NOT NULL DEFAULT 0,
  entry_confidence TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS ux_symbol ON symbol(project_id, module, qualname);

CREATE TABLE IF NOT EXISTS edge (
  src_symbol INTEGER NOT NULL,
  dst_symbol INTEGER NOT NULL,
  kind TEXT NOT NULL,
  confidence TEXT NOT NULL,
  source TEXT NOT NULL,
  call_line INTEGER,
  PRIMARY KEY (src_symbol, dst_symbol, kind, call_line)
);
CREATE INDEX IF NOT EXISTS ix_edge_src ON edge(src_symbol);
CREATE INDEX IF NOT EXISTS ix_edge_dst ON edge(dst_symbol);

CREATE TABLE IF NOT EXISTS session (
  id INTEGER PRIMARY KEY,
  project_id INTEGER NOT NULL,
  root_symbol INTEGER,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS visit (
  session_id INTEGER NOT NULL,
  symbol_id INTEGER NOT NULL,
  parent_symbol_id INTEGER,
  ts INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS note (
  id INTEGER PRIMARY KEY,
  symbol_id INTEGER NOT NULL,
  text TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS symbol_alias (
  old_symbol_id INTEGER NOT NULL,
  new_symbol_id INTEGER NOT NULL,
  reason TEXT NOT NULL,
  PRIMARY KEY (old_symbol_id, new_symbol_id)
);
"#;