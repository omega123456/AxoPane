#[path = "common/mod.rs"]
mod common;

use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use file_explorer_lib::fs::{DirectoryEntry, SortDirection, SortKey};
use file_explorer_lib::watch::{
    diff_entries, pane_scope, snapshot_for_target, DirPatch, WatchService, WatchTarget,
};
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
fn helper_scope_and_diff_functions_describe_changes() {
    assert_eq!(pane_scope("left-1"), "left");
    assert_eq!(pane_scope("right"), "right");

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
        .any(|path: &String| path.ends_with("before.txt")));
    assert_eq!(common::bootstrap_message(), "phase-1-common");
}

#[test]
fn snapshot_helper_respects_filter_and_hidden_flags() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::write(root.join("alpha.txt"), b"a").expect("alpha");
    fs::write(root.join("beta.log"), b"b").expect("beta");
    fs::write(root.join(".secret"), b"s").expect("secret");

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
    assert!(filtered
        .keys()
        .any(|path: &String| path.ends_with("alpha.txt")));

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
    assert!(with_hidden
        .keys()
        .any(|path: &String| path.ends_with(".secret")));
}

#[test]
fn clearing_watches_stops_future_patch_delivery() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let patches = Arc::new(Mutex::new(Vec::<DirPatch>::new()));
    let service = WatchService::default();

    let patches_for_callback = patches.clone();
    service
        .set_tab_watch(
            Some(WatchTarget {
                tab_id: String::from("left-1"),
                path: root.to_string_lossy().into_owned(),
                sort_key: SortKey::Name,
                sort_direction: SortDirection::Asc,
                filter: String::new(),
                show_hidden: true,
                include_item_counts: true,
            }),
            None,
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

    service
        .set_tab_watch(None, None, None, Arc::new(|_| {}), Arc::new(|_, _| {}))
        .expect("clear watches");
    fs::write(root.join("after-clear.txt"), b"x").expect("after clear");
    thread::sleep(Duration::from_millis(250));

    let deadline = Instant::now() + Duration::from_millis(100);
    while Instant::now() < deadline {
        assert!(patches.lock().expect("patches lock").is_empty());
        thread::sleep(Duration::from_millis(10));
    }
}
