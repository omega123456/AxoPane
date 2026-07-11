use file_explorer_lib::fs::{SortDirection, SortKey};
use file_explorer_lib::watch::coordinator::{CompactedBatch, MutationKind, WatchId};
use file_explorer_lib::watch::{
    add_watch, canonical_dir_for_tests, create_runtime, first_error_path_for_tests,
    handle_debounce_result_for_tests, insert_tab_for_tests, noop_watch_error_for_tests,
    process_compacted_batch_for_tests, raw_mutations_for_event_for_tests, remove_watch,
    snapshot_for_target, DirPatch, WatchTarget,
};
use notify::event::{
    AccessKind, CreateKind, DataChange, EventKind, ModifyKind, RemoveKind, RenameMode,
};
use notify::Event;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

#[test]
fn watch_runtime_tracks_reference_counts_for_paths() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let mut runtime = create_runtime(Arc::new(|_| {}), Arc::new(|_, _| {})).expect("runtime");

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
fn watch_runtime_test_hooks_drive_targeted_and_rescan_callbacks() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let before = root.join("before.txt");
    std::fs::write(&before, b"before").expect("before");

    let mut runtime = create_runtime(Arc::new(|_| {}), Arc::new(|_, _| {})).expect("runtime");
    let target = WatchTarget {
        tab_id: "left-1".to_string(),
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
        include_item_counts: true,
    };
    insert_tab_for_tests(
        &mut runtime,
        target.clone(),
        snapshot_for_target(&target).expect("initial snapshot"),
    );

    let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
    let errors = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let patches_for_callback = patches.clone();
    let errors_for_callback = errors.clone();

    let created = root.join("created.txt");
    std::fs::write(&created, b"created").expect("created");
    handle_debounce_result_for_tests(
        &runtime,
        vec![Ok(
            Event::new(EventKind::Create(CreateKind::File)).add_path(created.clone())
        )],
        Arc::new(move |patch| {
            patches_for_callback
                .lock()
                .expect("patches lock")
                .push(patch);
        }),
        Arc::new(move |path, error| {
            errors_for_callback
                .lock()
                .expect("errors lock")
                .push((path, error));
        }),
    );
    assert!(patches
        .lock()
        .expect("patches lock")
        .iter()
        .any(|patch| patch
            .changed
            .iter()
            .any(|entry| entry.path.ends_with("created.txt"))));
    assert!(errors.lock().expect("errors lock").is_empty());

    std::fs::remove_file(&before).expect("remove before");
    let after = root.join("after.txt");
    std::fs::write(&after, b"after").expect("after");
    let patches_for_rescan = patches.clone();
    let errors_for_rescan = errors.clone();
    handle_debounce_result_for_tests(
        &runtime,
        vec![Ok(Event::new(EventKind::Any).add_path(before.clone()))],
        Arc::new(move |patch| {
            patches_for_rescan.lock().expect("patches lock").push(patch);
        }),
        Arc::new(move |path, error| {
            errors_for_rescan
                .lock()
                .expect("errors lock")
                .push((path, error));
        }),
    );
    let patches = patches.lock().expect("patches lock");
    assert!(patches.iter().any(|patch| patch
        .removed
        .iter()
        .any(|path| path.ends_with("before.txt"))));
    assert!(patches.iter().any(|patch| patch
        .changed
        .iter()
        .any(|entry| entry.path.ends_with("after.txt"))));
}

