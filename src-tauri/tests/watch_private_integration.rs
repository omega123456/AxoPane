mod fs {
    pub use file_explorer_lib::fs::*;
}

mod watch_src {
    include!("../src/watch/mod.rs");

    #[cfg(test)]
    mod tests {
        use super::*;
        use notify::event::{
            CreateKind, DataChange, EventKind, Flag, ModifyKind, RemoveKind, RenameMode,
        };
        use notify::Event;
        use notify_debouncer_full::DebouncedEvent;
        use std::collections::HashSet;
        use std::path::PathBuf;
        use std::time::Instant;
        use tempfile::tempdir;

        fn entry(path: &str, name: &str) -> DirectoryEntry {
            DirectoryEntry {
                id: path.to_string(),
                name: name.to_string(),
                path: path.to_string(),
                is_dir: false,
                icon_data_url: None,
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
        fn targeted_events_patch_snapshot_without_rescanning() {
            let fixture = tempdir().expect("temp dir");
            let root = fixture.path();
            let target = WatchTarget {
                tab_id: "left-1".to_string(),
                path: root.to_string_lossy().into_owned(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: String::new(),
                show_hidden: true,
            };
            let alpha = root.join("alpha.txt");
            std::fs::write(&alpha, b"a").expect("alpha");

            let created = DebouncedEvent::new(
                Event::new(EventKind::Create(CreateKind::File)).add_path(alpha.clone()),
                Instant::now(),
            );
            let PatchResult::Targeted { patch, snapshot } =
                patch_for_events(&target, &HashMap::new(), &[created]).expect("create patch")
            else {
                panic!("create should be targeted");
            };
            assert!(patch
                .changed
                .iter()
                .any(|item| item.path.ends_with("alpha.txt")));
            assert!(snapshot.keys().any(|path| path.ends_with("alpha.txt")));

            std::fs::remove_file(&alpha).expect("remove alpha");
            let removed = DebouncedEvent::new(
                Event::new(EventKind::Remove(RemoveKind::File)).add_path(alpha),
                Instant::now(),
            );
            let PatchResult::Targeted { patch, snapshot } =
                patch_for_events(&target, &snapshot, &[removed]).expect("remove patch")
            else {
                panic!("remove should be targeted");
            };
            assert!(patch.removed.iter().any(|path| path.ends_with("alpha.txt")));
            assert!(snapshot.is_empty());
        }

        #[test]
        fn targeted_patch_helpers_handle_filters_renames_and_path_scope() {
            let fixture = tempdir().expect("temp dir");
            let root = fixture.path();
            let nested = root.join("nested");
            std::fs::create_dir(&nested).expect("nested");
            let keep = root.join("keep.txt");
            let drop = root.join("drop.txt");
            let old = root.join("old.txt");
            let new = root.join("new.txt");
            std::fs::write(&keep, b"keep").expect("keep");
            std::fs::write(&drop, b"drop").expect("drop");
            std::fs::write(&old, b"old").expect("old");
            std::fs::write(&new, b"new").expect("new");

            let mut changed_paths = HashSet::new();
            changed_paths.insert(keep.clone());
            changed_paths.insert(nested.join("ignored.txt"));
            assert!(matches_watched_parent(
                &changed_paths,
                &root.to_string_lossy()
            ));
            assert!(is_direct_child(&keep, root));
            assert!(!is_direct_child(&nested.join("ignored.txt"), root));

            let target = WatchTarget {
                tab_id: "left-1".to_string(),
                path: root.to_string_lossy().into_owned(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: "keep".to_string(),
                show_hidden: true,
            };
            let previous = HashMap::from([
                (
                    fs::display_path_from_path(&drop),
                    fs::directory_entry_from_path(&drop).expect("drop entry"),
                ),
                (
                    fs::display_path_from_path(&old),
                    fs::directory_entry_from_path(&old).expect("old entry"),
                ),
            ]);
            let filtered_out = DebouncedEvent::new(
                Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
                    .add_path(drop.clone()),
                Instant::now(),
            );
            let rename = DebouncedEvent::new(
                Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                    .add_path(old)
                    .add_path(new),
                Instant::now(),
            );
            let PatchResult::Targeted { patch, snapshot } =
                patch_for_events(&target, &previous, &[filtered_out, rename])
                    .expect("targeted patch")
            else {
                panic!("events should be targeted");
            };
            assert!(patch.removed.iter().any(|path| path.ends_with("drop.txt")));
            assert!(patch.removed.iter().any(|path| path.ends_with("old.txt")));
            assert!(!snapshot.keys().any(|path| path.ends_with("drop.txt")));
            assert!(!snapshot.keys().any(|path| path.ends_with("new.txt")));

            let hidden = DirectoryEntry {
                is_hidden: true,
                ..entry("C:/hidden.txt", "hidden.txt")
            };
            assert!(!matches_target_filter(
                &hidden,
                &WatchTarget {
                    show_hidden: false,
                    ..target
                }
            ));
        }

        #[test]
        fn ambiguous_watch_events_request_full_resnapshot() {
            let fixture = tempdir().expect("temp dir");
            let target = WatchTarget {
                tab_id: "left-1".to_string(),
                path: fixture.path().to_string_lossy().into_owned(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: String::new(),
                show_hidden: true,
            };
            let one_sided_rename = DebouncedEvent::new(
                Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::From)))
                    .add_path(fixture.path().join("old.txt")),
                Instant::now(),
            );
            assert!(matches!(
                patch_for_events(&target, &HashMap::new(), &[one_sided_rename])
                    .expect("rename decision"),
                PatchResult::NeedsResnapshot
            ));

            let rescan = DebouncedEvent::new(
                Event::new(EventKind::Create(CreateKind::Any))
                    .add_path(fixture.path().join("new.txt"))
                    .set_flag(Flag::Rescan),
                Instant::now(),
            );
            assert!(matches!(
                patch_for_events(&target, &HashMap::new(), &[rescan]).expect("rescan decision"),
                PatchResult::NeedsResnapshot
            ));
        }

        #[test]
        fn watch_error_path_prefers_first_notify_path() {
            let with_path = notify::Error::generic("boom").add_path(PathBuf::from("C:/broken.txt"));
            assert!(first_error_path(&with_path).ends_with("broken.txt"));

            let without_path = notify::Error::generic("boom");
            assert_eq!(first_error_path(&without_path), "");
        }

        #[test]
        fn included_watch_service_methods_are_exercised() {
            let fixture = tempdir().expect("temp dir");
            let target = WatchTarget {
                tab_id: "left-1".to_string(),
                path: fixture.path().to_string_lossy().into_owned(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: String::new(),
                show_hidden: true,
            };
            let service = WatchService::default();
            let patch = service
                .refresh_tab(target.clone(), Arc::new(|_| {}))
                .expect("refresh");
            assert!(patch.changed.is_empty());

            service
                .set_tab_watch(Some(target), Arc::new(|_| {}), Arc::new(|_, _| {}))
                .expect("set watch");
            service
                .set_tab_watch(None, Arc::new(|_| {}), Arc::new(|_, _| {}))
                .expect("clear watch");
        }

        #[test]
        fn add_and_remove_watch_updates_reference_counts() {
            let fixture = tempdir().expect("temp dir");
            let root = fixture.path();
            let mut runtime =
                create_runtime(Arc::new(|_| {}), Arc::new(|_, _| {})).expect("runtime");

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
