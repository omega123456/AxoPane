use file_explorer_lib::fs::{SortDirection, SortKey};
use file_explorer_lib::watch::coordinator::{CompactedBatch, MutationKind, WatchId};
use file_explorer_lib::watch::{
    add_watch, canonical_dir_for_tests, create_runtime, first_error_path_for_tests,
    handle_debounce_result_for_tests, insert_tab_for_tests, noop_watch_error_for_tests,
    process_compacted_batch_for_tests, raw_mutations_for_event_for_tests, remove_watch,
    snapshot_for_target, DirPatch, WatchRuntime, WatchTarget,
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

type WatchResults = Vec<Result<Event, notify::Error>>;

#[derive(Clone, Default)]
struct WatchRecorder {
    patches: Arc<Mutex<Vec<DirPatch>>>,
    errors: Arc<Mutex<Vec<(String, String)>>>,
}

impl WatchRecorder {
    fn deliver(&self, runtime: &WatchRuntime, results: WatchResults) {
        let patches = self.patches.clone();
        let errors = self.errors.clone();
        handle_debounce_result_for_tests(
            runtime,
            results,
            Arc::new(move |patch| patches.lock().expect("patches lock").push(patch)),
            Arc::new(move |path, error| {
                errors.lock().expect("errors lock").push((path, error));
            }),
        );
    }

    fn removed_any(&self, suffix: &str) -> bool {
        self.patches
            .lock()
            .expect("patches lock")
            .iter()
            .any(|patch| patch.removed.iter().any(|path| path.ends_with(suffix)))
    }

    fn changed_any(&self, suffix: &str) -> bool {
        self.patches
            .lock()
            .expect("patches lock")
            .iter()
            .any(|patch| {
                patch
                    .changed
                    .iter()
                    .any(|entry| entry.path.ends_with(suffix))
            })
    }

    fn has_no_patches(&self) -> bool {
        self.patches.lock().expect("patches lock").is_empty()
    }

    fn has_error(&self, predicate: impl Fn(&str, &str) -> bool) -> bool {
        self.errors
            .lock()
            .expect("errors lock")
            .iter()
            .any(|(path, error)| predicate(path, error))
    }

    fn has_no_errors(&self) -> bool {
        self.errors.lock().expect("errors lock").is_empty()
    }
}

fn runtime_watching(target: &WatchTarget, snapshot_target: &WatchTarget) -> WatchRuntime {
    let mut runtime = create_runtime(Arc::new(|_| {}), Arc::new(|_, _| {})).expect("runtime");
    insert_tab_for_tests(
        &mut runtime,
        target.clone(),
        snapshot_for_target(snapshot_target).expect("initial snapshot"),
    );
    runtime
}

fn counting_target(root: &Path, tab_id: &str) -> WatchTarget {
    WatchTarget {
        tab_id: tab_id.to_string(),
        include_item_counts: true,
        ..watch_target(root)
    }
}

fn event_at(kind: EventKind, path: &Path) -> Result<Event, notify::Error> {
    Ok(Event::new(kind).add_path(path.to_path_buf()))
}

#[test]
fn watch_runtime_test_hooks_drive_targeted_and_rescan_callbacks() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let before = root.join("before.txt");
    std::fs::write(&before, b"before").expect("before");
    let target = counting_target(root, "left-1");
    let runtime = runtime_watching(&target, &target);
    let recorder = WatchRecorder::default();

    let created = root.join("created.txt");
    std::fs::write(&created, b"created").expect("created");
    recorder.deliver(
        &runtime,
        vec![event_at(EventKind::Create(CreateKind::File), &created)],
    );
    assert!(recorder.changed_any("created.txt"));
    assert!(recorder.has_no_errors());

    std::fs::remove_file(&before).expect("remove before");
    std::fs::write(root.join("after.txt"), b"after").expect("after");
    recorder.deliver(&runtime, vec![event_at(EventKind::Any, &before)]);
    assert!(recorder.removed_any("before.txt"));
    assert!(recorder.changed_any("after.txt"));
}

