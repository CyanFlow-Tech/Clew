//! 收藏与最近使用：一个 JSON 文件的读改写。
//!
//! 文件里按项目路径分组，每组两串卡片 id。整个文件是「读全部、改、写回」，
//! 所以同时进来的两个调用会互相覆盖（表现是刚收藏的东西下次打开不见了）。
//! 调用方必须串行，锁不在这一层：见 `AppState::roots`。

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 同一个项目的收藏与最近使用。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectRoots {
    #[serde(default)]
    pub favorites: Vec<String>,
    #[serde(default)]
    pub recent: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RootsFile {
    #[serde(default)]
    pub projects: BTreeMap<String, ProjectRoots>,
}

/// 最近使用留这么多条，再多就压掉最早的。
pub const RECENT_MAX: usize = 12;

/// 一个 roots.json 的句柄。所有方法都只碰磁盘，不碰 Tauri。
pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(path: impl Into<PathBuf>) -> Store {
        Store { path: path.into() }
    }


    /// 读整个文件。文件不存在、读不动、或者内容坏了都当成空表。
    ///
    /// 坏文件会先复制成 `roots.json.bad` 再当空表用：早先这里直接返回错误，
    /// 于是一次半截写入就让之后所有收藏都写不进去，而且用户看不到任何提示。
    pub fn read(&self) -> RootsFile {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(_) => return RootsFile::default(),
        };
        match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => {
                let _ = std::fs::copy(&self.path, self.path.with_extension("json.bad"));
                RootsFile::default()
            }
        }
    }

    /// 写整个文件。先写临时文件再改名，被打断也不会留下半截 JSON。
    pub fn write(&self, data: &RootsFile) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &self.path)
    }

    /// 读、改、写一次完成，返回该项目的最新状态。
    ///
    /// `changed` 为 false 表示这条命令只是查询，文件没动过 —— 早先连 `get`
    /// 都会把整个文件重写一遍，白写一次还多开一次覆盖的窗口。
    pub fn apply(
        &self,
        project: &str,
        action: &str,
        card_id: Option<&str>,
    ) -> Result<(ProjectRoots, bool), String> {
        let mut data = self.read();
        let entry = data.projects.entry(project.to_string()).or_default();
        let mut changed = false;

        match action {
            "toggle" => {
                let id = card_id.ok_or("缺少 card_id")?;
                if let Some(pos) = entry.favorites.iter().position(|x| x == id) {
                    entry.favorites.remove(pos);
                } else {
                    entry.favorites.push(id.to_string());
                }
                changed = true;
            }
            "recent" => {
                let id = card_id.ok_or("缺少 card_id")?;
                entry.recent.retain(|x| x != id);
                entry.recent.insert(0, id.to_string());
                entry.recent.truncate(RECENT_MAX);
                changed = true;
            }
            _ => {}
        }

        let out = entry.clone();
        if changed {
            self.write(&data).map_err(|e| e.to_string())?;
        }
        Ok((out, changed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn temp_store(name: &str) -> (Store, PathBuf) {
        let dir = std::env::temp_dir().join(format!("clew-roots-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roots.json");
        (Store::new(path.clone()), path)
    }

    #[test]
    fn write_then_read_roundtrips() {
        let (store, _) = temp_store("roundtrip");
        store
            .apply("proj", "toggle", Some("pkg.mod:main"))
            .unwrap();
        let data = store.read();
        assert_eq!(data.projects["proj"].favorites, vec!["pkg.mod:main"]);
    }

    #[test]
    fn toggle_adds_then_removes() {
        let (store, _) = temp_store("toggle");
        let (after_add, changed) = store.apply("p", "toggle", Some("a:b")).unwrap();
        assert!(changed);
        assert_eq!(after_add.favorites, vec!["a:b"]);
        let (after_remove, _) = store.apply("p", "toggle", Some("a:b")).unwrap();
        assert!(after_remove.favorites.is_empty());
    }

    #[test]
    fn recent_dedupes_and_caps() {
        let (store, _) = temp_store("recent");
        for i in 0..20 {
            store.apply("p", "recent", Some(&format!("m:f{i}"))).unwrap();
        }
        let data = store.read();
        let recent = &data.projects["p"].recent;
        assert_eq!(recent.len(), RECENT_MAX, "only the newest survive");
        assert_eq!(recent[0], "m:f19", "newest first");

        // 再点一次老条目：提到最前，不重复
        store.apply("p", "recent", Some("m:f15")).unwrap();
        let data = store.read();
        assert_eq!(data.projects["p"].recent[0], "m:f15");
        assert_eq!(
            data.projects["p"].recent.iter().filter(|x| *x == "m:f15").count(),
            1
        );
    }

    #[test]
    fn query_does_not_rewrite_the_file() {
        let (store, path) = temp_store("query");
        store.apply("p", "toggle", Some("a:b")).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        let (roots, changed) = store.apply("p", "get", None).unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(!changed);
        assert_eq!(before, after, "get must not touch the file");
        assert_eq!(roots.favorites, vec!["a:b"]);
    }

    #[test]
    fn broken_file_is_kept_as_bad_copy() {
        let (store, path) = temp_store("broken");
        std::fs::write(&path, "{\"projects\": {\"p\": {\"favorites\": [\"a").unwrap();
        let data = store.read();
        assert!(data.projects.is_empty(), "broken json reads as empty");
        assert!(
            path.with_extension("json.bad").exists(),
            "the broken file is preserved for inspection"
        );
        // 还能继续写
        store.apply("p", "toggle", Some("x:y")).unwrap();
        assert_eq!(store.read().projects["p"].favorites, vec!["x:y"]);
    }

    #[test]
    fn concurrent_toggles_through_one_lock_do_not_lose_entries() {
        // 模拟命令层：读改写被一把锁串起来，二十个 id 一个都不能少
        let (store, _) = temp_store("concurrent");
        let store = Arc::new(Mutex::new(store));
        let mut handles = Vec::new();
        for i in 0..20 {
            let store = store.clone();
            handles.push(std::thread::spawn(move || {
                let guard = store.lock().unwrap();
                guard
                    .apply("p", "toggle", Some(&format!("m:f{i}")))
                    .unwrap();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let data = store.lock().unwrap().read();
        assert_eq!(data.projects["p"].favorites.len(), 20);
    }
}