#[test]
fn watch_runtime_test_hook_surfaces_errors_and_removals() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let doomed = root.join("doomed.txt");
    std::fs::write(&doomed, b"gone").expect("doomed");

    let mut runtime = create_runtime(Arc::new(|_| {}), Arc::new(|_, _| {})).expect("runtime");
    let target = WatchTarget {
        tab_id: "right-1".to_string(),
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
        include_item_counts: true,
    };
    insert_tab_for_tests(
        &mut runtime,
        target.clone(),
        snapshot_for_target(&target).expect("snapshot"),
    );

    let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
    let errors = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    std::fs::remove_file(&doomed).expect("remove doomed");
    handle_debounce_result_for_tests(
        &runtime,
        vec![Ok(
            Event::new(EventKind::Remove(RemoveKind::File)).add_path(doomed.clone())
        )],
        Arc::new({
            let patches = patches.clone();
            move |patch| {
                patches.lock().expect("patches lock").push(patch);
            }
        }),
        Arc::new({
            let errors = errors.clone();
            move |path, error| {
                errors.lock().expect("errors lock").push((path, error));
            }
        }),
    );
    assert!(patches
        .lock()
        .expect("patches lock")
        .iter()
        .any(|patch| patch
            .removed
            .iter()
            .any(|path| path.ends_with("doomed.txt"))));

    handle_debounce_result_for_tests(
        &runtime,
        vec![Err(
            notify::Error::generic("boom").add_path(root.join("broken.txt"))
        )],
        Arc::new(|_| {}),
        Arc::new({
            let errors = errors.clone();
            move |path, error| {
                errors.lock().expect("errors lock").push((path, error));
            }
        }),
    );
    assert!(errors
        .lock()
        .expect("errors lock")
        .iter()
        .any(|(path, error)| path.ends_with("broken.txt") && error.contains("boom")));
}

#[test]
fn watch_runtime_test_hook_covers_modify_and_rename_filters() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let drop = root.join("drop.txt");
    let old = root.join("old.txt");
    let new = root.join("new.txt");
    std::fs::write(&drop, b"drop").expect("drop");
    std::fs::write(&old, b"old").expect("old");
    std::fs::write(&new, b"new").expect("new");

    let mut runtime = create_runtime(Arc::new(|_| {}), Arc::new(|_, _| {})).expect("runtime");
    let target = WatchTarget {
        tab_id: "left-2".to_string(),
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: "keep".to_string(),
        show_hidden: true,
        include_item_counts: true,
    };
    insert_tab_for_tests(
        &mut runtime,
        target.clone(),
        snapshot_for_target(&WatchTarget {
            filter: String::new(),
            ..target.clone()
        })
        .expect("snapshot"),
    );

    let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
    handle_debounce_result_for_tests(
        &runtime,
        vec![
            Ok(
                Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
                    .add_path(drop.clone()),
            ),
            Ok(
                Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                    .add_path(old.clone())
                    .add_path(new.clone()),
            ),
        ],
        Arc::new({
            let patches = patches.clone();
            move |patch| {
                patches.lock().expect("patches lock").push(patch);
            }
        }),
        Arc::new(|_, _| {}),
    );

    let patches = patches.lock().expect("patches lock");
    assert!(patches
        .iter()
        .any(|patch| patch.removed.iter().any(|path| path.ends_with("drop.txt"))));
    assert!(patches
        .iter()
        .any(|patch| patch.removed.iter().any(|path| path.ends_with("old.txt"))));
    assert!(!patches.iter().any(|patch| patch
        .changed
        .iter()
        .any(|entry| entry.path.ends_with("new.txt"))));
}

