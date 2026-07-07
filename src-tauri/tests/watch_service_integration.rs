use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use file_explorer_lib::fs::{self as app_fs, SortDirection, SortKey};
use file_explorer_lib::watch::{tab_snapshot_for_tests, DirPatch, WatchService, WatchTarget};
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
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
        )
        .expect("set first watch");
    service
        .set_tab_watch(
            Some(target("left-2", &left_b)),
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
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
        )
        .expect("set watch");
    assert!(tab_snapshot_for_tests(&service, &watch_target.tab_id).is_some());

    service
        .set_tab_watch(None, None, Arc::new(|_| {}), Arc::new(|_, _| {}))
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
