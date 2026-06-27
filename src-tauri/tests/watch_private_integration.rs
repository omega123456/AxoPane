mod fs {
    pub use file_explorer_lib::fs::*;
}

mod watch_src {
    include!("../src/watch/mod.rs");

    #[cfg(test)]
    mod tests {
        use super::*;
        use tempfile::tempdir;

        fn entry(path: &str, name: &str) -> DirectoryEntry {
            DirectoryEntry {
                id: path.to_string(),
                name: name.to_string(),
                path: path.to_string(),
                is_dir: false,
                size_bytes: Some(1),
                item_count: None,
                type_label: "TXT file".to_string(),
                modified_at: None,
                created_at: None,
                attributes: Vec::new(),
                is_hidden: false,
                is_system: false,
            }
        }

        #[test]
        fn pane_scope_uses_prefix_before_dash() {
            assert_eq!(pane_scope("left-1"), "left");
            assert_eq!(pane_scope("right"), "right");
        }

        #[test]
        fn diff_entries_reports_changed_and_removed_paths() {
            let previous = HashMap::from([
                (
                    "C:/before.txt".to_string(),
                    entry("C:/before.txt", "before.txt"),
                ),
                ("C:/same.txt".to_string(), entry("C:/same.txt", "same.txt")),
            ]);
            let next = HashMap::from([
                ("C:/same.txt".to_string(), entry("C:/same.txt", "same.txt")),
                (
                    "C:/after.txt".to_string(),
                    entry("C:/after.txt", "after.txt"),
                ),
            ]);

            let patch = diff_entries("left-1", "C:/", "refresh", &previous, &next);
            assert_eq!(patch.tab_id, "left-1");
            assert_eq!(patch.reason, "refresh");
            assert!(patch
                .changed
                .iter()
                .any(|item| item.path.ends_with("after.txt")));
            assert!(patch
                .removed
                .iter()
                .any(|path| path.ends_with("before.txt")));
        }

        #[test]
        fn snapshot_for_target_applies_hidden_and_filter_rules() {
            let fixture = tempdir().expect("temp dir");
            let root = fixture.path();
            std::fs::write(root.join("alpha.txt"), b"a").expect("alpha");
            std::fs::write(root.join("beta.log"), b"b").expect("beta");
            std::fs::write(root.join(".secret"), b"s").expect("secret");

            let filtered = snapshot_for_target(&WatchTarget {
                tab_id: "left-1".to_string(),
                path: root.to_string_lossy().into_owned(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: "alpha".to_string(),
                show_hidden: false,
            })
            .expect("filtered snapshot");
            assert_eq!(filtered.len(), 1);
            assert!(filtered.keys().any(|path| path.ends_with("alpha.txt")));

            let with_hidden = snapshot_for_target(&WatchTarget {
                tab_id: "left-1".to_string(),
                path: root.to_string_lossy().into_owned(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: String::new(),
                show_hidden: true,
            })
            .expect("hidden snapshot");
            assert!(with_hidden.keys().any(|path| path.ends_with(".secret")));
        }

        #[test]
        fn add_and_remove_watch_updates_reference_counts() {
            let fixture = tempdir().expect("temp dir");
            let root = fixture.path();
            let mut runtime = create_runtime(|_| {}, |_, _| {}).expect("runtime");

            add_watch(&mut runtime, root).expect("first add");
            add_watch(&mut runtime, root).expect("second add");
            assert_eq!(runtime.watch_counts.get(root), Some(&2));

            remove_watch(&mut runtime, root).expect("first remove");
            assert_eq!(runtime.watch_counts.get(root), Some(&1));

            remove_watch(&mut runtime, root).expect("second remove");
            assert!(!runtime.watch_counts.contains_key(root));

            remove_watch(&mut runtime, root).expect("missing remove is noop");
        }
    }
}
