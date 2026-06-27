#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use file_explorer_lib::fs::{SortDirection, SortKey};
use file_explorer_lib::watch::{DirPatch, WatchService, WatchTarget};
use tempfile::tempdir;

#[test]
fn refresh_tab_emits_incremental_patch() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::write(root.join("before.txt"), "a").expect("before");

    let service = WatchService::default();
    let target = WatchTarget {
        tab_id: String::from("left-1"),
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
    };

    service
        .set_tab_watch(Some(target.clone()), Arc::new(|_| {}), Arc::new(|_, _| {}))
        .expect("set watch");

    fs::remove_file(root.join("before.txt")).expect("remove before");
    fs::write(root.join("after.txt"), "b").expect("after");

    let patch = service
        .refresh_tab(target, Arc::new(|_| {}))
        .expect("refresh");
    assert_eq!(patch.tab_id, "left-1");
    assert!(patch
        .removed
        .iter()
        .any(|path| path.ends_with("before.txt")));
    assert!(patch
        .changed
        .iter()
        .any(|entry| entry.path.ends_with("after.txt")));
}

#[test]
fn refresh_tab_initializes_snapshot_on_first_use() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::write(root.join("initial.txt"), "seed").expect("initial");

    let service = WatchService::default();
    let target = WatchTarget {
        tab_id: String::from("left-2"),
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
    };

    let first_patch = service
        .refresh_tab(target.clone(), Arc::new(|_| {}))
        .expect("first refresh");
    assert!(first_patch
        .changed
        .iter()
        .any(|entry| entry.path.ends_with("initial.txt")));

    let second_patch = service
        .refresh_tab(target, Arc::new(|_| {}))
        .expect("second refresh");
    assert!(second_patch.changed.is_empty());
    assert!(second_patch.removed.is_empty());
}

#[test]
fn watcher_coalesces_changes_and_emits_patch() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let service = WatchService::default();

    let target = WatchTarget {
        tab_id: String::from("right-1"),
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
    };

    let patches_for_callback = patches.clone();
    let errors_for_callback = errors.clone();
    service
        .set_tab_watch(
            Some(target),
            Arc::new(move |patch| {
                patches_for_callback
                    .lock()
                    .expect("patches lock")
                    .push(patch);
            }),
            Arc::new(move |_, message| {
                errors_for_callback
                    .lock()
                    .expect("errors lock")
                    .push(message);
            }),
        )
        .expect("set watch");

    fs::write(root.join("storm.txt"), "one").expect("storm create");
    fs::write(root.join("storm.txt"), "two").expect("storm update");
    fs::write(root.join("storm-2.txt"), "three").expect("storm create 2");

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if !patches.lock().expect("patches lock").is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    let recorded = patches.lock().expect("patches lock").clone();
    assert!(!recorded.is_empty());
    assert!(errors.lock().expect("errors lock").is_empty());
    assert!(recorded.iter().any(|patch| patch
        .changed
        .iter()
        .any(|entry| entry.path.ends_with("storm.txt"))));
    assert_eq!(common::bootstrap_message(), "phase-1-common");
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
            }),
            Arc::new(move |patch| {
                patches_for_callback
                    .lock()
                    .expect("patches lock")
                    .push(patch);
            }),
            Arc::new(move |_, _| {}),
        )
        .expect("set first watch");

    service
        .set_tab_watch(
            Some(WatchTarget {
                tab_id: String::from("left-2"),
                path: left_b.to_string_lossy().into_owned(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: String::new(),
                show_hidden: true,
            }),
            Arc::new(|_| {}),
            Arc::new(|_, _| {}),
        )
        .expect("replace watch");

    fs::write(left_a.join("stale.txt"), "stale").expect("write stale");
    fs::write(left_b.join("fresh.txt"), "fresh").expect("write fresh");

    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let recorded = patches.lock().expect("patches lock");
        if recorded
            .iter()
            .any(|patch| patch.path == left_b.to_string_lossy())
        {
            break;
        }
        drop(recorded);
        thread::sleep(Duration::from_millis(25));
    }

    let recorded = patches.lock().expect("patches lock").clone();
    assert!(recorded
        .iter()
        .any(|patch| patch.path == left_b.to_string_lossy()));
    assert!(!recorded
        .iter()
        .any(|patch| patch.path == left_a.to_string_lossy()));
}
