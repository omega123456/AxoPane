use std::cell::Cell;
use std::sync::Mutex;

use file_explorer_lib::search::everything_strategy::{candidates, FUZZY_PASS_THRESHOLD};
use file_explorer_lib::search::path_style::PathStyle;
use file_explorer_lib::search::query::{parse, ParsedQuery};
use file_explorer_lib::search::results::BROWSE_PARENT_CAP;
use file_explorer_lib::search::source::EverythingSource;
use file_explorer_lib::search::{CandidateBatch, Control, SearchError, SearchOptions, SearchScope};
use file_explorer_lib::size::everything::{EverythingQuery, EverythingQueryResult, EverythingRow};

type Respond = dyn Fn(&EverythingQuery) -> Result<EverythingQueryResult, SearchError> + Send + Sync;

/// An Everything source answered by a closure, recording every query.
struct Source {
    respond: Box<Respond>,
    seen: Mutex<Vec<EverythingQuery>>,
}

impl Source {
    fn new(
        respond: impl Fn(&EverythingQuery) -> Result<EverythingQueryResult, SearchError>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        Self {
            respond: Box::new(respond),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn searches(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .map(|q| q.search.clone())
            .collect()
    }
}

impl EverythingSource for Source {
    fn run(&self, query: &EverythingQuery) -> Result<EverythingQueryResult, SearchError> {
        self.seen.lock().unwrap().push(query.clone());
        (self.respond)(query)
    }
}

fn rows(paths: &[(&str, bool)]) -> Vec<EverythingRow> {
    paths
        .iter()
        .map(|(path, is_folder)| {
            let (parent, name) = path.rsplit_once('\\').expect("path");
            EverythingRow {
                path: parent.to_owned(),
                file_name: name.to_owned(),
                is_folder: *is_folder,
                size: None,
                modified_filetime: None,
            }
        })
        .collect()
}

fn result(paths: &[(&str, bool)], total: u32) -> EverythingQueryResult {
    EverythingQueryResult {
        rows: rows(paths),
        total,
    }
}

const ALL: SearchOptions = SearchOptions {
    include_hidden: true,
    scope: SearchScope::All,
};

fn q(raw: &str) -> ParsedQuery {
    parse(raw, PathStyle::Windows, Some(r"C:\Users\alex")).expect("query")
}

fn run(source: &Source, raw: &str) -> Result<CandidateBatch, SearchError> {
    let never = || false;
    let control = Control {
        cancelled: &never,
        expired: &never,
    };
    candidates(source, &q(raw), &ALL, &control)
}

fn paths(batch: &CandidateBatch) -> Vec<&str> {
    batch.candidates.iter().map(|c| c.path.as_str()).collect()
}

#[test]
fn passes_run_strictest_first_and_merge_without_duplicates() {
    let source = Source::new(|q| {
        Ok(if q.search.starts_with("startwith") {
            result(&[(r"C:\a\invoice.pdf", false)], 1)
        } else {
            result(
                &[(r"C:\a\invoice.pdf", false), (r"C:\b\old invoice", true)],
                2,
            )
        })
    });
    let batch = run(&source, "invoice").expect("batch");
    assert_eq!(paths(&batch), [r"C:\a\invoice.pdf", r"C:\b\old invoice"]);
    assert!(!batch.truncated);
    assert_eq!(source.searches().len(), 3);
    let seen = source.seen.lock().unwrap();
    assert_eq!(seen[0].max, 5_000);
    assert_eq!(seen[1].max, 4_999);
}

#[test]
fn the_fuzzy_pass_is_skipped_when_strict_passes_found_plenty() {
    let many: Vec<String> = (0..FUZZY_PASS_THRESHOLD)
        .map(|i| format!(r"C:\x\invoice-{i}"))
        .collect();
    let source = Source::new(move |_| {
        let refs: Vec<(&str, bool)> = many.iter().map(|p| (p.as_str(), false)).collect();
        Ok(result(&refs, refs.len() as u32))
    });
    run(&source, "invoice").expect("batch");
    assert!(source.searches().iter().all(|s| !s.contains('*')));
}

#[test]
fn more_matches_than_returned_mark_the_batch_truncated() {
    let source = Source::new(|_| Ok(result(&[(r"C:\a\invoice", true)], 9_000)));
    assert!(run(&source, "invoice").expect("batch").truncated);
}

#[test]
fn a_full_cap_stops_further_passes() {
    let full: Vec<String> = (0..5_000).map(|i| format!(r"C:\x\invoice-{i}")).collect();
    let source = Source::new(move |_| {
        let refs: Vec<(&str, bool)> = full.iter().map(|p| (p.as_str(), false)).collect();
        Ok(result(&refs, 5_000))
    });
    let batch = run(&source, "invoice").expect("batch");
    assert!(batch.truncated);
    assert_eq!(batch.candidates.len(), 5_000);
    assert_eq!(source.searches().len(), 1);
}

#[test]
fn cancellation_errors_and_an_expired_budget_keeps_partial_results() {
    let source = Source::new(|_| Ok(result(&[(r"C:\a\invoice", true)], 1)));
    let yes = || true;
    let no = || false;
    let cancelled = Control {
        cancelled: &yes,
        expired: &no,
    };
    assert_eq!(
        candidates(&source, &q("invoice"), &ALL, &cancelled),
        Err(SearchError::Cancelled)
    );

    let calls = Cell::new(0);
    let after_one = || {
        calls.set(calls.get() + 1);
        calls.get() > 1
    };
    let expiring = Control {
        cancelled: &no,
        expired: &after_one,
    };
    let batch = candidates(&source, &q("invoice"), &ALL, &expiring).expect("partial");
    assert!(batch.truncated);
    assert_eq!(paths(&batch), [r"C:\a\invoice"]);
}

#[test]
fn backend_errors_propagate() {
    let source = Source::new(|_| Err(SearchError::NotReady));
    assert_eq!(run(&source, "invoice"), Err(SearchError::NotReady));
}

#[test]
fn unanchored_browsing_lists_children_of_the_best_matching_folders() {
    let source = Source::new(|q| {
        Ok(if q.search.contains("parent:") {
            result(
                &[
                    (r"C:\Users\alex\Desktop", true),
                    (r"D:\Libraries\alex\invoice.pdf", false),
                ],
                2,
            )
        } else {
            result(
                &[
                    (r"D:\Libraries\alex", true),
                    (r"C:\Users\alex", true),
                    (r"C:\Users\alexios", true),
                ],
                3,
            )
        })
    });
    let batch = run(&source, "alex/").expect("batch");
    assert_eq!(
        paths(&batch),
        [r"C:\Users\alex\Desktop", r"D:\Libraries\alex\invoice.pdf"]
    );
    let searches = source.searches();
    assert!(searches[0].starts_with(r#"folder: startwith:"alex""#));
    let listing = searches.last().expect("listing");
    assert!(listing.contains(r#"parent:"C:\Users\alex""#));
    assert!(listing.contains(r#"parent:"D:\Libraries\alex""#));
    assert!(listing.contains(r#"parent:"C:\Users\alexios""#));
    assert_eq!(BROWSE_PARENT_CAP, 128);
}

#[test]
fn anchored_browsing_lists_the_anchor_directly() {
    let source = Source::new(|_| Ok(result(&[(r"C:\Users\alex\Documents", true)], 1)));
    let batch = run(&source, "~/").expect("batch");
    assert_eq!(paths(&batch), [r"C:\Users\alex\Documents"]);
    assert_eq!(source.searches(), [r#"<parent:"C:\Users\alex">"#]);
}
