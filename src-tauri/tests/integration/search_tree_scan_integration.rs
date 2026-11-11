use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use file_explorer_lib::search::path_style::PathStyle;
use file_explorer_lib::search::query::{parse, ParsedQuery};
use file_explorer_lib::search::results::{rank_candidates, CANDIDATE_CAP};
use file_explorer_lib::search::source::FolderTreeSource;
use file_explorer_lib::search::tree_scan::{
    candidates, candidates_in_chunks, TreeSearchRun, SLOTS_PER_READ,
};
use file_explorer_lib::search::{CandidateBatch, Control, SearchError, SearchOptions, SearchScope};
use file_explorer_lib::size::index::names::FileNameStore;
use file_explorer_lib::size::index::tree::{DirectoryListing, FolderSizeTree};
use file_explorer_lib::size::index::IndexRead;

const NO_NAMES_FILE: &str = "no-file-names.bin";

/// A folder tree and its file names behind the source trait, counting reads.
pub(crate) struct Source {
    pub(crate) tree: Mutex<FolderSizeTree>,
    pub(crate) names: Mutex<FileNameStore>,
    pub(crate) blocked: Vec<PathBuf>,
    reads: AtomicUsize,
    pub(crate) fail: Option<SearchError>,
    pub(crate) compactions_after_reads: Vec<usize>,
}

impl Source {
    fn new(folders: &[&str]) -> Self {
        Self::from_tree(build(folders))
    }

    fn from_tree(tree: FolderSizeTree) -> Self {
        Self::with_names(tree, FileNameStore::new(PathBuf::from(NO_NAMES_FILE)))
    }

    pub(crate) fn with_names(tree: FolderSizeTree, names: FileNameStore) -> Self {
        Self {
            tree: Mutex::new(tree),
            names: Mutex::new(names),
            blocked: Vec::new(),
            reads: AtomicUsize::new(0),
            fail: None,
            compactions_after_reads: Vec::new(),
        }
    }

    pub(crate) fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }
}

