#[path = "common/mod.rs"]
mod common;

use std::cmp::Ordering;
use std::fs;
use std::time::SystemTime;

use file_explorer_lib::fs::{
    collect_attributes, compare_entries, compare_optional_string, compare_optional_u64,
    directory_entry_from_path, infer_type_label, natural_name_compare, platform_root,
    read_item_count, system_time_to_rfc3339, validate_name, DirectoryEntry, SortDirection, SortKey,
};
use tempfile::tempdir;

fn entry(name: &str, is_dir: bool) -> DirectoryEntry {
    DirectoryEntry {
        id: name.to_string(),
        name: name.to_string(),
        path: name.to_string(),
        is_dir,
        icon_data_url: None,
        size_bytes: (!is_dir).then_some(name.len() as u64),
        item_count: is_dir.then_some(name.len() as u64),
        type_label: infer_type_label(name, is_dir),
        modified_at: Some(format!("2026-01-0{}T00:00:00Z", name.len().min(9))),
        created_at: Some(format!("2025-01-0{}T00:00:00Z", name.len().min(9))),
        attributes: Vec::new(),
        is_hidden: false,
        is_system: false,
    }
}

#[test]
fn helper_validation_and_sorting_cover_private_branches() {
    if cfg!(windows) {
        assert_eq!(platform_root(), std::path::PathBuf::from("C:\\"));
    } else {
        assert_eq!(platform_root(), std::path::PathBuf::from("/"));
    }

    assert!(validate_name("report.txt").is_ok());
    for invalid in ["", " ", ".", "..", "a/b", "a\\b", "bad\0name"] {
        assert!(
            validate_name(invalid).is_err(),
            "{invalid:?} should be rejected"
        );
    }

    assert_eq!(infer_type_label("archive.zip", false), "ZIP file");
    assert_eq!(infer_type_label("folder", true), "Folder");
    assert_eq!(infer_type_label("LICENSE", false), "File");

    assert_eq!(
        natural_name_compare("file2.txt", "file10.txt"),
        Ordering::Less
    );
    assert_eq!(compare_optional_u64(Some(4), None), Ordering::Greater);
    assert_eq!(compare_optional_u64(None, Some(4)), Ordering::Less);
    assert_eq!(
        compare_optional_string(Some("b"), Some("a")),
        Ordering::Greater
    );
    assert_eq!(compare_optional_string(None, None), Ordering::Equal);

    let directory = entry("docs", true);
    let file = entry("todo.txt", false);
    assert_eq!(
        compare_entries(&directory, &file, SortKey::Name, SortDirection::Asc),
        Ordering::Less
    );

    let small = entry("a.txt", false);
    let large = entry("longer-name.log", false);
    assert_eq!(
        compare_entries(&small, &large, SortKey::Size, SortDirection::Desc),
        Ordering::Greater
    );
    assert_eq!(
        compare_entries(&small, &large, SortKey::Type, SortDirection::Asc),
        Ordering::Greater
    );
    assert_eq!(
        compare_entries(&small, &large, SortKey::Modified, SortDirection::Asc),
        Ordering::Less
    );
    assert_eq!(
        compare_entries(&small, &large, SortKey::Created, SortDirection::Desc),
        Ordering::Greater
    );
    let mut left = entry("alpha.txt", false);
    let mut right = entry("beta.txt", false);
    left.modified_at = Some("2026-01-01T00:00:00Z".to_string());
    right.modified_at = Some("2026-01-01T00:00:00Z".to_string());
    assert_eq!(
        compare_entries(&left, &right, SortKey::Modified, SortDirection::Asc),
        Ordering::Less
    );
    left.created_at = Some("2025-01-01T00:00:00Z".to_string());
    right.created_at = Some("2025-01-01T00:00:00Z".to_string());
    assert_eq!(
        compare_entries(&left, &right, SortKey::Created, SortDirection::Asc),
        Ordering::Less
    );

    // Equal primary keys force the `then_with` name tiebreak closures to run for
    // the Size and Items sort keys (names are equal length, so sizes/item counts
    // match and the comparison falls through to the natural-name ordering).
    let size_a = entry("aaa.txt", false);
    let size_b = entry("bbb.txt", false);
    assert_eq!(size_a.size_bytes, size_b.size_bytes);
    assert_eq!(
        compare_entries(&size_a, &size_b, SortKey::Size, SortDirection::Asc),
        Ordering::Less
    );
    let items_a = entry("one", true);
    let items_b = entry("two", true);
    assert_eq!(items_a.item_count, items_b.item_count);
    assert_eq!(
        compare_entries(&items_a, &items_b, SortKey::Items, SortDirection::Asc),
        Ordering::Less
    );

    assert!(system_time_to_rfc3339(Some(SystemTime::UNIX_EPOCH))
        .expect("timestamp")
        .starts_with("1970-01-01T00:00:00"));
    assert_eq!(common::bootstrap_message(), "phase-1-common");
}

#[test]
fn helper_metadata_collects_attributes_and_item_counts() {
    let fixture = tempdir().expect("temp dir");
    let root = fixture.path();
    let nested = root.join("nested");
    fs::create_dir(&nested).expect("nested");
    fs::write(root.join(".hidden.txt"), b"hello").expect("hidden file");
    fs::write(nested.join("child.txt"), b"x").expect("child");

    let hidden = root.join(".hidden.txt");
    let mut permissions = fs::metadata(&hidden).expect("metadata").permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&hidden, permissions).expect("readonly");

    let metadata = fs::metadata(&hidden).expect("metadata");
    let attributes = collect_attributes(&hidden, &metadata);
    assert!(attributes.iter().any(|attribute| attribute == "readonly"));
    #[cfg(not(windows))]
    assert!(attributes.iter().any(|attribute| attribute == "hidden"));

    assert_eq!(read_item_count(root), Some(2));

    let entry = directory_entry_from_path(&hidden).expect("entry from path");
    assert_eq!(entry.name, ".hidden.txt");
    assert_eq!(entry.size_bytes, Some(5));
}
