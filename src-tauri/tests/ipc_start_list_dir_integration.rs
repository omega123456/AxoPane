use std::fs;

use file_explorer_lib::fs::{DirectoryEntry, SortDirection, SortKey};
use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::StartListDirRequest;
use file_explorer_lib::listing::{split_first_chunk, ListingService, LIST_CHUNK_SIZE};
use tempfile::tempdir;

/// Builds a `tauri::State` from a plain reference so the `State`-taking command
/// can be exercised without standing up a full mock runtime (which the coverage
/// alias does not run). Mirrors the helper used by the other aliased IPC
/// integration tests.
fn as_state<T: Send + Sync + 'static>(value: &T) -> tauri::State<'_, T> {
    // Safety: tauri::State is a transparent wrapper over an immutable reference,
    // and `value` outlives the borrow.
    unsafe { std::mem::transmute::<&T, tauri::State<'_, T>>(value) }
}

fn entry(name: &str) -> DirectoryEntry {
    DirectoryEntry {
        id: name.to_string(),
        name: name.to_string(),
        path: format!("C:\\root\\{name}"),
        is_dir: false,
        icon_data_url: None,
        size_bytes: Some(1),
        item_count: None,
        type_label: "File".to_string(),
        modified_at: None,
        created_at: None,
        attributes: Vec::new(),
        is_hidden: false,
        is_system: false,
    }
}

fn entries(count: usize) -> Vec<DirectoryEntry> {
    (0..count).map(|index| entry(&format!("file-{index}"))).collect()
}

#[test]
fn request_ids_are_monotonic_per_tab_and_independent_across_tabs() {
    let service = ListingService::default();

    assert_eq!(service.next_request_id("left-1"), 1);
    assert_eq!(service.next_request_id("left-1"), 2);
    // A second tab keeps its own counter.
    assert_eq!(service.next_request_id("right-1"), 1);
    assert_eq!(service.next_request_id("left-1"), 3);
}

#[test]
fn is_current_only_matches_the_latest_request_for_a_tab() {
    let service = ListingService::default();

    let first = service.next_request_id("left-1");
    assert!(service.is_current("left-1", first));

    // A newer navigation supersedes the earlier request.
    let second = service.next_request_id("left-1");
    assert!(service.is_current("left-1", second));
    assert!(!service.is_current("left-1", first));

    // Unknown tabs / ids are never current.
    assert!(!service.is_current("right-1", 1));
}

#[test]
fn split_first_chunk_keeps_everything_inline_when_it_fits() {
    let (first, rest) = split_first_chunk(entries(3), 5);
    assert_eq!(first.len(), 3);
    assert!(rest.is_empty());

    // Exactly chunk-sized still fits in a single chunk (no streaming).
    let (first, rest) = split_first_chunk(entries(5), 5);
    assert_eq!(first.len(), 5);
    assert!(rest.is_empty());
}

#[test]
fn split_first_chunk_splits_the_remainder_when_it_overflows() {
    let (first, rest) = split_first_chunk(entries(12), 5);
    assert_eq!(first.len(), 5);
    assert_eq!(rest.len(), 7);
    assert_eq!(rest[0].name, "file-5");
    assert_eq!(rest.last().unwrap().name, "file-11");
}

#[test]
fn start_list_dir_returns_a_complete_head_for_a_small_directory() {
    let service = ListingService::default();
    let dir = tempdir().expect("temp dir");
    for index in 0..3 {
        fs::write(dir.path().join(format!("f{index}.txt")), b"x").expect("write file");
    }

    let request = StartListDirRequest {
        tab_id: "left-1".to_string(),
        path: dir.path().to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: false,
        include_item_counts: false,
    };

    let head = commands::start_list_dir(request.clone(), as_state(&service)).expect("start list dir");

    assert_eq!(head.total, 3);
    assert!(head.total as usize <= LIST_CHUNK_SIZE);
    assert!(head.done, "a directory smaller than one chunk is complete at the head");
    assert_eq!(head.first_chunk.len(), 3);
    assert_eq!(head.request_id, 1);

    // A second listing for the same tab bumps the request id (superseding).
    let next = commands::start_list_dir(request, as_state(&service)).expect("start list dir again");
    assert_eq!(next.request_id, 2);
}

#[test]
fn start_list_dir_reports_a_missing_directory_as_an_error() {
    let service = ListingService::default();
    let request = StartListDirRequest {
        tab_id: "left-1".to_string(),
        path: "C:\\definitely\\missing\\path".to_string(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: false,
        include_item_counts: false,
    };

    let result = commands::start_list_dir(request, as_state(&service));
    assert!(result.is_err());
}
