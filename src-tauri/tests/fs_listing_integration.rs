#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::Path;

use file_explorer_lib::fs::{
    canonicalize_dir, default_start_dir, display_path_from_text, list_dir, ListDirOptions,
    SortDirection, SortKey,
};
use tempfile::tempdir;

#[test]
fn lists_sorted_filtered_entries_with_metadata() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();

    fs::create_dir(root.join("folder1")).expect("folder1");
    fs::create_dir(root.join("folder10")).expect("folder10");
    fs::write(root.join("file10.txt"), "ten").expect("file10");
    fs::write(root.join("file2.txt"), "two").expect("file2");
    fs::write(root.join(".hidden.txt"), "hidden").expect("hidden");

    let response = list_dir(&ListDirOptions {
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: "file".to_string(),
        show_hidden: false,
    })
    .expect("list dir");

    let names: Vec<_> = response
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(names, vec!["file2.txt", "file10.txt"]);

    let file_entry = &response.entries[0];
    assert!(!file_entry.is_dir);
    assert_eq!(file_entry.size_bytes, Some(3));
    assert_eq!(file_entry.item_count, None);
    assert_eq!(file_entry.type_label, "TXT file");
    assert!(file_entry.modified_at.is_some());
    assert!(file_entry.created_at.is_some() || cfg!(target_os = "linux"));
}

#[test]
fn keeps_folders_first_and_respects_hidden_toggle() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();

    fs::create_dir(root.join("beta2")).expect("beta2");
    fs::create_dir(root.join("beta10")).expect("beta10");
    fs::write(root.join("alpha2.txt"), "a").expect("alpha2");
    fs::write(root.join("alpha10.txt"), "b").expect("alpha10");
    fs::write(root.join(".secret"), "s").expect("secret");

    let hidden_filtered = list_dir(&ListDirOptions {
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: false,
    })
    .expect("list dir without hidden");

    let visible_names: Vec<_> = hidden_filtered
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(
        visible_names,
        vec!["beta2", "beta10", "alpha2.txt", "alpha10.txt"]
    );
    assert!(hidden_filtered.entries.iter().all(|entry| !entry.is_hidden));
    assert_eq!(hidden_filtered.entries[0].item_count, Some(0));

    let hidden_included = list_dir(&ListDirOptions {
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
    })
    .expect("list dir with hidden");

    assert!(hidden_included
        .entries
        .iter()
        .any(|entry| entry.name == ".secret"));
    let hidden_entry = hidden_included
        .entries
        .iter()
        .find(|entry| entry.name == ".secret")
        .expect("hidden entry");
    assert!(hidden_entry.is_hidden);
    assert!(hidden_entry
        .attributes
        .iter()
        .any(|attribute| attribute == "hidden"));
    assert_eq!(common::bootstrap_message(), "phase-1-common");
}

#[test]
fn list_dir_returns_canonical_absolute_path() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::create_dir(root.join("child")).expect("child");

    // A non-canonical request (with a redundant "." segment) must still resolve
    // to the canonical absolute directory in the response.
    let requested = root.join(".").to_string_lossy().into_owned();
    let response = list_dir(&ListDirOptions {
        path: requested,
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: false,
    })
    .expect("list dir");

    let expected = canonicalize_dir(root).expect("canonical root");
    assert_eq!(Path::new(&response.path), expected.as_path());
    assert!(Path::new(&response.path).is_absolute());
    assert!(!response.path.starts_with("\\\\?\\"));
    assert_eq!(response.entries.len(), 1);
    assert_eq!(response.entries[0].name, "child");
}

#[test]
fn list_dir_rejects_relative_dot_without_real_directory() {
    let fixture = tempdir().expect("temp dir");
    let missing = fixture.path().join("does-not-exist");

    let error = list_dir(&ListDirOptions {
        path: missing.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: false,
    })
    .expect_err("missing directory should error");

    // The error must carry a useful message rather than an opaque value.
    assert!(!error.to_string().is_empty());
}

/// Regression test for drive roots / home folders failing to load.
///
/// Every NTFS drive root contains access-denied folders (`System Volume
/// Information`, `$RECYCLE.BIN`) and home folders contain access-denied legacy
/// junctions. Counting their children must never abort the parent listing: the
/// unreadable child has to appear with an unknown (`None`) item count instead.
#[test]
fn lists_parent_even_when_a_subdirectory_is_unreadable() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();

    fs::create_dir(root.join("readable")).expect("readable");
    fs::write(root.join("readable").join("inner.txt"), "x").expect("inner");
    let locked = root.join("locked");
    fs::create_dir(&locked).expect("locked");

    if !deny_read(&locked) {
        return;
    }

    let response = list_dir(&ListDirOptions {
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: false,
    });

    restore_read(&locked);

    let response = response.expect("listing must succeed despite an unreadable child");

    let readable = response
        .entries
        .iter()
        .find(|entry| entry.name == "readable")
        .expect("readable folder present");
    assert_eq!(readable.item_count, Some(1));

    let locked_entry = response
        .entries
        .iter()
        .find(|entry| entry.name == "locked")
        .expect("locked folder still listed");
    assert!(locked_entry.is_dir);
    assert_eq!(
        locked_entry.item_count, None,
        "an unreadable directory reports an unknown item count"
    );
}

