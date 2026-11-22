use std::fs;

use axopane_lib::ipc::commands;
use axopane_lib::ipc::types::{DeleteFromTrashRequest, RestoreTrashRequest, TrashEntriesRequest};
use axopane_lib::trash::set_fake_trash_dir_for_tests;
use tempfile::tempdir;

// nextest runs each test in its own process, so this override never reaches another test.
fn use_isolated_fake_trash_dir() -> tempfile::TempDir {
    let override_dir = tempdir().expect("override trash dir");
    set_fake_trash_dir_for_tests(override_dir.path());
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
    assert!(
        commands::list_trash()
            .expect("list after restore")
            .entries
            .is_empty()
    );

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
    assert!(
        commands::list_trash()
            .expect("list after empty")
            .entries
            .is_empty()
    );
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

    assert!(
        commands::list_trash()
            .expect("list after delete")
            .entries
            .is_empty()
    );
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

#[test]
fn list_trash_surfaces_errors_when_the_fake_trash_override_is_a_file() {
    let fixture = tempdir().expect("temp dir");
    let blocker = fixture.path().join("not-a-directory");
    fs::write(&blocker, b"blocker").expect("blocker file");
    set_fake_trash_dir_for_tests(&blocker);

    let error = commands::list_trash().expect_err("file override should fail");

    assert!(error.contains("Failed to list trash"));
}
