use std::fs;

use file_explorer_lib::fs::{create_directory, create_file, rename_entry, FsError};
use tempfile::tempdir;

#[test]
fn create_operations_trim_user_supplied_names_before_writing() {
    let fixture = tempdir().expect("temp dir");
    let parent = fixture.path().to_string_lossy().into_owned();

    let dir = create_directory(&parent, "  Trimmed Folder  ").expect("create dir");
    let file = create_file(&parent, "  trimmed.txt  ").expect("create file");

    assert_eq!(dir.name, "Trimmed Folder");
    assert_eq!(file.name, "trimmed.txt");
    assert!(fixture.path().join("Trimmed Folder").is_dir());
    assert!(fixture.path().join("trimmed.txt").is_file());
}

#[test]
fn rename_entry_rejects_root_paths_without_a_renameable_file_name() {
    let root = if cfg!(windows) { "C:\\" } else { "/" };

    let error = rename_entry(root, "renamed").expect_err("root rename should fail");

    assert!(matches!(error, FsError::InvalidFileName(_)));
}

#[test]
fn rename_entry_trims_the_new_name_before_moving() {
    let fixture = tempdir().expect("temp dir");
    let original = fixture.path().join("before.txt");
    fs::write(&original, "data").expect("seed file");

    let entry = rename_entry(&original.to_string_lossy(), "  after.txt  ").expect("rename");

    assert_eq!(entry.name, "after.txt");
    assert!(!original.exists());
    assert!(fixture.path().join("after.txt").is_file());
}
