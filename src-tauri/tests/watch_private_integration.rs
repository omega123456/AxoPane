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
        fn diff_entries_covers_added_changed_and_removed_branches_directly() {
            // "added": a key present only in `next`.
            let added_path = "C:/added.txt".to_string();
            // "changed": the same key present in both maps but with a different
            // entry value (e.g. size changed on disk).
            let changed_path = "C:/changed.txt".to_string();
            // "removed": a key present only in `previous`.
            let removed_path = "C:/removed.txt".to_string();
            // unchanged: same key, identical value in both maps — must produce
            // neither a `changed` nor a `removed` patch entry.
            let unchanged_path = "C:/unchanged.txt".to_string();

            let previous = HashMap::from([
                (
                    changed_path.clone(),
                    DirectoryEntry {
                        size_bytes: Some(1),
                        ..entry(&changed_path, "changed.txt")
                    },
                ),
                (
                    removed_path.clone(),
                    entry(&removed_path, "removed.txt"),
                ),
                (
                    unchanged_path.clone(),
                    entry(&unchanged_path, "unchanged.txt"),
                ),
            ]);
            let next = HashMap::from([
                (
                    added_path.clone(),
                    entry(&added_path, "added.txt"),
                ),
                (
                    changed_path.clone(),
                    DirectoryEntry {
                        size_bytes: Some(2),
                        ..entry(&changed_path, "changed.txt")
                    },
                ),
                (
                    unchanged_path.clone(),
                    entry(&unchanged_path, "unchanged.txt"),
                ),
            ]);

            let patch = diff_entries("left-3", "C:/", "watch", &previous, &next);

            assert!(patch
                .changed
                .iter()
                .any(|item| item.path == added_path && item.entry.is_some()));
            assert!(patch
                .changed
                .iter()
                .any(|item| item.path == changed_path
                    && item.entry.as_ref().and_then(|e| e.size_bytes) == Some(2)));
            assert!(!patch.changed.iter().any(|item| item.path == unchanged_path));
            assert!(patch.removed.iter().any(|path| path == &removed_path));
            assert!(!patch.removed.contains(&changed_path));
            assert!(!patch.removed.contains(&added_path));
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
                include_item_counts: true,
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
                include_item_counts: true,
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
                include_item_counts: true,
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
                include_item_counts: true,
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
                include_item_counts: true,
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
                include_item_counts: true,
            };
            let service = WatchService::default();

            service
                .set_tab_watch(
                    Some(target.clone()),
                    None,
                    Arc::new(|_| {}),
                    Arc::new(|_, _| {}),
                )
                .expect("set watch");
            service
                .set_tab_watch(None, None, Arc::new(|_| {}), Arc::new(|_, _| {}))
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

        #[test]
        fn watch_service_reuses_runtime_and_replaces_same_pane_paths() {
            let fixture = tempdir().expect("temp dir");
            let left_a = fixture.path().join("left-a");
            let left_b = fixture.path().join("left-b");
            let left_c = fixture.path().join("left-c");
            std::fs::create_dir_all(&left_a).expect("left a");
            std::fs::create_dir_all(&left_b).expect("left b");
            std::fs::create_dir_all(&left_c).expect("left c");

            let service = WatchService::default();
            let base = WatchTarget {
                tab_id: "left-1".to_string(),
                path: left_a.to_string_lossy().into_owned(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: String::new(),
                show_hidden: true,
                include_item_counts: true,
            };

            service
                .set_tab_watch(
                    Some(base.clone()),
                    None,
                    Arc::new(|_| {}),
                    Arc::new(|_, _| {}),
                )
                .expect("set first watch");
            service
                .set_tab_watch(
                    Some(base.clone()),
                    None,
                    Arc::new(|_| {}),
                    Arc::new(|_, _| {}),
                )
                .expect("refresh same watch");

            {
                let guard = service.inner.lock().expect("service lock");
                let runtime = guard.as_ref().expect("runtime");
                assert_eq!(runtime.watch_counts.get(&left_a), Some(&1));
                assert_eq!(runtime.tabs.lock().expect("tabs lock").len(), 1);
            }

            service
                .set_tab_watch(
                    Some(WatchTarget {
                        path: left_b.to_string_lossy().into_owned(),
                        ..base.clone()
                    }),
                    None,
                    Arc::new(|_| {}),
                    Arc::new(|_, _| {}),
                )
                .expect("switch same tab path");

            {
                let guard = service.inner.lock().expect("service lock");
                let runtime = guard.as_ref().expect("runtime");
                assert!(!runtime.watch_counts.contains_key(&left_a));
                assert_eq!(runtime.watch_counts.get(&left_b), Some(&1));
            }

            service
                .set_tab_watch(
                    Some(WatchTarget {
                        tab_id: "left-2".to_string(),
                        path: left_c.to_string_lossy().into_owned(),
                        ..base
                    }),
                    None,
                    Arc::new(|_| {}),
                    Arc::new(|_, _| {}),
                )
                .expect("replace stale pane watch");

            let guard = service.inner.lock().expect("service lock");
            let runtime = guard.as_ref().expect("runtime");
            assert!(!runtime.watch_counts.contains_key(&left_b));
            assert_eq!(runtime.watch_counts.get(&left_c), Some(&1));
            let tabs = runtime.tabs.lock().expect("tabs lock");
            assert_eq!(tabs.len(), 1);
            assert!(tabs.contains_key("left-2"));
        }

        #[test]
        fn patch_helpers_cover_missing_and_filtered_paths() {
            let fixture = tempdir().expect("temp dir");
            let root = fixture.path();
            let visible = root.join("visible.txt");
            let hidden = root.join(".hidden.txt");
            std::fs::write(&visible, b"visible").expect("visible");
            std::fs::write(&hidden, b"hidden").expect("hidden");

            let target = WatchTarget {
                tab_id: "left-1".to_string(),
                path: root.to_string_lossy().into_owned(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: "visible".to_string(),
                show_hidden: false,
                include_item_counts: true,
            };

            let mut next = HashMap::new();
            let mut changed = Vec::new();
            let mut removed = Vec::new();

            patch_changed_path(&target, &mut next, &mut changed, &mut removed, &visible)
                .expect("visible patch");
            assert_eq!(changed.len(), 1);

            patch_changed_path(&target, &mut next, &mut changed, &mut removed, &hidden)
                .expect("hidden filtered");
            assert!(removed.iter().any(|path| path.ends_with(".hidden.txt")));

            std::fs::remove_file(&visible).expect("remove visible");
            patch_changed_path(&target, &mut next, &mut changed, &mut removed, &visible)
                .expect("missing visible");
            assert!(removed.iter().any(|path| path.ends_with("visible.txt")));

            let direct = fs::display_path_from_path(&hidden);
            next.insert(
                direct.clone(),
                fs::directory_entry_from_path(&hidden).expect("entry"),
            );
            remove_path(&mut next, &mut removed, &hidden);
            assert!(!next.contains_key(&direct));
        }
    }
}
