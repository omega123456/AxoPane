use std::fs;

use file_explorer_lib::trash::{
    delete_from_trash_for_tests, empty_trash_for_tests, ensure_fake_trash_dir_for_tests,
    list_trash_for_tests, move_to_trash_into_for_tests, restore_from_trash_for_tests,
};
use tempfile::tempdir;

/// Each test gets its own isolated trash directory (rather than the shared
/// `fake_trash_dir()` other trash tests use) because `list`/`empty` operate
/// on the *entire* directory contents, which would otherwise race with any
/// other test concurrently populating the shared fake trash dir.
fn isolated_trash_dir() -> tempfile::TempDir {
    tempdir().expect("isolated trash dir")
}

#[test]
fn list_trash_reflects_items_moved_in_with_original_path_and_kind() {
    let source_fixture = tempdir().expect("source dir");
    let trash_dir = isolated_trash_dir();

    let file = source_fixture.path().join("notes.txt");
    let folder = source_fixture.path().join("archive");
    fs::write(&file, b"payload").expect("file");
    fs::create_dir(&folder).expect("dir");

    move_to_trash_into_for_tests(
        &[
            file.to_string_lossy().into_owned(),
            folder.to_string_lossy().into_owned(),
        ],
        trash_dir.path(),
    )
    .expect("move into isolated trash");

    let mut entries = list_trash_for_tests(trash_dir.path()).expect("list trash");
    entries.sort_by(|left, right| left.name.cmp(&right.name));

    assert_eq!(entries.len(), 2);

    let folder_entry = entries
        .iter()
        .find(|entry| entry.name == "archive")
        .unwrap();
    assert!(folder_entry.is_dir);
    assert_eq!(
        folder_entry.original_path.as_deref(),
        Some(folder.to_string_lossy().as_ref())
    );
    assert!(folder_entry.deleted_at.is_some());

    let file_entry = entries
        .iter()
        .find(|entry| entry.name == "notes.txt")
        .unwrap();
    assert!(!file_entry.is_dir);
    assert_eq!(file_entry.size_bytes, Some(7));
    assert_eq!(
        file_entry.original_path.as_deref(),
        Some(file.to_string_lossy().as_ref())
    );
}

#[test]
fn list_trash_on_a_missing_directory_returns_an_empty_list() {
    let trash_dir = isolated_trash_dir();
    let never_created = trash_dir.path().join("does-not-exist");

    let entries = list_trash_for_tests(&never_created).expect("list trash");

    assert!(entries.is_empty());
}

#[cfg(unix)]
#[test]
fn list_trash_keeps_opening_when_one_entry_has_no_metadata() {
    use std::os::unix::fs::symlink;

    let trash_dir = isolated_trash_dir();
    fs::create_dir_all(trash_dir.path()).expect("trash dir");
    fs::write(trash_dir.path().join("visible.txt"), b"payload").expect("visible entry");
    symlink(
        trash_dir.path().join("missing-target"),
        trash_dir.path().join("orphan-link"),
    )
    .expect("broken symlink");

    let mut entries = list_trash_for_tests(trash_dir.path()).expect("list trash");
    entries.sort_by(|left, right| left.name.cmp(&right.name));

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "orphan-link");
    assert!(!entries[0].is_dir);
    assert_eq!(entries[0].size_bytes, None);
    assert_eq!(entries[1].name, "visible.txt");
    assert_eq!(entries[1].size_bytes, Some(7));
}

#[test]
fn restore_from_trash_moves_the_item_back_and_removes_it_from_the_listing() {
    let source_fixture = tempdir().expect("source dir");
    let trash_dir = isolated_trash_dir();

    let file = source_fixture.path().join("restore-me.txt");
    fs::write(&file, b"payload").expect("file");
    move_to_trash_into_for_tests(&[file.to_string_lossy().into_owned()], trash_dir.path())
        .expect("move into isolated trash");

    let entries = list_trash_for_tests(trash_dir.path()).expect("list trash");
    assert_eq!(entries.len(), 1);
    let id = entries[0].id.clone();

    restore_from_trash_for_tests(&[id], trash_dir.path()).expect("restore");

    assert!(file.exists());
    let entries_after = list_trash_for_tests(trash_dir.path()).expect("list trash after restore");
    assert!(entries_after.is_empty());
}