#[cfg(unix)]
fn deny_read(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o000)).expect("deny read");
    true
}

#[cfg(unix)]
fn restore_read(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("restore read");
}

#[cfg(windows)]
fn deny_read(path: &Path) -> bool {
    let user = std::env::var("USERNAME").expect("USERNAME set");
    let output = std::process::Command::new("icacls")
        .arg(path)
        .arg("/deny")
        .arg(format!("{user}:(RX)"))
        .output()
        .expect("run icacls /deny");
    output.status.success()
}

#[cfg(windows)]
fn restore_read(path: &Path) {
    let user = std::env::var("USERNAME").expect("USERNAME set");
    let _ = std::process::Command::new("icacls")
        .arg(path)
        .arg("/remove:d")
        .arg(user)
        .output();
}

#[test]
fn default_start_dir_is_a_real_absolute_directory() {
    let start = default_start_dir();
    assert!(start.is_absolute());
    assert!(start.is_dir());
}

#[test]
fn canonicalize_dir_strips_extended_length_prefix() {
    let fixture = tempdir().expect("temp dir");
    let canonical = canonicalize_dir(fixture.path()).expect("canonical");
    assert!(canonical.is_absolute());
    assert!(!canonical.to_string_lossy().starts_with("\\\\?\\"));
}

#[test]
fn display_paths_strip_windows_extended_unc_prefix() {
    if cfg!(windows) {
        assert_eq!(
            display_path_from_text("\\\\?\\UNC\\raspberry.pi\\share"),
            "\\\\raspberry.pi\\share"
        );
        assert_eq!(
            display_path_from_text("\\\\?\\C:\\Users\\Omega"),
            "C:\\Users\\Omega"
        );
    }
}

#[test]
fn list_dir_sorts_by_size_items_type_modified_and_created() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::create_dir(root.join("empty-folder")).expect("empty folder");
    fs::create_dir(root.join("full-folder")).expect("full folder");
    fs::write(root.join("full-folder").join("child.txt"), "x").expect("child");
    fs::write(root.join("small.txt"), "1").expect("small");
    fs::write(root.join("large.log"), "12345").expect("large");
    fs::write(root.join("no-extension"), "123").expect("plain");

    let by_size = list_names(root, SortKey::Size, SortDirection::Desc);
    assert_eq!(by_size, vec!["full-folder", "empty-folder", "large.log", "no-extension", "small.txt"]);

    let by_items = list_names(root, SortKey::Items, SortDirection::Desc);
    assert_eq!(by_items[..2], ["full-folder", "empty-folder"]);

    let by_type = list_names(root, SortKey::Type, SortDirection::Asc);
    assert_eq!(by_type, vec!["empty-folder", "full-folder", "no-extension", "large.log", "small.txt"]);

    let by_modified = list_names(root, SortKey::Modified, SortDirection::Asc);
    assert_eq!(by_modified.len(), 5);

    let by_created = list_names(root, SortKey::Created, SortDirection::Desc);
    assert_eq!(by_created.len(), 5);
}

#[test]
fn list_dir_reports_readonly_and_plain_file_type_metadata() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let plain = root.join("LICENSE");
    fs::write(&plain, "license").expect("plain");
    let mut permissions = fs::metadata(&plain).expect("metadata").permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&plain, permissions).expect("readonly");

    let response = list_dir(&ListDirOptions {
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: false,
    })
    .expect("list dir");

    let entry = response
        .entries
        .iter()
        .find(|entry| entry.name == "LICENSE")
        .expect("plain file");
    assert_eq!(entry.type_label, "File");
    assert!(entry.attributes.iter().any(|attribute| attribute == "readonly"));

    let mut permissions = fs::metadata(&plain).expect("metadata").permissions();
    permissions.set_readonly(false);
    fs::set_permissions(&plain, permissions).expect("restore writable");
}

fn list_names(root: &Path, sort_key: SortKey, sort_direction: SortDirection) -> Vec<String> {
    list_dir(&ListDirOptions {
        path: root.to_string_lossy().into_owned(),
        sort_key,
        sort_direction,
        filter: String::new(),
        show_hidden: true,
    })
    .expect("list dir")
    .entries
    .into_iter()
    .map(|entry| entry.name)
    .collect()
}