impl FolderTreeSource for Source {
    fn read(&self, read: &mut dyn FnMut(IndexRead<'_>)) -> Result<(), SearchError> {
        if let Some(error) = &self.fail {
            return Err(error.clone());
        }
        let read_number = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
        let compactions_so_far = self
            .compactions_after_reads
            .iter()
            .filter(|&&after| after < read_number)
            .count();
        let tree = self.tree.lock().unwrap();
        let names = self.names.lock().unwrap();
        read(IndexRead {
            tree: &tree,
            names: &names,
            blocked: &self.blocked,
            generation: compactions_so_far as u64,
        });
        Ok(())
    }
}

/// A tree rooted at `/` holding every listed folder (and its ancestors).
pub(crate) fn build(folders: &[&str]) -> FolderSizeTree {
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    children.entry("/".to_owned()).or_default();
    for folder in folders {
        let mut parent = "/".to_owned();
        for name in folder.trim_start_matches('/').split('/') {
            children
                .entry(parent.clone())
                .or_default()
                .push(name.to_owned());
            parent = PathStyle::Posix.join(&parent, name);
            children.entry(parent.clone()).or_default();
        }
    }
    let mut tree = FolderSizeTree::default();
    assert!(tree.add_root(Path::new("/")));
    for (parent, names) in &children {
        assert!(
            tree.merge_record(Path::new(parent), DirectoryListing::scanned(0, names)),
            "{parent}"
        );
    }
    tree
}

const LIBRARY: &[&str] = &[
    "/Users/alex/Desktop",
    "/Users/alex/Documents/Invoices 2026",
    "/Users/alex/Documents/Work/Invoices",
    "/Users/alex/Invoices",
    "/Users/alex/.config/invoices",
    "/Users/alex/Projects/alex-site/invoices-api",
    "/Users/alexios/Invoices",
    "/Applications",
];

pub(crate) const VISIBLE: SearchOptions = SearchOptions {
    include_hidden: false,
    scope: SearchScope::All,
};

pub(crate) fn parsed(raw: &str) -> ParsedQuery {
    parse(raw, PathStyle::Posix, Some("/Users/alex")).expect("query")
}

pub(crate) fn never() -> bool {
    false
}

pub(crate) fn run(source: &Source, raw: &str, options: &SearchOptions) -> CandidateBatch {
    let control = Control {
        cancelled: &never,
        expired: &never,
    };
    candidates(
        &TreeSearchRun {
            source,
            options,
            control: &control,
        },
        &parsed(raw),
    )
    .expect("batch")
}

pub(crate) fn sorted(batch: &CandidateBatch) -> Vec<&str> {
    let mut paths: Vec<&str> = batch
        .candidates
        .iter()
        .map(|candidate| candidate.path.as_str())
        .collect();
    paths.sort_unstable();
    paths
}

#[test]
fn name_scans_return_matching_folders_as_posix_paths() {
    let source = Source::new(LIBRARY);
    let batch = run(&source, "inv", &VISIBLE);
    assert_eq!(
        sorted(&batch),
        [
            "/Users/alex/Documents/Invoices 2026",
            "/Users/alex/Documents/Work/Invoices",
            "/Users/alex/Invoices",
            "/Users/alex/Projects/alex-site/invoices-api",
            "/Users/alexios/Invoices",
        ]
    );
    assert!(batch
        .candidates
        .iter()
        .all(|candidate| candidate.is_dir && candidate.modified.is_none()));
    assert!(!batch.truncated);
    assert_eq!(
        source.reads(),
        3,
        "one folder chunk, one path read for the kept hits and the file names check"
    );
    assert_eq!(SLOTS_PER_READ, 32_768);
}

#[test]
fn folder_signatures_keep_non_ascii_and_case_folded_names_matchable() {
    let source = Source::new(&[
        "/Users/alex/Cafe\u{301}/Invoices",
        "/Users/alex/\u{212A}elvin",
        "/Users/alex/Résumé",
        "/Users/alex/Resume",
        "/Users/alex/KELP",
    ]);
    assert_eq!(
        sorted(&run(&source, "café", &VISIBLE)),
        ["/Users/alex/Cafe\u{301}"]
    );
    assert_eq!(
        sorted(&run(&source, "kelvin", &VISIBLE)),
        ["/Users/alex/\u{212A}elvin"]
    );
    assert_eq!(
        sorted(&run(&source, "résumé", &VISIBLE)),
        ["/Users/alex/Résumé"]
    );
    assert_eq!(
        sorted(&run(&source, "kelp", &VISIBLE)),
        ["/Users/alex/KELP"]
    );
    assert!(run(&source, "kelpz", &VISIBLE).candidates.is_empty());
    assert_eq!(
        sorted(&run(&source, "café/inv", &VISIBLE)),
        ["/Users/alex/Cafe\u{301}/Invoices"]
    );
}

#[test]
fn hidden_folders_follow_the_option() {
    let source = Source::new(LIBRARY);
    let with_hidden = SearchOptions {
        include_hidden: true,
        ..VISIBLE
    };
    assert!(sorted(&run(&source, "inv", &with_hidden)).contains(&"/Users/alex/.config/invoices"));
    assert!(!sorted(&run(&source, "config", &VISIBLE)).contains(&"/Users/alex/.config"));
}

#[test]
fn blocked_protected_roots_hide_their_contents_only() {
    let mut source = Source::new(LIBRARY);
    source.blocked = vec![PathBuf::from("/Users/alex/Documents/")];
    let hits = sorted(&run(&source, "doc", &VISIBLE)).join(",");
    assert_eq!(hits, "/Users/alex/Documents");
    assert!(!sorted(&run(&source, "inv", &VISIBLE))
        .iter()
        .any(|path| path.contains("/Documents/")));
}

#[test]
fn file_scope_finds_nothing_without_file_names() {
    let source = Source::new(LIBRARY);
    let files = SearchOptions {
        scope: SearchScope::Files,
        ..VISIBLE
    };
    let batch = run(&source, "inv", &files);
    assert!(batch.candidates.is_empty());
    assert!(!batch.truncated);
    assert_eq!(source.reads(), 1, "only the names availability check reads");
    assert!(!source.file_names_available());
}

#[test]
fn browsing_lists_children_of_the_best_matching_folders() {
    let source = Source::new(LIBRARY);
    let batch = run(&source, "alex/", &VISIBLE);
    let hits = rank_candidates(batch.candidates, &parsed("alex/"), PathStyle::Posix);
    let paths: Vec<&str> = hits.iter().map(|hit| hit.path.as_str()).collect();
    assert_eq!(
        &paths[..4],
        [
            "/Users/alex/Desktop",
            "/Users/alex/Documents",
            "/Users/alex/Invoices",
            "/Users/alex/Projects",
        ]
    );
    assert!(paths.contains(&"/Users/alexios/Invoices"));
    assert!(!paths.iter().any(|path| path.ends_with(".config")));
}

#[test]
fn anchored_queries_list_or_scan_inside_the_anchor() {
    let source = Source::new(LIBRARY);
    assert_eq!(
        sorted(&run(&source, "~/Documents/", &VISIBLE)),
        [
            "/Users/alex/Documents/Invoices 2026",
            "/Users/alex/Documents/Work"
        ]
    );
    assert!(run(&source, "/Nowhere/", &VISIBLE).candidates.is_empty());
    let scanned = run(&source, "~/Documents/inv", &VISIBLE);
    let hits = rank_candidates(
        scanned.candidates,
        &parsed("~/Documents/inv"),
        PathStyle::Posix,
    );
    let paths: Vec<&str> = hits.iter().map(|hit| hit.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "/Users/alex/Documents/Invoices 2026",
            "/Users/alex/Documents/Work/Invoices"
        ]
    );
}

