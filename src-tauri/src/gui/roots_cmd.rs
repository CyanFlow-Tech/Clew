use std::path::PathBuf;

use tauri::State;

use super::state::*;

#[tauri::command]
pub(crate) fn check_paths(paths: Vec<String>) -> Vec<bool> {
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

pub(crate) fn roots_path() -> Result<PathBuf, String> {
    let dir = clew_core::space::home();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("roots.json"))
}

/// 用户自己的入口：收藏与最近使用。读改写放在 crate::roots::Store，锁在这里。
#[tauri::command]
pub(crate) fn roots(
    project: String,
    action: String,
    card_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<crate::roots::ProjectRoots, String> {
    // 整份文件是「读、改、写回」，两个 invoke 撞一起就会互相覆盖，
    // 表现是刚收藏的东西下次打开不见了
    let _guard = state.roots.lock().unwrap_or_else(|e| e.into_inner());
    let store = crate::roots::Store::new(roots_path()?);
    store
        .apply(&project, &action, card_id.as_deref())
        .map(|(out, _)| out)
}

