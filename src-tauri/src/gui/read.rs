use std::path::Path;
use std::time::Instant;

use clew_core::snapshot::CardsPayload;
use clew_core::snapshot;
use clew_core::store::{repo, Conn};
use clew_core::walk;
use serde::Serialize;
use tauri::State;

use super::state::*;

#[tauri::command]
pub(crate) fn node(id: i64, state: State<'_, AppState>) -> Result<walk::NodeView, String> {
    let guard = index_guard(&state);
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    walk::node(index.mat.conn(), id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("找不到符号 {id}"))
}

#[tauri::command]
pub(crate) fn code(id: i64, state: State<'_, AppState>) -> Result<Vec<walk::CodeLine>, String> {
    let guard = index_guard(&state);
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    walk::card_lines(index.mat.conn(), id).map_err(|e| e.to_string())
}

/// One expansion step: the cards asked for, their outgoing call sites, and the
/// project-wide fan-in counts.
///
/// 卡片按文件懒物化，没算过的模块在这里现算，算过的直接读库。
#[tauri::command]
pub(crate) fn cards(ids: Vec<String>, state: State<'_, AppState>) -> Result<CardsPayload, String> {
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

pub(crate) fn project_id_of(conn: &Conn, root: &Path) -> Result<i64, String> {
    clew_core::store::repo::project_id_by_path(conn, &root.to_string_lossy())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "项目不在库里".to_string())
}

/// The whole graph in one shot, shaped like the prototype's `DATA`.
#[tauri::command]
pub(crate) fn graph(state: State<'_, AppState>) -> Result<clew_core::snapshot::Snapshot, String> {
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
pub(crate) fn notes(id: i64, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let guard = index_guard(&state);
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    walk::notes(index.mat.conn(), id).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn add_note(id: i64, text: String, state: State<'_, AppState>) -> Result<(), String> {
    let guard = index_guard(&state);
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    repo::insert_note(index.mat.conn(), id, text.trim())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn source_root(state: State<'_, AppState>) -> Result<String, String> {
    let guard = index_guard(&state);
    let index = guard.as_ref().ok_or("还没有打开项目")?;
    Ok(index.root.display().to_string())
}

/// 前端画出第一帧之后把窗口亮出来。窗口在配置里是隐藏的，
/// 这样 WebView2 初始化那段空白不会被看到。
#[tauri::command]
pub(crate) fn ready(window: tauri::WebviewWindow) -> Result<(), String> {
    window.show().map_err(|e| e.to_string())
}

/// 项目里所有被索引的文件，入口面板左栏的文件树用它。
#[tauri::command]
pub(crate) fn files(state: State<'_, AppState>) -> Result<Vec<walk::FileRow>, String> {
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
pub(crate) fn symbols_in(module: String, state: State<'_, AppState>) -> Result<Vec<walk::SymbolRow>, String> {
    let guard = index_guard(&state);
    let index = index_or(&guard)?;
    walk::symbols_in(index.mat.conn(), index.project_id, &module).map_err(|e| e.to_string())
}

/// 侧栏「入口函数」视图：全项目的入口定义，一次取完。
#[tauri::command]
pub(crate) fn entry_symbols(state: State<'_, AppState>) -> Result<Vec<walk::SymbolRow>, String> {
    let guard = index_guard(&state);
    let index = index_or(&guard)?;
    walk::entry_symbols(index.mat.conn(), index.project_id).map_err(|e| e.to_string())
}

/// 侧边栏符号树：整个项目的文件与定义，一次取完。
#[tauri::command]
pub(crate) fn tree(state: State<'_, AppState>) -> Result<Vec<walk::FileNode>, String> {
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
pub(crate) struct SymbolSearchPayload {
    total: usize,
    truncated: bool,
    hits: Vec<walk::SymbolHit>,
}

#[tauri::command]
pub(crate) fn search_symbols(
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

