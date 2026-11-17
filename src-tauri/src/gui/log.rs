use std::path::PathBuf;



pub(crate) fn data_dir() -> PathBuf {
    clew_core::space::home()
}

/// 追加一行日志。窗口程序没有控制台，出错时只剩「点了没反应」，
/// 不落一份文件就无从查起。
pub(crate) fn append_log(scope: &str, line: &str) {
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
pub(crate) fn log_line(text: String) {
    append_log("web", &text);
}

/// 命令里的 panic 记一份，否则它只是静悄悄地把锁毒掉。
pub(crate) fn install_panic_log() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        append_log("panic", &info.to_string());
        previous(info);
    }));
}