#[test]
fn restore_from_trash_rejects_a_collision_at_the_original_path() {
    let source_fixture = tempdir().expect("source dir");
    let trash_dir = isolated_trash_dir();

    let file = source_fixture.path().join("collide.txt");
    fs::write(&file, b"payload").expect("file");
    move_to_trash_into_for_tests(&[file.to_string_lossy().into_owned()], trash_dir.path())
        .expect("move into isolated trash");

    // Something new now occupies the original path.
    fs::write(&file, b"someone else's file").expect("recreate at original path");

    let entries = list_trash_for_tests(trash_dir.path()).expect("list trash");
    let id = entries[0].id.clone();

    let error = restore_from_trash_for_tests(&[id], trash_dir.path()).expect_err("collision");
    assert!(error.contains("already exists"));
}

#[test]
fn restore_from_trash_rejects_an_unknown_id() {
    let trash_dir = isolated_trash_dir();

    let error = restore_from_trash_for_tests(&["missing-id".to_string()], trash_dir.path())
        .expect_err("unknown id");

    assert!(error.contains("no known original location"));
}

#[test]
fn empty_trash_removes_every_item_and_the_manifest() {
    let source_fixture = tempdir().expect("source dir");
    let trash_dir = isolated_trash_dir();

    let file = source_fixture.path().join("gone.txt");
    let folder = source_fixture.path().join("gone-dir");
    fs::write(&file, b"payload").expect("file");
    fs::create_dir(&folder).expect("dir");

    move_to_trash_into_for_tests(
        &[
            file.to_string_lossy().into_owned(),
            folder.to_string_lossy().into_owned(),
        ],
        trash_dir.path(),
    )
    .expect("move into isolated trash");

    empty_trash_for_tests(trash_dir.path()).expect("empty trash");

    let entries = list_trash_for_tests(trash_dir.path()).expect("list trash after empty");
    assert!(entries.is_empty());

    // The manifest sidecar file itself may remain (cleared of entries) —
    // only the trashed items themselves must be gone.
    let remaining: Vec<_> = fs::read_dir(trash_dir.path())
        .expect("read trash dir")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| !name.starts_with(".axopane-trash-manifest"))
        })
        .collect();
    assert!(remaining.is_empty());
}

#[test]
fn empty_trash_on_a_missing_directory_is_a_no_op() {
    let trash_dir = isolated_trash_dir();
    let never_created = trash_dir.path().join("does-not-exist");

    empty_trash_for_tests(&never_created).expect("empty missing trash");
}

#[test]
fn delete_from_trash_removes_only_the_targeted_item() {
    let source_fixture = tempdir().expect("source dir");
    let trash_dir = isolated_trash_dir();

    let keep = source_fixture.path().join("keep.txt");
    let purge = source_fixture.path().join("purge.txt");
    fs::write(&keep, b"payload").expect("file");
    fs::write(&purge, b"payload").expect("file");

    move_to_trash_into_for_tests(
        &[
            keep.to_string_lossy().into_owned(),
            purge.to_string_lossy().into_owned(),
        ],
        trash_dir.path(),
    )
    .expect("move into isolated trash");

    delete_from_trash_for_tests(&["purge.txt".to_string()], trash_dir.path())
        .expect("delete from trash");

    let entries = list_trash_for_tests(trash_dir.path()).expect("list trash after delete");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "keep.txt");
    assert!(!trash_dir.path().join("purge.txt").exists());
}

#[test]
fn delete_from_trash_rejects_an_unknown_id() {
    let trash_dir = isolated_trash_dir();

    let error = delete_from_trash_for_tests(&["missing-id".to_string()], trash_dir.path())
        .expect_err("unknown id");

    assert!(error.contains("no longer in the trash"));
}

#[test]
fn ensure_fake_trash_dir_surfaces_create_dir_all_errors_when_the_path_is_a_file() {
    let fixture = tempdir().expect("temp dir");
    let blocker = fixture.path().join("blocker");
    fs::write(&blocker, b"not a directory").expect("blocker file");

    let error = ensure_fake_trash_dir_for_tests(&blocker).expect_err("path is a plain file");
    assert!(!error.is_empty());
}

