//! Entry-point discovery: `__main__` guards and `[project.scripts]` in `pyproject.toml`.

use std::path::Path;

/// Placeholder qualname used by module-level entry symbols.
///
/// 定义在 [`crate::card`] 里——卡片身份是 `card` 模块的概念，这里只重新导出，
/// 让原来的 `entry::MODULE_ENTRY_QUALNAME` 路径继续可用。
pub use crate::card::MODULE_ENTRY_QUALNAME;

/// Parse `[project.scripts]` from `pyproject.toml` and return `(module, qualname)` pairs.
///
/// A value looks like `"pkg.mod:func"`: dotted module before the colon, function name after.
pub fn scripts_from_pyproject(root: &Path) -> Vec<(String, String)> {
    let path = root.join("pyproject.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(value) = toml::from_str::<toml::Value>(&text) else {
        return Vec::new();
    };
    let Some(scripts) = value
        .get("project")
        .and_then(|p| p.get("scripts"))
        .and_then(|s| s.as_table())
    else {
        return Vec::new();
    };

    scripts
        .values()
        .filter_map(|v| v.as_str())
        .filter_map(crate::card::CardId::parse)
        .map(|id| (id.module().trim().to_string(), id.qualname().trim().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_project_scripts() {
        let dir = std::env::temp_dir().join(format!("clew-entry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("pyproject.toml"),
            r#"
[project]
name = "demo"

[project.scripts]
demo = "pkg.cli:main"
other = "pkg.other:run"
"#,
        )
        .unwrap();

        let mut got = scripts_from_pyproject(&dir);
        got.sort();
        assert_eq!(
            got,
            vec![
                ("pkg.cli".to_string(), "main".to_string()),
                ("pkg.other".to_string(), "run".to_string()),
            ]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_pyproject_is_empty() {
        let dir = std::env::temp_dir().join(format!("clew-entry-none-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(scripts_from_pyproject(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
