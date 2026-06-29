use std::fs;

use file_explorer_lib::trash::{fake_trash_dir, move_to_trash};
use tempfile::tempdir;

#[test]
fn fake_trash_relocates_sources_under_test_utils() {
    let _ = fs::remove_dir_all(fake_trash_dir());
    let fixture = tempdir().expect("temp dir");
    let file = fixture.path().join("report.txt");
    let dir = fixture.path().join("folder");
    fs::write(&file, b"report").expect("file");
    fs::create_dir(&dir).expect("dir");

    move_to_trash(&[
        file.to_string_lossy().into_owned(),
        dir.to_string_lossy().into_owned(),
    ])
    .expect("move to fake trash");

    assert!(!file.exists());
    assert!(!dir.exists());
    assert!(fake_trash_dir().join("report.txt").exists());
    assert!(fake_trash_dir().join("folder").exists());
}

#[test]
fn fake_trash_noops_empty_requests_and_resolves_name_collisions() {
    let _ = fs::remove_dir_all(fake_trash_dir());
    move_to_trash(&[]).expect("empty trash request");

    let fixture = tempdir().expect("temp dir");
    let first = fixture.path().join("duplicate.txt");
    fs::write(&first, b"first").expect("first");
    move_to_trash(&[first.to_string_lossy().into_owned()]).expect("first move");

    let second = fixture.path().join("duplicate.txt");
    fs::write(&second, b"second").expect("second");
    move_to_trash(&[second.to_string_lossy().into_owned()]).expect("second move");

    assert!(fake_trash_dir().join("duplicate.txt").exists());
    assert!(fake_trash_dir().join("duplicate.txt.1").exists());
}

#[test]
fn fake_trash_surfaces_missing_path_errors() {
    let fixture = tempdir().expect("temp dir");
    let missing = fixture.path().join("missing.txt");

    let error = move_to_trash(&[missing.to_string_lossy().into_owned()]).expect_err("missing path");

    assert!(error.contains("No such file") || error.contains("os error"));
}
