//! Tests for the OS-independent helpers backing the Windows 11 modern
//! context-menu path (`native_menu::modern_match`). The COM/registry code in
//! `windows_modern.rs` is Windows-only and feature-gated out of the test build
//! (the sanctioned machine-global-API pattern), so these cover the pure
//! discovery/matching logic that feeds it.

use std::collections::HashSet;

use file_explorer_lib::native_menu::modern_match::{
    format_guid_braced, handler_matches, parse_guid_u128, path_key, path_type_tokens,
    selected_paths, selection_type_tokens,
};
use file_explorer_lib::native_menu::types::{LoadNativeMenuRequest, NativeMenuTargetKind};

const WINRAR_CLSID: u128 = 0xB41DB860_64E4_11D2_9906_E49FADC173CA;

fn request(
    target_kind: NativeMenuTargetKind,
    target_path: Option<&str>,
    selected: &[&str],
) -> LoadNativeMenuRequest {
    LoadNativeMenuRequest {
        request_id: "req".to_string(),
        target_kind,
        target_path: target_path.map(str::to_string),
        folder_path: None,
        selected_paths: selected.iter().map(|p| (*p).to_string()).collect(),
    }
}

#[test]
fn parses_guids_in_every_common_form() {
    let expected = Some(WINRAR_CLSID);
    assert_eq!(
        parse_guid_u128("{B41DB860-64E4-11D2-9906-E49FADC173CA}"),
        expected
    );
    assert_eq!(
        parse_guid_u128("B41DB860-64E4-11D2-9906-E49FADC173CA"),
        expected
    );
    assert_eq!(parse_guid_u128("b41db86064e411d29906e49fadc173ca"), expected);
}

#[test]
fn rejects_guids_with_wrong_digit_count_or_non_hex() {
    assert_eq!(parse_guid_u128(""), None);
    assert_eq!(parse_guid_u128("{1234}"), None);
    // 31 hex digits (one short).
    assert_eq!(parse_guid_u128("B41DB860-64E4-11D2-9906-E49FADC173C"), None);
    // Non-hex characters are filtered, leaving too few digits.
    assert_eq!(parse_guid_u128("not-a-guid-zzzz"), None);
}

#[test]
fn formats_guid_in_braced_uppercase_registry_form() {
    assert_eq!(
        format_guid_braced(WINRAR_CLSID),
        "{B41DB860-64E4-11D2-9906-E49FADC173CA}"
    );
}

#[test]
fn guid_parse_and_format_round_trip() {
    let braced = format_guid_braced(WINRAR_CLSID);
    assert_eq!(parse_guid_u128(&braced), Some(WINRAR_CLSID));
}

#[test]
fn detects_drive_roots_only() {
    assert!(path_type_tokens("C:\\").contains("drive"));
    assert!(path_type_tokens("c:").contains("drive"));
    // Non-roots do not get the drive token.
    assert!(!path_type_tokens("C:\\Users").contains("drive"));
}

#[test]
fn path_key_handles_root_and_nested_paths() {
    assert_eq!(path_key(&[]), "root");
    assert_eq!(path_key(&[0, 1, 2]), "0-1-2");
    assert_eq!(path_key(&[3]), "3");
}

#[test]
fn drive_root_tokens_cover_drive_and_folder() {
    let tokens = path_type_tokens("D:\\");
    assert!(tokens.contains("drive"));
    assert!(tokens.contains("folder"));
    assert!(tokens.contains("allfilesystemobjects"));
    assert!(!tokens.contains("*"));
}

#[test]
fn directory_tokens_cover_directory_and_folder() {
    let dir = tempfile::tempdir().expect("tempdir");
    let tokens = path_type_tokens(&dir.path().to_string_lossy());
    assert!(tokens.contains("directory"));
    assert!(tokens.contains("folder"));
    assert!(tokens.contains("allfilesystemobjects"));
    assert!(!tokens.contains("*"));
}

#[test]
fn file_tokens_cover_wildcard_and_extension() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("Report.TXT");
    std::fs::write(&file, b"x").expect("write file");
    let tokens = path_type_tokens(&file.to_string_lossy());
    assert!(tokens.contains("*"));
    assert!(tokens.contains("allfilesystemobjects"));
    assert!(tokens.contains(".txt"));
    assert!(!tokens.contains("directory"));
}

#[test]
fn selected_paths_prefers_selection_then_falls_back_to_target() {
    let with_selection = request(
        NativeMenuTargetKind::Multi,
        Some("C:\\a.txt"),
        &["C:\\a.txt", "C:\\b.txt"],
    );
    assert_eq!(selected_paths(&with_selection).len(), 2);

    let target_only = request(NativeMenuTargetKind::File, Some("C:\\only.txt"), &[]);
    assert_eq!(selected_paths(&target_only), vec!["C:\\only.txt".to_string()]);

    let empty = request(NativeMenuTargetKind::File, None, &[]);
    assert!(selected_paths(&empty).is_empty());
}

#[test]
fn background_selection_uses_directory_background_token() {
    let req = request(NativeMenuTargetKind::Background, Some("C:\\folder"), &[]);
    let selection = selection_type_tokens(&req);
    assert_eq!(selection.len(), 1);
    assert!(selection[0].contains("directory\\background"));
}

#[test]
fn selection_type_tokens_maps_each_selected_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("notes.md");
    std::fs::write(&file, b"x").expect("write file");
    let req = request(
        NativeMenuTargetKind::File,
        Some(&file.to_string_lossy()),
        &[&file.to_string_lossy()],
    );
    let selection = selection_type_tokens(&req);
    assert_eq!(selection.len(), 1);
    assert!(selection[0].contains(".md"));
}

#[test]
fn handler_matches_requires_every_selected_item_to_be_covered() {
    let item_types = HashSet::from(["*".to_string(), "directory".to_string()]);

    let files_only = vec![HashSet::from(["*".to_string(), "allfilesystemobjects".to_string()])];
    assert!(handler_matches(&item_types, &files_only));

    let mixed = vec![
        HashSet::from(["*".to_string()]),
        HashSet::from(["directory".to_string()]),
    ];
    assert!(handler_matches(&item_types, &mixed));

    let uncovered = vec![HashSet::from(["drive".to_string()])];
    assert!(!handler_matches(&item_types, &uncovered));

    // No selection never matches.
    assert!(!handler_matches(&item_types, &[]));
}
