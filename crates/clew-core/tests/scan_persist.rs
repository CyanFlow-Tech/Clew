//! End-to-end: one scan writes the graph into the index DB.

use std::path::Path;

use clew_core::scan::run::scan;
use clew_core::store::{open, repo};

const A_PY: &str = "\
class A:
    def __init__(self, x):
        self.x = x

    def run(self):
        return helper(self.x)


def helper(v):
    return v
";

const B_PY: &str = "\
from a import helper


if __name__ == \"__main__\":
    helper(1)
";

fn setup(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("clew-scan-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.py"), A_PY).unwrap();
    std::fs::write(dir.join("b.py"), B_PY).unwrap();
    // scan() stores the canonicalised root, so look it up the same way
    dir.canonicalize().unwrap()
}

#[test]
fn scan_persists_symbols_and_calls() {
    let root = setup("rows");
    let db_path = root.join("index.db");
    let mut conn = open(&db_path).unwrap();
    let stats = scan(&mut conn, &root, false).unwrap();

    assert_eq!(stats.modules, 2, "one row per source file");
    assert_eq!(stats.classes, 1);
    assert_eq!(stats.symbols, 4, "A, A.__init__, A.run, helper");
    assert_eq!(stats.entries_main_guard, 1);

    let project_id = repo::project_id_by_path(&conn, &root.to_string_lossy())
        .unwrap()
        .expect("project row");

    let entries = repo::list_entries(&conn, project_id).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].module, "b");
    assert_eq!(entries[0].qualname, "<module>");

    // `helper(self.x)` inside A.run resolves to the module-level helper
    let rows = repo::call_args_on_line(&conn, project_id, "A.run", 6).unwrap();
    assert_eq!(rows.len(), 1, "one argument binding on line 6");
    let row = &rows[0];
    assert_eq!(row.target, "helper");
    assert_eq!(row.confidence, "exact");
    assert_eq!(row.param.as_deref(), Some("v"));
    assert_eq!(row.expr.as_deref(), Some("self.x"));

    // class metadata: bases (none) and the `__init__` reference
    let ctor: i64 = conn
        .query_row(
            "SELECT ctor_symbol FROM class_meta WHERE qualname = 'A'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let ctor_qual: String = conn
        .query_row("SELECT qualname FROM symbol WHERE id = ?1", [ctor], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(ctor_qual, "A.__init__");

    // import table feeds cross-module resolution
    let imported: i64 = conn
        .query_row(
            "SELECT count(*) FROM import WHERE module = 'b' AND local = 'helper'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(imported, 1);

    // the `__main__` guard body becomes the card of the module entry symbol
    let guard_lines: i64 = conn
        .query_row(
            "SELECT count(*) FROM card_line l JOIN symbol s ON s.id = l.symbol_id
              WHERE s.kind = 'module' AND s.module = 'b'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(guard_lines, 1, "one statement inside the guard");

    let guard_calls: i64 = conn
        .query_row(
            "SELECT count(*) FROM call_site WHERE owner = '<module>'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(guard_calls, 1, "the guard's call is attributed to the module");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn rescan_replaces_rows_instead_of_duplicating() {
    let root = setup("rescan");
    let db_path = root.join("index.db");
    let mut conn = open(&db_path).unwrap();

    let first = scan(&mut conn, &root, false).unwrap();
    let second = scan(&mut conn, &root, false).unwrap();
    assert_eq!(first.symbols, second.symbols);
    assert_eq!(first.call_sites, second.call_sites);

    let project_id = repo::project_id_by_path(&conn, &root.to_string_lossy())
        .unwrap()
        .unwrap();
    // stats count definition symbols; `<module>` entry rows are extra
    let symbols: i64 = conn
        .query_row(
            "SELECT count(*) FROM symbol WHERE project_id = ?1 AND kind != 'module'",
            [project_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(symbols as usize, second.symbols);

    // a deleted line must not linger in the card after the second scan
    std::fs::write(root.join("b.py"), "from a import helper\n").unwrap();
    let third = scan(&mut conn, &root, false).unwrap();
    assert_eq!(third.entries_main_guard, 0);
    let entries = repo::list_entries(&conn, project_id).unwrap();
    assert!(entries.is_empty(), "stale entry survived the rescan: {entries:?}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn missing_project_reports_nothing() {
    let db = std::env::temp_dir().join(format!("clew-missing-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&db);
    let conn = open(&db).unwrap();
    let hit = repo::project_id_by_path(&conn, Path::new("D:/nope").to_string_lossy().as_ref())
        .unwrap();
    assert!(hit.is_none());
    let _ = std::fs::remove_file(&db);
}
