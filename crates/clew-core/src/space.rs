//! 项目空间：索引、收藏、缩略图以外的所有用户数据都放在这里。
//!
//! 软件只做阅读，所以被阅读的项目目录一个字节都不落。空间根默认是程序根下的
//! `.clew-home`，用户选了别的目录之后写进程序根下的 `clew.home.txt`——「空间在
//! 哪」这个答案不能存在空间里。`CLEW_HOME` 环境变量优先级最高，留给调试。

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 空间根的指针文件。放在程序根下，不放在空间里。
const POINTER: &str = "clew.home.txt";

/// 空间里受管理的项。**新增用户数据文件必须在这里注册**：迁移按这张表走，
/// 没注册的东西换空间时会被留在旧位置。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Dir,
    File,
}

pub struct DataEntry {
    pub name: &'static str,
    pub kind: EntryKind,
}

pub const DATA_ENTRIES: &[DataEntry] = &[
    DataEntry {
        name: "projects",
        kind: EntryKind::Dir,
    },
    DataEntry {
        name: "roots.json",
        kind: EntryKind::File,
    },
    DataEntry {
        name: "settings.json",
        kind: EntryKind::File,
    },
];

/// 进程内的覆盖。用户改完空间根要立刻生效，不必等重启。
static OVERRIDE: Mutex<Option<PathBuf>> = Mutex::new(None);

/// 程序根：开发期是仓库根，编译期定死，与启动时的工作目录无关。
pub fn program_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // crates/
        .and_then(Path::parent) // 仓库根
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

/// 默认空间根：程序根下的 `.clew-home`。
pub fn default_home() -> PathBuf {
    program_root().join(".clew-home")
}

/// 指针文件位置。
pub fn pointer_path() -> PathBuf {
    program_root().join(POINTER)
}

fn read_pointer() -> Option<PathBuf> {
    let text = std::fs::read_to_string(pointer_path()).ok()?;
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))?;
    let path = PathBuf::from(line);
    // 相对路径不可信：进程的工作目录会变，认错空间比认不到更糟
    path.is_absolute().then_some(path)
}

/// 项目空间根。优先级：`CLEW_HOME` > 本进程设置过的 > 指针文件 > 默认值。
pub fn home() -> PathBuf {
    if let Some(value) = std::env::var_os("CLEW_HOME") {
        if !value.is_empty() {
            return PathBuf::from(value);
        }
    }
    if let Some(path) = OVERRIDE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
    {
        return path;
    }
    if let Some(path) = read_pointer() {
        return path;
    }
    default_home()
}

/// 换空间根：写指针文件，同时让本进程立刻用新的。
pub fn set_home(path: &Path) -> io::Result<()> {
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "空间根要用绝对路径",
        ));
    }
    std::fs::create_dir_all(path)?;
    std::fs::write(pointer_path(), format!("{}\n", path.display()))?;
    *OVERRIDE.lock().unwrap_or_else(|e| e.into_inner()) = Some(path.to_path_buf());
    Ok(())
}

/// 一个路径归一成能当 id 用的串：去掉 Windows 的 `\\?\` 前缀，分隔符统一成
/// `/`，统一小写，去掉末尾的分隔符。同一个目录从窗口和命令行进来必须得到同一个
/// 字符串，否则两边会各自算出一套目录。
fn normalize(root: &Path) -> String {
    let text = root.to_string_lossy().to_string();
    let text = match text.strip_prefix(r"\\?\UNC\") {
        Some(rest) => format!(r"\\{rest}"),
        None => text,
    };
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text).replace('\\', "/");
    let text = text.to_lowercase();
    let text = text.trim_end_matches('/').to_string();
    // 盘符根 `d:` 补回斜杠，免得 `d:` 和 `d:/` 算出两个 id
    if text.ends_with(':') {
        format!("{text}/")
    } else {
        text
    }
}

/// 项目 id：规范化路径的 blake3 前 16 位。
///
/// 路径被移动或删除即作废，不做「认回」——与主流 IDE 一致。目录已经不在时退回
/// 传入的路径继续算，这样删索引还能找到空间里的目录。
pub fn project_id(root: &Path) -> String {
    let resolved = std::fs::canonicalize(root).unwrap_or_else(|_| {
        if root.is_absolute() {
            root.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(root))
                .unwrap_or_else(|_| root.to_path_buf())
        }
    });
    let hash = blake3::hash(normalize(&resolved).as_bytes());
    hash.to_hex().chars().take(16).collect()
}

