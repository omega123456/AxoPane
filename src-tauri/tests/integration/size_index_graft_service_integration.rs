use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use file_explorer_lib::size::index::names::OVERLAY_THRESHOLD_FOLDERS;
use file_explorer_lib::size::index::persist::{self, ChangeFlags};
use file_explorer_lib::size::index::platform::{PowerSignal, RawEvent};
use file_explorer_lib::size::index::HEAP_BATCH_RECORDS;
use file_explorer_lib::size::{IndexState, RequestMode, SizeStateKind};

use crate::size_index_names_service_integration::{
    barrier, base_segment, blur, files_on_disk, finish_replay, indexed_names, indexing_runs, owned,
    relaunch, saved, scan_and_save,
};
use crate::size_index_service_integration::{fresh_total, terminal, wait_until, Harness};

const SPILL_EVERY_SECOND_FOLDER: usize = 1;
const EVENT_ID: u64 = 500;
const MORE_EVENTS_THAN_A_WALK_LOGS: usize = 1_100;
const READ_ONLY: u32 = 0o555;
const WRITABLE: u32 = 0o755;
const ORPHAN_WALK_SPILL: &str = "walk-spill-000099.bin.run0";

fn deep_harness() -> Harness {
    let mut index = Harness::new();
    fs::create_dir_all(index.path("a/b/c/d")).unwrap();
    fs::write(index.path("a/b/c/c.bin"), vec![0; 20]).unwrap();
    fs::write(index.path("a/b/c/d/d.bin"), vec![0; 40]).unwrap();
    index.batching.heap_records = SPILL_EVERY_SECOND_FOLDER;
    index
}

fn cache_directory(index: &Harness) -> PathBuf {
    index
        .snapshot
        .parent()
        .expect("cache directory")
        .to_path_buf()
}

fn refreshed_size(index: &Harness, relative: &str) -> (SizeStateKind, Option<u64>) {
    let key = index.key(relative);
    let update = terminal(&index.request(&[relative], RequestMode::Refresh), &key);
    (update.state, update.size_bytes)
}

