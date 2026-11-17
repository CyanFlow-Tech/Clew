//! 项目空间：索引、收藏、缩略图以外的所有用户数据都放在这里。
//!
//! 软件只做阅读，所以被阅读的项目目录一个字节都不落。空间根默认是程序根下的
//! `.clew-home`，用户选了别的目录之后写进程序根下的 `clew.home.txt`——「空间在
//! 哪」这个答案不能存在空间里。`CLEW_HOME` 环境变量优先级最高，留给调试。

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::error::ClewError;

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
pub fn ensure_empty_target(target: &Path) -> Result<(), ClewError> {
    match std::fs::read_dir(target) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                Err(ClewError::refused(format!(
                    "目标目录不是空的：{}",
                    target.display()
                )))
            } else {
                Ok(())
            }
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(ClewError::Io {
            doing: "读目标目录",
            path: target.to_path_buf(),
            source,
        }),
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
    ensure_empty_target(to)?;
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

mod cache;
pub use cache::*;


#[cfg(test)]
mod tests;
