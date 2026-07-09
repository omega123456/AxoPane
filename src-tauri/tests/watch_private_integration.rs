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
    }
}

use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::{SetTabWatchRequest, WatchSeedReference};
use file_explorer_lib::listing::ListingService;
use file_explorer_lib::watch::{tab_snapshot_for_tests, WatchService, WatchTarget};

fn as_state<'a, T: Send + Sync + 'static>(value: &'a T) -> tauri::State<'a, T> {
    unsafe { std::mem::transmute::<&'a T, tauri::State<'a, T>>(value) }
}

#[test]
fn stale_seed_reference_falls_back_to_current_directory_snapshot() {
    let fixture = tempfile::tempdir().expect("temp dir");
    let root = fixture.path();
    std::fs::write(root.join("real.txt"), b"real").expect("real file");

    let listing_service = ListingService::default();
    let session = listing_service.begin_session("left-1");
    assert!(listing_service.complete_session(
        &session,
        root.to_string_lossy().into_owned(),
        file_explorer_lib::fs::SortKey::Name,
        file_explorer_lib::fs::SortDirection::Asc,
        String::new(),
        true,
        true,
        vec![file_explorer_lib::fs::DirectoryEntry {
            id: root.join("phantom.txt").to_string_lossy().into_owned(),
            name: "phantom.txt".to_string(),
            path: root.join("phantom.txt").to_string_lossy().into_owned(),
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
        }],
    ));

    let watch_service = WatchService::default();
    commands::set_tab_watch(
        SetTabWatchRequest {
            target: Some(WatchTarget {
                tab_id: "left-1".to_string(),
                path: root.to_string_lossy().into_owned(),
                sort_key: file_explorer_lib::fs::SortKey::Name,
                sort_direction: file_explorer_lib::fs::SortDirection::Asc,
                filter: String::new(),
                show_hidden: true,
                include_item_counts: true,
            }),
            seed_reference: Some(WatchSeedReference {
                tab_id: "left-1".to_string(),
                request_id: session.request_id + 1,
                path: root.to_string_lossy().into_owned(),
            }),
            entries: None,
        },
        as_state(&listing_service),
        as_state(&watch_service),
    )
    .expect("set watch with stale seed reference");

    let baseline = tab_snapshot_for_tests(&watch_service, "left-1").expect("baseline recorded");
    assert!(baseline.keys().any(|path| path.ends_with("real.txt")));
    assert!(!baseline.keys().any(|path| path.ends_with("phantom.txt")));
}
