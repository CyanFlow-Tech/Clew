use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use clew_core::scan::materialize::Materializer;
use clew_core::scan::progress::Progress;
use clew_core::snapshot::Entry;
use clew_core::store::{self, repo};
use clew_core::walk;
use tauri::State;

use super::state::*;

pub(crate) fn open_ready(path: &Path) -> Result<(Index, OpenResult), String> {
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
            let card = clew_core::card::CardId::new(&module, &qualname).into_string();
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

pub(crate) fn remember(state: &State<'_, AppState>, index: Index) -> Result<(), String> {
    *index_guard(&state) = Some(index);
    Ok(())
}

/// Kick off a scan on a worker thread and return the handle the UI polls.
pub(crate) fn spawn_scan(root: PathBuf) -> Arc<ScanJob> {
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
pub(crate) fn open_project(path: String, state: State<'_, AppState>) -> Result<OpenResult, String> {
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
pub(crate) fn scan_status(state: State<'_, AppState>) -> Result<ScanStatus, String> {
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
pub(crate) fn delete_index(path: String, state: State<'_, AppState>) -> Result<(), String> {
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