fn times_opened(index: &Harness, relative: &str) -> usize {
    let path = index.path(relative);
    index
        .opened
        .lock()
        .unwrap()
        .iter()
        .filter(|opened| **opened == path)
        .count()
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn a_first_scan_grafts_subtrees_larger_than_the_heap_batch() {
    let mut index = deep_harness();
    scan_and_save(&mut index);

    for relative in ["a", "a/b", "a/b/c", "a/b/c/d"] {
        assert_eq!(
            indexed_names(&index, relative),
            files_on_disk(&index.path(relative)),
            "{relative}"
        );
    }
    let (state, bytes) = refreshed_size(&index, "a");
    assert_eq!(state, SizeStateKind::Ready);
    assert_eq!(bytes, Some(fresh_total(&index.path("a"))));
    assert!(!saved(&index).meta.full_scan_running);
}

#[test]
fn a_large_refresh_is_grafted_kept_in_the_manifest_and_survives_a_relaunch() {
    let mut index = deep_harness();
    index.platform.set_event_id(EVENT_ID);
    scan_and_save(&mut index);
    let base = base_segment(&index);
    fs::write(index.path("a/b/c/d/new.bin"), vec![0; 10]).unwrap();

    let (state, bytes) = refreshed_size(&index, "a/b/c");
    assert_eq!(state, SizeStateKind::Ready);
    assert_eq!(bytes, Some(fresh_total(&index.path("a/b/c"))));
    assert_eq!(
        indexed_names(&index, "a/b/c/d"),
        owned(&["d.bin", "new.bin"])
    );
    blur(&index);
    let segments = saved(&index).segments;
    assert_eq!(segments.len(), 2, "the base plus the graft");
    assert_eq!(segments[0], base);

    relaunch(&mut index);
    assert_eq!(index.platform.stream_starts().last(), Some(&Some(EVENT_ID)));
    finish_replay(&index);
    index.wait_state(IndexState::UpToDate);
    assert_eq!(indexing_runs(&index), 0, "no rescan");
    assert_eq!(
        indexed_names(&index, "a/b/c/d"),
        owned(&["d.bin", "new.bin"])
    );
    let folder_a = index.path("a");
    let total = index
        .service()
        .folder_size_index()
        .expect("index attached")
        .read_index(|view| view.tree.total(&folder_a).map(|total| total.bytes))
        .expect("readable");
    assert_eq!(total, Some(fresh_total(&folder_a)));
}

#[test]
fn an_event_inside_a_running_walk_is_applied_again_after_the_graft() {
    let mut index = deep_harness();
    scan_and_save(&mut index);
    index.opened.lock().unwrap().clear();
    index.blocker.block(&index.path("a/b/c/d"));
    let updates = index.request(&["a/b/c"], RequestMode::Refresh);
    wait_until("the walk lists c and reaches d", || index.opened("a/b/c/d"));

    fs::write(index.path("a/b/c/late.bin"), b"late").unwrap();
    index.event("a/b/c", ChangeFlags::default());
    barrier(&index);
    index.blocker.release(&index.path("a/b/c/d"));
    let update = terminal(&updates, &index.key("a/b/c"));

    assert_eq!(update.state, SizeStateKind::Ready);
    assert_eq!(
        indexed_names(&index, "a/b/c"),
        owned(&["c.bin", "late.bin"])
    );
    assert_eq!(update.size_bytes, Some(fresh_total(&index.path("a/b/c"))));
}

#[test]
fn too_many_events_inside_a_running_walk_requeue_the_walk() {
    let mut index = deep_harness();
    scan_and_save(&mut index);
    index.opened.lock().unwrap().clear();
    index.blocker.block(&index.path("a/b/c/d"));
    let updates = index.request(&["a/b/c"], RequestMode::Refresh);
    wait_until("the walk reaches d", || index.opened("a/b/c/d"));

    let events = (0..MORE_EVENTS_THAN_A_WALK_LOGS)
        .map(|number| RawEvent {
            path: index.path(&format!("a/b/c/gone-{number}")),
            id: EVENT_ID,
            flags: ChangeFlags::default(),
            history_done: false,
        })
        .collect();
    assert!(index.platform.emit(events));
    barrier(&index);
    index.blocker.release(&index.path("a/b/c/d"));
    terminal(&updates, &index.key("a/b/c"));

    wait_until("the walk runs again in the background", || {
        times_opened(&index, "a/b/c") == 2
    });
    index.wait_state(IndexState::UpToDate);
}

#[test]
fn a_graft_that_cannot_be_written_reports_an_error_and_keeps_the_index() {
    let mut index = deep_harness();
    scan_and_save(&mut index);
    let before = indexed_names(&index, "a/b/c/d");
    let cache = cache_directory(&index);

    set_mode(&cache, READ_ONLY);
    let (failed, _) = refreshed_size(&index, "a/b/c");
    set_mode(&cache, WRITABLE);

    assert_eq!(failed, SizeStateKind::Error);
    assert_eq!(indexed_names(&index, "a/b/c/d"), before);
    let (state, bytes) = refreshed_size(&index, "a/b/c");
    assert_eq!(state, SizeStateKind::Ready);
    assert_eq!(bytes, Some(fresh_total(&index.path("a/b/c"))));
}

#[test]
fn a_quit_mid_scan_resumes_only_the_unscanned_subtrees() {
    let mut index = deep_harness();
    index.batching.heap_records = HEAP_BATCH_RECORDS;
    index.platform.set_event_id(EVENT_ID);
    index.blocker.block(&index.path("a/b/c"));
    index.start();
    wait_until("the scan reaches the subtree c", || index.opened("a/b/c"));
    index.platform.signal(PowerSignal::Conditions {
        low_power: true,
        thermal_pressure: false,
    });
    index.wait_state(IndexState::Paused);
    index.blocker.release(&index.path("a/b/c"));
    wait_until("walk parked", || index.parked.load(Ordering::SeqCst) > 0);
    index.service().flush_index();
    assert!(saved(&index).meta.full_scan_running);
    fs::write(cache_directory(&index).join(ORPHAN_WALK_SPILL), b"run").unwrap();

    relaunch(&mut index);
    assert!(!cache_directory(&index).join(ORPHAN_WALK_SPILL).exists());
    index.platform.signal(PowerSignal::Conditions {
        low_power: false,
        thermal_pressure: false,
    });
    finish_replay(&index);
    wait_until("the resumed scan finished", || {
        indexing_runs(&index) == 1 && index.state() == IndexState::UpToDate
    });

    assert!(!index.opened(""), "the top levels are not listed again");
    assert!(index.opened("a/b/c"));
    assert_eq!(indexed_names(&index, "a/b/c/d"), owned(&["d.bin"]));
    blur(&index);
    assert!(!saved(&index).meta.full_scan_running);
}

#[test]
fn a_boost_on_an_unindexed_folder_grafts_it_and_keeps_its_indexed_children() {
    let mut index = deep_harness();
    scan_and_save(&mut index);
    index.platform.signal(PowerSignal::Conditions {
        low_power: true,
        thermal_pressure: false,
    });
    fs::create_dir_all(index.path("p/kept")).unwrap();
    fs::create_dir_all(index.path("p/q/r")).unwrap();
    fs::write(index.path("p/kept/kept.bin"), vec![0; 3]).unwrap();
    fs::write(index.path("p/q/r/r.bin"), vec![0; 30]).unwrap();
    fs::write(index.path("p/p.bin"), vec![0; 300]).unwrap();
    index.event("", ChangeFlags::default());
    barrier(&index);

    let (kept_state, _) = auto_size(&index, "p/kept");
    assert_eq!(kept_state, SizeStateKind::Ready);
    index.opened.lock().unwrap().clear();
    let (state, bytes) = auto_size(&index, "p");

    assert_eq!(state, SizeStateKind::Ready);
    assert_eq!(bytes, Some(fresh_total(&index.path("p"))));
    assert!(
        !index.opened("p/kept"),
        "the indexed child is not walked again"
    );
    assert_eq!(indexed_names(&index, "p/kept"), owned(&["kept.bin"]));
    assert_eq!(indexed_names(&index, "p/q/r"), owned(&["r.bin"]));
    blur(&index);
    assert!(saved(&index).segments.len() > 1, "the walk of p is a graft");
    index.platform.signal(PowerSignal::Conditions {
        low_power: false,
        thermal_pressure: false,
    });
}

fn auto_size(index: &Harness, relative: &str) -> (SizeStateKind, Option<u64>) {
    let key = index.key(relative);
    let update = terminal(&index.request(&[relative], RequestMode::Auto), &key);
    (update.state, update.size_bytes)
}

#[test]
fn a_full_scan_batch_that_cannot_be_written_is_walked_again() {
    let mut index = deep_harness();
    index.batching.heap_records = HEAP_BATCH_RECORDS;
    let cache = cache_directory(&index);
    fs::create_dir_all(&cache).unwrap();
    set_mode(&cache, READ_ONLY);
    index.start();
    let scanned = || index.state() == IndexState::UpToDate && indexing_runs(&index) == 1;
    wait_until("the scan finished", scanned);
    set_mode(&cache, WRITABLE);

    assert!(
        times_opened(&index, "a/b/c") >= 2,
        "the subtree is walked again"
    );
    assert_eq!(indexed_names(&index, "a/b/c/d"), owned(&["d.bin"]));
    let (state, bytes) = auto_size(&index, "a");
    assert_eq!(state, SizeStateKind::Ready);
    assert_eq!(bytes, Some(fresh_total(&index.path("a"))));
}

const ONE_FOLDER_PER_FOREST: u64 = 1;

#[test]
fn a_full_scan_flushes_a_forest_whenever_the_batch_is_full() {
    let mut index = deep_harness();
    index.batching.forest_records = ONE_FOLDER_PER_FOREST;
    fs::create_dir_all(index.path("s/t/u")).unwrap();
    fs::write(index.path("s/t/u/u.bin"), vec![0; 9]).unwrap();
    scan_and_save(&mut index);

    assert_eq!(indexed_names(&index, "a/b/c/d"), owned(&["d.bin"]));
    assert_eq!(indexed_names(&index, "s/t/u"), owned(&["u.bin"]));
    for relative in ["a", "s"] {
        let (state, bytes) = auto_size(&index, relative);
        assert_eq!(state, SizeStateKind::Ready, "{relative}");
        assert_eq!(
            bytes,
            Some(fresh_total(&index.path(relative))),
            "{relative}"
        );
    }
}

#[test]
fn a_full_overlay_is_compacted_while_the_scan_is_still_running() {
    let mut index = deep_harness();
    for number in 0..=OVERLAY_THRESHOLD_FOLDERS {
        fs::create_dir_all(index.path(&format!("many/d{number}"))).unwrap();
    }
    index.blocker.block(&index.path("a/b/c"));
    index.start();
    wait_until("the scan reaches the subtree c", || index.opened("a/b/c"));

    wait_until("a compaction saved a base mid-scan", || {
        persist::load(&index.snapshot).is_some_and(|snapshot| {
            snapshot.meta.full_scan_running && !snapshot.segments.is_empty()
        })
    });
    index.blocker.release(&index.path("a/b/c"));
    index.wait_state(IndexState::UpToDate);
    assert_eq!(indexed_names(&index, "a/b/c/d"), owned(&["d.bin"]));
}
