mod fs {
    pub use file_explorer_lib::fs::*;
}

mod directory_session {
    pub mod model {
        pub use file_explorer_lib::directory_session::model::*;
    }
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
        use std::collections::HashSet;
        use std::path::PathBuf;
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
                (removed_path.clone(), entry(&removed_path, "removed.txt")),
                (
                    unchanged_path.clone(),
                    entry(&unchanged_path, "unchanged.txt"),
                ),
            ]);
            let next = HashMap::from([
                (added_path.clone(), entry(&added_path, "added.txt")),
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
            assert!(patch.changed.iter().any(|item| item.path == changed_path
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

            let created = Event::new(EventKind::Create(CreateKind::File)).add_path(alpha.clone());
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
            let removed = Event::new(EventKind::Remove(RemoveKind::File)).add_path(alpha);
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
            let filtered_out = Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
                .add_path(drop.clone());
            let rename = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                .add_path(old)
                .add_path(new);
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
            let one_sided_rename =
                Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::From)))
                    .add_path(fixture.path().join("old.txt"));
            assert!(matches!(
                patch_for_events(&target, &HashMap::new(), &[one_sided_rename])
                    .expect("rename decision"),
                PatchResult::NeedsResnapshot
            ));

            let rescan = Event::new(EventKind::Create(CreateKind::Any))
                .add_path(fixture.path().join("new.txt"))
                .set_flag(Flag::Rescan);
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
                    None,
                    Arc::new(|_| {}),
                    Arc::new(|_, _| {}),
                )
                .expect("set watch");
            service
                .set_tab_watch(None, None, None, Arc::new(|_| {}), Arc::new(|_, _| {}))
                .expect("clear watch");
        }

        #[test]
        fn included_watch_service_reconcile_emits_refresh_patches() {
            let fixture = tempdir().expect("temp dir");
            let root = fixture.path();
            let old_path = root.join("old.txt");
            std::fs::write(&old_path, b"old").expect("old");

            let target = WatchTarget {
                tab_id: "left-1".to_string(),
                path: root.to_string_lossy().into_owned(),
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
                    None,
                    Arc::new(|_| {}),
                    Arc::new(|_, _| {}),
                )
                .expect("set watch");

            let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
            let errors = Arc::new(Mutex::new(Vec::<(String, String)>::new()));

            std::fs::remove_file(&old_path).expect("remove old");
            let fresh_path = root.join("fresh");
            std::fs::create_dir(&fresh_path).expect("fresh dir");

            service.reconcile(
                Arc::new({
                    let patches = Arc::clone(&patches);
                    move |patch| patches.lock().expect("patches lock").push(patch)
                }),
                Arc::new({
                    let errors = Arc::clone(&errors);
                    move |path, error| errors.lock().expect("errors lock").push((path, error))
                }),
            );

            assert!(errors.lock().expect("errors lock").is_empty());

            let patches = patches.lock().expect("patches lock");
            let patch = patches
                .iter()
                .find(|patch| patch.path == target.path)
                .expect("refresh patch for watched path");
            assert_eq!(patch.reason, "refresh");
            assert!(patch.removed.iter().any(|path| path.ends_with("old.txt")));
            assert!(patch.changed.iter().any(|change| {
                change.path.ends_with("fresh")
                    && change.entry.as_ref().is_some_and(|entry| entry.is_dir)
            }));
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
                    None,
                    Arc::new(|_| {}),
                    Arc::new(|_, _| {}),
                )
                .expect("set first watch");
            service
                .set_tab_watch(
                    Some(base.clone()),
                    None,
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

        #[test]
        fn patch_changed_path_builds_directory_entries_without_item_counts() {
            let fixture = tempdir().expect("temp dir");
            let root = fixture.path();
            let child = root.join("child");
            std::fs::create_dir(&child).expect("child dir");
            std::fs::write(child.join("nested.txt"), b"nested").expect("nested");

            let target = WatchTarget {
                tab_id: "left-1".to_string(),
                path: root.to_string_lossy().into_owned(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: String::new(),
                show_hidden: true,
                include_item_counts: true,
            };

            let mut next = HashMap::new();
            let mut changed = Vec::new();
            let mut removed = Vec::new();
            patch_changed_path(&target, &mut next, &mut changed, &mut removed, &child)
                .expect("directory patch");

            let changed_entry = changed
                .into_iter()
                .find_map(|patch| patch.entry)
                .expect("changed entry");
            assert!(changed_entry.is_dir);
            assert_eq!(changed_entry.item_count, None);
        }

        #[test]
        fn items_sort_watch_baselines_stay_non_counting_and_order_agnostic() {
            let fixture = tempdir().expect("temp dir");
            let root = fixture.path();
            let child = root.join("child");
            std::fs::create_dir(&child).expect("child dir");
            std::fs::write(child.join("nested.txt"), b"nested").expect("nested");

            let target = WatchTarget {
                tab_id: "left-1".to_string(),
                path: root.to_string_lossy().into_owned(),
                sort_key: SortKey::Items,
                sort_direction: SortDirection::Desc,
                filter: String::new(),
                show_hidden: true,
                include_item_counts: true,
            };
            let service = WatchService::default();

            service
                .set_tab_watch(
                    Some(target.clone()),
                    None,
                    None,
                    Arc::new(|_| {}),
                    Arc::new(|_, _| {}),
                )
                .expect("set items watch");

            let baseline = tab_snapshot_for_tests(&service, &target.tab_id).expect("baseline");
            let child_entry = baseline
                .values()
                .find(|entry| entry.name == "child")
                .expect("child entry");
            assert_eq!(child_entry.item_count, None);
        }

        #[test]
        fn raw_mutations_for_event_covers_access_remove_unresolved_and_changed_kinds() {
            let watched = WatchedTab {
                watch_id: WatchId(7),
                target: WatchTarget {
                    tab_id: "left-1".to_string(),
                    path: "/watched".to_string(),
                    sort_key: SortKey::Name,
                    sort_direction: SortDirection::Asc,
                    filter: String::new(),
                    show_hidden: true,
                    include_item_counts: false,
                },
                snapshot: HashMap::new(),
            };

            // Access events never produce a mutation.
            let access = Event::new(EventKind::Access(notify::event::AccessKind::Read));
            assert!(raw_mutations_for_event(&access, &watched).is_empty());

            // A remove event for a direct child becomes a `Removed` mutation.
            let removed_child = PathBuf::from("/watched/gone.txt");
            let remove_event =
                Event::new(EventKind::Remove(RemoveKind::File)).add_path(removed_child.clone());
            let removed = raw_mutations_for_event(&remove_event, &watched);
            assert_eq!(removed.len(), 1);
            assert_eq!(removed[0].kind, MutationKind::Removed);
            assert_eq!(removed[0].child_path, removed_child);

            // An `Any`/`Other`-shaped event cannot be resolved into a definite
            // child identity: exactly one `Unresolved` mutation with an empty
            // child path, regardless of how many paths the event carried.
            let unresolved_event = Event::new(EventKind::Any)
                .add_path(PathBuf::from("/watched/a.txt"))
                .add_path(PathBuf::from("/watched/b.txt"));
            let unresolved = raw_mutations_for_event(&unresolved_event, &watched);
            assert_eq!(unresolved.len(), 1);
            assert_eq!(unresolved[0].kind, MutationKind::Unresolved);
            assert_eq!(unresolved[0].child_path, PathBuf::new());

            // A rename-both event that resolves both paths but is not a
            // direct child of the watched target contributes nothing.
            let outside_child = PathBuf::from("/elsewhere/child.txt");
            let outside_rename = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                .add_path(outside_child.clone())
                .add_path(PathBuf::from("/elsewhere/child2.txt"));
            assert!(raw_mutations_for_event(&outside_rename, &watched).is_empty());

            // A direct-child data-change event resolves to one `Changed`
            // mutation carrying the child path.
            let changed_child = PathBuf::from("/watched/changed.txt");
            let changed_event =
                Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
                    .add_path(changed_child.clone());
            let changed = raw_mutations_for_event(&changed_event, &watched);
            assert_eq!(changed.len(), 1);
            assert_eq!(changed[0].kind, MutationKind::Changed);
            assert_eq!(changed[0].child_path, changed_child);
        }

        #[test]
        fn raw_mutations_for_event_flags_need_rescan_events_as_unresolved() {
            let watched = WatchedTab {
                watch_id: WatchId(9),
                target: WatchTarget {
                    tab_id: "left-1".to_string(),
                    path: "/watched".to_string(),
                    sort_key: SortKey::Name,
                    sort_direction: SortDirection::Asc,
                    filter: String::new(),
                    show_hidden: true,
                    include_item_counts: false,
                },
                snapshot: HashMap::new(),
            };
            let mut rescan_event =
                Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
                    .add_path(PathBuf::from("/watched/x.txt"));
            rescan_event = rescan_event.set_flag(Flag::Rescan);

            let mutations = raw_mutations_for_event(&rescan_event, &watched);
            assert_eq!(mutations.len(), 1);
            assert_eq!(mutations[0].kind, MutationKind::Unresolved);
        }

        #[test]
        fn process_compacted_batch_ignores_batches_for_unknown_watch_ids() {
            let tabs = Arc::new(Mutex::new(HashMap::<String, WatchedTab>::new()));
            let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
            let errors = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
            let patches_for_emit = Arc::clone(&patches);
            let errors_for_emit = Arc::clone(&errors);
            let patch_emitter: Arc<dyn Fn(DirPatch) + Send + Sync> =
                Arc::new(move |patch| patches_for_emit.lock().unwrap().push(patch));
            let error_emitter: Arc<dyn Fn(String, String) + Send + Sync> =
                Arc::new(move |path, error| errors_for_emit.lock().unwrap().push((path, error)));

            // No tab is registered for this watch id at all: the batch is
            // silently dropped (a stale/superseded watch generation).
            process_compacted_batch(
                &tabs,
                CompactedBatch::Targeted {
                    watch_id: WatchId(42),
                    changed: Vec::new(),
                    removed: Vec::new(),
                },
                &patch_emitter,
                &error_emitter,
            );
            assert!(patches.lock().unwrap().is_empty());
            assert!(errors.lock().unwrap().is_empty());
        }

        #[test]
        fn process_compacted_batch_applies_targeted_changes_and_emits_a_patch() {
            let fixture = tempdir().expect("temp dir");
            let root = fixture.path();
            let new_file = root.join("new.txt");
            std::fs::write(&new_file, b"new").expect("write new file");

            let target = WatchTarget {
                tab_id: "left-1".to_string(),
                path: root.to_string_lossy().into_owned(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: String::new(),
                show_hidden: true,
                include_item_counts: false,
            };
            let watch_id = WatchId(1);
            let tabs = Arc::new(Mutex::new(HashMap::from([(
                target.tab_id.clone(),
                WatchedTab {
                    watch_id,
                    target: target.clone(),
                    snapshot: HashMap::new(),
                },
            )])));

            let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
            let patches_for_emit = Arc::clone(&patches);
            let patch_emitter: Arc<dyn Fn(DirPatch) + Send + Sync> =
                Arc::new(move |patch| patches_for_emit.lock().unwrap().push(patch));
            let error_emitter: Arc<dyn Fn(String, String) + Send + Sync> = Arc::new(|_, _| {});

            process_compacted_batch(
                &tabs,
                CompactedBatch::Targeted {
                    watch_id,
                    changed: vec![new_file.clone()],
                    removed: Vec::new(),
                },
                &patch_emitter,
                &error_emitter,
            );

            let emitted = patches.lock().unwrap();
            assert_eq!(emitted.len(), 1);
            assert!(emitted[0]
                .changed
                .iter()
                .any(|item| item.path.ends_with("new.txt")));
            let guard = tabs.lock().unwrap();
            let stored_snapshot = &guard.get(&target.tab_id).unwrap().snapshot;
            assert!(stored_snapshot.keys().any(|path| path.ends_with("new.txt")));
        }

        #[test]
        fn process_compacted_batch_dirty_resnapshots_from_disk() {
            let fixture = tempdir().expect("temp dir");
            let root = fixture.path();
            std::fs::write(root.join("existing.txt"), b"x").expect("existing file");

            let target = WatchTarget {
                tab_id: "left-1".to_string(),
                path: root.to_string_lossy().into_owned(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: String::new(),
                show_hidden: true,
                include_item_counts: false,
            };
            let watch_id = WatchId(2);
            let tabs = Arc::new(Mutex::new(HashMap::from([(
                target.tab_id.clone(),
                WatchedTab {
                    watch_id,
                    target: target.clone(),
                    snapshot: HashMap::new(),
                },
            )])));

            let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
            let patches_for_emit = Arc::clone(&patches);
            let patch_emitter: Arc<dyn Fn(DirPatch) + Send + Sync> =
                Arc::new(move |patch| patches_for_emit.lock().unwrap().push(patch));
            let error_emitter: Arc<dyn Fn(String, String) + Send + Sync> = Arc::new(|_, _| {});

            process_compacted_batch(
                &tabs,
                CompactedBatch::Dirty {
                    watch_id,
                    generation: 1,
                },
                &patch_emitter,
                &error_emitter,
            );

            let guard = tabs.lock().unwrap();
            let stored_snapshot = &guard.get(&target.tab_id).unwrap().snapshot;
            assert!(stored_snapshot
                .keys()
                .any(|path| path.ends_with("existing.txt")));
        }

        #[test]
        fn process_compacted_batch_treats_a_missing_changed_path_as_a_removal() {
            let fixture = tempdir().expect("temp dir");
            let root = fixture.path();
            let target = WatchTarget {
                tab_id: "left-1".to_string(),
                path: root.to_string_lossy().into_owned(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: String::new(),
                show_hidden: true,
                include_item_counts: false,
            };
            let watch_id = WatchId(3);
            let missing = root.join("never-existed.txt");
            let tabs = Arc::new(Mutex::new(HashMap::from([(
                target.tab_id.clone(),
                WatchedTab {
                    watch_id,
                    target: target.clone(),
                    snapshot: HashMap::from([(
                        fs::display_path_from_path(&missing),
                        fs::DirectoryEntry {
                            id: missing.to_string_lossy().into_owned(),
                            name: "never-existed.txt".to_string(),
                            path: fs::display_path_from_path(&missing),
                            is_dir: false,
                            icon_data_url: None,
                            size_bytes: Some(0),
                            item_count: None,
                            type_label: "TXT file".to_string(),
                            modified_at: None,
                            created_at: None,
                            attributes: Vec::new(),
                            is_hidden: false,
                            is_system: false,
                        },
                    )]),
                },
            )])));

            let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
            let patches_for_emit = Arc::clone(&patches);
            let patch_emitter: Arc<dyn Fn(DirPatch) + Send + Sync> =
                Arc::new(move |patch| patches_for_emit.lock().unwrap().push(patch));
            let error_emitter: Arc<dyn Fn(String, String) + Send + Sync> = Arc::new(|_, _| {});

            process_compacted_batch(
                &tabs,
                CompactedBatch::Targeted {
                    watch_id,
                    changed: vec![missing.clone()],
                    removed: Vec::new(),
                },
                &patch_emitter,
                &error_emitter,
            );

            let emitted = patches.lock().unwrap();
            assert_eq!(emitted.len(), 1);
            assert!(emitted[0]
                .removed
                .iter()
                .any(|path| path.ends_with("never-existed.txt")));
            let guard = tabs.lock().unwrap();
            let stored_snapshot = &guard.get(&target.tab_id).unwrap().snapshot;
            assert!(stored_snapshot.is_empty());
        }
    }
}