#[test]
fn watch_runtime_test_hooks_cover_filtered_nonmatching_and_error_branches() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let sibling = fixture.path().join("sibling");
    std::fs::create_dir(&sibling).expect("sibling");
    let visible = root.join("visible.txt");
    let hidden = root.join(".hidden.txt");
    std::fs::write(&visible, b"visible").expect("visible");
    std::fs::write(&hidden, b"hidden").expect("hidden");

    let mut runtime = create_runtime(Arc::new(|_| {}), Arc::new(|_, _| {})).expect("runtime");
    let target = WatchTarget {
        tab_id: "left-branches".to_string(),
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: false,
        include_item_counts: true,
    };
    insert_tab_for_tests(
        &mut runtime,
        target.clone(),
        snapshot_for_target(&WatchTarget {
            show_hidden: true,
            ..target.clone()
        })
        .expect("snapshot"),
    );

    let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
    let errors = Arc::new(Mutex::new(Vec::<(String, String)>::new()));

    handle_debounce_result_for_tests(
        &runtime,
        vec![Ok(
            Event::new(EventKind::Create(CreateKind::File)).add_path(sibling.join("ignore.txt"))
        )],
        Arc::new({
            let patches = patches.clone();
            move |patch| patches.lock().expect("patches lock").push(patch)
        }),
        Arc::new({
            let errors = errors.clone();
            move |path, error| errors.lock().expect("errors lock").push((path, error))
        }),
    );
    assert!(patches.lock().expect("patches lock").is_empty());
    assert!(errors.lock().expect("errors lock").is_empty());

    handle_debounce_result_for_tests(
        &runtime,
        vec![
            Ok(
                Event::new(EventKind::Access(notify::event::AccessKind::Close(
                    notify::event::AccessMode::Write,
                )))
                .add_path(visible.clone()),
            ),
            Ok(Event::new(EventKind::Modify(ModifyKind::Any)).add_path(visible.clone())),
        ],
        Arc::new({
            let patches = patches.clone();
            move |patch| patches.lock().expect("patches lock").push(patch)
        }),
        Arc::new({
            let errors = errors.clone();
            move |path, error| errors.lock().expect("errors lock").push((path, error))
        }),
    );
    assert!(patches
        .lock()
        .expect("patches lock")
        .iter()
        .any(|patch| patch
            .removed
            .iter()
            .any(|path| path.ends_with(".hidden.txt"))));

    std::fs::remove_dir_all(root).expect("remove watched dir");
    handle_debounce_result_for_tests(
        &runtime,
        vec![Ok(
            Event::new(EventKind::Any).add_path(Path::new(&target.path).join("gone.txt"))
        )],
        Arc::new(|_| {}),
        Arc::new({
            let errors = errors.clone();
            move |path, error| errors.lock().expect("errors lock").push((path, error))
        }),
    );
    assert!(errors
        .lock()
        .expect("errors lock")
        .iter()
        .any(|(path, error)| path == &target.path && !error.is_empty()));

    handle_debounce_result_for_tests(
        &runtime,
        vec![Err(notify::Error::generic("plain boom"))],
        Arc::new(|_| {}),
        Arc::new({
            let errors = errors.clone();
            move |path, error| errors.lock().expect("errors lock").push((path, error))
        }),
    );
    assert!(errors
        .lock()
        .expect("errors lock")
        .iter()
        .any(|(path, error)| path.is_empty() && error.contains("plain boom")));

    noop_watch_error_for_tests(String::new(), String::new());
    let plain_error = notify::Error::generic("plain boom");
    assert!(first_error_path_for_tests(&plain_error).is_empty());
    let pathed_error = notify::Error::generic("path boom").add_path(visible.clone());
    assert_eq!(
        first_error_path_for_tests(&pathed_error),
        visible.to_string_lossy()
    );
    assert_eq!(
        canonical_dir_for_tests(&root.join("missing-dir")),
        root.join("missing-dir")
    );
    assert!(snapshot_for_target(&WatchTarget {
        tab_id: "missing".to_string(),
        path: root.join("missing-again").to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
        include_item_counts: true,
    })
    .is_err());
}

fn watch_target(root: &Path) -> WatchTarget {
    WatchTarget {
        tab_id: "left-1".to_string(),
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
        include_item_counts: false,
    }
}

#[test]
fn raw_mutations_for_event_classifies_every_kind_through_the_public_test_hook() {
    let target = watch_target(Path::new("/watched"));
    let watch_id = WatchId(123);

    let access = Event::new(EventKind::Access(AccessKind::Read));
    assert!(raw_mutations_for_event_for_tests(&access, watch_id, target.clone()).is_empty());

    let removed_child = PathBuf::from("/watched/gone.txt");
    let remove_event =
        Event::new(EventKind::Remove(RemoveKind::File)).add_path(removed_child.clone());
    let removed = raw_mutations_for_event_for_tests(&remove_event, watch_id, target.clone());
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].kind, MutationKind::Removed);
    assert_eq!(removed[0].child_path, removed_child);

    let unresolved_event = Event::new(EventKind::Any).add_path(PathBuf::from("/watched/a.txt"));
    let unresolved = raw_mutations_for_event_for_tests(&unresolved_event, watch_id, target.clone());
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].kind, MutationKind::Unresolved);
    assert_eq!(unresolved[0].child_path, PathBuf::new());

    let changed_child = PathBuf::from("/watched/changed.txt");
    let changed_event = Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
        .add_path(changed_child.clone());
    let changed = raw_mutations_for_event_for_tests(&changed_event, watch_id, target);
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].kind, MutationKind::Changed);
    assert_eq!(changed[0].child_path, changed_child);
}

