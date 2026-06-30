use file_explorer_lib::fs::{SortDirection, SortKey};
use file_explorer_lib::watch::{
    add_watch, canonical_dir_for_tests, create_runtime, handle_debounce_result_for_tests,
    insert_tab_for_tests, noop_watch_error_for_tests, remove_watch, snapshot_for_target,
    first_error_path_for_tests, DirPatch, WatchTarget,
};
use notify::event::{CreateKind, DataChange, EventKind, ModifyKind, RemoveKind, RenameMode};
use notify::Event;
use notify_debouncer_full::DebouncedEvent;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
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
        Ok(vec![DebouncedEvent::new(
            Event::new(EventKind::Create(CreateKind::File)).add_path(created.clone()),
            Instant::now(),
        )]),
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
        .any(|patch| patch.changed.iter().any(|entry| entry.path.ends_with("created.txt"))));
    assert!(errors.lock().expect("errors lock").is_empty());

    std::fs::remove_file(&before).expect("remove before");
    let after = root.join("after.txt");
    std::fs::write(&after, b"after").expect("after");
    let patches_for_rescan = patches.clone();
    let errors_for_rescan = errors.clone();
    handle_debounce_result_for_tests(
        &runtime,
        Ok(vec![DebouncedEvent::new(
            Event::new(EventKind::Any).add_path(before.clone()),
            Instant::now(),
        )]),
        Arc::new(move |patch| {
            patches_for_rescan
                .lock()
                .expect("patches lock")
                .push(patch);
        }),
        Arc::new(move |path, error| {
            errors_for_rescan
                .lock()
                .expect("errors lock")
                .push((path, error));
        }),
    );
    let patches = patches.lock().expect("patches lock");
    assert!(patches
        .iter()
        .any(|patch| patch.removed.iter().any(|path| path.ends_with("before.txt"))));
    assert!(patches
        .iter()
        .any(|patch| patch.changed.iter().any(|entry| entry.path.ends_with("after.txt"))));
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
        Ok(vec![DebouncedEvent::new(
            Event::new(EventKind::Remove(RemoveKind::File)).add_path(doomed.clone()),
            Instant::now(),
        )]),
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
        .any(|patch| patch.removed.iter().any(|path| path.ends_with("doomed.txt"))));

    handle_debounce_result_for_tests(
        &runtime,
        Err(vec![notify::Error::generic("boom").add_path(root.join("broken.txt"))]),
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
        Ok(vec![
            DebouncedEvent::new(
                Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
                    .add_path(drop.clone()),
                Instant::now(),
            ),
            DebouncedEvent::new(
                Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                    .add_path(old.clone())
                    .add_path(new.clone()),
                Instant::now(),
            ),
        ]),
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
    assert!(!patches
        .iter()
        .any(|patch| patch.changed.iter().any(|entry| entry.path.ends_with("new.txt"))));
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
        Ok(vec![DebouncedEvent::new(
            Event::new(EventKind::Create(CreateKind::File)).add_path(sibling.join("ignore.txt")),
            Instant::now(),
        )]),
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
        Ok(vec![
            DebouncedEvent::new(
                Event::new(EventKind::Access(notify::event::AccessKind::Close(
                    notify::event::AccessMode::Write,
                )))
                .add_path(visible.clone()),
                Instant::now(),
            ),
            DebouncedEvent::new(
                Event::new(EventKind::Modify(ModifyKind::Any)).add_path(visible.clone()),
                Instant::now(),
            ),
        ]),
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
        .any(|patch| patch.removed.iter().any(|path| path.ends_with(".hidden.txt"))));

    std::fs::remove_dir_all(root).expect("remove watched dir");
    handle_debounce_result_for_tests(
        &runtime,
        Ok(vec![DebouncedEvent::new(
            Event::new(EventKind::Any).add_path(Path::new(&target.path).join("gone.txt")),
            Instant::now(),
        )]),
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
        Err(vec![notify::Error::generic("plain boom")]),
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
    })
    .is_err());
}