/// `manifest::remove`'s `load_json_or_default` call fails when the sidecar
/// file exists but holds invalid JSON. `delete_from_trash_in` propagates that
/// failure (unlike `record`, which is best-effort), so it's the most direct
/// way to exercise that closure without reaching into the private `manifest`
/// module (which isn't exported outside `trash/mod.rs`).
#[test]
fn delete_from_trash_surfaces_manifest_load_errors_from_a_corrupt_sidecar_file() {
    let trash_dir = isolated_trash_dir();
    fs::create_dir_all(trash_dir.path()).expect("trash dir");
    fs::write(trash_dir.path().join("purge.txt"), b"payload").expect("item");
    fs::write(
        trash_dir.path().join(".axopane-trash-manifest.json"),
        b"not json",
    )
    .expect("corrupt manifest");

    let error = delete_from_trash_for_tests(&["purge.txt".to_string()], trash_dir.path())
        .expect_err("corrupt manifest should surface as an error");
    assert!(!error.is_empty());
    // The physical delete still happens before the manifest bookkeeping runs.
    assert!(!trash_dir.path().join("purge.txt").exists());
}

/// `write_json_atomic` writes to a `.tmp` sibling before renaming it over the
/// real manifest path. Pre-creating that `.tmp` path as a directory makes the
/// write fail deterministically (no permissions trickery needed), exercising
/// `manifest::remove`'s second (write) error closure.
#[test]
fn delete_from_trash_surfaces_manifest_write_errors_when_the_tmp_path_is_a_directory() {
    let trash_dir = isolated_trash_dir();
    fs::create_dir_all(trash_dir.path()).expect("trash dir");
    fs::write(trash_dir.path().join("purge.txt"), b"payload").expect("item");
    fs::create_dir_all(trash_dir.path().join(".axopane-trash-manifest.json.tmp"))
        .expect("tmp collision directory");

    let error = delete_from_trash_for_tests(&["purge.txt".to_string()], trash_dir.path())
        .expect_err("manifest write collision should surface as an error");
    assert!(!error.is_empty());
    assert!(!trash_dir.path().join("purge.txt").exists());
}

/// Same `.tmp`-path-is-a-directory trick, this time reached through
/// `empty_trash_in` with nothing left to remove, so it exercises
/// `manifest::clear`'s write closure specifically.
#[test]
fn empty_trash_surfaces_manifest_write_errors_when_the_tmp_path_is_a_directory() {
    let trash_dir = isolated_trash_dir();
    fs::create_dir_all(trash_dir.path()).expect("trash dir");
    fs::create_dir_all(trash_dir.path().join(".axopane-trash-manifest.json.tmp"))
        .expect("tmp collision directory");

    let error =
        empty_trash_for_tests(trash_dir.path()).expect_err("manifest clear write collision");
    assert!(!error.is_empty());
}

/// `move_to_trash_impl_in` treats a manifest write failure as best-effort: the
/// item still lands in the trash even though `manifest::record`'s
/// `load_json_or_default` call fails on a corrupt sidecar file.
#[test]
fn move_to_trash_still_relocates_the_item_when_the_manifest_record_fails() {
    let source_fixture = tempdir().expect("source dir");
    let trash_dir = isolated_trash_dir();
    fs::create_dir_all(trash_dir.path()).expect("trash dir");
    fs::write(
        trash_dir.path().join(".axopane-trash-manifest.json"),
        b"not json",
    )
    .expect("corrupt manifest");

    let file = source_fixture.path().join("report.txt");
    fs::write(&file, b"payload").expect("file");

    move_to_trash_into_for_tests(&[file.to_string_lossy().into_owned()], trash_dir.path())
        .expect("move still succeeds despite manifest write failure");

    assert!(!file.exists());
    assert!(trash_dir.path().join("report.txt").exists());
}

#[cfg(unix)]
fn deny_all(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o000)).is_ok()
}

#[cfg(unix)]
fn restore_rwx(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o755));
}

#[cfg(unix)]
#[test]
fn list_trash_surfaces_read_dir_permission_errors() {
    let trash_dir = isolated_trash_dir();
    fs::create_dir_all(trash_dir.path()).expect("trash dir");

    if !deny_all(trash_dir.path()) {
        return;
    }

    let result = list_trash_for_tests(trash_dir.path());
    restore_rwx(trash_dir.path());

    assert!(result.is_err(), "unreadable trash dir should error");
}

