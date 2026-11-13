#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri shell: the frontend reads the index through these commands only.

// 配置里现在有 devUrl：dev 构建不带资源、直接连本地服务，release 必须带上
// custom-protocol（`cargo app`）才会把 src/ 嵌进二进制。忘了带就是一扇白窗，
// 这里在编译期拦住，别等运行起来才发现。
#[cfg(all(dev, not(debug_assertions)))]
compile_error!(
    "release 构建要带前端资源：cargo build -p clew-app --release --features custom-protocol，或直接用 `cargo app`"
);

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use clew_core::scan::materialize::Materializer;
use clew_core::scan::progress::Progress;
use clew_core::snapshot::{self, CardsPayload, Entry};
use clew_core::store::{self, Conn, repo};
use clew_core::walk;
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

/// Process start, so every stage can print an absolute offset from launch.
static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn since_start_ms() -> f64 {
    START
        .get()
        .map(|t| t.elapsed().as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

struct Index {
    root: PathBuf,
    project_id: i64,
    /// 卡片按文件懒物化，连接与解析缓存都放在这里
    mat: Materializer,
}

/// A background scan started when a project is opened for the first time.
struct ScanJob {
    root: PathBuf,
    started: Instant,
    progress: Arc<Mutex<Progress>>,
    finished: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
}

#[derive(Default)]
struct AppState {
    index: Mutex<Option<Index>>,
    scan: Mutex<Option<Arc<ScanJob>>>,
}

/// One answer to "open this project": either the index is ready, or a scan is
/// running and the caller should poll `scan_status`.
#[derive(Serialize)]
struct OpenResult {
    /// `ready` or `indexing`
    state: &'static str,
    root: String,
    modules: i64,
    symbols: i64,
    calls: i64,
    /// Milliseconds from process start to the end of this open, for the perf trace.
    boot_ms: f64,
    /// Card ids of the entry points, shaped like the snapshot's entries.
    entries: Vec<Entry>,
}

#[derive(Serialize)]
struct ScanStatus {
    /// `idle`, `indexing`, `ready` or `failed`
    state: &'static str,
    phase: String,
    done: u64,
    total: u64,
    fraction: f32,
    elapsed_ms: f64,
    /// 0 when the fraction is still too small to project from
    eta_ms: f64,
    error: Option<String>,
}

/// Open an already-built index. Never scans: a missing index is the caller's cue
/// to start a background scan and let the UI open straight away.
fn open_ready(path: &Path) -> Result<(Index, OpenResult), String> {
    let t_open = Instant::now();
    let root = std::fs::canonicalize(path).map_err(|e| format!("打不开这个目录：{e}"))?;
    let db = root.join(".clew").join("index.db");

    let t_index = Instant::now();
    let (conn, project_id) = store::open_index(&db, &root)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("{} 还没有索引", root.display()))?;
    let d_index = t_index.elapsed();

    let t_rest = Instant::now();
    let entries: Vec<Entry> = walk::entry_points(&conn, project_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(_id, module, qualname, _confidence)| {
            let card = format!("{module}:{qualname}");
            Entry {
                id: card.clone(),
                label: card,
            }
        })
        .collect();

    let info = OpenResult {
        state: "ready",
        root: root.display().to_string(),
        modules: repo::count(&conn, "module", project_id).map_err(|e| e.to_string())?,
        symbols: repo::count(&conn, "symbol", project_id).map_err(|e| e.to_string())?,
        calls: repo::count(&conn, "call_site", project_id).map_err(|e| e.to_string())?,
        boot_ms: since_start_ms(),
        entries,
    };
    let d_rest = t_rest.elapsed();

    eprintln!(
        "[t] open_at @{:.0}ms index {:.1}ms entries+counts {:.1}ms total {:.1}ms",
        info.boot_ms,
        d_index.as_secs_f64() * 1000.0,
        d_rest.as_secs_f64() * 1000.0,
        t_open.elapsed().as_secs_f64() * 1000.0
    );

    let mut mat = Materializer::open(project_id, conn).map_err(|e| e.to_string())?;
    // 全项目索引在这里建好，第一次点击就不用再等
    let t_pre = Instant::now();
    if let Err(e) = mat.preload() {
        eprintln!("[t] preload failed: {e}");
    }
    eprintln!(
        "[t] global index @{:.0}ms took {:.1}ms",
        since_start_ms(),
        t_pre.elapsed().as_secs_f64() * 1000.0
    );
    Ok((Index { root, project_id, mat }, info))
}

fn remember(state: &State<'_, AppState>, index: Index) -> Result<(), String> {
    *index_guard(&state) = Some(index);
    Ok(())
}

/// Kick off a scan on a worker thread and return the handle the UI polls.
fn spawn_scan(root: PathBuf) -> Arc<ScanJob> {
    let progress = Arc::new(Mutex::new(Progress::default()));
    let finished = Arc::new(AtomicBool::new(false));
    let error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let job = Arc::new(ScanJob {
        root: root.clone(),
        started: Instant::now(),
        progress: progress.clone(),
        finished: finished.clone(),
        error: error.clone(),
    });

    std::thread::spawn(move || {
        let clew_dir = root.join(".clew");
        let db = clew_dir.join("index.db");
        let result = (|| -> Result<(), String> {
            std::fs::create_dir_all(&clew_dir).map_err(|e| e.to_string())?;
            let mut conn = store::open(&db).map_err(|e| e.to_string())?;
            let stats = clew_core::scan::run::scan_with(
                &mut conn,
                &root,
                false,
                // 先只建目录，卡片等点开再算
                false,
                &mut |phase, done, total, fraction| {
                    if let Ok(mut p) = progress.lock() {
                        *p = Progress {
                            phase,
                            done,
                            total,
                            fraction,
                        };
                    }
                },
            )
            .map_err(|e| format!("建索引失败：{e}"))?;
            if stats.modules == 0 {
                drop(conn);
                let _ = std::fs::remove_dir_all(&clew_dir);
                return Err(format!("{} 里没有 Python 文件", root.display()));
            }
            eprintln!(
                "[t] scan @{:.0}ms done in {:.1}s",
                since_start_ms(),
                stats.elapsed_ms as f64 / 1000.0
            );
            Ok(())
        })();

        match result {
            Ok(()) => {}
            Err(msg) => {
                *error.lock().unwrap_or_else(|e| e.into_inner()) = Some(msg);
            }
        }
        finished.store(true, Ordering::SeqCst);
    });

    job
}

/// Open a project. A first-open returns `indexing` at once and scans in the
/// background, so the window can show its own UI instead of freezing.
#[tauri::command]
fn open_project(path: String, state: State<'_, AppState>) -> Result<OpenResult, String> {
    let root = std::fs::canonicalize(Path::new(&path))
        .map_err(|e| format!("打不开这个目录：{e}"))?;
    let db = root.join(".clew").join("index.db");

    // 索引能用就直接开。只判断库文件在不在是不够的：中断的扫描会留下一个
    // 空库，那样会把「没有索引」误报成有索引。
    if matches!(store::open_index(&db, &root), Ok(Some(_))) {
        let (index, info) = open_ready(&root)?;
        remember(&state, index)?;
        return Ok(info);
    }

    // a scan for this same directory may already be running
    {
        let guard = scan_guard(&state);
        if let Some(job) = guard.as_ref() {
            if job.root == root && !job.finished.load(Ordering::SeqCst) {
                return Ok(OpenResult {
                    state: "indexing",
                    root: root.display().to_string(),
                    modules: 0,
                    symbols: 0,
                    calls: 0,
                    boot_ms: since_start_ms(),
                    entries: Vec::new(),
                });
            }
        }
    }

    // 残留的库没有可用索引，删掉重建，免得 scan 在半个库上继续写
    if root.join(".clew").exists() {
        let _ = std::fs::remove_dir_all(root.join(".clew"));
    }

    let job = spawn_scan(root.clone());
    *scan_guard(&state) = Some(job);
    eprintln!("[t] open_project @{:.0}ms started a background scan", since_start_ms());

    Ok(OpenResult {
        state: "indexing",
        root: root.display().to_string(),
        modules: 0,
        symbols: 0,
        calls: 0,
        boot_ms: since_start_ms(),
        entries: Vec::new(),
    })
}

/// Progress of the background scan, polled by the status bar.
#[tauri::command]
fn scan_status(state: State<'_, AppState>) -> Result<ScanStatus, String> {
    let guard = scan_guard(&state);
    let Some(job) = guard.as_ref() else {
        return Ok(ScanStatus {
            state: "idle",
            phase: String::new(),
            done: 0,
            total: 0,
            fraction: 0.0,
            elapsed_ms: 0.0,
            eta_ms: 0.0,
            error: None,
        });
    };

    let p = *job.progress.lock().unwrap_or_else(|e| e.into_inner());
    let elapsed = job.started.elapsed().as_secs_f64();
    let f = p.fraction.clamp(0.0, 1.0) as f64;
    // only project once something measurable has happened
    let eta = if f > 0.02 && f < 1.0 {
        elapsed / f * (1.0 - f)
    } else {
        0.0
    };

    let err = job.error.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let done = job.finished.load(Ordering::SeqCst);
    let state_name = if err.is_some() {
        "failed"
    } else if done {
        "ready"
    } else {
        "indexing"
    };

    Ok(ScanStatus {
        state: state_name,
        phase: p.phase.to_string(),
        done: p.done,
        total: p.total,
        fraction: p.fraction,
        elapsed_ms: elapsed * 1000.0,
        eta_ms: eta * 1000.0,
        error: err,
    })
}

/// Whether each path still exists. The welcome page marks projects whose
/// directory was moved or deleted outside Clew.
#[tauri::command]
fn check_paths(paths: Vec<String>) -> Vec<bool> {
    paths
        .iter()
        .map(|p| std::path::Path::new(p).exists())
        .collect()
}

/// Drop a project's index. The connection is released first, otherwise Windows
/// keeps the database file open and the directory cannot be removed.
#[tauri::command]
fn delete_index(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let dir = Path::new(&path).join(".clew");

    // 目录本身已经不在了，索引跟着一起没了，只剩历史记录要清
    if !Path::new(&path).exists() {
        eprintln!("[t] delete_index skipped, missing {}", path);
        return Ok(());
    }

    let root = std::fs::canonicalize(Path::new(&path))
        .map_err(|e| format!("打不开这个目录：{e}"))?;

    {
        let mut guard = index_guard(&state);
        if guard.as_ref().map(|i| i.root.as_path()) == Some(root.as_path()) {
            *guard = None;
        }
    }

    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("删不掉索引目录：{e}"))?;
        eprintln!("[t] delete_index removed {}", dir.display());
    }
    Ok(())
}

