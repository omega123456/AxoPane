use std::fs;

use file_explorer_lib::ipc::commands;
use file_explorer_lib::ipc::types::{
    DeleteFromTrashRequest, RestoreTrashRequest, TrashEntriesRequest,
};
use tempfile::tempdir;

/// `commands::list_trash`/`restore_from_trash`/`empty_trash` operate on the
/// shared `fake_trash_dir()` (they can't take a directory override — their
/// signatures must match the real `#[tauri::command]` IPC contract), and
/// `empty_trash` wipes that entire directory. Point the fake trash dir at a
/// private override (`AXOPANE_TEST_FAKE_TRASH_DIR`) for the lifetime of this
/// process so it never races with another test's fixtures in the otherwise
/// shared directory. This is safe because nextest runs every `#[test]` in its
/// own process, so the env var never leaks across tests.
fn use_isolated_fake_trash_dir() -> tempfile::TempDir {
    let override_dir = tempdir().expect("override trash dir");
    std::env::set_var("AXOPANE_TEST_FAKE_TRASH_DIR", override_dir.path());
    override_dir
}

#[test]
fn list_restore_and_empty_round_trip_through_the_command_layer() {
    let _override_dir = use_isolated_fake_trash_dir();

    let source = tempdir().expect("source dir");
    let file = source.path().join("round-trip.txt");
    fs::write(&file, b"payload").expect("file");

    commands::move_to_trash(TrashEntriesRequest {
        paths: vec![file.to_string_lossy().into_owned()],
    })
    .expect("move to trash");

    let listed = commands::list_trash().expect("list trash");
    assert_eq!(listed.entries.len(), 1);
    assert_eq!(listed.entries[0].name, "round-trip.txt");
    assert_eq!(
        listed.entries[0].original_path.as_deref(),
        Some(file.to_string_lossy().as_ref())
    );

    commands::restore_from_trash(RestoreTrashRequest {
        ids: vec![listed.entries[0].id.clone()],
    })
    .expect("restore from trash");
    assert!(file.exists());
    assert!(commands::list_trash()
        .expect("list after restore")
        .entries
        .is_empty());

    commands::move_to_trash(TrashEntriesRequest {
        paths: vec![file.to_string_lossy().into_owned()],
    })
    .expect("move to trash again");
    assert_eq!(
        commands::list_trash()
            .expect("list before empty")
            .entries
            .len(),
        1
    );

    commands::empty_trash().expect("empty trash");
    assert!(commands::list_trash()
        .expect("list after empty")
        .entries
        .is_empty());
}

#[test]
fn restore_from_trash_surfaces_errors_for_unknown_ids() {
    let _override_dir = use_isolated_fake_trash_dir();

    let error = commands::restore_from_trash(RestoreTrashRequest {
        ids: vec!["missing".to_string()],
    })
    .expect_err("unknown id");

    assert!(error.contains("Failed to restore items from trash"));
}

#[test]
fn delete_from_trash_removes_the_item_through_the_command_layer() {
    let _override_dir = use_isolated_fake_trash_dir();

    let source = tempdir().expect("source dir");
    let file = source.path().join("delete-me.txt");
    fs::write(&file, b"payload").expect("file");

    commands::move_to_trash(TrashEntriesRequest {
        paths: vec![file.to_string_lossy().into_owned()],
    })
    .expect("move to trash");

    let listed = commands::list_trash().expect("list trash");
    assert_eq!(listed.entries.len(), 1);

    commands::delete_from_trash(DeleteFromTrashRequest {
        ids: vec![listed.entries[0].id.clone()],
    })
    .expect("delete from trash");

    assert!(commands::list_trash()
        .expect("list after delete")
        .entries
        .is_empty());
}

#[test]
fn delete_from_trash_surfaces_errors_for_unknown_ids() {
    let _override_dir = use_isolated_fake_trash_dir();

    let error = commands::delete_from_trash(DeleteFromTrashRequest {
        ids: vec!["missing".to_string()],
    })
    .expect_err("unknown id");

    assert!(error.contains("Failed to permanently delete items from trash"));
}
