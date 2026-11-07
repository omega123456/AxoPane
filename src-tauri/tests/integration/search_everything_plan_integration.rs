use std::time::{Duration, SystemTime};

use file_explorer_lib::search::everything_plan::{
    browse_children_queries, passes, row_to_candidate, EverythingPass,
};
use file_explorer_lib::search::path_style::PathStyle;
use file_explorer_lib::search::query::{parse, ParsedQuery};
use file_explorer_lib::search::{SearchOptions, SearchScope};
use file_explorer_lib::size::everything::EverythingRow;

const HIDDEN: SearchOptions = SearchOptions {
    include_hidden: true,
    scope: SearchScope::All,
};
const VISIBLE: SearchOptions = SearchOptions {
    include_hidden: false,
    scope: SearchScope::All,
};

fn parsed(raw: &str) -> ParsedQuery {
    parse(raw, PathStyle::Windows, Some(r"C:\Users\alex")).expect("query")
}

fn searches(raw: &str, options: &SearchOptions) -> Vec<(String, bool)> {
    passes(&parsed(raw), options)
        .into_iter()
        .map(|EverythingPass { search, fuzzy }| (search, fuzzy))
        .collect()
}

fn expected_pass(search: &str, fuzzy: bool) -> (String, bool) {
    (search.to_owned(), fuzzy)
}

#[test]
fn a_name_runs_prefix_contains_then_fuzzy_passes() {
    assert_eq!(
        searches("Invoice", &HIDDEN),
        [
            expected_pass(r#"startwith:"invoice""#, false),
            expected_pass(r#""invoice""#, false),
            expected_pass(r#""*i*n*v*o*i*c*e*""#, true),
        ]
    );
}

#[test]
fn short_words_globs_and_phrases_skip_passes_that_add_nothing() {
    assert_eq!(
        searches("q3 rep", &HIDDEN),
        [
            expected_pass(r#""q3" "rep""#, false),
            expected_pass(r#""q3" "*r*e*p*""#, true)
        ]
    );
    assert_eq!(
        searches("inv*.pdf", &HIDDEN),
        [expected_pass(r#""inv*.pdf""#, false)]
    );
    assert_eq!(
        searches("abc", &HIDDEN).last(),
        Some(&expected_pass(r#""*a*b*c*""#, true))
    );
}

#[test]
fn folder_segments_become_path_terms() {
    assert_eq!(
        searches("alex/invoice", &HIDDEN),
        [
            expected_pass(r#"startwith:"invoice" path:"alex""#, false),
            expected_pass(r#""invoice" path:"alex""#, false),
            expected_pass(r#""*i*n*v*o*i*c*e*" path:"*a*l*e*x*""#, true),
        ]
    );
    assert_eq!(
        searches("inv*/2026", &HIDDEN)[0],
        expected_pass(r#"startwith:"2026" path:"*\inv*\*""#, false)
    );
}

#[test]
fn anchors_restrict_the_path_prefix() {
    assert_eq!(
        searches(r"~\inv", &HIDDEN)[0],
        expected_pass(r#"startwith:"inv" path:"C:\Users\alex\""#, false)
    );
    assert_eq!(
        searches(r"C:\inv", &HIDDEN)[0],
        expected_pass(r#"startwith:"inv" path:"C:\""#, false)
    );
}

#[test]
fn scope_and_hidden_items_add_filters() {
    let folders = SearchOptions {
        scope: SearchScope::Folders,
        ..VISIBLE
    };
    let files = SearchOptions {
        scope: SearchScope::Files,
        ..HIDDEN
    };
    assert_eq!(
        searches("abc", &folders)[0],
        expected_pass(r#"folder: !attrib:H startwith:"abc""#, false)
    );
    assert_eq!(
        searches("abc", &files)[0],
        expected_pass(r#"file: startwith:"abc""#, false)
    );
    assert!(passes(&parsed("alex/"), &HIDDEN).is_empty());
}

#[test]
fn browsing_finds_parents_then_lists_their_children() {
    let parents = parsed("users/alex/").browse_parents();
    assert!(!parents.browse && parents.anchor.is_none());
    assert_eq!(
        parents
            .name_seg
            .as_ref()
            .map(|segment| segment.raw.as_str()),
        Some("alex")
    );
    assert_eq!(parents.folder_segs.len(), 1);

    let many: Vec<String> = (0..70).map(|index| format!(r"C:\p{index}")).collect();
    let listings = browse_children_queries(&many, &VISIBLE);
    assert_eq!(listings.len(), 2);
    assert!(listings[0].starts_with(r#"!attrib:H <parent:"C:\p0" | parent:"C:\p1" | "#));
    assert_eq!(
        listings[1],
        r#"!attrib:H <parent:"C:\p64" | parent:"C:\p65" | parent:"C:\p66" | parent:"C:\p67" | parent:"C:\p68" | parent:"C:\p69">"#
    );
    assert_eq!(
        browse_children_queries(&[r"C:\".to_owned(), r"\\nas\home".to_owned()], &HIDDEN),
        [r#"<parent:"C:" | parent:"C:\" | parent:"\\nas\home">"#]
    );
}

fn row(path: &str, name: &str, modified: Option<u64>) -> EverythingRow {
    EverythingRow {
        path: path.to_owned(),
        file_name: name.to_owned(),
        is_folder: name.ends_with('/'),
        size: None,
        modified_filetime: modified,
    }
}

#[test]
fn rows_become_candidates_with_full_paths() {
    let drive = row_to_candidate(row(
        "C:",
        "Users",
        Some(116_444_736_000_000_000 + 10_000_000),
    ))
    .expect("candidate");
    assert_eq!(drive.path, r"C:\Users");
    assert_eq!(
        drive.modified,
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1))
    );
    assert_eq!(
        row_to_candidate(row(r"\\nas\home", "alex.k", None)).map(|candidate| candidate.path),
        Some(r"\\nas\home\alex.k".to_owned())
    );
    assert!(row_to_candidate(row(r"C:\a", "", None)).is_none());
    assert!(row_to_candidate(row(r"C:\a", "bad\u{FFFD}", None)).is_none());
    assert!(row_to_candidate(row("C:\\\u{FFFD}", "ok", None)).is_none());
}