#[test]
fn scans_read_in_chunks_and_honour_cancellation_and_budget() {
    let source = Source::new(LIBRARY);
    let control = Control {
        cancelled: &never,
        expired: &never,
    };
    let chunked = candidates_in_chunks(
        &TreeSearchRun {
            source: &source,
            options: &VISIBLE,
            control: &control,
        },
        &parsed("inv"),
        4,
    )
    .expect("batch");
    assert_eq!(chunked.candidates.len(), 5);
    assert!(source.reads() > 2);

    let yes = || true;
    let cancelled = Control {
        cancelled: &yes,
        expired: &never,
    };
    assert_eq!(
        candidates(
            &TreeSearchRun {
                source: &source,
                options: &VISIBLE,
                control: &cancelled
            },
            &parsed("inv")
        ),
        Err(SearchError::Cancelled)
    );

    let checks = Cell::new(0);
    let after_one = || {
        checks.set(checks.get() + 1);
        checks.get() > 1
    };
    let expiring = Control {
        cancelled: &never,
        expired: &after_one,
    };
    let partial = candidates_in_chunks(
        &TreeSearchRun {
            source: &source,
            options: &VISIBLE,
            control: &expiring,
        },
        &parsed("inv"),
        1,
    )
    .expect("partial");
    assert!(partial.truncated);
}

#[test]
fn freed_slots_are_skipped_and_reused_slots_reported() {
    let mut tree = build(LIBRARY);
    assert!(tree.prune(Path::new("/Users/alex/Documents")));
    let source = Source::from_tree(tree);
    assert!(!sorted(&run(&source, "inv", &VISIBLE))
        .iter()
        .any(|path| path.contains("Documents")));
    {
        let mut tree = source.tree.lock().unwrap();
        assert!(tree.merge_record(
            Path::new("/Users/alex"),
            DirectoryListing::scanned(0, &["Invoices", "Ledger"])
        ));
    }
    assert!(sorted(&run(&source, "ledger", &VISIBLE)).contains(&"/Users/alex/Ledger"));
}

