#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri shell: the frontend reads the index through these commands only.

// 配置里现在有 devUrl：dev 构建不带资源、直接连本地服务，release 必须带上
// custom-protocol（`cargo app`）才会把 src/ 嵌进二进制。忘了带就是一扇白窗，
// 这里在编译期拦住，别等运行起来才发现。
#[cfg(all(dev, not(debug_assertions)))]
compile_error!(
    "release 构建要带前端资源：cargo build -p clew-app --release --features custom-protocol，或直接用 `cargo app`"
);

use std::time::Instant;

mod gui;
mod roots;

use gui::*;


/// Process start, so every stage can print an absolute offset from launch.
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
            gui::read::ready,
            gui::log::log_line,
            gui::read::files,
            gui::read::symbols_in,
            gui::read::entry_symbols,
            gui::read::search_symbols,
            gui::read::tree,
            gui::roots_cmd::roots,
            gui::space::space_state,
            gui::space::migrate_webview_dir,
            gui::space::migrate_space,
            gui::scan::open_project,
            gui::space::boot,
            gui::scan::scan_status,
            gui::scan::delete_index,
            gui::roots_cmd::check_paths,
            gui::read::node,
            gui::read::code,
            gui::read::cards,
            gui::read::graph,
            gui::read::notes,
            gui::read::add_note,
            gui::read::source_root
        ])
        .run(tauri::generate_context!())
        .expect("Clew failed to start");
}
