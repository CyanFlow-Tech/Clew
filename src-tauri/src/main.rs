#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri shell: the frontend reads the index through these commands only.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use clew_core::store::{self, Conn, repo};
use clew_core::walk;
use serde::Serialize;
use tauri::State;

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
    entries: Vec<EntryItem>,
}

fn open_at(path: &Path) -> Result<(Index, ProjectInfo), String> {
    let root = std::fs::canonicalize(path).map_err(|e| format!("打不开这个目录：{e}"))?;
    let db = root.join(".clew").join("index.db");
    let (conn, project_id) = store::open_index(&db, &root)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            format!(
                "还没有索引：{}\n先在终端跑 clew scan {}",
                root.display(),
                root.display()
            )
        })?;

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
        entries,
    };

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
    let guard = state.index.lock().map_err(|_| "内部状态损坏".to_string())?;
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    let project_id = clew_core::store::repo::project_id_by_path(
        &index.conn,
        &index.root.to_string_lossy(),
    )
    .map_err(|e| e.to_string())?
    .ok_or("项目不在库里")?;
    clew_core::snapshot::snapshot(&index.conn, project_id).map_err(|e| e.to_string())
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
    tauri::Builder::default()
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
