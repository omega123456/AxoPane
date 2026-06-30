use std::fs;

use file_explorer_lib::trash::{
    ensure_fake_trash_dir_for_tests, fake_trash_dir, move_to_fake_trash_path_for_tests,
    move_to_trash,
};
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

    let duplicate_entries = fs::read_dir(fake_trash_dir())
        .expect("fake trash entries")
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name == "duplicate.txt" || name.starts_with("duplicate.txt."))
        .count();
    assert!(duplicate_entries >= 2);
}

#[test]
fn fake_trash_surfaces_missing_path_errors() {
    let fixture = tempdir().expect("temp dir");
    let missing = fixture.path().join("missing.txt");

    let error = move_to_trash(&[missing.to_string_lossy().into_owned()]).expect_err("missing path");

    assert!(error.contains("No such file") || error.contains("os error"));
}

#[test]
fn fake_trash_path_helper_moves_a_file_directly() {
    let fixture = tempdir().expect("temp dir");
    let source = fixture.path().join("source.txt");
    let target = fixture.path().join("target.txt");
    fs::write(&source, b"payload").expect("source");

    move_to_fake_trash_path_for_tests(&source, &target).expect("direct move");

    assert!(!source.exists());
    assert!(target.exists());
}

#[test]
fn fake_trash_dir_helper_creates_nested_directories() {
    let fixture = tempdir().expect("temp dir");
    let nested = fixture.path().join("a").join("b");

    ensure_fake_trash_dir_for_tests(&nested).expect("ensure nested");

    assert!(nested.is_dir());
}