#[cfg(unix)]
#[test]
fn empty_trash_surfaces_read_dir_permission_errors() {
    let trash_dir = isolated_trash_dir();
    fs::create_dir_all(trash_dir.path()).expect("trash dir");

    if !deny_all(trash_dir.path()) {
        return;
    }

    let result = empty_trash_for_tests(trash_dir.path());
    restore_rwx(trash_dir.path());

    assert!(result.is_err(), "unreadable trash dir should error");
}

/// `restore_from_trash_in`'s `create_dir_all(parent)` call fails when the
/// original path's parent directory has since been replaced by a plain file.
#[test]
fn restore_from_trash_surfaces_create_dir_all_errors_when_the_parent_is_a_file() {
    let source_fixture = tempdir().expect("source dir");
    let trash_dir = isolated_trash_dir();

    let original_parent = source_fixture.path().join("will-be-blocked");
    fs::create_dir_all(&original_parent).expect("original parent");
    let file = original_parent.join("restore-me.txt");
    fs::write(&file, b"payload").expect("file");

    move_to_trash_into_for_tests(&[file.to_string_lossy().into_owned()], trash_dir.path())
        .expect("move into isolated trash");

    // Replace the (now empty) original parent directory with a plain file so
    // `create_dir_all` can no longer recreate it on restore.
    fs::remove_dir(&original_parent).expect("remove now-empty original parent");
    fs::write(&original_parent, b"not a directory anymore").expect("blocker file");

    let entries = list_trash_for_tests(trash_dir.path()).expect("list trash");
    let id = entries[0].id.clone();

    let error =
        restore_from_trash_for_tests(&[id], trash_dir.path()).expect_err("parent is now a file");
    assert!(!error.is_empty());
}

#[cfg(unix)]
#[test]
fn restore_from_trash_surfaces_rename_errors_when_the_parent_is_read_only() {
    use std::os::unix::fs::PermissionsExt;

    let source_fixture = tempdir().expect("source dir");
    let trash_dir = isolated_trash_dir();
    let restore_parent = source_fixture.path().join("restore-parent");
    fs::create_dir_all(&restore_parent).expect("restore parent");

    let file = restore_parent.join("restore-me.txt");
    fs::write(&file, b"payload").expect("file");
    move_to_trash_into_for_tests(&[file.to_string_lossy().into_owned()], trash_dir.path())
        .expect("move into isolated trash");

    let entries = list_trash_for_tests(trash_dir.path()).expect("list trash");
    let id = entries[0].id.clone();

    if !deny_all(&restore_parent) {
        return;
    }

    let result = restore_from_trash_for_tests(&[id], trash_dir.path());
    fs::set_permissions(&restore_parent, fs::Permissions::from_mode(0o755))
        .expect("restore parent permissions");

    assert!(
        result.is_err(),
        "renaming into a read-only parent should fail"
    );
}

#[cfg(unix)]
#[test]
fn delete_from_trash_surfaces_removal_errors_when_the_trash_dir_is_read_only() {
    let source_fixture = tempdir().expect("source dir");
    let trash_dir = isolated_trash_dir();

    let file = source_fixture.path().join("locked-purge.txt");
    let folder = source_fixture.path().join("locked-purge-dir");
    fs::write(&file, b"payload").expect("file");
    fs::create_dir(&folder).expect("dir");
    move_to_trash_into_for_tests(
        &[
            file.to_string_lossy().into_owned(),
            folder.to_string_lossy().into_owned(),
        ],
        trash_dir.path(),
    )
    .expect("move into isolated trash");

    if !deny_all(trash_dir.path()) {
        return;
    }

    let file_result =
        delete_from_trash_for_tests(&["locked-purge.txt".to_string()], trash_dir.path());
    let dir_result =
        delete_from_trash_for_tests(&["locked-purge-dir".to_string()], trash_dir.path());
    restore_rwx(trash_dir.path());

    assert!(file_result.is_err(), "removing a file needs write access");
    assert!(dir_result.is_err(), "removing a dir needs write access");
}
