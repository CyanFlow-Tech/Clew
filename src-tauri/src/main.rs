#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri shell: the frontend reads the index through these commands only.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

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
    conn: Conn,
    root: PathBuf,
}

#[derive(Default)]
struct AppState {
    index: Mutex<Option<Index>>,
}

#[derive(Serialize)]
struct EntryItem {
    id: i64,
    module: String,
    qualname: String,
    confidence: String,
}

#[derive(Serialize)]
struct ProjectInfo {
    root: String,
    modules: i64,
    symbols: i64,
    calls: i64,
    /// Milliseconds from process start to the end of this open, for the perf trace.
    boot_ms: f64,
    entries: Vec<EntryItem>,
}

fn open_at(path: &Path) -> Result<(Index, ProjectInfo), String> {
    let t_open = Instant::now();
    let root = std::fs::canonicalize(path).map_err(|e| format!("打不开这个目录：{e}"))?;
    let clew_dir = root.join(".clew");
    let db = clew_dir.join("index.db");
    let d_canon = t_open.elapsed();

    // 首次打开一个项目：就地建索引，而不是让人回终端手动跑 scan
    let t_index = Instant::now();
    let (conn, project_id) = match store::open_index(&db, &root).map_err(|e| e.to_string())? {
        Some(pair) => pair,
        None => {
            std::fs::create_dir_all(&clew_dir).map_err(|e| e.to_string())?;
            let mut fresh = store::open(&db).map_err(|e| e.to_string())?;
            let stats = clew_core::scan::run::scan(&mut fresh, &root, false)
                .map_err(|e| format!("建索引失败：{e}"))?;
            if stats.modules == 0 {
                // 没有 Python 文件就别留下一个空库，下次还要重来
                drop(fresh);
                let _ = std::fs::remove_dir_all(&clew_dir);
                return Err(format!("{} 里没有 Python 文件", root.display()));
            }
            let project_id = repo::project_id_by_path(&fresh, &root.to_string_lossy())
                .map_err(|e| e.to_string())?
                .ok_or("建好索引但找不到项目记录")?;
            (fresh, project_id)
        }
    };
    let d_index = t_index.elapsed();

    let t_rest = Instant::now();
    let entries = walk::entry_points(&conn, project_id)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(id, module, qualname, confidence)| EntryItem {
            id,
            module,
            qualname,
            confidence,
        })
        .collect();

    let info = ProjectInfo {
        root: root.display().to_string(),
        modules: repo::count(&conn, "module", project_id).map_err(|e| e.to_string())?,
        symbols: repo::count(&conn, "symbol", project_id).map_err(|e| e.to_string())?,
        calls: repo::count(&conn, "call_site", project_id).map_err(|e| e.to_string())?,
        boot_ms: since_start_ms(),
        entries,
    };
    let d_rest = t_rest.elapsed();

    eprintln!(
        "[t] open_at @{:.0}ms canon {:.1}ms index {:.1}ms entries+counts {:.1}ms total {:.1}ms",
        info.boot_ms,
        d_canon.as_secs_f64() * 1000.0,
        d_index.as_secs_f64() * 1000.0,
        d_rest.as_secs_f64() * 1000.0,
        t_open.elapsed().as_secs_f64() * 1000.0
    );

    Ok((Index { conn, root }, info))
}

fn remember(state: &State<'_, AppState>, index: Index) -> Result<(), String> {
    *state.index.lock().map_err(|_| "内部状态损坏".to_string())? = Some(index);
    Ok(())
}

/// Open an already-scanned project and describe it.
#[tauri::command]
fn open_project(path: String, state: State<'_, AppState>) -> Result<ProjectInfo, String> {
    let (index, info) = open_at(Path::new(&path))?;
    remember(&state, index)?;
    Ok(info)
}

/// Open the project given on the command line, if any.
#[tauri::command]
fn boot(state: State<'_, AppState>) -> Result<Option<ProjectInfo>, String> {
    let Some(arg) = std::env::args().nth(1) else {
        return Ok(None);
    };
    match open_at(Path::new(&arg)) {
        Ok((index, info)) => {
            remember(&state, index)?;
            Ok(Some(info))
        }
        Err(msg) => Err(msg),
    }
}

#[tauri::command]
fn node(id: i64, state: State<'_, AppState>) -> Result<walk::NodeView, String> {
    let guard = state.index.lock().map_err(|_| "内部状态损坏".to_string())?;
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    walk::node(&index.conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("找不到符号 {id}"))
}

#[tauri::command]
fn code(id: i64, state: State<'_, AppState>) -> Result<Vec<walk::CodeLine>, String> {
    let guard = state.index.lock().map_err(|_| "内部状态损坏".to_string())?;
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    walk::card_lines(&index.conn, id).map_err(|e| e.to_string())
}

/// The whole graph in one shot, shaped like the prototype's `DATA`.
#[tauri::command]
fn graph(state: State<'_, AppState>) -> Result<clew_core::snapshot::Snapshot, String> {
    let t = Instant::now();
    let guard = state.index.lock().map_err(|_| "内部状态损坏".to_string())?;
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    let project_id = clew_core::store::repo::project_id_by_path(
        &index.conn,
        &index.root.to_string_lossy(),
    )
    .map_err(|e| e.to_string())?
    .ok_or("项目不在库里")?;
    let snap =
        clew_core::snapshot::snapshot(&index.conn, project_id).map_err(|e| e.to_string())?;
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
    let guard = state.index.lock().map_err(|_| "内部状态损坏".to_string())?;
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    walk::notes(&index.conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
fn add_note(id: i64, text: String, state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.index.lock().map_err(|_| "内部状态损坏".to_string())?;
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    repo::insert_note(&index.conn, id, text.trim())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn source_root(state: State<'_, AppState>) -> Result<String, String> {
    let guard = state.index.lock().map_err(|_| "内部状态损坏".to_string())?;
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    Ok(index.root.display().to_string())
}

fn main() {
    START.set(Instant::now()).ok();
    eprintln!("[t] main starts");
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            open_project,
            boot,
            node,
            code,
            graph,
            notes,
            add_note,
            source_root
        ])
        .run(tauri::generate_context!())
        .expect("Clew failed to start");
}
