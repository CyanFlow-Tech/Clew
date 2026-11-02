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

CREATE TABLE IF NOT EXISTS module (
  id INTEGER PRIMARY KEY,
  project_id INTEGER NOT NULL,
  module TEXT NOT NULL,
  path TEXT NOT NULL,
  src_hash TEXT NOT NULL,
  is_third_party INTEGER NOT NULL DEFAULT 0,
  has_main_guard INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX IF NOT EXISTS ux_module ON module(project_id, module);

-- card title parameters, in signature order
CREATE TABLE IF NOT EXISTS card_param (
  symbol_id INTEGER NOT NULL,
  ord INTEGER NOT NULL,
  name TEXT NOT NULL,
  PRIMARY KEY (symbol_id, ord)
);

-- one row per rendered card line, in card order
CREATE TABLE IF NOT EXISTS card_line (
  symbol_id INTEGER NOT NULL,
  ord INTEGER NOT NULL,
  kind TEXT NOT NULL,
  text TEXT NOT NULL,
  indent INTEGER NOT NULL,
  ln INTEGER NOT NULL,
  call_id TEXT,
  PRIMARY KEY (symbol_id, ord)
);

-- every call site, resolved or not, so an anchor never points at a missing row
CREATE TABLE IF NOT EXISTS call_site (
  id TEXT PRIMARY KEY,
  project_id INTEGER NOT NULL,
  symbol_id INTEGER,
  module TEXT NOT NULL,
  owner TEXT NOT NULL,
  target TEXT NOT NULL,
  kind TEXT NOT NULL,
  recv TEXT,
  confidence TEXT NOT NULL,
  dst_symbol INTEGER,
  imported TEXT,
  typed TEXT,
  call_line INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_call_site_symbol ON call_site(symbol_id);
CREATE INDEX IF NOT EXISTS ix_call_site_dst ON call_site(dst_symbol);

CREATE TABLE IF NOT EXISTS call_arg (
  call_id TEXT NOT NULL,
  param TEXT NOT NULL,
  expr TEXT NOT NULL,
  PRIMARY KEY (call_id, param)
);

CREATE TABLE IF NOT EXISTS call_candidate (
  call_id TEXT NOT NULL,
  ord INTEGER NOT NULL,
  card_id TEXT NOT NULL,
  symbol_id INTEGER,
  PRIMARY KEY (call_id, ord)
);

CREATE TABLE IF NOT EXISTS import (
  project_id INTEGER NOT NULL,
  module TEXT NOT NULL,
  local TEXT NOT NULL,
  orig TEXT NOT NULL,
  source_module TEXT NOT NULL,
  is_alias INTEGER NOT NULL DEFAULT 0,
  is_module_import INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (project_id, module, local)
);

CREATE TABLE IF NOT EXISTS class_meta (
  symbol_id INTEGER PRIMARY KEY,
  module TEXT NOT NULL,
  qualname TEXT NOT NULL,
  ctor_symbol INTEGER
);

CREATE TABLE IF NOT EXISTS class_base (
  symbol_id INTEGER NOT NULL,
  ord INTEGER NOT NULL,
  base_name TEXT NOT NULL,
  base_symbol INTEGER,
  PRIMARY KEY (symbol_id, ord)
);

CREATE TABLE IF NOT EXISTS local_type (
  symbol_id INTEGER NOT NULL,
  var TEXT NOT NULL,
  class_card TEXT NOT NULL,
  class_symbol INTEGER,
  PRIMARY KEY (symbol_id, var)
);
"#;