/// Open the project given on the command line, if any.
#[tauri::command]
fn boot(state: State<'_, AppState>) -> Result<Option<OpenResult>, String> {
    let Some(arg) = std::env::args().nth(1) else {
        return Ok(None);
    };
    open_project(arg, state).map(Some)
}

#[tauri::command]
fn node(id: i64, state: State<'_, AppState>) -> Result<walk::NodeView, String> {
    let guard = index_guard(&state);
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    walk::node(index.mat.conn(), id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("找不到符号 {id}"))
}

#[tauri::command]
fn code(id: i64, state: State<'_, AppState>) -> Result<Vec<walk::CodeLine>, String> {
    let guard = index_guard(&state);
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    walk::card_lines(index.mat.conn(), id).map_err(|e| e.to_string())
}

/// One expansion step: the cards asked for, their outgoing call sites, and the
/// project-wide fan-in counts.
///
/// 卡片按文件懒物化，没算过的模块在这里现算，算过的直接读库。
#[tauri::command]
fn cards(ids: Vec<String>, state: State<'_, AppState>) -> Result<CardsPayload, String> {
    let t = Instant::now();
    let mut guard = index_guard(&state);
    let index = guard.as_mut().ok_or("还没有打开项目")?;
    let project_id = index.project_id;

    let mut materialised = 0usize;
    for id in &ids {
        if let Some(module) = clew_core::scan::materialize::module_of_card(id) {
            // 建卡时 panic 只该坏这一个文件。让它顺着 unwind 穿出去，整个会话的
            // 索引锁都会带上中毒标记，之后每张卡都取不到，只能重启。
            let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                index.mat.ensure(module)
            }));
            match built {
                Ok(Ok(n)) => materialised += n,
                Ok(Err(e)) => eprintln!("[t] materialize {module} failed: {e}"),
                Err(_) => eprintln!("[t] materialize {module} panicked"),
            }
        }
    }

    let payload =
        snapshot::cards_of(index.mat.conn(), project_id, &ids).map_err(|e| e.to_string())?;
    eprintln!(
        "[t] cards @{:.0}ms took {:.1}ms wanted={} cards={} calls={} built={}",
        since_start_ms(),
        t.elapsed().as_secs_f64() * 1000.0,
        ids.len(),
        payload.cards.len(),
        payload.calls.len(),
        materialised
    );
    Ok(payload)
}

