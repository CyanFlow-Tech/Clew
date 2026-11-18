use std::path::PathBuf;
use std::time::Instant;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use clew_core::scan::materialize::Materializer;
use clew_core::scan::progress::Progress;
use clew_core::snapshot::Entry;
use serde::Serialize;
use tauri::State;

pub(crate) static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

pub(crate) fn since_start_ms() -> f64 {
    START
        .get()
        .map(|t| t.elapsed().as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

pub(crate) struct Index {
    pub(crate) root: PathBuf,
    pub(crate) project_id: i64,
    /// 卡片按文件懒物化，连接与解析缓存都放在这里
    pub(crate) mat: Materializer,
}

/// A background scan started when a project is opened for the first time.
pub(crate) struct ScanJob {
    pub(crate) root: PathBuf,
    pub(crate) started: Instant,
    pub(crate) progress: Arc<Mutex<Progress>>,
    pub(crate) finished: Arc<AtomicBool>,
    pub(crate) error: Arc<Mutex<Option<String>>>,
}

#[derive(Default)]
pub(crate) struct AppState {
    pub(crate) index: Mutex<Option<Index>>,
    pub(crate) scan: Mutex<Option<Arc<ScanJob>>>,
    /// roots.json 是「读整个文件、改、写回」，两个 invoke 撞一起就会互相覆盖，
    /// 表现是刚收藏的东西下一次打开不见了
    pub(crate) roots: Mutex<()>,
}

/// One answer to "open this project": either the index is ready, or a scan is
/// running and the caller should poll `scan_status`.
#[derive(Serialize)]
pub(crate) struct OpenResult {
    /// `ready` or `indexing`
    pub(crate) state: &'static str,
    pub(crate) root: String,
    pub(crate) modules: i64,
    pub(crate) symbols: i64,
    pub(crate) calls: i64,
    /// Milliseconds from process start to the end of this open, for the perf trace.
    pub(crate) boot_ms: f64,
    /// Card ids of the entry points, shaped like the snapshot's entries.
    pub(crate) entries: Vec<Entry>,
}

#[derive(Serialize)]
pub(crate) struct ScanStatus {
    /// `idle`, `indexing`, `ready` or `failed`
    pub(crate) state: &'static str,
    pub(crate) phase: String,
    pub(crate) done: u64,
    pub(crate) total: u64,
    pub(crate) fraction: f32,
    pub(crate) elapsed_ms: f64,
    /// 0 when the fraction is still too small to project from
    pub(crate) eta_ms: f64,
    pub(crate) error: Option<String>,
}

/// Open an already-built index. Never scans: a missing index is the caller's cue
/// to start a background scan and let the UI open straight away.

pub(crate) fn index_or(guard: &Option<Index>) -> Result<&Index, String> {
    guard.as_ref().ok_or_else(|| "还没有打开项目".to_string())
}

/// 索引锁。命令里只要有一次 panic，这把锁就带上中毒标记，之后每次取锁都失败，
/// 表现成「点什么都画不出卡片，重启才好」。中毒的只是这一次的数据，接着用就是，
/// 不能让一条命令的崩溃拖死整场会话。
pub(crate) fn index_guard<'a>(state: &'a State<'_, AppState>) -> std::sync::MutexGuard<'a, Option<Index>> {
    state.index.lock().unwrap_or_else(|e| e.into_inner())
}

/// 后台扫描任务的锁，理由同 `index_guard`。
pub(crate) fn scan_guard<'a>(state: &'a State<'_, AppState>) -> std::sync::MutexGuard<'a, Option<Arc<ScanJob>>> {
    state.scan.lock().unwrap_or_else(|e| e.into_inner())
}
