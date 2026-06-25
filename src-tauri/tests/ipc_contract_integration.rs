#[path = "common/mod.rs"]
mod common;

use std::fs;

use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::events;
use file_explorer_lib::ipc::mock;
use file_explorer_lib::ipc::types::{
    CreateEntryRequest, DeleteEntriesRequest, OpenPathRequest, RenameEntryRequest,
};
use tempfile::tempdir;

#[test]
fn exposes_mock_contract_data() {
    let shell = mock::initial_shell();
    assert_eq!(shell.panes.len(), 2);
    assert_eq!(shell.tree_roots[0].label, "This PC");
}

#[test]
fn exposes_event_channel_names() {
    assert_eq!(events::DIR_PATCH, "dir://patch");
    assert_eq!(events::WATCH_ERROR, "watch://error");
    assert_eq!(common::bootstrap_message(), "phase-1-common");
}

#[test]
fn create_rename_delete_commands_round_trip() {
    let fixture = tempdir().expect("temp dir");
    let parent = fixture.path().to_string_lossy().into_owned();

    let folder = commands::create_folder(CreateEntryRequest {
        parent: parent.clone(),
        name: "Docs".to_string(),
    })
    .expect("create folder");
    assert!(folder.is_dir);

    let file = commands::create_file(CreateEntryRequest {
        parent: parent.clone(),
        name: "todo.txt".to_string(),
    })
    .expect("create file");
    assert!(!file.is_dir);

    let renamed = commands::rename_entry(RenameEntryRequest {
        path: file.path.clone(),
        new_name: "done.txt".to_string(),
    })
    .expect("rename");
    assert_eq!(renamed.name, "done.txt");

    commands::delete_entries(DeleteEntriesRequest {
        paths: vec![renamed.path.clone(), folder.path.clone()],
    })
    .expect("delete");
    assert!(!fixture.path().join("done.txt").exists());
    assert!(!fixture.path().join("Docs").exists());
}

#[test]
fn create_command_surfaces_errors_as_strings() {
    let fixture = tempdir().expect("temp dir");
    let parent = fixture.path().to_string_lossy().into_owned();
    fs::create_dir(fixture.path().join("Taken")).expect("seed");

    let error = commands::create_folder(CreateEntryRequest {
        parent,
        name: "Taken".to_string(),
    })
    .expect_err("should conflict");
    assert!(error.contains("Failed to create folder"));
}

#[cfg(feature = "test-utils")]
#[test]
fn open_path_command_uses_safe_test_utils_fallback() {
    let fixture = tempdir().expect("temp dir");
    let file_path = fixture.path().join("report.txt");
    fs::write(&file_path, b"report").expect("seed file");

    let error = commands::open_path(OpenPathRequest {
        path: file_path.to_string_lossy().into_owned(),
    })
    .expect_err("test-utils should block real app launching");

    assert!(error.contains("unsupported"));
}
