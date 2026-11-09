use std::fs;
use std::path::Path;

use file_explorer_lib::ipc::commands;
use file_explorer_lib::size::index::names::file::spill_path;
use file_explorer_lib::size::index::names_path_beside;
use file_explorer_lib::size::{FolderSizeIndexStorage, IndexState};

use crate::ipc_state_manager_integration::as_state;
use crate::size_index_service_integration::{wait_until, Harness};

const INDEXED_FOLDERS: u64 = 6;
const INDEXED_FILES: u64 = 5;

fn storage(index: &Harness) -> FolderSizeIndexStorage {
    index
        .service()
        .folder_size_index_storage()
        .expect("index attached")
}

fn cache_directory(index: &Harness) -> String {
    let directory = index.snapshot.parent().expect("cache directory");
    directory.to_string_lossy().into_owned()
}

fn file_bytes(path: &Path) -> u64 {
    fs::metadata(path).map_or(0, |metadata| metadata.len())
}

fn bytes_of_saved_pair(index: &Harness) -> u64 {
    file_bytes(&index.snapshot) + file_bytes(&names_path_beside(&index.snapshot))
}

fn rebuild(index: &Harness) {
    assert_eq!(index.service().rebuild_folder_size_index(), Ok(()));
}

fn add_file_without_an_event(index: &Harness) {
    fs::write(index.path("a/unreported.txt"), b"quiet").unwrap();
}

/// Names merged by the rebuild are only seen after it published `Indexing`,
/// so `UpToDate` afterwards means the rebuild finished.
fn wait_for_rebuild_to_find_the_unreported_file(index: &Harness) {
    wait_until("rebuild finished", || {
        let found = storage(index).file_count == Some(INDEXED_FILES + 1);
        found && index.state() == IndexState::UpToDate
    });
}

#[test]
fn storage_reports_the_cache_folder_its_bytes_and_every_indexed_item() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);

    let reported = storage(&index);

    assert_eq!(reported.directory, cache_directory(&index));
    assert!(bytes_of_saved_pair(&index) > 0);
    assert_eq!(reported.size_bytes, bytes_of_saved_pair(&index));
    assert_eq!(reported.folder_count, Some(INDEXED_FOLDERS));
    assert_eq!(reported.file_count, Some(INDEXED_FILES));
}

#[test]
fn storage_counts_the_spill_bytes_but_no_files_while_the_first_scan_spills() {
    let mut index = Harness::new();
    index.blocker.block(&index.path("a"));
    index.start();
    wait_until("walk reaches a", || index.opened("a"));
    let spill = spill_path(&names_path_beside(&index.snapshot));
    assert!(spill.exists());
    assert_eq!(bytes_of_saved_pair(&index), 0, "no pair saved yet");

    let spilled_before = file_bytes(&spill);
    let reported = storage(&index);
    let spilled_after = file_bytes(&spill);

    assert!((spilled_before..=spilled_after).contains(&reported.size_bytes));
    assert!(reported.folder_count.is_some());
    assert_eq!(reported.file_count, None, "spilled names are not counted");
    index.blocker.release(&index.path("a"));
    index.wait_state(IndexState::UpToDate);
    assert_eq!(storage(&index).file_count, Some(INDEXED_FILES));
}

#[test]
fn storage_without_a_running_stream_reports_the_folder_but_no_counts() {
    let mut index = Harness::new();
    index.platform.fail_stream();
    index.start();
    wait_until("unavailable", || index.state() == IndexState::Unavailable);

    let reported = storage(&index);

    assert_eq!(reported.directory, cache_directory(&index));
    assert_eq!(reported.size_bytes, 0);
    assert_eq!(reported.folder_count, None);
    assert_eq!(reported.file_count, None);
}

#[test]
fn rebuild_walks_the_whole_disk_again_and_finds_changes_no_event_reported() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    add_file_without_an_event(&index);
    index.statuses.lock().unwrap().clear();
    index.opened.lock().unwrap().clear();

    rebuild(&index);

    wait_for_rebuild_to_find_the_unreported_file(&index);
    let states: Vec<IndexState> = index
        .statuses
        .lock()
        .unwrap()
        .iter()
        .map(|status| status.state)
        .collect();
    assert!(states.contains(&IndexState::Indexing), "{states:?}");
    assert!(index.opened(""));
}

#[test]
fn rebuild_without_a_running_stream_changes_nothing() {
    let mut index = Harness::new();
    index.platform.fail_stream();
    index.start();
    wait_until("unavailable", || index.state() == IndexState::Unavailable);

    rebuild(&index);

    assert_eq!(index.state(), IndexState::Unavailable);
    assert_eq!(index.platform.stream_starts().len(), 1);
    assert!(!index.opened(""), "no walk starts");
}

#[test]
fn the_storage_and_rebuild_commands_reach_the_attached_index() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);

    let reported = commands::folder_size_index_storage(as_state(index.service()));
    assert_eq!(reported, Ok(storage(&index)));

    add_file_without_an_event(&index);
    assert_eq!(
        commands::rebuild_folder_size_index(as_state(index.service())),
        Ok(())
    );
    wait_for_rebuild_to_find_the_unreported_file(&index);
}
