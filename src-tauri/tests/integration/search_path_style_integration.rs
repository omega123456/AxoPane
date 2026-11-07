use file_explorer_lib::search::path_style::{PathParts, PathStyle};

const WIN: PathStyle = PathStyle::Windows;
const POSIX: PathStyle = PathStyle::Posix;

fn parts(root: &str, components: &[&'static str]) -> PathParts<'static> {
    PathParts {
        root: root.to_owned(),
        components: components.to_vec(),
    }
}

#[test]
fn separators_follow_the_style() {
    assert_eq!(WIN.separator(), '\\');
    assert_eq!(POSIX.separator(), '/');
    assert!(WIN.is_separator('\\') && WIN.is_separator('/'));
    assert!(POSIX.is_separator('/') && !POSIX.is_separator('\\'));
}

#[test]
fn verbatim_prefixes_are_stripped_on_windows_only() {
    assert_eq!(WIN.strip_verbatim(r"\\?\C:\Users\a"), r"C:\Users\a");
    assert_eq!(WIN.strip_verbatim(r"\\?\UNC\nas\home\a"), r"\\nas\home\a");
    assert_eq!(WIN.strip_verbatim(r"C:\plain"), r"C:\plain");
    assert_eq!(POSIX.strip_verbatim(r"\\?\C:\x"), r"\\?\C:\x");
}

#[test]
fn windows_paths_split_into_drive_unc_or_relative_roots() {
    assert_eq!(
        WIN.split(r"C:\Users\alex"),
        parts(r"C:\", &["Users", "alex"])
    );
    assert_eq!(
        WIN.split("d:/Photos/2026/"),
        parts(r"d:\", &["Photos", "2026"])
    );
    assert_eq!(WIN.split("C:"), parts(r"C:\", &[]));
    assert_eq!(
        WIN.split(r"\\nas\home\alex.k"),
        parts(r"\\nas\home\", &["alex.k"])
    );
    assert_eq!(WIN.split("//nas/team"), parts(r"\\nas\team\", &[]));
    assert_eq!(WIN.split(r"\\nas"), parts("", &["nas"]));
    assert_eq!(WIN.split(r"alex\invoice"), parts("", &["alex", "invoice"]));
}

#[test]
fn posix_paths_split_into_slash_or_relative_roots() {
    assert_eq!(POSIX.split("/Users/alex/"), parts("/", &["Users", "alex"]));
    assert_eq!(POSIX.split("/"), parts("/", &[]));
    assert_eq!(POSIX.split("alex/inv"), parts("", &["alex", "inv"]));
    assert_eq!(POSIX.split(r"a\b"), parts("", &[r"a\b"]));
}

#[test]
fn join_and_compose_add_separators_only_where_needed() {
    assert_eq!(WIN.join(r"C:\", "Users"), r"C:\Users");
    assert_eq!(WIN.join(r"C:\Users", "alex"), r"C:\Users\alex");
    assert_eq!(WIN.join("", "alex"), "alex");
    assert_eq!(POSIX.join("/", "Users"), "/Users");
    assert_eq!(POSIX.compose("/", &["Users", "alex"]), "/Users/alex");
    assert_eq!(WIN.compose(r"\\nas\home\", &["a", "b"]), r"\\nas\home\a\b");
}

#[test]
fn parent_walks_up_one_level_and_stops_at_roots() {
    assert_eq!(WIN.parent(r"C:\Users\alex").as_deref(), Some(r"C:\Users"));
    assert_eq!(WIN.parent(r"C:\Users").as_deref(), Some(r"C:\"));
    assert_eq!(WIN.parent(r"C:\"), None);
    assert_eq!(WIN.parent("alex"), None);
    assert_eq!(WIN.parent(r"alex\inv").as_deref(), Some("alex"));
    assert_eq!(POSIX.parent("/Users").as_deref(), Some("/"));
    assert_eq!(POSIX.parent("/"), None);
}

#[test]
fn roots_are_recognised_for_both_styles() {
    assert!(WIN.is_root(r"C:\"));
    assert!(WIN.is_root(r"\\nas\home"));
    assert!(!WIN.is_root(r"C:\Users"));
    assert!(!WIN.is_root("alex"));
    assert!(POSIX.is_root("/"));
    assert!(!POSIX.is_root("/Users"));
}
