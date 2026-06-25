use std::fs;

use file_explorer_lib::fs::{
    create_directory, create_file, delete_entries, rename_entry, FsError,
};
use tempfile::tempdir;

#[test]
fn creates_a_folder_and_reports_its_entry() {
    let fixture = tempdir().expect("temp dir");
    let parent = fixture.path().to_string_lossy().into_owned();

    let entry = create_directory(&parent, "Reports").expect("create dir");

    assert!(entry.is_dir);
    assert_eq!(entry.name, "Reports");
    assert_eq!(entry.type_label, "Folder");
    assert!(fixture.path().join("Reports").is_dir());
}

#[test]
fn creates_an_empty_file_and_reports_its_entry() {
    let fixture = tempdir().expect("temp dir");
    let parent = fixture.path().to_string_lossy().into_owned();

    let entry = create_file(&parent, "notes.txt").expect("create file");

    assert!(!entry.is_dir);
    assert_eq!(entry.name, "notes.txt");
    assert_eq!(entry.size_bytes, Some(0));
    assert_eq!(entry.type_label, "TXT file");
    assert!(fixture.path().join("notes.txt").is_file());
}

#[test]
fn refuses_to_create_over_an_existing_item() {
    let fixture = tempdir().expect("temp dir");
    let parent = fixture.path().to_string_lossy().into_owned();
    fs::create_dir(fixture.path().join("Taken")).expect("seed dir");

    let error = create_directory(&parent, "Taken").expect_err("should conflict");
    assert!(matches!(error, FsError::AlreadyExists(_)));

    fs::write(fixture.path().join("taken.txt"), "x").expect("seed file");
    let file_error = create_file(&parent, "taken.txt").expect_err("should conflict");
    assert!(matches!(file_error, FsError::AlreadyExists(_)));
}

#[test]
fn rejects_invalid_names() {
    let fixture = tempdir().expect("temp dir");
    let parent = fixture.path().to_string_lossy().into_owned();

    for invalid in ["", "  ", ".", "..", "a/b", "a\\b"] {
        let error = create_directory(&parent, invalid).expect_err("invalid name");
        assert!(matches!(error, FsError::InvalidName(_)), "name {invalid:?}");
    }
}

#[test]
fn renames_an_entry_in_place() {
    let fixture = tempdir().expect("temp dir");
    let original = fixture.path().join("old.txt");
    fs::write(&original, "data").expect("seed file");

    let entry = rename_entry(&original.to_string_lossy(), "new.txt").expect("rename");

    assert_eq!(entry.name, "new.txt");
    assert!(!original.exists());
    assert!(fixture.path().join("new.txt").is_file());
}

#[test]
fn renaming_to_the_same_name_is_a_noop() {
    let fixture = tempdir().expect("temp dir");
    let original = fixture.path().join("keep.txt");
    fs::write(&original, "data").expect("seed file");

    let entry = rename_entry(&original.to_string_lossy(), "keep.txt").expect("rename");
    assert_eq!(entry.name, "keep.txt");
    assert!(original.is_file());
}

#[test]
fn refuses_to_rename_over_an_existing_item() {
    let fixture = tempdir().expect("temp dir");
    let source = fixture.path().join("a.txt");
    fs::write(&source, "a").expect("seed a");
    fs::write(fixture.path().join("b.txt"), "b").expect("seed b");

    let error = rename_entry(&source.to_string_lossy(), "b.txt").expect_err("conflict");
    assert!(matches!(error, FsError::AlreadyExists(_)));
}

#[test]
fn rejects_invalid_rename_targets() {
    let fixture = tempdir().expect("temp dir");
    let source = fixture.path().join("a.txt");
    fs::write(&source, "a").expect("seed a");

    let error = rename_entry(&source.to_string_lossy(), "nested/name").expect_err("invalid");
    assert!(matches!(error, FsError::InvalidName(_)));
}

#[test]
fn deletes_files_and_directories_recursively() {
    let fixture = tempdir().expect("temp dir");
    let file = fixture.path().join("doomed.txt");
    fs::write(&file, "x").expect("seed file");
    let dir = fixture.path().join("tree");
    fs::create_dir(&dir).expect("seed dir");
    fs::write(dir.join("child.txt"), "y").expect("seed child");

    delete_entries(&[
        file.to_string_lossy().into_owned(),
        dir.to_string_lossy().into_owned(),
    ])
    .expect("delete");

    assert!(!file.exists());
    assert!(!dir.exists());
}

#[test]
fn deleting_a_missing_path_is_an_error() {
    let fixture = tempdir().expect("temp dir");
    let missing = fixture.path().join("ghost.txt");

    let error = delete_entries(&[missing.to_string_lossy().into_owned()]).expect_err("missing");
    assert!(matches!(error, FsError::Io(_)));
}