#[test]
fn watched_directory_events_force_an_authoritative_resnapshot() {
    let fixture = tempdir().expect("temp dir");
    let target = watch_target(fixture.path());
    let root_event = Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
        .add_path(fixture.path().to_path_buf());

    let mutations = raw_mutations_for_event_for_tests(&root_event, WatchId(11), target);

    assert_eq!(mutations.len(), 1);
    assert_eq!(mutations[0].kind, MutationKind::Unresolved);
    assert!(mutations[0].child_path.as_os_str().is_empty());
}

#[test]
fn process_compacted_batch_ignores_unknown_watch_ids_through_the_public_test_hook() {
    let mut runtime = create_runtime(Arc::new(|_| {}), Arc::new(|_, _| {})).expect("runtime");
    let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
    let errors = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let patches_for_emit = patches.clone();
    let errors_for_emit = errors.clone();

    let fixture = tempdir().expect("temp dir");
    let target = watch_target(fixture.path());
    insert_tab_for_tests(&mut runtime, target, Default::default());

    process_compacted_batch_for_tests(
        &runtime,
        CompactedBatch::Targeted {
            watch_id: WatchId(999_999),
            changed: Vec::new(),
            removed: Vec::new(),
        },
        Arc::new(move |patch| patches_for_emit.lock().expect("patches lock").push(patch)),
        Arc::new(move |path, error| {
            errors_for_emit
                .lock()
                .expect("errors lock")
                .push((path, error))
        }),
    );

    assert!(patches.lock().expect("patches lock").is_empty());
    assert!(errors.lock().expect("errors lock").is_empty());
}

#[test]
fn process_compacted_batch_applies_targeted_and_dirty_batches_through_the_public_test_hook() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let new_file = root.join("new.txt");
    std::fs::write(&new_file, b"new").expect("write new file");

    let mut runtime = create_runtime(Arc::new(|_| {}), Arc::new(|_, _| {})).expect("runtime");
    let target = watch_target(root);
    let watch_id = insert_tab_for_tests(&mut runtime, target, Default::default());

    let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
    let patches_for_emit = patches.clone();
    process_compacted_batch_for_tests(
        &runtime,
        CompactedBatch::Targeted {
            watch_id,
            changed: vec![new_file.clone()],
            removed: Vec::new(),
        },
        Arc::new(move |patch| patches_for_emit.lock().expect("patches lock").push(patch)),
        Arc::new(|_, _| {}),
    );
    assert!(patches
        .lock()
        .expect("patches lock")
        .iter()
        .any(|patch| patch
            .changed
            .iter()
            .any(|item| item.path.ends_with("new.txt"))));

    // A dirty batch forces an authoritative resnapshot from disk.
    let dirty_patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
    let dirty_patches_for_emit = dirty_patches.clone();
    process_compacted_batch_for_tests(
        &runtime,
        CompactedBatch::Dirty {
            watch_id,
            generation: 1,
        },
        Arc::new(move |patch| {
            dirty_patches_for_emit
                .lock()
                .expect("dirty patches lock")
                .push(patch)
        }),
        Arc::new(|_, _| {}),
    );
    // The resnapshot reflects the same file already folded in by the
    // targeted batch, so no new changes to report is an acceptable and
    // deterministic outcome — the call completing without panicking is
    // what's covered here.
    assert!(dirty_patches.lock().expect("dirty patches lock").len() <= 1);
}
