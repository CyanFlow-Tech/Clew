use std::path::{Path, PathBuf};

use super::*;

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
