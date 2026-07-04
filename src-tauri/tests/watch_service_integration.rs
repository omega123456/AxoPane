#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use file_explorer_lib::fs::{SortDirection, SortKey};
use file_explorer_lib::watch::{DirPatch, WatchService, WatchTarget};
use tempfile::tempdir;

/// Polls the shared patches buffer (fed by the real, debounced watcher
/// callback) until a patch matching `predicate` shows up, or panics after a
/// bounded timeout. Condition-based waiting keeps this deterministic despite
/// the watcher's internal debounce window.
fn wait_for_patch<F>(patches: &Arc<Mutex<Vec<DirPatch>>>, predicate: F) -> DirPatch
where
    F: Fn(&DirPatch) -> bool,
{
    let start = Instant::now();
    loop {
        if let Some(patch) = patches
            .lock()
            .expect("patches lock")
            .iter()
            .find(|patch| predicate(patch))
            .cloned()
        {
            return patch;
        }
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "timed out waiting for watcher patch"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Waits until `predicate` holds across the *union* of every changed entry
/// seen in all patches emitted so far. The real debouncer may coalesce a
/// storm of writes into one patch or split it across a couple of emissions
/// depending on timing, so callers that care about "did the watcher observe
/// these files changing" (rather than "did they all land in one patch")
/// should assert on the union rather than a single `DirPatch`.
fn wait_for_changed_union<F>(patches: &Arc<Mutex<Vec<DirPatch>>>, predicate: F)
where
    F: Fn(&[String]) -> bool,
{
    let start = Instant::now();
    loop {
        let changed_paths: Vec<String> = patches
            .lock()
            .expect("patches lock")
            .iter()
            .flat_map(|patch| patch.changed.iter().map(|entry| entry.path.clone()))
            .collect();
        if predicate(&changed_paths) {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "timed out waiting for watcher patches to cover expected changes"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn watcher_coalesces_changes_and_emits_patch() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let service = WatchService::default();

    let target = WatchTarget {
        tab_id: String::from("right-1"),
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
        include_item_counts: true,
    };

    let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
    let patches_for_callback = patches.clone();
    service
        .set_tab_watch(
            Some(target.clone()),
            None,
            Arc::new(move |patch| {
                patches_for_callback
                    .lock()
                    .expect("patches lock")
                    .push(patch);
            }),
            Arc::new(|_, _| {}),
        )
        .expect("set watch");

    fs::write(root.join("storm.txt"), "one").expect("storm create");
    fs::write(root.join("storm.txt"), "two").expect("storm update");
    fs::write(root.join("storm-2.txt"), "three").expect("storm create 2");

    // The debouncer may coalesce this storm of writes into one patch, or
    // split it across a couple of debounce windows depending on timing;
    // either way both files must eventually show up as `changed` somewhere.
    wait_for_changed_union(&patches, |changed_paths| {
        changed_paths.iter().any(|path| path.ends_with("storm.txt"))
            && changed_paths
                .iter()
                .any(|path| path.ends_with("storm-2.txt"))
    });
    assert_eq!(common::bootstrap_message(), "phase-1-common");
}

#[test]
fn watcher_emits_targeted_removal_patch() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::write(root.join("gone.txt"), "gone").expect("seed");

    let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
    let service = WatchService::default();
    let patches_for_callback = patches.clone();
    let target = WatchTarget {
        tab_id: String::from("left-1"),
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
        include_item_counts: true,
    };
    service
        .set_tab_watch(
            Some(target.clone()),
            None,
            Arc::new(move |patch| {
                patches_for_callback
                    .lock()
                    .expect("patches lock")
                    .push(patch);
            }),
            Arc::new(|_, _| {}),
        )
        .expect("set watch");

    fs::remove_file(root.join("gone.txt")).expect("remove");

    let patch = wait_for_patch(&patches, |patch| {
        patch.removed.iter().any(|path| path.ends_with("gone.txt"))
    });

    assert!(patch.removed.iter().any(|path| path.ends_with("gone.txt")));
}

#[test]
fn replacing_a_pane_watch_discards_the_previous_tab_watch() {
    let fixture = tempdir().expect("temp dir");
    let left_a = fixture.path().join("left-a");
    let left_b = fixture.path().join("left-b");
    fs::create_dir_all(&left_a).expect("left a");
    fs::create_dir_all(&left_b).expect("left b");

    let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
    let service = WatchService::default();
    let second_target = WatchTarget {
        tab_id: String::from("left-2"),
        path: left_b.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
        include_item_counts: true,
    };

    let patches_for_callback = patches.clone();
    service
        .set_tab_watch(
            Some(WatchTarget {
                tab_id: String::from("left-1"),
                path: left_a.to_string_lossy().into_owned(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: String::new(),
                show_hidden: true,
                include_item_counts: true,
            }),
            None,
            Arc::new(move |patch| {
                patches_for_callback
                    .lock()
                    .expect("patches lock")
                    .push(patch);
            }),
            Arc::new(move |_, _| {}),
        )
        .expect("set first watch");

    let patches_for_second = patches.clone();
    service
        .set_tab_watch(
            Some(second_target.clone()),
            None,
            Arc::new(move |patch| {
                patches_for_second.lock().expect("patches lock").push(patch);
            }),
            Arc::new(|_, _| {}),
        )
        .expect("replace watch");

    fs::write(left_a.join("stale.txt"), "stale").expect("write stale");
    fs::write(left_b.join("fresh.txt"), "fresh").expect("write fresh");

    let patch = wait_for_patch(&patches, |patch| {
        patch
            .changed
            .iter()
            .any(|entry| entry.path.ends_with("fresh.txt"))
    });

    assert_eq!(patch.path, left_b.to_string_lossy());
    assert!(patch
        .changed
        .iter()
        .any(|entry| entry.path.ends_with("fresh.txt")));
    assert!(!patch
        .changed
        .iter()
        .any(|entry| entry.path.ends_with("stale.txt")));
}
