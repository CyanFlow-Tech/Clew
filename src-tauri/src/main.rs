#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri shell: the frontend reads the index through these commands only.

// 配置里现在有 devUrl：dev 构建不带资源、直接连本地服务，release 必须带上
// custom-protocol（`cargo app`）才会把 src/ 嵌进二进制。忘了带就是一扇白窗，
// 这里在编译期拦住，别等运行起来才发现。
#[cfg(all(dev, not(debug_assertions)))]
compile_error!(
    "release 构建要带前端资源：cargo build -p clew-app --release --features custom-protocol，或直接用 `cargo app`"
);

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

mod roots;

use clew_core::scan::materialize::Materializer;
use clew_core::scan::progress::Progress;
use clew_core::snapshot::{self, CardsPayload, Entry};
use clew_core::store::{self, Conn, repo};
use clew_core::walk;
use serde::Serialize;
use tauri::State;

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
    /// roots.json 是「读整个文件、改、写回」，两个 invoke 撞一起就会互相覆盖，
    /// 表现是刚收藏的东西下一次打开不见了
    roots: Mutex<()>,
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
    let db = clew_core::space::db_path(&root);

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
        let project_dir = clew_core::space::project_dir(&root);
        let db = project_dir.join("index.db");
        let result = (|| -> Result<(), String> {
            std::fs::create_dir_all(&project_dir).map_err(|e| e.to_string())?;
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
                let _ = std::fs::remove_dir_all(&project_dir);
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
    let db = clew_core::space::db_path(&root);

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

    // 残留的库没有可用索引，删掉重建，免得 scan 在半个库上继续写。
    // 删的是项目空间里的目录，不动被阅读的项目。
    let space_dir = clew_core::space::project_dir(&root);
    if space_dir.exists() {
        let _ = std::fs::remove_dir_all(&space_dir);
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
///
/// 库在项目空间里，所以被阅读的目录已经删掉了也要照样清；这时拿不到规范化
/// 路径，`space::project_id` 会退回原路径继续算，算出的目录还是同一个。
#[tauri::command]
fn delete_index(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let target = Path::new(&path);
    let dir = clew_core::space::project_dir(target);

    if let Ok(root) = std::fs::canonicalize(target) {
        let mut guard = index_guard(&state);
        if guard.as_ref().map(|i| i.root.as_path()) == Some(root.as_path()) {
            *guard = None;
        }
    }

    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("删不掉索引目录：{e}"))?;
        eprintln!("[t] delete_index removed {}", dir.display());
    } else {
        eprintln!("[t] delete_index found nothing at {}", dir.display());
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


/// 收藏与最近使用的位置：项目空间里，不跟索引一起删，也不写进被阅读的项目。
fn roots_path() -> Result<PathBuf, String> {
    let dir = clew_core::space::home();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("roots.json"))
}

/// 用户自己的入口：收藏与最近使用。读改写放在 roots::Store，锁在这里。
#[tauri::command]
fn roots(
    project: String,
    action: String,
    card_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<roots::ProjectRoots, String> {
    // 整份文件是「读、改、写回」，两个 invoke 撞一起就会互相覆盖，
    // 表现是刚收藏的东西下次打开不见了
    let _guard = state.roots.lock().unwrap_or_else(|e| e.into_inner());
    let store = roots::Store::new(roots_path()?);
    store
        .apply(&project, &action, card_id.as_deref())
        .map(|(out, _)| out)
}

/// 数据目录在哪里，界面用来显示当前设置。界面缓存的路径一并给出去。
#[derive(serde::Serialize)]
struct SpaceState {
    home: String,
    default_home: String,
    pointer: String,
    is_default: bool,
    webview_dir: String,
    webview_default: String,
    webview_custom: bool,
}

#[tauri::command]
fn space_state() -> SpaceState {
    let home = clew_core::space::home();
    let default_home = clew_core::space::default_home();
    let webview_dir = clew_core::settings::webview_dir();
    let webview_default = clew_core::settings::default_webview_dir();
    SpaceState {
        is_default: home == default_home,
        webview_custom: webview_dir != webview_default,
        home: home.display().to_string(),
        default_home: default_home.display().to_string(),
        pointer: clew_core::space::pointer_path().display().to_string(),
        webview_dir: webview_dir.display().to_string(),
        webview_default: webview_default.display().to_string(),
    }
}

/// 迁移后登记旧缓存目录，等下次启动时删。
///
/// 为什么不当场删：迁移这一步是在程序运行中做的，旧目录正被当前这个 WebView2
/// 用着（它承载的就是这个设置页），文件删不掉。而缓存换目录本来就要重启才生效，
/// 所以统一排到重启那一次——`setup` 里趁 WebView2 还没起来先清掉。
/// 一律排队，不做「有时删得掉有时删不掉」的分支。
fn queue_cache_cleanup(dir: &Path) {
    if !dir.join("EBWebView").exists() {
        return;
    }
    if let Err(err) = clew_core::settings::add_pending_cleanup(dir) {
        append_log("space", &format!("queue cache cleanup failed: {err}"));
    } else {
        eprintln!("[t] old cache queued for cleanup: {}", dir.display());
    }
}

/// 清掉上次没删成的旧缓存目录。**必须在建窗口之前调用**：WebView2 一起来，
/// 目录又会被锁住。
fn cleanup_pending_cache_dirs() {
    for dir in clew_core::settings::pending_cleanup() {
        let target = dir.join("EBWebView");
        if !target.exists() {
            let _ = clew_core::settings::remove_pending_cleanup(&dir);
            continue;
        }
        match std::fs::remove_dir_all(&target) {
            Ok(()) => {
                eprintln!("[t] cleaned old cache {}", target.display());
                let _ = clew_core::settings::remove_pending_cleanup(&dir);
            }
            Err(err) => append_log(
                "space",
                &format!("cleanup {} failed: {err}", target.display()),
            ),
        }
    }
}

/// 迁移界面缓存目录：把当前目录里的 Local Storage 带过去，再写设置。
///
/// 只搬 `Local Storage`——项目列表、工作区快照、主题都在那里；GPU 缓存和
/// Code Cache 搬过来只会占地方。搬到新目录时目标必须为空；搬到默认位置是例外，
/// 见下面那一支。生效时机不同：WebView2 在进程起来时就把目录锁死了，
/// 所以这一项要重启才生效。
#[tauri::command]
fn migrate_webview_dir(target: String) -> Result<String, String> {
    let to = PathBuf::from(target.trim());
    if to.as_os_str().is_empty() {
        return Err("路径是空的".to_string());
    }

    let default = clew_core::settings::default_webview_dir();
    let from = clew_core::settings::webview_dir();
    if from == to {
        return Ok(to.display().to_string());
    }

    // 搬回默认位置不要求目标为空：默认目录里通常还留着上一次的 EBWebView。
    // 搬的是「现在正在用的那份」，原有那份先留档成 `Local Storage.previous`。
    if to == default {
        std::fs::create_dir_all(&to).map_err(|e| format!("建不了缓存目录：{e}"))?;
        if let Err(err) = clew_core::space::replace_cache(&from, &to) {
            append_log("space", &format!("replace local storage failed: {err}"));
        }
        clew_core::settings::set_webview_dir(None)
            .map_err(|e| format!("写不了设置：{e}"))?;
        queue_cache_cleanup(&from);
        return Ok(to.display().to_string());
    }

    clew_core::space::ensure_empty_target(&to)?;
    std::fs::create_dir_all(&to).map_err(|e| format!("建不了缓存目录：{e}"))?;
    match clew_core::space::carry_cache(&from, &to) {
        Ok(moved) => eprintln!("[t] carry_cache moved {moved} entries"),
        Err(err) => append_log("space", &format!("carry local storage failed: {err}")),
    }

    clew_core::settings::set_webview_dir(Some(&to))
        .map_err(|e| format!("写不了设置：{e}"))?;
    queue_cache_cleanup(&from);
    Ok(to.display().to_string())
}

/// 迁移结果，界面只用来把按钮变灰，不显示数字。
#[derive(serde::Serialize)]
struct SpaceReport {
    home: String,
    files: usize,
    bytes: u64,
    items: Vec<String>,
}

/// 把空间搬到 `target`：目标必须为空，搬完写指针，再清旧空间的注册项。
///
/// 迁移前要放掉已打开的项目，否则 Windows 占着库文件，复制出来的是半份。
#[tauri::command]
fn migrate_space(target: String, state: State<'_, AppState>) -> Result<SpaceReport, String> {
    {
        let scan = scan_guard(&state);
        if scan
            .as_ref()
            .is_some_and(|job| !job.finished.load(Ordering::SeqCst))
        {
            return Err("索引还在建立，等它完成再迁移".to_string());
        }
    }

    let from = clew_core::space::home();
    let to = PathBuf::from(&target);
    clew_core::space::ensure_empty_target(&to)?;

    // 复制前把 WAL 收进主库：迁移搬的是目录，别让 -wal 里还压着没落盘的数据
    {
        let guard = index_guard(&state);
        if let Some(index) = guard.as_ref() {
            if let Err(err) = index
                .mat
                .conn()
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            {
                append_log("space", &format!("wal checkpoint failed: {err}"));
            }
        }
    }

    // 复制时库文件还开着：SQLite 允许别的句柄读，复制不会被打断
    let report = clew_core::space::migrate(&from, &to).map_err(|e| e.to_string())?;

    // 界面缓存不跟着数据目录走：两个默认位置是平级的，缓存只在自己换目录时搬。
    // 所以这里什么都不用做，旧数据目录也能被完整清掉。

    // 复制完再松手：旧库的句柄挡着删目录
    let root = {
        let mut guard = index_guard(&state);
        guard.take().map(|index| index.root.clone())
    };

    clew_core::space::set_home(&to).map_err(|e| format!("写不了空间指针：{e}"))?;

    // 用新空间里刚搬过去的库重新打开：界面还停在原来那个项目上，
    // 迁移不该把人踢回欢迎页
    if let Some(root) = root {
        match open_ready(&root) {
            Ok((index, _)) => remember(&state, index)?,
            Err(err) => {
                // 新库打不开：指针退回旧空间，清掉新空间里刚复制的那份，
                // 免得下次迁移又卡在「目标非空」
                let _ = clew_core::space::set_home(&from);
                let _ = clew_core::space::clear_entries(&to);
                let _ = std::fs::remove_dir(&to);
                // 旧库还在，把项目重新挂上，界面不至于变成「没打开项目」
                if let Ok((index, _)) = open_ready(&root) {
                    let _ = remember(&state, index);
                }
                return Err(format!("新空间里的索引打不开：{err}"));
            }
        }
    }

    // 旧空间只清注册项和我们自己的日志；用户自己放进去的东西不动
    if let Err(err) = clew_core::space::clear_entries(&from) {
        append_log("space", &format!("clear old space failed: {err}"));
    }
    let _ = std::fs::remove_file(from.join("clew.log"));
    if let Err(err) = clew_core::space::remove_dir_if_empty(&from) {
        append_log("space", &format!("remove old space dir failed: {err}"));
    }

    eprintln!(
        "[t] migrate_space {} -> {} files={} bytes={}",
        from.display(),
        to.display(),
        report.files,
        report.bytes
    );

    Ok(SpaceReport {
        home: to.display().to_string(),
        files: report.files,
        bytes: report.bytes,
        items: report.items,
    })
}

/// 项目空间根，日志也落在这里。
fn data_dir() -> PathBuf {
    clew_core::space::home()
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
        // 窗口必须在这里建，不能在 tauri.conf.json 里声明：Tauri 在 Windows
        // 上会强制把 WebView2 的数据目录指到 app_local_data_dir()，也就是
        // %LOCALAPPDATA%\<identifier>，环境变量 WEBVIEW2_USER_DATA_FOLDER 顶不过它。
        // 只有 WebviewWindowBuilder::data_directory 能让它落到我们的数据目录里。
        .setup(|app| {
            // 先清上次迁移留下的旧缓存目录：此刻 WebView2 还没起来，文件没锁
            cleanup_pending_cache_dirs();

            let webview_dir = clew_core::settings::webview_dir();
            std::fs::create_dir_all(&webview_dir)?;
            tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
                .title("Clew")
                .inner_size(1440.0, 920.0)
                .min_inner_size(900.0, 600.0)
                .center()
                .decorations(false)
                .shadow(true)
                .visible(false)
                .background_color(tauri::window::Color(0x14, 0x17, 0x1c, 0xff))
                .data_directory(webview_dir)
                .build()?;
            Ok(())
        })
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
            space_state,
            migrate_webview_dir,
            migrate_space,
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
