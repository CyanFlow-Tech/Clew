// 前端资源（index.html / app.js / shell.js / themes.js / vendor）是在编译期
// 嵌进二进制的。cargo 默认只看这个 crate 的源码，改完 js 跑 cargo build 会直接
// 复用旧产物，界面还是老样子，看不出改过。这里把 src 下每个文件都登记成依赖。
use std::path::Path;

fn watch(dir: &Path) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            watch(&path);
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

fn main() {
    tauri_build::build();
    // 相对路径以 crate 根（src-tauri）为基准
    watch(Path::new("../src"));
}
