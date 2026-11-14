//! Walk the project and collect Python source files.

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// Directory names skipped entirely while walking.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".clew",
    ".clew-home",
    ".venv",
    "venv",
    "env",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    "__pycache__",
    "build",
    "dist",
    "node_modules",
];

/// A Python file waiting to be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PyFile {
    pub path: PathBuf,
    pub module: String,
    pub is_third_party: bool,
}

pub fn collect(root: &Path) -> Vec<PyFile> {
    collect_with(root, false)
}

pub fn collect_with(root: &Path, include_venv: bool) -> Vec<PyFile> {
    let mut files: Vec<PyFile> = WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_skipped(e.path(), include_venv))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| is_python(e.path()))
        .map(|e| PyFile {
            path: e.path().to_path_buf(),
            module: module_of(root, e.path()),
            is_third_party: is_third_party(e.path()),
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

fn is_skipped(path: &Path, include_venv: bool) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if include_venv && matches!(name, ".venv" | "venv" | "env") {
        return false;
    }
    SKIP_DIRS.contains(&name)
}

fn is_python(path: &Path) -> bool {
    matches!(path.extension().and_then(|s| s.to_str()), Some("py"))
}

/// Decide whether code is third-party, per IMPLEMENTATION section 8.
pub fn is_third_party(path: &Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/");
    s.contains("/site-packages/") || s.contains("/dist-packages/")
}

/// Derive the dotted module name from the file path; `__init__.py` takes its package name.
pub fn module_of(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let mut parts: Vec<String> = rel
        .components()
        .filter_map(|c| c.as_os_str().to_str().map(str::to_string))
        .collect();
    if let Some(last) = parts.last_mut() {
        if last == "__init__.py" {
            parts.pop();
        } else if let Some(stem) = last.strip_suffix(".py") {
            *last = stem.to_string();
        }
    }
    parts.join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_names() {
        let root = Path::new("D:/proj");
        assert_eq!(
            module_of(root, Path::new("D:/proj/pkg/sub/mod.py")),
            "pkg.sub.mod"
        );
        assert_eq!(module_of(root, Path::new("D:/proj/pkg/__init__.py")), "pkg");
        assert_eq!(module_of(root, Path::new("D:/proj/top.py")), "top");
    }

    #[test]
    fn third_party_detection() {
        assert!(is_third_party(Path::new(
            "D:/proj/.venv/Lib/site-packages/requests/api.py"
        )));
        assert!(!is_third_party(Path::new("D:/proj/pkg/api.py")));
    }
}