#[test]
fn read_errors_propagate() {
    let mut source = Source::new(LIBRARY);
    source.fail = Some(SearchError::NotReady);
    let control = Control {
        cancelled: &never,
        expired: &never,
    };
    assert_eq!(
        candidates(
            &TreeSearchRun {
                source: &source,
                options: &VISIBLE,
                control: &control
            },
            &parsed("inv")
        ),
        Err(SearchError::NotReady)
    );
    assert_eq!(
        candidates(
            &TreeSearchRun {
                source: &source,
                options: &VISIBLE,
                control: &control
            },
            &parsed("~/")
        ),
        Err(SearchError::NotReady)
    );
}

#[test]
fn a_query_with_nothing_to_match_finds_nothing() {
    let source = Source::new(LIBRARY);
    let empty = ParsedQuery {
        anchor: None,
        folder_segs: Vec::new(),
        name_seg: None,
        browse: true,
    };
    let control = Control {
        cancelled: &never,
        expired: &never,
    };
    assert!(candidates(
        &TreeSearchRun {
            source: &source,
            options: &VISIBLE,
            control: &control
        },
        &empty
    )
    .expect("batch")
    .candidates
    .is_empty());
}

fn bulk(count: usize, exact: usize) -> Source {
    let mut names: Vec<String> = (0..count).map(|index| format!("inv-{index:05}")).collect();
    names.extend((0..exact).map(|index| format!("inv{index}")));
    names.push("inv".to_owned());
    let mut tree = FolderSizeTree::default();
    assert!(tree.add_root(Path::new("/")));
    assert!(tree.merge_record(Path::new("/"), DirectoryListing::scanned(0, &["bulk"])));
    assert!(tree.merge_record(Path::new("/bulk"), DirectoryListing::scanned(0, &names)));
    Source::from_tree(tree)
}

#[test]
fn broad_scans_keep_the_best_tiers_up_to_the_cap() {
    let control = Control {
        cancelled: &never,
        expired: &never,
    };
    for count in [2 * CANDIDATE_CAP + 500, CANDIDATE_CAP + 500] {
        let source = bulk(count, 0);
        let batch = candidates_in_chunks(
            &TreeSearchRun {
                source: &source,
                options: &VISIBLE,
                control: &control,
            },
            &parsed("inv"),
            1_000,
        )
        .expect("batch");
        assert!(batch.truncated, "{count}");
        assert_eq!(batch.candidates.len(), CANDIDATE_CAP);
        assert!(
            batch
                .candidates
                .iter()
                .any(|candidate| candidate.path == "/bulk/inv"),
            "{count}"
        );
    }
}

#[test]
fn listing_a_huge_folder_stops_at_the_cap() {
    let source = bulk(CANDIDATE_CAP + 10, 0);
    let batch = run(&source, "/bulk/", &VISIBLE);
    assert!(batch.truncated);
    assert_eq!(batch.candidates.len(), CANDIDATE_CAP);
}

#[test]
fn a_compaction_during_a_query_restarts_it_once_with_the_same_hits() {
    let steady = Source::new(LIBRARY);
    let expected = sorted(&run(&steady, "inv", &VISIBLE)).join(",");
    let reads_per_query = steady.reads();
    let mut compacted = Source::new(LIBRARY);
    compacted.compactions_after_reads = vec![1];

    let batch = run(&compacted, "inv", &VISIBLE);

    assert_eq!(sorted(&batch).join(","), expected);
    assert!(!batch.truncated);
    assert_eq!(compacted.reads(), 2 * reads_per_query, "one restart");
}

#[test]
fn a_second_compaction_during_the_restart_marks_the_hits_truncated() {
    let steady = Source::new(LIBRARY);
    run(&steady, "inv", &VISIBLE);
    let reads_per_query = steady.reads();
    let mut compacted = Source::new(LIBRARY);
    compacted.compactions_after_reads = vec![1, reads_per_query + 1];

    let batch = run(&compacted, "inv", &VISIBLE);

    assert!(batch.truncated);
    assert_eq!(compacted.reads(), 2 * reads_per_query, "never a third try");
}
