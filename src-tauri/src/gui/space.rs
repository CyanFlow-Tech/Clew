use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::State;

use super::state::*;
use super::log::append_log;
use super::scan::{open_project, open_ready, remember};

#[tauri::command]
pub(crate) fn boot(state: State<'_, AppState>) -> Result<Option<OpenResult>, String> {
    let Some(arg) = std::env::args().nth(1) else {
        return Ok(None);
    };
    open_project(arg, state).map(Some)
}

#[derive(Serialize)]
pub(crate) struct SpaceState {
    home: String,
    default_home: String,
    pointer: String,
    is_default: bool,
    webview_dir: String,
    webview_default: String,
    webview_custom: bool,
}

#[tauri::command]
pub(crate) fn space_state() -> SpaceState {
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
pub(crate) fn queue_cache_cleanup(dir: &Path) {
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
pub(crate) fn cleanup_pending_cache_dirs() {
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
pub(crate) fn migrate_webview_dir(target: String) -> Result<String, String> {
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

    clew_core::space::ensure_empty_target(&to).map_err(|e| e.to_string())?;
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
pub(crate) struct SpaceReport {
    home: String,
    files: usize,
    bytes: u64,
    items: Vec<String>,
}

/// 把空间搬到 `target`：目标必须为空，搬完写指针，再清旧空间的注册项。
///
/// 迁移前要放掉已打开的项目，否则 Windows 占着库文件，复制出来的是半份。
#[tauri::command]
pub(crate) fn migrate_space(target: String, state: State<'_, AppState>) -> Result<SpaceReport, String> {
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
    clew_core::space::ensure_empty_target(&to).map_err(|e| e.to_string())?;

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

