#[path = "common/mod.rs"]
mod common;

use std::fs;
use std::path::Path;

use file_explorer_lib::fs::{
    canonicalize_dir, default_start_dir, display_path_from_text, expand_home_path_with, list_dir,
    list_tree_children, ListDirOptions, ListTreeChildrenOptions, SortDirection, SortKey,
    TreeExpandability,
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
        include_item_counts: true,
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
        include_item_counts: true,
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
        include_item_counts: true,
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
fn executable_icon_lookup_uses_safe_test_utils_fallback() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::write(root.join("installer.exe"), "binary").expect("exe");
    fs::write(root.join("readme"), "text").expect("readme");

    let response = list_dir(&ListDirOptions {
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: true,
        include_item_counts: true,
    })
    .expect("list dir");

    let installer = response
        .entries
        .iter()
        .find(|entry| entry.name == "installer.exe")
        .expect("installer");
    let readme = response
        .entries
        .iter()
        .find(|entry| entry.name == "readme")
        .expect("readme");
    assert_eq!(installer.type_label, "EXE file");
    assert_eq!(installer.icon_data_url, None);
    assert_eq!(readme.icon_data_url, None);
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
        include_item_counts: true,
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
fn expands_home_shorthand_only_for_path_prefixes() {
    let home = Path::new("home-dir");

    assert_eq!(expand_home_path_with("~", Some(home)), home);
    assert_eq!(
        expand_home_path_with("~/Projects/AxoPane", Some(home)),
        home.join("Projects").join("AxoPane")
    );
    assert_eq!(
        expand_home_path_with("~\\Projects\\AxoPane", Some(home)),
        home.join("Projects\\AxoPane")
    );
    assert_eq!(
        expand_home_path_with("~archive", Some(home)),
        Path::new("~archive")
    );
    assert_eq!(
        expand_home_path_with("~/Projects", None),
        Path::new("~/Projects")
    );
}

#[test]
fn list_tree_children_returns_only_lightweight_directory_nodes() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::create_dir(root.join("folder10")).expect("folder10");
    fs::create_dir(root.join("folder2")).expect("folder2");
    fs::create_dir(root.join("folder2").join("child")).expect("child");
    fs::create_dir(root.join(".hidden")).expect("hidden");
    fs::write(root.join("notes.txt"), "n").expect("notes");

    let visible = list_tree_children(&ListTreeChildrenOptions {
        path: root.to_string_lossy().into_owned(),
        show_hidden: false,
    })
    .expect("tree children");
    assert_eq!(
        visible
            .children
            .iter()
            .map(|child| child.name.as_str())
            .collect::<Vec<_>>(),
        vec!["folder2", "folder10"]
    );
    let folder2 = visible
        .children
        .iter()
        .find(|child| child.name == "folder2")
        .expect("folder2");
    // A parent listing does not open every child to probe grandchildren.
    assert_eq!(
        folder2.expandability,
        file_explorer_lib::fs::TreeExpandability::Unknown
    );
    assert!(!folder2.has_children);
    assert!(visible.children.iter().all(|child| child.name != ".hidden"));

    let with_hidden = list_tree_children(&ListTreeChildrenOptions {
        path: root.to_string_lossy().into_owned(),
        show_hidden: true,
    })
    .expect("tree children with hidden");
    assert!(with_hidden
        .children
        .iter()
        .any(|child| child.name == ".hidden"));
}

#[test]
fn list_tree_children_handles_hidden_only_and_missing_directories() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::create_dir(root.join(".hidden-only")).expect("hidden only");

    let visible = list_tree_children(&ListTreeChildrenOptions {
        path: root.to_string_lossy().into_owned(),
        show_hidden: false,
    })
    .expect("tree children");
    assert!(visible.children.is_empty());

    let missing = root.join("missing");
    let error = list_tree_children(&ListTreeChildrenOptions {
        path: missing.to_string_lossy().into_owned(),
        show_hidden: false,
    })
    .expect_err("missing directory");
    assert!(!error.to_string().is_empty());
}

