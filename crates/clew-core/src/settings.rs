//! 用户设置。跟数据目录同级的一份 json，目前只有一项：界面缓存目录。
//!
//! 只记「用户显式改过」的值。没设过就不写文件，缓存跟着数据目录走——
//! 这样迁移数据目录时缓存路径不会变成指向旧目录的绝对路径。

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// settings.json 的形状。字段加进来时保持向后兼容：缺字段按默认值读。
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    /// WebView2 的用户数据目录。相对路径不可信，读的时候用默认值顶回去。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webview_dir: Option<String>,
    /// 迁移后没删掉的旧缓存目录。WebView2 当时还占着文件，只能等程序退出后
    /// 下次启动再清，所以记在这里。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_cleanup: Vec<String>,
}

/// settings.json 的位置：数据目录下，跟 roots.json 并排。
pub fn settings_path() -> PathBuf {
    crate::space::home().join("settings.json")
}

/// 界面缓存的默认位置：程序根下的 `.clew-cache`。
///
/// 刻意**不和数据目录嵌套**。缓存默认放在 `<数据目录>\webview` 时，两个设置项
/// 一个套在另一个里：迁移数据目录要顺带搬它、旧目录清完还留个删不掉的
/// `EBWebView`、恢复默认又会撞上默认目录里已有的那份。平级就没有这些纠缠——
/// 迁移数据目录完全不碰缓存，缓存只是自己换地方。
pub fn default_webview_dir() -> PathBuf {
    crate::space::program_root().join(".clew-cache")
}

pub fn read() -> Settings {
    read_at(&settings_path())
}

fn read_at(path: &Path) -> Settings {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Settings::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn write_at(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // 先写临时文件再改名：中断时不会留下半份 json，把设置读坏
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_vec_pretty(settings)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, path)
}

/// 界面缓存该放哪。用户设过绝对路径就用，否则默认。
pub fn webview_dir() -> PathBuf {
    webview_dir_at(&settings_path())
}

fn webview_dir_at(settings_path: &Path) -> PathBuf {
    read_at(settings_path)
        .webview_dir
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(default_webview_dir)
}

/// 改界面缓存目录。传 `None` 回到默认。
///
/// 只影响**下次启动**：WebView2 在进程起来时就把目录锁死了，当前进程改不了。
pub fn set_webview_dir(dir: Option<&Path>) -> io::Result<PathBuf> {
    set_webview_dir_at(&settings_path(), dir)
}

fn set_webview_dir_at(path: &Path, dir: Option<&Path>) -> io::Result<PathBuf> {    let mut settings = read_at(path);
    match dir {
        Some(target) if !target.is_absolute() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "缓存目录要用绝对路径",
            ));
        }
        Some(target) => settings.webview_dir = Some(target.display().to_string()),
        None => settings.webview_dir = None,
    }
    write_at(path, &settings)?;
    Ok(webview_dir_at(path))
}

/// 迁移后没删掉的旧缓存目录，按登记顺序返回。
pub fn pending_cleanup() -> Vec<PathBuf> {
    pending_cleanup_at(&settings_path())
}

fn pending_cleanup_at(path: &Path) -> Vec<PathBuf> {
    read_at(path)
        .pending_cleanup
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .collect()
}

/// 记下一个待清理的旧缓存目录。WebView2 还占着文件时只能先记着，
/// 等下次启动、窗口还没建起来的时候再删。
pub fn add_pending_cleanup(dir: &Path) -> io::Result<()> {
    add_pending_cleanup_at(&settings_path(), dir)
}

fn add_pending_cleanup_at(path: &Path, dir: &Path) -> io::Result<()> {
    if !dir.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "待清理的目录要用绝对路径",
        ));
    }
    let mut settings = read_at(path);
    let entry = dir.display().to_string();
    if !settings.pending_cleanup.contains(&entry) {
        settings.pending_cleanup.push(entry);
        write_at(path, &settings)?;
    }
    Ok(())
}

/// 清掉一条待清理记录（删成功或目录本来就不在了）。
pub fn remove_pending_cleanup(dir: &Path) -> io::Result<()> {
    remove_pending_cleanup_at(&settings_path(), dir)
}

fn remove_pending_cleanup_at(path: &Path, dir: &Path) -> io::Result<()> {
    let mut settings = read_at(path);
    let entry = dir.display().to_string();
    let before = settings.pending_cleanup.len();
    settings.pending_cleanup.retain(|item| *item != entry);
    if settings.pending_cleanup.len() != before {
        write_at(path, &settings)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 全部落在临时目录里，不碰真实的 settings.json。
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("clew-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_default_sits_beside_the_space_not_inside_it() {
        let default = default_webview_dir();
        assert_eq!(default, crate::space::program_root().join(".clew-cache"));
        // 两个默认目录不能嵌套：嵌起来迁移与恢复默认都会互相牵扯
        assert!(!default.starts_with(crate::space::home()));
        assert_ne!(default, crate::space::home());
    }

    #[test]
    fn a_relative_path_is_refused() {
        let dir = scratch("relative");
        let file = dir.join("settings.json");
        let err = set_webview_dir_at(&file, Some(Path::new("webview"))).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn setting_a_directory_round_trips() {
        let dir = scratch("roundtrip");
        let file = dir.join("settings.json");
        let target = dir.join("cache");

        assert_eq!(set_webview_dir_at(&file, Some(&target)).unwrap(), target);
        assert_eq!(webview_dir_at(&file), target);

        // 传 None 回到默认，文件里不再留这个字段
        assert_eq!(set_webview_dir_at(&file, None).unwrap(), default_webview_dir());
        let saved = std::fs::read_to_string(&file).unwrap();
        assert!(!saved.contains("webview_dir"), "field should be gone: {saved}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_relative_path_in_the_file_falls_back_to_the_default() {
        let dir = scratch("fallback");
        let file = dir.join("settings.json");
        write_at(
            &file,
            &Settings {
                webview_dir: Some("webview".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(webview_dir_at(&file), default_webview_dir());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn pending_cleanup_round_trips_and_dedupes() {
        let dir = scratch("pending");
        let file = dir.join("settings.json");
        let one = dir.join("old-cache");
        let two = dir.join("older-cache");

        add_pending_cleanup_at(&file, &one).unwrap();
        add_pending_cleanup_at(&file, &one).unwrap(); // 同一条不该重复记
        add_pending_cleanup_at(&file, &two).unwrap();
        assert_eq!(pending_cleanup_at(&file), vec![one.clone(), two.clone()]);

        remove_pending_cleanup_at(&file, &one).unwrap();
        assert_eq!(pending_cleanup_at(&file), vec![two]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn pending_cleanup_refuses_relative_paths() {
        let dir = scratch("pending-rel");
        let file = dir.join("settings.json");
        let err = add_pending_cleanup_at(&file, Path::new("old-cache")).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_broken_file_reads_as_defaults() {
        let dir = scratch("broken");
        let file = dir.join("settings.json");
        std::fs::write(&file, "{ not json").unwrap();
        assert!(read_at(&file).webview_dir.is_none());
        assert_eq!(webview_dir_at(&file), default_webview_dir());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
