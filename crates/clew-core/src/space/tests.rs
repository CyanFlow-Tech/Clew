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
