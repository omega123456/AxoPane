use file_explorer_lib::search::matcher::{FoldedName, NameKind, Token};
use file_explorer_lib::search::path_style::PathStyle;
use file_explorer_lib::search::query::{parse, ParseError, ParsedQuery, MIN_QUERY_CHARS};

const WIN: PathStyle = PathStyle::Windows;
const POSIX: PathStyle = PathStyle::Posix;

fn ok(raw: &str, style: PathStyle, home: Option<&str>) -> ParsedQuery {
    parse(raw, style, home).unwrap_or_else(|e| panic!("{raw:?}: {e:?}"))
}

fn folder_raws(query: &ParsedQuery) -> Vec<&str> {
    query.folder_segs.iter().map(|s| s.raw.as_str()).collect()
}

fn name_raw(query: &ParsedQuery) -> Option<&str> {
    query.name_seg.as_ref().map(|s| s.raw.as_str())
}

#[test]
fn blank_and_short_queries_are_rejected() {
    assert_eq!(MIN_QUERY_CHARS, 3);
    assert_eq!(parse("   ", WIN, None), Err(ParseError::Empty));
    assert_eq!(parse("ab", WIN, None), Err(ParseError::TooShort));
    assert_eq!(parse("a b", POSIX, None), Err(ParseError::TooShort));
    assert_eq!(
        parse("~", POSIX, Some("/Users/alex")),
        Err(ParseError::TooShort)
    );
    // Only separators: long enough, but nothing to search for.
    assert_eq!(parse("///", WIN, None), Err(ParseError::Empty));
}

#[test]
fn a_plain_name_is_a_name_search() {
    let query = ok("report", WIN, None);
    assert!(query.anchor.is_none() && !query.browse && query.folder_segs.is_empty());
    assert_eq!(name_raw(&query), Some("report"));
}

#[test]
fn slashes_mean_inside_with_either_separator_on_both_styles() {
    for (raw, style) in [
        ("alex/invoice", WIN),
        (r"alex\invoice", WIN),
        ("alex/invoice", POSIX),
        (r"alex\invoice", POSIX),
        (" alex / invoice ", POSIX),
        ("alex//invoice", POSIX),
    ] {
        let query = ok(raw, style, None);
        assert_eq!(folder_raws(&query), ["alex"], "{raw}");
        assert_eq!(name_raw(&query), Some("invoice"), "{raw}");
        assert!(!query.browse);
    }
}

#[test]
fn a_trailing_separator_lists_children() {
    let query = ok("users/alex/", WIN, None);
    assert!(query.browse && query.name_seg.is_none() && query.anchor.is_none());
    assert_eq!(folder_raws(&query), ["users", "alex"]);
}

#[test]
fn roots_anchor_the_query_and_may_be_short_when_browsing() {
    let root = ok("/", POSIX, None);
    let anchor = root.anchor.as_ref().expect("anchored");
    assert!(root.browse && anchor.components.is_empty());
    assert_eq!(anchor.path(POSIX), "/");

    let drive = ok(r"C:\Us", WIN, None);
    assert_eq!(drive.anchor.as_ref().expect("anchored").path(WIN), r"C:\");
    assert_eq!(name_raw(&drive), Some("Us"));

    let unc = ok(r"\\nas\home\", WIN, None);
    assert_eq!(unc.anchor.as_ref().expect("anchored").root, r"\\nas\home\");
    assert!(unc.browse);
}

#[test]
fn tilde_expands_to_home() {
    let home = ok("~/", POSIX, Some("/Users/alex/"));
    let anchor = home.anchor.as_ref().expect("anchored");
    assert_eq!(anchor.path(POSIX), "/Users/alex");
    assert!(home.browse);

    let docs = ok(r"~\Docs\inv", WIN, Some(r"C:\Users\alex"));
    assert_eq!(
        docs.anchor.as_ref().expect("anchored").path(WIN),
        r"C:\Users\alex\Docs"
    );
    assert_eq!(name_raw(&docs), Some("inv"));

    // Only `~` followed by a separator (or alone) expands.
    assert_eq!(
        name_raw(&ok("~xyz", POSIX, Some("/Users/alex"))),
        Some("~xyz")
    );
    let no_home = ok("~/abc", POSIX, None);
    assert!(no_home.anchor.is_none());
    assert_eq!(folder_raws(&no_home), ["~"]);
}

#[test]
fn segments_split_words_and_detect_globs() {
    let query = ok("q3 *.docx", WIN, None);
    let segment = query.name_seg.as_ref().expect("name");
    assert!(matches!(
        segment.tokens(),
        [Token::Literal(_), Token::Glob(_)]
    ));
    let name = FoldedName::new("Q3 report.docx");
    assert!(segment.matches(&name, NameKind::File).is_some());
}

#[test]
fn anchors_compare_roots_and_ancestors_case_insensitively() {
    let query = ok(r"c:\users\ALEX\", WIN, None);
    let anchor = query.anchor.as_ref().expect("anchored");
    assert!(anchor.contains(r"C:\", &["Users", "alex"]));
    assert!(anchor.contains(r"C:\", &["Users", "alex", "Documents"]));
    assert!(!anchor.contains(r"D:\", &["Users", "alex"]));
    assert!(!anchor.contains(r"C:\", &["Users"]));
    assert!(!anchor.contains(r"C:\", &["Users", "alexios"]));
}
