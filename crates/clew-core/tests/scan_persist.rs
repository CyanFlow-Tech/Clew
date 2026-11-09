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


class B(A):
    def extra(self):
        return 1


class D(dict):
    def __getattr__(self, name):
        return self[name]


class E(unittest.TestCase):
    def test_x(self):
        return 1


class Plain:
    def go(self):
        return 1


def helper(v):
    return v
";

const B_PY: &str = "\
from a import A, helper


if __name__ == \"__main__\":
    a = A(1)
    a.run()
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
    assert_eq!(stats.classes, 5);
    assert_eq!(
        stats.symbols, 12,
        "A, A.__init__, A.run, B, B.extra, D, D.__getattr__, E, E.test_x, Plain, Plain.go, helper"
    );
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
    assert_eq!(guard_lines, 2, "two statements inside the guard");

    let guard_calls: i64 = conn
        .query_row(
            "SELECT count(*) FROM call_site WHERE owner = '<module>'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(guard_calls, 2, "the guard's calls are attributed to the module");

    // `a = A(1)` at module level must feed `a.run()` resolution, same as in a function
    let (conf, dst): (String, i64) = conn
        .query_row(
            "SELECT confidence, dst_symbol FROM call_site
              WHERE owner = '<module>' AND target = 'run'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(conf, "exact", "module-level local type must resolve a.run()");
    let dst_qual: String = conn
        .query_row("SELECT qualname FROM symbol WHERE id = ?1", [dst], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(dst_qual, "A.run");

    // the snapshot the GUI consumes carries base classes and the method directory
    let snap = clew_core::snapshot::snapshot(&conn, project_id).unwrap();
    let b = snap.cards.get("a:B").expect("class B card");
    assert_eq!(b.bases, vec!["A".to_string()]);
    assert!(b.methods.iter().any(|m| m.name == "extra"));
    let a = snap.cards.get("a:A").expect("class A card");
    assert!(a.methods.iter().any(|m| m.name == "__init__"));
    assert_eq!(snap.entries.len(), 1);

    // constructors follow the MRO: B inherits A.__init__, D falls back to `dict`
    assert_eq!(a.ctor.as_deref(), Some("a:A.__init__"));
    assert_eq!(a.ctor_owner.as_deref(), Some("A"));
    assert_eq!(b.ctor.as_deref(), Some("a:A.__init__"));
    assert_eq!(b.ctor_owner.as_deref(), Some("A"), "inherited from A");
    let d = snap.cards.get("a:D").expect("class D card");
    assert_eq!(d.ctor, None, "dict.__init__ is outside the project");
    assert_eq!(d.external_base.as_deref(), Some("dict"));
    assert!(d.methods.iter().any(|m| m.name == "__getattr__"));

    // dotted bases are kept whole, and land in external_base
    let e = snap.cards.get("a:E").expect("class E card");
    assert_eq!(e.bases, vec!["unittest.TestCase".to_string()]);
    assert_eq!(e.ctor, None);
    assert_eq!(e.external_base.as_deref(), Some("unittest.TestCase"));

    // no base and no __init__: object is the implicit base
    let d_plain = snap.cards.get("a:Plain").expect("class Plain card");
    assert!(d_plain.bases.is_empty());
    assert_eq!(d_plain.ctor, None);
    assert_eq!(d_plain.external_base.as_deref(), Some("object"));

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
fn nested_calls_keep_multibyte_text_intact() {
    let dir = std::env::temp_dir().join(format!("clew-scan-cn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("app.py"),
        "def f(*a):\n\
         \x20   pass\n\
         \n\
         \n\
         def g(a):\n\
         \x20   pass\n\
         \n\
         \n\
         def h(a):\n\
         \x20   pass\n\
         \n\
         \n\
         def i():\n\
         \x20   pass\n\
         \n\
         \n\
         if __name__ == \"__main__\":\n\
         \x20   f(g(h(i())), \"中文\")\n",
    )
    .unwrap();
    let root = dir.canonicalize().unwrap();

    let db_path = root.join("index.db");
    let mut conn = open(&db_path).unwrap();
    let stats = scan(&mut conn, &root, false).unwrap();
    assert_eq!(stats.entries_main_guard, 1);

    // innermost call first, each nested call lifted into a %N temporary
    let mut stmt = conn
        .prepare(
            "SELECT text FROM card_line l JOIN symbol s ON s.id = l.symbol_id
              WHERE s.kind = 'module' ORDER BY l.ord",
        )
        .unwrap();
    let lines: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(
        lines,
        vec![
            "%0 = i()".to_string(),
            "%1 = h(%0)".to_string(),
            "%2 = g(%1)".to_string(),
            "f(%2, \"中文\")".to_string(),
        ]
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// A `def` under `if`/`while`/`with` still belongs to the enclosing scope, and
/// the calls in those headers are call sites of the enclosing card.
#[test]
fn conditional_and_nested_definitions_are_indexed() {
    let dir = std::env::temp_dir().join(format!("clew-scan-nested-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("app.py"),
        "import contextlib\n\
         \n\
         \n\
         def outer():\n\
         \x20   if gate():\n\
         \x20       def in_if():\n\
         \x20           return 1\n\
         \n\
         \x20   while spin():\n\
         \x20       def in_while():\n\
         \x20           return 2\n\
         \n\
         \x20   with contextlib.suppress(OSError):\n\
         \x20       def in_with():\n\
         \x20           return 3\n\
         \n\
         \x20   return 0\n\
         \n\
         \n\
         if module_gate():\n\
         \x20   def in_module_if():\n\
         \x20       return 4\n\
         \n\
         \n\
         class Box:\n\
         \x20   if TYPE_CHECKING:\n\
         \x20       def type_only(self):\n\
         \x20           return 5\n\
         \n\
         \n\
         def gate():\n\
         \x20   return True\n",
    )
    .unwrap();
    let root = dir.canonicalize().unwrap();

    let db_path = root.join("index.db");
    let mut conn = open(&db_path).unwrap();
    scan(&mut conn, &root, false).unwrap();
    let project_id = repo::project_id_by_path(&conn, &root.to_string_lossy())
        .unwrap()
        .unwrap();

    let mut stmt = conn
        .prepare(
            "SELECT qualname, kind FROM symbol
              WHERE project_id = ?1 AND kind != 'module'
              ORDER BY qualname",
        )
        .unwrap();
    let found: Vec<(String, String)> = stmt
        .query_map([project_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let want: Vec<(&str, &str)> = vec![
        ("Box", "class"),
        ("Box.type_only", "method"),
        ("gate", "function"),
        ("in_module_if", "function"),
        ("outer", "function"),
        ("outer.in_if", "function"),
        ("outer.in_while", "function"),
        ("outer.in_with", "function"),
    ];
    assert_eq!(
        found,
        want.iter()
            .map(|(q, k)| (q.to_string(), k.to_string()))
            .collect::<Vec<_>>(),
        "conditional and nested definitions must be indexed"
    );
    drop(stmt);

    // the branch headers are call sites of the enclosing function
    let mut stmt = conn
        .prepare("SELECT target FROM call_site WHERE owner = 'outer' ORDER BY target")
        .unwrap();
    let targets: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    drop(stmt);
    assert_eq!(targets, vec!["gate", "spin", "suppress"]);

    // and the nested `def` lines stay visible inside the outer card
    let mut stmt = conn
        .prepare(
            "SELECT l.text, l.kind FROM card_line l JOIN symbol s ON s.id = l.symbol_id
              WHERE s.qualname = 'outer' AND l.text LIKE 'def %' ORDER BY l.ord",
        )
        .unwrap();
    let defs: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    drop(stmt);
    assert_eq!(
        defs,
        vec![
            ("def in_if():".to_string(), "branch".to_string()),
            ("def in_while():".to_string(), "branch".to_string()),
            ("def in_with():".to_string(), "branch".to_string()),
        ]
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// The progress bar must never jump backwards, and must end at 100%.
#[test]
fn scan_reports_monotonic_progress() {
    use clew_core::scan::run::scan_with;

    let root = setup("progress");
    let db_path = root.join("index.db");
    let mut conn = open(&db_path).unwrap();

    let mut seen: Vec<f32> = Vec::new();
    scan_with(&mut conn, &root, false, true, &mut |_phase, _done, _total, f| {
        seen.push(f);
    })
    .unwrap();

    assert!(seen.len() >= 5, "too few progress reports: {seen:?}");
    let mut prev = -1.0;
    for f in &seen {
        assert!(
            *f >= prev,
            "progress went backwards: {seen:?}"
        );
        assert!(*f >= 0.0 && *f <= 1.0, "fraction out of range: {f}");
        prev = *f;
    }
    assert!((prev - 1.0).abs() < 1e-6, "did not finish at 100%: {seen:?}");

    let _ = std::fs::remove_dir_all(&root);
}

/// 只建目录之后按需物化一个文件，结果应与全量扫描完全一致。
#[test]
fn materialize_matches_full_scan() {
    use clew_core::scan::materialize::Materializer;
    use clew_core::scan::run::scan_with;
    use clew_core::snapshot::{cards_of, snapshot};

    // 基准：一次全量扫描
    let full_root = setup("mat-full");
    let mut full_conn = open(&full_root.join("index.db")).unwrap();
    scan(&mut full_conn, &full_root, false).unwrap();
    let full_pid = repo::project_id_by_path(&full_conn, &full_root.to_string_lossy())
        .unwrap()
        .unwrap();
    let full = snapshot(&full_conn, full_pid).unwrap();
    let a_cards: Vec<String> = full
        .cards
        .keys()
        .filter(|id| id.starts_with("a:"))
        .cloned()
        .collect();
    assert!(!a_cards.is_empty());

    // 只建目录
    let root = setup("mat-lazy");
    let db = root.join("index.db");
    let mut conn = open(&db).unwrap();
    let stats = scan_with(&mut conn, &root, false, false, &mut |_, _, _, _| {}).unwrap();
    assert_eq!(stats.symbols, 12, "目录层应当照常建立符号");
    assert_eq!(stats.call_sites, 0, "目录层不应记录调用点");
    let pid = repo::project_id_by_path(&conn, &root.to_string_lossy())
        .unwrap()
        .unwrap();
    let empty = cards_of(&conn, pid, &a_cards).unwrap();
    assert!(
        empty.cards.values().all(|c| c.lines.is_empty()),
        "物化之前不该有卡片内容"
    );
    drop(conn);

    // 按需物化 a 这一个文件
    let conn = open(&db).unwrap();
    let mut mat = Materializer::open(pid, conn).unwrap();
    let written = mat.ensure("a").unwrap();
    assert!(written >= 5, "a 里应当物化出若干卡片，实际 {written}");
    let lazy_no = cards_of(mat.conn(), pid, &a_cards).unwrap();

    assert_eq!(
        serde_json::to_string(&full.cards["a:A"]).unwrap(),
        serde_json::to_string(&lazy_no.cards["a:A"]).unwrap(),
        "类卡片与全量结果不一致"
    );
    for id in &a_cards {
        let want = serde_json::to_string(&full.cards[id]).unwrap();
        let got = serde_json::to_string(&lazy_no.cards[id]).unwrap();
        assert_eq!(want, got, "卡片 {id} 与全量结果不一致");
    }

    // 跨模块的调用目标也要能落库：只看本模块的符号表会查不到别的模块
    mat.ensure("b").unwrap();
    let b_module = cards_of(mat.conn(), pid, &["b:<module>".to_string()]).unwrap();
    drop(mat);
    assert!(
        b_module
            .calls
            .values()
            .any(|c| c.to.as_deref() == Some("a:A")),
        "b 里对 a.A 的调用没有解析到目标"
    );

    let _ = std::fs::remove_dir_all(&full_root);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn missing_project_reports_nothing() {    let db = std::env::temp_dir().join(format!("clew-missing-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&db);
    let conn = open(&db).unwrap();
    let hit = repo::project_id_by_path(&conn, Path::new("D:/nope").to_string_lossy().as_ref())
        .unwrap();
    assert!(hit.is_none());
    let _ = std::fs::remove_file(&db);
}

/// The on-demand read must return exactly what the one-shot snapshot returns,
/// otherwise the GUI would show a different graph depending on how it loaded.
#[test]
fn cards_of_matches_the_whole_snapshot() {
    use std::collections::BTreeMap;

    use clew_core::snapshot::{cards_of, snapshot};

    let root = setup("cards");
    let db_path = root.join("index.db");
    let mut conn = open(&db_path).unwrap();
    scan(&mut conn, &root, false).unwrap();
    let project_id = repo::project_id_by_path(&conn, &root.to_string_lossy())
        .unwrap()
        .unwrap();

    let snap = snapshot(&conn, project_id).unwrap();
    let ids: Vec<String> = snap.cards.keys().cloned().collect();
    let one = cards_of(&conn, project_id, &ids).unwrap();

    assert_eq!(
        serde_json::to_string(&snap.cards).unwrap(),
        serde_json::to_string(&one.cards).unwrap(),
        "cards differ between the snapshot and the on-demand read"
    );
    assert_eq!(
        serde_json::to_string(&snap.calls).unwrap(),
        serde_json::to_string(&one.calls).unwrap(),
        "calls differ between the snapshot and the on-demand read"
    );

    // fan-in must match the in-degree counted from the snapshot
    let mut expect: BTreeMap<String, i64> = BTreeMap::new();
    for call in snap.calls.values() {
        if let Some(to) = &call.to {
            *expect.entry(to.clone()).or_insert(0) += 1;
        }
    }
    for (id, n) in &one.fanin {
        assert_eq!(
            expect.get(id).copied().unwrap_or(0),
            *n,
            "fan-in mismatch for {id}"
        );
    }

    // asking for one card at a time yields the same records as asking for all
    for id in &ids {
        let solo = cards_of(&conn, project_id, std::slice::from_ref(id)).unwrap();
        assert!(solo.cards.contains_key(id), "missing {id}");
        assert_eq!(
            serde_json::to_string(&snap.cards[id]).unwrap(),
            serde_json::to_string(&solo.cards[id]).unwrap(),
            "solo card differs for {id}"
        );
        for (cs, call) in &solo.calls {
            let all = snap.calls.get(cs).expect("unknown call site");
            assert_eq!(
                serde_json::to_string(all).unwrap(),
                serde_json::to_string(call).unwrap(),
                "call {cs} differs when read solo"
            );
        }
    }

    let _ = std::fs::remove_dir_all(&root);
}