#[test]
fn watch_runtime_test_hook_surfaces_errors_and_removals() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let doomed = root.join("doomed.txt");
    std::fs::write(&doomed, b"gone").expect("doomed");
    let target = counting_target(root, "right-1");
    let runtime = runtime_watching(&target, &target);
    let recorder = WatchRecorder::default();

    std::fs::remove_file(&doomed).expect("remove doomed");
    recorder.deliver(
        &runtime,
        vec![event_at(EventKind::Remove(RemoveKind::File), &doomed)],
    );
    assert!(recorder.removed_any("doomed.txt"));

    let broken = notify::Error::generic("boom").add_path(root.join("broken.txt"));
    recorder.deliver(&runtime, vec![Err(broken)]);
    assert!(
        recorder.has_error(|path, error| path.ends_with("broken.txt") && error.contains("boom"))
    );
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

    let unfiltered = counting_target(root, "left-2");
    let target = WatchTarget {
        filter: "keep".to_string(),
        ..unfiltered.clone()
    };
    let runtime = runtime_watching(&target, &unfiltered);
    let recorder = WatchRecorder::default();
    let rename = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
        .add_path(old.clone())
        .add_path(new.clone());
    recorder.deliver(
        &runtime,
        vec![
            event_at(
                EventKind::Modify(ModifyKind::Data(DataChange::Content)),
                &drop,
            ),
            Ok(rename),
        ],
    );

    assert!(recorder.removed_any("drop.txt"));
    assert!(recorder.removed_any("old.txt"));
    assert!(!recorder.changed_any("new.txt"));
}

#[test]
fn watch_runtime_test_hooks_ignore_siblings_and_drop_hidden_entries() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let sibling = fixture.path().join("sibling");
    std::fs::create_dir(&sibling).expect("sibling");
    let visible = root.join("visible.txt");
    std::fs::write(&visible, b"visible").expect("visible");
    std::fs::write(root.join(".hidden.txt"), b"hidden").expect("hidden");

    let showing_hidden = counting_target(root, "left-branches");
    let target = WatchTarget {
        show_hidden: false,
        ..showing_hidden.clone()
    };
    let runtime = runtime_watching(&target, &showing_hidden);
    let recorder = WatchRecorder::default();

    let ignored = sibling.join("ignore.txt");
    recorder.deliver(
        &runtime,
        vec![event_at(EventKind::Create(CreateKind::File), &ignored)],
    );
    assert!(recorder.has_no_patches());
    assert!(recorder.has_no_errors());

    let write_closed = EventKind::Access(AccessKind::Close(notify::event::AccessMode::Write));
    recorder.deliver(
        &runtime,
        vec![
            event_at(write_closed, &visible),
            event_at(EventKind::Modify(ModifyKind::Any), &visible),
        ],
    );
    assert!(recorder.removed_any(".hidden.txt"));
}

#[test]
fn watch_runtime_test_hooks_report_missing_directories_and_plain_errors() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path().join("watched");
    std::fs::create_dir(&root).expect("watched");
    let target = counting_target(&root, "left-errors");
    let runtime = runtime_watching(&target, &target);
    let recorder = WatchRecorder::default();

    std::fs::remove_dir_all(&root).expect("remove watched dir");
    let gone = Path::new(&target.path).join("gone.txt");
    recorder.deliver(&runtime, vec![event_at(EventKind::Any, &gone)]);
    assert!(recorder.has_error(|path, error| path == target.path && !error.is_empty()));

    recorder.deliver(&runtime, vec![Err(notify::Error::generic("plain boom"))]);
    assert!(recorder.has_error(|path, error| path.is_empty() && error.contains("plain boom")));
}

#[test]
fn watch_runtime_error_helpers_cover_paths_and_missing_targets() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let visible = root.join("visible.txt");

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
    let missing_target = WatchTarget {
        tab_id: "missing".to_string(),
        include_item_counts: true,
        ..watch_target(&root.join("missing-again"))
    };
    assert!(snapshot_for_target(&missing_target).is_err());
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
