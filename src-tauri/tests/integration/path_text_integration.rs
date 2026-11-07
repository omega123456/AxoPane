use file_explorer_lib::path_text::{
    is_drive_designator, starts_with_drive_designator, strip_unc_prefix, strip_verbatim_prefix,
};

#[test]
fn verbatim_unc_paths_become_plain_unc_paths_in_any_case() {
    assert_eq!(
        strip_verbatim_prefix(r"\\?\UNC\server\share\dir"),
        r"\\server\share\dir"
    );
    assert_eq!(
        strip_verbatim_prefix(r"\\?\unc\server\share"),
        r"\\server\share"
    );
}

#[test]
fn verbatim_drive_paths_drop_their_prefix() {
    assert_eq!(strip_verbatim_prefix(r"\\?\C:\Users"), r"C:\Users");
}

#[test]
fn non_verbatim_paths_are_unchanged() {
    assert_eq!(strip_verbatim_prefix(r"C:\Users"), r"C:\Users");
    assert_eq!(strip_verbatim_prefix(r"\\server\share"), r"\\server\share");
    assert_eq!(strip_verbatim_prefix("/Users/alex"), "/Users/alex");
}

#[test]
fn unc_prefix_is_stripped_from_plain_and_verbatim_forms() {
    assert_eq!(
        strip_unc_prefix(r"\\?\UNC\server\share"),
        Some(r"server\share")
    );
    assert_eq!(strip_unc_prefix(r"\\server\share"), Some(r"server\share"));
    assert_eq!(strip_unc_prefix(r"C:\Users"), None);
}

#[test]
fn drive_designators_are_recognised() {
    assert!(is_drive_designator("C:"));
    assert!(!is_drive_designator(r"C:\"));
    assert!(!is_drive_designator("/"));
    assert!(starts_with_drive_designator(r"d:\data"));
    assert!(!starts_with_drive_designator("1:"));
    assert!(!starts_with_drive_designator("C"));
}
