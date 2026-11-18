//! Walk the project and collect source files.

use std::path::{Path, PathBuf};

use tree_sitter::Parser;
use walkdir::WalkDir;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub path: PathBuf,
    pub module: String,
    pub is_third_party: bool,
}

pub trait Language: Send + Sync {
    fn name(&self) -> &'static str;
    fn is_source(&self, path: &Path) -> bool;
    fn module_of(&self, root: &Path, path: &Path) -> String;
    fn new_parser(&self) -> Parser;
}

pub struct Python;

impl Language for Python {
    fn name(&self) -> &'static str {
        "python"
    }

    fn is_source(&self, path: &Path) -> bool {
        matches!(path.extension().and_then(|s| s.to_str()), Some("py"))
    }

    fn module_of(&self, root: &Path, path: &Path) -> String {
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

    fn new_parser(&self) -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .expect("tree-sitter-python grammar");
        parser
    }
}

pub fn python() -> &'static Python {
    &Python
}

pub fn collect(root: &Path) -> Vec<SourceFile> {
    collect_for(python(), root, false)
}

pub fn collect_for(lang: &dyn Language, root: &Path, include_venv: bool) -> Vec<SourceFile> {
    let mut files: Vec<SourceFile> = WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_skipped(e.path(), include_venv))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| lang.is_source(e.path()))
        .map(|e| SourceFile {
            path: e.path().to_path_buf(),
            module: lang.module_of(root, e.path()),
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

pub fn is_third_party(path: &Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/");
    s.contains("/site-packages/") || s.contains("/dist-packages/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_names() {
        let root = Path::new("D:/proj");
        let py = python();
        assert_eq!(
            py.module_of(root, Path::new("D:/proj/pkg/sub/mod.py")),
            "pkg.sub.mod"
        );
        assert_eq!(
            py.module_of(root, Path::new("D:/proj/pkg/__init__.py")),
            "pkg"
        );
        assert_eq!(py.module_of(root, Path::new("D:/proj/top.py")), "top");
    }

    #[test]
    fn only_python_files_are_sources() {
        let py = python();
        assert!(py.is_source(Path::new("a/b.py")));
        assert!(!py.is_source(Path::new("a/b.js")));
        assert!(!py.is_source(Path::new("a/b")));
    }

    #[test]
    fn third_party_detection() {
        assert!(is_third_party(Path::new(
            "D:/proj/.venv/Lib/site-packages/requests/api.py"
        )));
        assert!(!is_third_party(Path::new("D:/proj/pkg/api.py")));
    }
}