/// 一个项目在空间里的目录。
pub fn project_dir(root: &Path) -> PathBuf {
    home().join("projects").join(project_id(root))
}

/// 一个项目的库文件位置。
pub fn db_path(root: &Path) -> PathBuf {
    project_dir(root).join("index.db")
}

/// 迁移结果，给界面报数用。
#[derive(Debug, Default)]
pub struct MigrateReport {
    pub files: usize,
    pub bytes: u64,
    /// 真的搬过去的注册项
    pub items: Vec<String>,
}

fn copy_file(src: &Path, dst: &Path, report: &mut MigrateReport) -> anyhow::Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(src, dst)?;
    // 复制完比一次大小：磁盘满或断电会留下短文件，等以后读不出来才发现就晚了
    let copied = std::fs::metadata(dst)?.len();
    if copied != std::fs::metadata(src)?.len() {
        anyhow::bail!("复制不完整：{}", src.display());
    }
    report.files += 1;
    report.bytes += copied;
    Ok(())
}

fn copy_tree(src: &Path, dst: &Path, report: &mut MigrateReport) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.metadata()?.is_dir() {
            copy_tree(&entry.path(), &target, report)?;
        } else {
            copy_file(&entry.path(), &target, report)?;
        }
    }
    Ok(())
}

/// 目标必须是空目录（不存在也算空）。宁可让用户自己清，也不做覆盖。
pub fn ensure_empty_target(target: &Path) -> Result<(), String> {
    match std::fs::read_dir(target) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                Err(format!("目标目录不是空的：{}", target.display()))
            } else {
                Ok(())
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("读不了目标目录 {}：{err}", target.display())),
    }
}

/// 把 `from` 里注册过的项搬到 `to`。目标必须为空，注册项以外的文件不碰。
pub fn migrate(from: &Path, to: &Path) -> anyhow::Result<MigrateReport> {
    if same_path(from, to) {
        anyhow::bail!("新旧空间是同一个目录");
    }
    if to.starts_with(from) {
        anyhow::bail!("目标目录在旧空间里面：{}", to.display());
    }
    ensure_empty_target(to).map_err(|err| anyhow::anyhow!(err))?;
    std::fs::create_dir_all(to)?;

    let mut report = MigrateReport::default();
    for item in DATA_ENTRIES {
        let src = from.join(item.name);
        if !src.exists() {
            continue;
        }
        let dst = to.join(item.name);
        match item.kind {
            EntryKind::Dir => copy_tree(&src, &dst, &mut report)?,
            EntryKind::File => copy_file(&src, &dst, &mut report)?,
        }
        report.items.push(item.name.to_string());
    }
    Ok(report)
}

fn same_path(a: &Path, b: &Path) -> bool {
    normalize(a) == normalize(b)
}

/// 界面缓存目录里值得跟着走的项。
///
/// `EBWebView` 下绝大多数内容是 GPU 缓存、Code Cache 这类可丢的垃圾，几万个文件；
/// 真正属于用户的是 `Local Storage`——项目列表、工作区快照、主题都存在那里，
/// 丢了就是「打开程序一个项目都没有」。所以只登记这一项。
pub fn cache_entries() -> Vec<PathBuf> {
    vec![PathBuf::from("EBWebView")
        .join("Default")
        .join("Local Storage")]
}

/// 把 `from_cache` 里登记过的缓存项搬到 `to_cache`。
///
/// 目标已经有同名项就跳过：新位置攒下的 localStorage 可能更新，不覆盖。
pub fn carry_cache(from_cache: &Path, to_cache: &Path) -> anyhow::Result<usize> {
    let mut moved = 0;
    for rel in cache_entries() {
        let src = from_cache.join(&rel);
        if !src.exists() {
            continue;
        }
        let dst = to_cache.join(&rel);
        if dst.exists() {
            continue;
        }
        let mut report = MigrateReport::default();
        copy_tree(&src, &dst, &mut report)?;
        moved += 1;
    }
    Ok(moved)
}

