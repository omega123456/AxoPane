use std::path::Path;

use file_explorer_lib::trash::{parse_put_back_for_tests, resolve_original_path_for_tests};

const FIXTURE: &[u8] = include_bytes!("fixtures/trash_put_back.dsstore");

#[test]
fn parse_put_back_extracts_ustr_records_from_a_real_layout_fixture() {
    let map = parse_put_back_for_tests(FIXTURE);

    let report = map.get("restored-report.txt").expect("recorded entry");
    assert_eq!(report.original_dir, "Users/example/Documents/reports/");
    assert_eq!(report.original_name, "restored-report.txt");
}

#[test]
fn parse_put_back_falls_back_to_the_bookmark_blob_for_older_ptbl_encodings() {
    let map = parse_put_back_for_tests(FIXTURE);

    let legacy = map.get("legacy-notes.md").expect("recorded entry");
    assert_eq!(legacy.original_dir, "Users/example/Archive/legacy/");
    assert_eq!(legacy.original_name, "legacy-notes.md");
}

#[test]
fn parse_put_back_skips_entries_missing_a_directory_record() {
    let map = parse_put_back_for_tests(FIXTURE);

    assert!(!map.contains_key("orphan-only-name.txt"));
}

#[test]
fn parse_put_back_on_an_empty_buffer_returns_an_empty_map() {
    assert!(parse_put_back_for_tests(&[]).is_empty());
}

#[test]
fn parse_put_back_on_a_truncated_header_returns_an_empty_map() {
    assert!(parse_put_back_for_tests(&FIXTURE[..20]).is_empty());
}

#[test]
fn parse_put_back_on_garbage_bytes_returns_an_empty_map_without_panicking() {
    let garbage = vec![0xAAu8; 4096];
    assert!(parse_put_back_for_tests(&garbage).is_empty());
}

#[test]
fn parse_put_back_rejects_a_bad_magic() {
    let mut corrupt = FIXTURE.to_vec();
    corrupt[4..8].copy_from_slice(b"NOPE");
    assert!(parse_put_back_for_tests(&corrupt).is_empty());
}

#[test]
fn resolve_original_path_joins_home_trash_volume_root_with_the_recovered_directory() {
    let resolved = resolve_original_path_for_tests(
        Path::new("/"),
        "Users/example/Documents/reports/",
        "restored-report.txt",
    );
    assert_eq!(
        resolved,
        "/Users/example/Documents/reports/restored-report.txt"
    );
}

#[test]
fn resolve_original_path_normalizes_a_directory_without_a_trailing_slash() {
    let resolved =
        resolve_original_path_for_tests(Path::new("/"), "Users/example/Documents", "file.txt");
    assert_eq!(resolved, "/Users/example/Documents/file.txt");
}

#[test]
fn resolve_original_path_keeps_the_private_var_firmlink_form_as_is() {
    let resolved =
        resolve_original_path_for_tests(Path::new("/"), "private/var/folders/xy/T/", "scratch.tmp");
    assert_eq!(resolved, "/private/var/folders/xy/T/scratch.tmp");
}
