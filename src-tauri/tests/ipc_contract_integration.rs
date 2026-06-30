#[path = "common/mod.rs"]
mod common;

use std::fs;

use file_explorer_lib::fs::{SortDirection, SortKey};
use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::events;
use file_explorer_lib::ipc::mock;
use file_explorer_lib::ipc::types::{
    CreateEntryRequest, FileClipboardMode, ListDirRequest, ListTreeChildrenRequest,
    OpenPathRequest, RenameEntryRequest, TrashEntriesRequest, WriteFileClipboardRequest,
};
use tempfile::tempdir;

#[test]
fn exposes_mock_contract_data() {
    let shell = mock::initial_shell();
    assert_eq!(shell.panes.len(), 2);
    assert_eq!(shell.tree_roots[0].label, "This PC");

    let config = mock::config();
    assert_eq!(config.theme, "system");
    assert!(!config.show_hidden_files);

    let session = mock::session();
    assert_eq!(session.active_pane, "left");
    assert_eq!(session.left_path, "C:\\Users\\Omega");
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

    commands::move_to_trash(TrashEntriesRequest {
        paths: vec![renamed.path.clone(), folder.path.clone()],
    })
    .expect("move to trash");
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

#[test]
fn list_dir_command_surfaces_errors_as_strings() {
    let fixture = tempdir().expect("temp dir");
    let missing = fixture.path().join("missing");

    let error = commands::list_dir(ListDirRequest {
        path: missing.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: false,
    })
    .expect_err("missing directory");

    assert!(error.contains("Failed to load"));
    assert!(error.contains("missing"));
}

#[test]
fn list_tree_children_command_returns_directories_only() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::create_dir(root.join("Docs")).expect("docs");
    fs::write(root.join("todo.txt"), "todo").expect("todo");

    let response = commands::list_tree_children(ListTreeChildrenRequest {
        path: root.to_string_lossy().into_owned(),
        show_hidden: true,
    })
    .expect("tree children");

    assert_eq!(response.children.len(), 1);
    assert_eq!(response.children[0].name, "Docs");
}

#[test]
fn file_rename_and_delete_commands_surface_errors_as_strings() {
    let fixture = tempdir().expect("temp dir");
    let parent = fixture.path().to_string_lossy().into_owned();
    fs::write(fixture.path().join("taken.txt"), "taken").expect("taken");
    fs::write(fixture.path().join("source.txt"), "source").expect("source");

    let create_error = commands::create_file(CreateEntryRequest {
        parent: parent.clone(),
        name: "taken.txt".to_string(),
    })
    .expect_err("create conflict");
    assert!(create_error.contains("Failed to create file"));

    let rename_error = commands::rename_entry(RenameEntryRequest {
        path: fixture
            .path()
            .join("source.txt")
            .to_string_lossy()
            .into_owned(),
        new_name: "taken.txt".to_string(),
    })
    .expect_err("rename conflict");
    assert!(rename_error.contains("Failed to rename"));

    let delete_error = commands::move_to_trash(TrashEntriesRequest {
        paths: vec![fixture
            .path()
            .join("missing.txt")
            .to_string_lossy()
            .into_owned()],
    })
    .expect_err("trash missing");
    assert!(delete_error.contains("Failed to move items to trash"));
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

#[cfg(feature = "test-utils")]
#[test]
fn file_clipboard_commands_are_safe_noops_under_test_utils() {
    commands::write_file_clipboard(WriteFileClipboardRequest {
        mode: FileClipboardMode::Copy,
        paths: vec!["C:\\fixture\\Report.txt".to_string()],
    })
    .expect("clipboard write");

    commands::clear_file_clipboard().expect("clipboard clear");
    commands::noop_dir_patch(file_explorer_lib::watch::DirPatch {
        tab_id: String::new(),
        path: String::new(),
        reason: String::new(),
        changed: Vec::new(),
        removed: Vec::new(),
    });
    commands::noop_watch_error(String::new(), String::new());
}

#[cfg(feature = "test-utils")]
#[test]
fn file_clipboard_command_rejects_blank_paths_before_touching_os_apis() {
    let error = commands::write_file_clipboard(WriteFileClipboardRequest {
        mode: FileClipboardMode::Move,
        paths: vec!["   ".to_string()],
    })
    .expect_err("blank clipboard path");

    assert!(error.contains("Failed to update OS clipboard"));
    assert!(error.contains("must not be empty"));
}

#[test]
fn open_path_command_reports_missing_path_before_launching() {
    let fixture = tempdir().expect("temp dir");
    let missing = fixture.path().join("missing.txt");

    let error = commands::open_path(OpenPathRequest {
        path: missing.to_string_lossy().into_owned(),
    })
    .expect_err("missing path");

    assert!(error.contains("Failed to open"));
}

#[test]
fn list_tree_children_command_surfaces_errors_as_strings() {
    let fixture = tempdir().expect("temp dir");
    let missing = fixture.path().join("missing");

    let error = commands::list_tree_children(ListTreeChildrenRequest {
        path: missing.to_string_lossy().into_owned(),
        show_hidden: false,
    })
    .expect_err("missing tree path");

    assert!(error.contains("Failed to load tree children"));
}
