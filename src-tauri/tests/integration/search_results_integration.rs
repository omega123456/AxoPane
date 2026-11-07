use file_explorer_lib::search::path_style::PathStyle;
use file_explorer_lib::search::query::parse;
use file_explorer_lib::search::results::{rank_candidates, ResultSet, CANDIDATE_CAP, PAGE_SIZE};
use file_explorer_lib::search::Candidate;

fn folder(path: String) -> Candidate {
    Candidate {
        path,
        is_dir: true,
        modified: None,
    }
}

#[test]
fn ranking_drops_duplicates_and_non_matches() {
    let query = parse("invoice", PathStyle::Posix, None).expect("query");
    let hits = rank_candidates(
        vec![
            folder("/a/invoices".into()),
            folder("/a/invoices".into()),
            folder("/a/receipts".into()),
            folder("/b/invoice".into()),
        ],
        &query,
        PathStyle::Posix,
    );
    let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(paths, ["/b/invoice", "/a/invoices"]);
}

#[test]
fn result_sets_serve_pages_of_fifty() {
    assert_eq!((PAGE_SIZE, CANDIDATE_CAP), (50, 5_000));
    let query = parse("item", PathStyle::Posix, None).expect("query");
    let hits = rank_candidates(
        (0..120).map(|i| folder(format!("/root/item-{i:03}"))),
        &query,
        PathStyle::Posix,
    );
    let set = ResultSet::new(7, hits, true);
    assert_eq!(
        (set.query_id(), set.total(), set.truncated()),
        (7, 120, true)
    );
    assert_eq!(set.page(0).len(), 50);
    assert_eq!(set.page(0)[0].path, "/root/item-000");
    assert_eq!(set.page(100).len(), 20);
    assert_eq!(set.page(100)[0].path, "/root/item-100");
    assert!(set.page(500).is_empty());
    assert!(set.page(usize::MAX).is_empty());
}