/// 把当前缓存目录的登记项搬到目标，**覆盖**目标已有的同名项。
///
/// 覆盖前目标那份先改名留档成 `<项>.previous`：恢复默认时用户想带回去的是
/// 现在正在用的那份（否则刚攒的列表又看不见），但默认目录里上一次留下的
/// 也不该直接消失。
pub fn replace_cache(from_cache: &Path, to_cache: &Path) -> anyhow::Result<()> {
    for rel in cache_entries() {
        let src = from_cache.join(&rel);
        if !src.exists() {
            continue;
        }
        let dst = to_cache.join(&rel);
        if dst.exists() {
            let backup = dst.with_extension("previous");
            if backup.exists() {
                std::fs::remove_dir_all(&backup)?;
            }
            std::fs::rename(&dst, &backup)?;
        }
        let mut report = MigrateReport::default();
        copy_tree(&src, &dst, &mut report)?;
    }
    Ok(())
}

/// 清掉空间里注册过的项。**不删空间根目录本身**——用户可能在里面放了自己的东西。
pub fn clear_entries(dir: &Path) -> io::Result<()> {
    for item in DATA_ENTRIES {
        let path = dir.join(item.name);
        if !path.exists() {
            continue;
        }
        match item.kind {
            EntryKind::Dir => std::fs::remove_dir_all(&path)?,
            EntryKind::File => std::fs::remove_file(&path)?,
        }
    }
    Ok(())
}