fn project_id_of(conn: &Conn, root: &Path) -> Result<i64, String> {
    clew_core::store::repo::project_id_by_path(conn, &root.to_string_lossy())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "项目不在库里".to_string())
}

/// The whole graph in one shot, shaped like the prototype's `DATA`.
#[tauri::command]
fn graph(state: State<'_, AppState>) -> Result<clew_core::snapshot::Snapshot, String> {
    let t = Instant::now();
    let guard = index_guard(&state);
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    let project_id = project_id_of(index.mat.conn(), &index.root)?;
    let snap = snapshot::snapshot(index.mat.conn(), project_id).map_err(|e| e.to_string())?;
    eprintln!(
        "[t] graph @{:.0}ms took {:.1}ms cards={} calls={}",
        since_start_ms(),
        t.elapsed().as_secs_f64() * 1000.0,
        snap.cards.len(),
        snap.calls.len()
    );
    Ok(snap)
}

#[tauri::command]
fn notes(id: i64, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let guard = index_guard(&state);
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    walk::notes(index.mat.conn(), id).map_err(|e| e.to_string())
}

#[tauri::command]
fn add_note(id: i64, text: String, state: State<'_, AppState>) -> Result<(), String> {
    let guard = index_guard(&state);
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    repo::insert_note(index.mat.conn(), id, text.trim())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn source_root(state: State<'_, AppState>) -> Result<String, String> {
    let guard = index_guard(&state);
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    Ok(index.root.display().to_string())
}

/// 前端画出第一帧之后把窗口亮出来。窗口在配置里是隐藏的，
/// 这样 WebView2 初始化那段空白不会被看到。
#[tauri::command]
fn ready(window: tauri::WebviewWindow) -> Result<(), String> {
    window.show().map_err(|e| e.to_string())
}

/// 项目里所有被索引的文件，入口面板左栏的文件树用它。
#[tauri::command]
fn files(state: State<'_, AppState>) -> Result<Vec<walk::FileRow>, String> {
    let guard = index_guard(&state);
    let index = index_or(&guard)?;
    let mut rows = walk::files(index.mat.conn(), index.project_id).map_err(|e| e.to_string())?;
    for r in rows.iter_mut() {
        r.path = walk::rel_path(&index.root, &r.path);
    }
    Ok(rows)
}

/// 一个文件里的全部定义，按行号排序。
#[tauri::command]
fn symbols_in(module: String, state: State<'_, AppState>) -> Result<Vec<walk::SymbolRow>, String> {
    let guard = index_guard(&state);
    let index = index_or(&guard)?;
    walk::symbols_in(index.mat.conn(), index.project_id, &module).map_err(|e| e.to_string())
}

/// 侧栏「入口函数」视图：全项目的入口定义，一次取完。
#[tauri::command]
fn entry_symbols(state: State<'_, AppState>) -> Result<Vec<walk::SymbolRow>, String> {
    let guard = index_guard(&state);
    let index = index_or(&guard)?;
    walk::entry_symbols(index.mat.conn(), index.project_id).map_err(|e| e.to_string())
}

/// 侧边栏符号树：整个项目的文件与定义，一次取完。
#[tauri::command]
fn tree(state: State<'_, AppState>) -> Result<Vec<walk::FileNode>, String> {
    let t = Instant::now();
    let guard = index_guard(&state);
    let index = index_or(&guard)?;
    let root = index.root.clone();
    let mut rows = walk::tree(index.mat.conn(), index.project_id).map_err(|e| e.to_string())?;
    let mut symbols = 0usize;
    for r in rows.iter_mut() {
        r.path = walk::rel_path(&root, &r.path);
        symbols += r.symbols.len();
    }
    eprintln!(
        "[t] tree @{:.0}ms took {:.1}ms files={} symbols={}",
        since_start_ms(),
        t.elapsed().as_secs_f64() * 1000.0,
        rows.len(),
        symbols
    );
    Ok(rows)
}

/// 侧栏搜索：路径一个过滤条件、函数名一个过滤条件，都是任意子串。
/// 一次只回一页，前端滚到底再要下一页。
#[derive(Serialize)]
struct SymbolSearchPayload {
    total: usize,
    truncated: bool,
    hits: Vec<walk::SymbolHit>,
}

#[tauri::command]
fn search_symbols(
    q_path: Option<String>,
    q_name: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<SymbolSearchPayload, String> {
    let t = Instant::now();
    let guard = index_guard(&state);
    let index = index_or(&guard)?;
    let root = index.root.clone();
    let out = walk::search_symbols(
        index.mat.conn(),
        index.project_id,
        &index.root,
        q_path.as_deref().unwrap_or(""),
        q_name.as_deref().unwrap_or(""),
    )
    .map_err(|e| e.to_string())?;

    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(50).max(1);
    let mut hits: Vec<walk::SymbolHit> = out.hits.into_iter().skip(offset).take(limit).collect();
    for h in hits.iter_mut() {
        h.path = walk::rel_path(&root, &h.path);
    }
    eprintln!(
        "[t] search_symbols @{:.0}ms took {:.1}ms total={} page={} took_rows={}",
        since_start_ms(),
        t.elapsed().as_secs_f64() * 1000.0,
        out.total,
        offset / limit + 1,
        hits.len()
    );
    Ok(SymbolSearchPayload {
        total: out.total,
        truncated: out.truncated,
        hits,
    })
}

fn index_or(guard: &Option<Index>) -> Result<&Index, String> {
    guard.as_ref().ok_or_else(|| "还没有打开项目".to_string())
}

/// 索引锁。命令里只要有一次 panic，这把锁就带上中毒标记，之后每次取锁都失败，
/// 表现成「点什么都画不出卡片，重启才好」。中毒的只是这一次的数据，接着用就是，
/// 不能让一条命令的崩溃拖死整场会话。
fn index_guard<'a>(state: &'a State<'_, AppState>) -> std::sync::MutexGuard<'a, Option<Index>> {
    state.index.lock().unwrap_or_else(|e| e.into_inner())
}

/// 后台扫描任务的锁，理由同 `index_guard`。
fn scan_guard<'a>(state: &'a State<'_, AppState>) -> std::sync::MutexGuard<'a, Option<Arc<ScanJob>>> {
    state.scan.lock().unwrap_or_else(|e| e.into_inner())
}


/// 用户自己的入口：收藏与最近打开，存在应用数据目录，不跟索引一起删。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProjectRoots {
    #[serde(default)]
    favorites: Vec<String>,
    #[serde(default)]
    recent: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RootsFile {
    #[serde(default)]
    projects: BTreeMap<String, ProjectRoots>,
}

fn roots_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("找不到应用数据目录：{e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("roots.json"))
}

fn read_roots(app: &tauri::AppHandle) -> Result<RootsFile, String> {
    let path = roots_path(app)?;
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("roots.json 读不出来：{e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(RootsFile::default()),
        Err(e) => Err(e.to_string()),
    }
}

fn write_roots(app: &tauri::AppHandle, data: &RootsFile) -> Result<(), String> {
    let path = roots_path(app)?;
    let text = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

#[tauri::command]
fn roots(
    app: tauri::AppHandle,
    project: String,
    action: String,
    card_id: Option<String>,
) -> Result<ProjectRoots, String> {
    let mut data = read_roots(&app)?;
    let entry = data.projects.entry(project).or_default();

    match action.as_str() {
        "toggle" => {
            let id = card_id.ok_or("缺少 card_id")?;
            if let Some(pos) = entry.favorites.iter().position(|x| *x == id) {
                entry.favorites.remove(pos);
            } else {
                entry.favorites.push(id);
            }
        }
        "recent" => {
            let id = card_id.ok_or("缺少 card_id")?;
            entry.recent.retain(|x| *x != id);
            entry.recent.insert(0, id);
            entry.recent.truncate(12);
        }
        _ => {}
    }

    let out = entry.clone();
    write_roots(&app, &data)?;
    Ok(out)
}

/// 应用数据目录，日志和入口记录都放这里。
fn data_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_default();
    Path::new(&base).join("tech.cyanflow.clew")
}

/// 追加一行日志。窗口程序没有控制台，出错时只剩「点了没反应」，
/// 不落一份文件就无从查起。
fn append_log(scope: &str, line: &str) {
    let dir = data_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let text = format!("[{secs}] {scope}: {line}\n");
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("clew.log"))
        .and_then(|mut f| std::io::Write::write_all(&mut f, text.as_bytes()));
}

/// 前端把 JS 异常和取数失败送到这里，和 Rust 侧的 panic 记在同一份日志里。
#[tauri::command]
fn log_line(text: String) {
    append_log("web", &text);
}

/// 命令里的 panic 记一份，否则它只是静悄悄地把锁毒掉。
fn install_panic_log() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        append_log("panic", &info.to_string());
        previous(info);
    }));
}

fn main() {
    START.set(Instant::now()).ok();
    install_panic_log();
    eprintln!("[t] main starts");
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        // 页面加载完就一定有内容可画，这时候亮窗口。前端脚本正常时会先喊
        // ready，这里兜住它没跑到那一步的情况，不靠固定超时去猜。
        .on_page_load(|webview, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                let _ = webview.window().show();
            }
        })
        .invoke_handler(tauri::generate_handler![
            ready,
            log_line,
            files,
            symbols_in,
            entry_symbols,
            search_symbols,
            tree,
            roots,
            open_project,
            boot,
            scan_status,
            delete_index,
            check_paths,
            node,
            code,
            cards,
            graph,
            notes,
            add_note,
            source_root
        ])
        .run(tauri::generate_context!())
        .expect("Clew failed to start");
}
