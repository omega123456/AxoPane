use std::fs;

use file_explorer_lib::trash::{
    ensure_fake_trash_dir_for_tests, fake_trash_dir, move_to_fake_trash_path_for_tests,
    move_to_trash,
};
use tempfile::tempdir;

/// `fake_trash_dir()` is one fixed directory shared by every test process
/// that exercises trash behavior (this file, `trash_fake_edges_integration`,
/// and several `ipc_*_integration` files), and nextest runs test binaries —
/// and tests within a binary — concurrently by default. The previous version
/// of these tests called `fs::remove_dir_all(fake_trash_dir())` and then
/// asserted on fixed file names ("report.txt", "duplicate.txt"), which raced
/// against any other test concurrently writing into (or wiping) that same
/// directory: one test's wipe could delete another's in-flight fixture, or
/// two tests' identically-named fixtures could collide. Deriving a
/// per-test-run unique name from `tempdir()`'s own randomized path (instead
/// of wiping the shared directory or using fixed names) makes every
/// assertion identify only files this test instance created, so concurrent
/// runs can never interfere with each other.
fn unique_token(fixture: &tempfile::TempDir) -> String {
    fixture
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .expect("tempdir yields a named path")
        .to_string()
}

#[test]
fn fake_trash_relocates_sources_under_test_utils() {
    let fixture = tempdir().expect("temp dir");
    let token = unique_token(&fixture);
    let file_name = format!("report-{token}.txt");
    let dir_name = format!("folder-{token}");
    let file = fixture.path().join(&file_name);
    let dir = fixture.path().join(&dir_name);
    fs::write(&file, b"report").expect("file");
    fs::create_dir(&dir).expect("dir");

    move_to_trash(&[
        file.to_string_lossy().into_owned(),
        dir.to_string_lossy().into_owned(),
    ])
    .expect("move to fake trash");

    assert!(!file.exists());
    assert!(!dir.exists());
    assert!(fake_trash_dir().join(&file_name).exists());
    assert!(fake_trash_dir().join(&dir_name).exists());
}

#[test]
fn fake_trash_noops_empty_requests_and_resolves_name_collisions() {
    move_to_trash(&[]).expect("empty trash request");

    let fixture = tempdir().expect("temp dir");
    let duplicate_name = format!("duplicate-{}.txt", unique_token(&fixture));

    let first = fixture.path().join(&duplicate_name);
    fs::write(&first, b"first").expect("first");
    move_to_trash(&[first.to_string_lossy().into_owned()]).expect("first move");

    let second = fixture.path().join(&duplicate_name);
    fs::write(&second, b"second").expect("second");
    move_to_trash(&[second.to_string_lossy().into_owned()]).expect("second move");

    let collision_suffix = format!("{duplicate_name}.");
    let duplicate_entries = fs::read_dir(fake_trash_dir())
        .expect("fake trash entries")
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| *name == duplicate_name || name.starts_with(&collision_suffix))
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
