use std::fs;
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use file_explorer_lib::fs::{self as app_fs, SortDirection, SortKey};
use file_explorer_lib::watch::{
    create_runtime, forward_notify_result_with_before_lock_for_tests, insert_tab_for_tests,
    snapshot_for_target, tab_snapshot_for_tests, while_tabs_locked_for_tests, DirPatch,
    WatchService, WatchTarget,
};
use notify::event::CreateKind;
use notify::{Event, EventKind};
use tempfile::tempdir;

fn canonical_display_path(path: &Path) -> String {
    app_fs::display_path_from_path(&std::fs::canonicalize(path).expect("canonical path"))
}

fn target(tab_id: &str, path: &Path) -> WatchTarget {
    WatchTarget {
        tab_id: tab_id.to_string(),
        path: canonical_display_path(path),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
        include_item_counts: true,
    }
}

#[test]
fn set_tab_watch_records_the_current_listing_as_the_tab_baseline() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::write(root.join("alpha.txt"), "alpha").expect("alpha");
    fs::write(root.join("beta.txt"), "beta").expect("beta");

    let service = WatchService::default();
    let watch_target = target("right-1", root);

    service
        .set_tab_watch(
            Some(watch_target.clone()),
            None,
            None,
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
        )
        .expect("set watch");

    let baseline =
        tab_snapshot_for_tests(&service, &watch_target.tab_id).expect("tab baseline recorded");
    assert!(baseline.keys().any(|path| path.ends_with("alpha.txt")));
    assert!(baseline.keys().any(|path| path.ends_with("beta.txt")));
}

#[test]
fn replacing_a_same_pane_watch_discards_the_previous_tab_snapshot() {
    let fixture = tempdir().expect("temp dir");
    let left_a = fixture.path().join("left-a");
    let left_b = fixture.path().join("left-b");
    fs::create_dir_all(&left_a).expect("left a");
    fs::create_dir_all(&left_b).expect("left b");
    fs::write(left_a.join("stale.txt"), "stale").expect("stale");
    fs::write(left_b.join("fresh.txt"), "fresh").expect("fresh");

    let service = WatchService::default();

    service
        .set_tab_watch(
            Some(target("left-1", &left_a)),
            None,
            None,
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
        )
        .expect("set first watch");
    service
        .set_tab_watch(
            Some(target("left-2", &left_b)),
            None,
            None,
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
        )
        .expect("replace same pane watch");

    assert!(
        tab_snapshot_for_tests(&service, "left-1").is_none(),
        "the previous left-pane tab should be discarded when a new left-pane watch replaces it"
    );

    let replacement = tab_snapshot_for_tests(&service, "left-2").expect("replacement baseline");
    assert!(replacement.keys().any(|path| path.ends_with("fresh.txt")));
    assert!(!replacement.keys().any(|path| path.ends_with("stale.txt")));
}

#[test]
fn clearing_watches_removes_all_recorded_tab_snapshots() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::write(root.join("keep.txt"), "keep").expect("keep");

    let service = WatchService::default();
    let watch_target = target("left-1", root);

    service
        .set_tab_watch(
            Some(watch_target.clone()),
            None,
            None,
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
        )
        .expect("set watch");
    assert!(tab_snapshot_for_tests(&service, &watch_target.tab_id).is_some());

    service
        .set_tab_watch(None, None, None, Arc::new(|_| {}), Arc::new(|_, _| {}))
        .expect("clear watches");

    assert!(tab_snapshot_for_tests(&service, &watch_target.tab_id).is_none());
}

#[test]
fn reconcile_emits_refresh_patch_for_missed_directory_changes() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::write(root.join("old.txt"), "old").expect("old");

    let service = WatchService::default();
    let watch_target = target("left-1", root);

    service
        .set_tab_watch(
            Some(watch_target.clone()),
            None,
            None,
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
        )
        .expect("set watch");

    let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
    let errors = Arc::new(Mutex::new(Vec::<(String, String)>::new()));

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
    assert!(patches.lock().expect("patches lock").is_empty());
    assert!(errors.lock().expect("errors lock").is_empty());

    fs::remove_file(root.join("old.txt")).expect("remove old");
    fs::create_dir(root.join("fresh")).expect("fresh dir");

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
        .find(|patch| patch.path == watch_target.path)
        .expect("refresh patch for watched path");
    assert_eq!(patch.reason, "refresh");
    assert!(patch.removed.iter().any(|path| path.ends_with("old.txt")));
    assert!(patch.changed.iter().any(|change| {
        change.path.ends_with("fresh") && change.entry.as_ref().is_some_and(|entry| entry.is_dir)
    }));

    let baseline =
        tab_snapshot_for_tests(&service, &watch_target.tab_id).expect("tab baseline refreshed");
    assert!(!baseline.keys().any(|path| path.ends_with("old.txt")));
    assert!(baseline.keys().any(|path| path.ends_with("fresh")));
}

#[test]
fn notify_callback_recovers_a_mutation_after_tabs_lock_contention() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let watch_target = target("left-1", root);
    let (patch_tx, patch_rx) = mpsc::channel();
    let mut runtime = create_runtime(
        Arc::new(move |patch| patch_tx.send(patch).expect("patch receiver")),
        Arc::new(|_, error| panic!("unexpected watch error: {error}")),
    )
    .expect("watch runtime");
    let snapshot = snapshot_for_target(&watch_target).expect("initial snapshot");
    insert_tab_for_tests(&mut runtime, watch_target.clone(), snapshot);
    let runtime = Arc::new(runtime);

    let added = root.join("arrived.txt");
    fs::write(&added, "arrived").expect("added file");
    let (before_lock_tx, before_lock_rx) = mpsc::channel();
    let (completed_tx, completed_rx) = mpsc::channel();
    let (handle_tx, handle_rx) = mpsc::channel();
    while_tabs_locked_for_tests(&runtime, || {
        let callback_runtime = Arc::clone(&runtime);
        let callback = thread::spawn(move || {
            forward_notify_result_with_before_lock_for_tests(
                &callback_runtime,
                Ok(Event::new(EventKind::Create(CreateKind::File)).add_path(added)),
                || before_lock_tx.send(()).expect("before-lock receiver"),
            );
            completed_tx.send(()).expect("completion receiver");
        });
        handle_tx.send(callback).expect("callback handle receiver");
        before_lock_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("callback reached its registration snapshot");
        assert!(
            completed_rx.try_recv().is_err(),
            "callback must wait rather than silently discard its event"
        );
    });
    completed_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("callback completed after lock release");
    let callback = handle_rx.recv().expect("callback handle");
    callback.join().expect("callback thread");

    let patch = patch_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("callback event must not be dropped while tab state is busy");
    assert_eq!(patch.path, watch_target.path);
    assert!(patch
        .changed
        .iter()
        .any(|entry| entry.path.ends_with("arrived.txt")));
}