/// 目录空了就一并删掉，留着只会让用户以为还有东西。
pub fn remove_dir_if_empty(dir: &Path) -> io::Result<bool> {
    match std::fs::read_dir(dir) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return Ok(false);
            }
            std::fs::remove_dir(dir)?;
            Ok(true)
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("clew-space-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn normalize_drops_the_windows_prefix() {
        assert_eq!(normalize(Path::new(r"\\?\D:\Foo\Clew")), "d:/foo/clew");
    }

    #[test]
    fn normalize_keeps_unc_shares_readable() {
        assert_eq!(normalize(Path::new(r"\\?\UNC\srv\share\x")), "//srv/share/x");
    }

    #[test]
    fn normalize_ignores_trailing_separators_and_case() {
        assert_eq!(normalize(Path::new(r"D:/Foo/Clew/")), "d:/foo/clew");
        assert_eq!(normalize(Path::new(r"D:\Foo\Clew")), "d:/foo/clew");
    }

    #[test]
    fn normalize_keeps_the_drive_root_slash() {
        assert_eq!(normalize(Path::new(r"D:\")), "d:/");
    }

    #[test]
    fn the_same_directory_gets_the_same_id() {
        let a = project_id(Path::new(r"D:\NoSuchClewDir\proj"));
        let b = project_id(Path::new(r"D:\NoSuchClewDir\proj"));
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn different_directories_get_different_ids() {
        let a = project_id(Path::new(r"D:\NoSuchClewDir\one"));
        let b = project_id(Path::new(r"D:\NoSuchClewDir\two"));
        assert_ne!(a, b);
    }

    #[test]
    fn the_database_lives_under_the_space() {
        let root = Path::new(r"D:\NoSuchClewDir\proj");
        let db = db_path(root);
        let id = project_id(root);
        assert!(db.starts_with(home()));
        assert!(db.ends_with(format!("projects/{id}/index.db")));
    }

    #[test]
    fn an_absent_target_counts_as_empty() {
        let dir = scratch("absent");
        let target = dir.join("nothing-here");
        assert!(ensure_empty_target(&target).is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_target_with_a_file_is_not_empty() {
        let dir = scratch("notempty");
        std::fs::write(dir.join("keep.txt"), "x").unwrap();
        assert!(ensure_empty_target(&dir).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn migration_copies_registered_items_only() {
        let base = scratch("migrate");
        let from = base.join("old");
        let to = base.join("new");
        std::fs::create_dir_all(from.join("projects").join("abc")).unwrap();
        std::fs::write(from.join("projects").join("abc").join("index.db"), vec![7u8; 32]).unwrap();
        std::fs::write(from.join("roots.json"), "{}").unwrap();
        std::fs::write(from.join("clew.log"), "log").unwrap();

        let report = migrate(&from, &to).unwrap();
        assert_eq!(report.files, 2);
        assert_eq!(report.bytes, 34);
        assert!(to.join("projects").join("abc").join("index.db").exists());
        assert!(to.join("roots.json").exists());
        // 没注册的文件留在原地，不跟着走
        assert!(!to.join("clew.log").exists());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn migration_refuses_a_target_inside_the_source() {
        let base = scratch("nested");
        let from = base.join("old");
        std::fs::create_dir_all(&from).unwrap();
        let err = migrate(&from, &from.join("inner")).unwrap_err();
        assert!(err.to_string().contains("里面"));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn migration_refuses_a_target_that_has_files() {
        let base = scratch("occupied");
        let from = base.join("old");
        let to = base.join("new");
        std::fs::create_dir_all(&from).unwrap();
        std::fs::create_dir_all(&to).unwrap();
        std::fs::write(to.join("someone.txt"), "x").unwrap();
        assert!(migrate(&from, &to).is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn cache_round_trip_keeps_local_storage_only() {
        let base = scratch("cache");
        let from = base.join("old");
        let to = base.join("new");

        let storage = from
            .join("EBWebView")
            .join("Default")
            .join("Local Storage")
            .join("leveldb");
        std::fs::create_dir_all(&storage).unwrap();
        std::fs::write(storage.join("000003.log"), b"projects").unwrap();
        // 纯缓存，不该跟着走
        let gpu = from.join("EBWebView").join("Default").join("GPUCache");
        std::fs::create_dir_all(&gpu).unwrap();
        std::fs::write(gpu.join("data_0"), vec![0u8; 64]).unwrap();

        let moved = carry_cache(&from, &to).unwrap();
        assert_eq!(moved, 1);
        assert!(to
            .join("EBWebView")
            .join("Default")
            .join("Local Storage")
            .join("leveldb")
            .join("000003.log")
            .exists());
        assert!(!to.join("EBWebView").join("Default").join("GPUCache").exists());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn cache_round_trip_does_not_overwrite_a_newer_copy() {
        let base = scratch("cache-skip");
        let from = base.join("old");
        let to = base.join("new");
        let rel = PathBuf::from("EBWebView")
            .join("Default")
            .join("Local Storage");
        std::fs::create_dir_all(from.join(&rel)).unwrap();
        std::fs::write(from.join(&rel).join("old.log"), b"old").unwrap();
        std::fs::create_dir_all(to.join(&rel)).unwrap();
        std::fs::write(to.join(&rel).join("new.log"), b"new").unwrap();

        let moved = carry_cache(&from, &to).unwrap();
        assert_eq!(moved, 0);
        assert!(to.join(&rel).join("new.log").exists());
        assert!(!to.join(&rel).join("old.log").exists());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn replace_cache_overwrites_and_keeps_a_backup() {
        let base = scratch("cache-replace");
        let from = base.join("old");
        let to = base.join("new");
        let rel = PathBuf::from("EBWebView")
            .join("Default")
            .join("Local Storage");

        std::fs::create_dir_all(from.join(&rel)).unwrap();
        std::fs::write(from.join(&rel).join("current.log"), b"newer").unwrap();
        std::fs::create_dir_all(to.join(&rel)).unwrap();
        std::fs::write(to.join(&rel).join("old.log"), b"older").unwrap();

        replace_cache(&from, &to).unwrap();

        // 目标里换成当前这份
        assert!(to.join(&rel).join("current.log").exists());
        assert!(!to.join(&rel).join("old.log").exists());
        // 原来那份留档，没被直接删掉
        let backup = to.join(&rel).with_extension("previous");
        assert!(backup.join("old.log").exists());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn clearing_leaves_unregistered_files_alone() {        let dir = scratch("clear");
        std::fs::create_dir_all(dir.join("projects").join("abc")).unwrap();
        std::fs::write(dir.join("roots.json"), "{}").unwrap();
        std::fs::write(dir.join("notes-of-the-user.txt"), "keep").unwrap();

        clear_entries(&dir).unwrap();
        assert!(!dir.join("projects").exists());
        assert!(!dir.join("roots.json").exists());
        assert!(dir.join("notes-of-the-user.txt").exists());
        assert!(!remove_dir_if_empty(&dir).unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