#[test]
fn list_tree_children_leaves_child_expandability_unknown_without_probing() {
    // A parent listing returns direct children only. It never probes each
    // child to derive descendants; expansion resolves that state lazily.
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let parent = root.join("parent");
    fs::create_dir(&parent).expect("parent");
    fs::create_dir(parent.join(".secret")).expect("hidden grandchild");

    let visible = list_tree_children(&ListTreeChildrenOptions {
        path: root.to_string_lossy().into_owned(),
        show_hidden: false,
    })
    .expect("tree children");
    let parent_node = visible
        .children
        .iter()
        .find(|child| child.name == "parent")
        .expect("parent node present");
    assert_eq!(parent_node.expandability, TreeExpandability::Unknown);

    let with_hidden = list_tree_children(&ListTreeChildrenOptions {
        path: root.to_string_lossy().into_owned(),
        show_hidden: true,
    })
    .expect("tree children with hidden");
    assert_eq!(
        with_hidden
            .children
            .iter()
            .find(|child| child.name == "parent")
            .expect("parent node present")
            .expandability,
        TreeExpandability::Unknown
    );
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
        include_item_counts: true,
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
        include_item_counts: true,
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
    if !output.status.success() {
        return false;
    }
    // Elevated / Administrator sessions bypass an ACL deny, so the directory
    // stays readable and the test's precondition never holds. Verify the deny
    // is actually effective; if the read still succeeds, restore access and
    // skip so the test is deterministic regardless of the host's privilege.
    if fs::read_dir(path).is_ok() {
        restore_read(path);
        return false;
    }
    true
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
    assert_eq!(
        by_size,
        vec![
            "full-folder",
            "empty-folder",
            "large.log",
            "no-extension",
            "small.txt"
        ]
    );

    let by_items = list_names(root, SortKey::Items, SortDirection::Desc);
    assert_eq!(by_items[..2], ["full-folder", "empty-folder"]);

    let by_type = list_names(root, SortKey::Type, SortDirection::Asc);
    assert_eq!(
        by_type,
        vec![
            "empty-folder",
            "full-folder",
            "no-extension",
            "large.log",
            "small.txt"
        ]
    );

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
        include_item_counts: true,
    })
    .expect("list dir");

    let entry = response
        .entries
        .iter()
        .find(|entry| entry.name == "LICENSE")
        .expect("plain file");
    assert_eq!(entry.type_label, "File");
    assert!(entry
        .attributes
        .iter()
        .any(|attribute| attribute == "readonly"));

    // Restore write permission so the temp dir cleans up (Windows refuses to
    // delete read-only files). Use an explicit mode on Unix to avoid making the
    // file world-writable.
    let mut permissions = fs::metadata(&plain).expect("metadata").permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o644);
    }
    #[cfg(windows)]
    permissions.set_readonly(false);
    fs::set_permissions(&plain, permissions).expect("restore writable");
}

/// A symlinked folder must be listed and treated as navigable — `is_dir` must
/// be `true` and it must show up in the folder tree — not silently downgraded
/// to a file. On Windows this guards against `std::fs::Metadata::is_dir()`
/// being `false` for any reparse point (junctions included) even when it
/// targets a directory.
#[test]
fn folder_symlink_is_listed_as_a_navigable_directory() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let real = root.join("real_target");
    fs::create_dir(&real).expect("real dir");
    fs::write(real.join("inside.txt"), "hi").expect("inside file");

    let link = root.join("linked_folder");
    create_dir_link(&real, &link);

    let response = list_dir(&ListDirOptions {
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: false,
        include_item_counts: true,
    })
    .expect("list dir");

    let link_entry = response
        .entries
        .iter()
        .find(|entry| entry.name == "linked_folder")
        .expect("linked_folder entry present");
    assert!(
        link_entry.is_dir,
        "folder symlink must report as a directory"
    );
    assert_eq!(link_entry.type_label, "Folder");
    assert_eq!(link_entry.item_count, Some(1));

    let tree = list_tree_children(&ListTreeChildrenOptions {
        path: root.to_string_lossy().into_owned(),
        show_hidden: false,
    })
    .expect("list tree children");
    assert!(
        tree.children
            .iter()
            .any(|child| child.name == "linked_folder"),
        "folder symlink must appear in the folder tree"
    );
}

/// Creates a directory symlink (Unix) or NTFS junction (Windows) at `link`
/// pointing at `target`. A junction is used on Windows instead of
/// `symlink_dir` because junction creation doesn't require elevated
/// privileges or Developer Mode, keeping the test runnable in CI.
fn create_dir_link(target: &Path, link: &Path) {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).expect("create symlink");
    }
    #[cfg(windows)]
    {
        let status = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                &link.to_string_lossy(),
                &target.to_string_lossy(),
            ])
            .status()
            .expect("run mklink");
        assert!(status.success(), "mklink /J failed");
    }
}

#[test]
fn list_dir_skips_item_counts_when_disabled() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    fs::create_dir(root.join("subfolder")).expect("subfolder");

    let response = list_dir(&ListDirOptions {
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: false,
        include_item_counts: false,
    })
    .expect("list dir without item counts");
    let entry = &response.entries[0];
    assert!(entry.is_dir);
    assert_eq!(entry.item_count, None);

    let response = list_dir(&ListDirOptions {
        path: root.to_string_lossy().into_owned(),
        sort_key: SortKey::Name,
        sort_direction: SortDirection::Asc,
        filter: String::new(),
        show_hidden: false,
        include_item_counts: true,
    })
    .expect("list dir with item counts");
    let entry = &response.entries[0];
    assert_eq!(entry.item_count, Some(0));
}

fn list_names(root: &Path, sort_key: SortKey, sort_direction: SortDirection) -> Vec<String> {
    list_dir(&ListDirOptions {
        path: root.to_string_lossy().into_owned(),
        sort_key,
        sort_direction,
        filter: String::new(),
        show_hidden: true,
        include_item_counts: true,
    })
    .expect("list dir")
    .entries
    .into_iter()
    .map(|entry| entry.name)
    .collect()
}
