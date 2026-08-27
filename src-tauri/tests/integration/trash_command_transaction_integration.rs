use std::fs;

use file_explorer_lib::trash::{
    delete_from_trash_for_tests, empty_trash_for_tests, list_trash_for_tests,
    move_to_trash_into_for_tests, restore_from_trash_for_tests,
};
use tempfile::tempdir;

fn block_manifest_commit(trash_dir: &std::path::Path) {
    fs::create_dir(trash_dir.join(".axopane-trash-manifest.json.tmp"))
        .expect("make the atomic manifest temp path unavailable");
}

#[test]
fn restore_loads_once_and_defers_its_single_manifest_commit_until_the_batch_finishes() {
    let sources = tempdir().expect("source directory");
    let trash = tempdir().expect("trash directory");
    let first = sources.path().join("first.txt");
    let second = sources.path().join("second.txt");
    fs::write(&first, b"one").expect("first source");
    fs::write(&second, b"two").expect("second source");
    move_to_trash_into_for_tests(
        &[
            first.to_string_lossy().into_owned(),
            second.to_string_lossy().into_owned(),
        ],
        trash.path(),
    )
    .expect("populate fake trash");
    block_manifest_commit(trash.path());

    let error = restore_from_trash_for_tests(
        &["first.txt".to_string(), "second.txt".to_string()],
        trash.path(),
    )
    .expect_err("the sole final manifest commit is blocked");

    assert!(!error.is_empty());
    assert!(first.exists());
    assert!(second.exists());
    assert!(!trash.path().join("first.txt").exists());
    assert!(!trash.path().join("second.txt").exists());
}

#[test]
fn purge_and_empty_each_finish_filesystem_work_before_one_manifest_commit() {
    let sources = tempdir().expect("source directory");
    let trash = tempdir().expect("trash directory");
    let first = sources.path().join("first.txt");
    let second = sources.path().join("second.txt");
    fs::write(&first, b"one").expect("first source");
    fs::write(&second, b"two").expect("second source");
    move_to_trash_into_for_tests(
        &[
            first.to_string_lossy().into_owned(),
            second.to_string_lossy().into_owned(),
        ],
        trash.path(),
    )
    .expect("populate fake trash");
    block_manifest_commit(trash.path());

    delete_from_trash_for_tests(
        &["first.txt".to_string(), "second.txt".to_string()],
        trash.path(),
    )
    .expect_err("the sole final manifest commit is blocked");
    assert!(!trash.path().join("first.txt").exists());
    assert!(!trash.path().join("second.txt").exists());

    fs::remove_dir(trash.path().join(".axopane-trash-manifest.json.tmp"))
        .expect("unblock manifest commits");
    empty_trash_for_tests(trash.path()).expect("empty already-purged trash");
    assert!(list_trash_for_tests(trash.path())
        .expect("list fake trash")
        .is_empty());
}

#[test]
fn empty_removes_file_and_directory_entries_before_clearing_the_manifest() {
    let sources = tempdir().expect("source directory");
    let trash = tempdir().expect("trash directory");
    let file = sources.path().join("file.txt");
    let directory = sources.path().join("folder");
    fs::write(&file, b"file").expect("file source");
    fs::create_dir(&directory).expect("directory source");
    fs::write(directory.join("nested.txt"), b"nested").expect("nested source");

    move_to_trash_into_for_tests(
        &[
            file.to_string_lossy().into_owned(),
            directory.to_string_lossy().into_owned(),
        ],
        trash.path(),
    )
    .expect("populate fake trash");

    empty_trash_for_tests(trash.path()).expect("empty fake trash");

    assert!(list_trash_for_tests(trash.path())
        .expect("list emptied trash")
        .is_empty());
    assert!(!trash.path().join("file.txt").exists());
    assert!(!trash.path().join("folder").exists());
}
