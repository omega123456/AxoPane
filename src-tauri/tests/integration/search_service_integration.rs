use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use file_explorer_lib::search::service::{
    RunTicket, SearchBackend, SearchBackendKind, SearchCapabilities, SearchPageRequest,
    SearchQueryRequest, SearchRejection, SearchService, SearchTicket,
};
use file_explorer_lib::search::source::FolderTreeSource;
use file_explorer_lib::search::{SearchError, SearchScope};
use file_explorer_lib::size::everything::{
    EverythingHandle, EverythingQuery, EverythingQueryResult, EverythingRow,
};
use file_explorer_lib::size::index::names::block::BLOCK_TARGET_BYTES;
use file_explorer_lib::size::index::names::file::NAMES_FILE;
use file_explorer_lib::size::index::names::{FileNameStore, PackedNames};
use file_explorer_lib::size::index::segment::SegmentTarget;
use file_explorer_lib::size::index::tree::{DirectoryListing, FolderSizeTree};
use file_explorer_lib::size::index::IndexRead;
use file_explorer_lib::size::SizeService;
use serde_json::json;
use tempfile::tempdir;

use crate::common::segment::{compacted, index_over, paths_in, Compaction};

const SEGMENT_TOKEN: u64 = 1;

struct RowKind {
    is_folder: bool,
    modified: Option<u64>,
}

fn row(parent: &str, name: &str, kind: RowKind) -> EverythingRow {
    EverythingRow {
        path: parent.to_owned(),
        file_name: name.to_owned(),
        is_folder: kind.is_folder,
        size: None,
        modified_filetime: kind.modified,
    }
}

/// An Everything backend whose every query returns `rows`.
fn everything(rows: Vec<EverythingRow>) -> SearchBackend {
    SearchBackend::Everything(Arc::new(EverythingHandle::test_with_responder(Arc::new(
        move |_: &EverythingQuery| {
            Some(EverythingQueryResult {
                rows: rows.clone(),
                total: rows.len() as u32,
            })
        },
    ))))
}

fn library() -> Vec<EverythingRow> {
    let mut rows: Vec<EverythingRow> = (0..60)
        .map(|index| {
            row(
                r"C:\Users\alex\Invoices",
                &format!("invoice-{index:02}.pdf"),
                RowKind {
                    is_folder: false,
                    modified: None,
                },
            )
        })
        .collect();
    rows.push(row(
        r"C:\Users\alex",
        "Invoices",
        RowKind {
            is_folder: true,
            modified: Some(116_444_736_000_000_000),
        },
    ));
    rows
}

fn request(query: &str) -> SearchQueryRequest {
    SearchQueryRequest {
        query: query.to_owned(),
        include_hidden: true,
        scope: SearchScope::All,
    }
}

fn live(ticket: SearchTicket) -> RunTicket<'static> {
    RunTicket {
        ticket,
        cancelled: &|| false,
    }
}

fn search(
    service: &SearchService,
    query: &str,
) -> Result<file_explorer_lib::search::service::SearchPage, SearchRejection> {
    service.run(live(service.begin()), &request(query))
}

#[test]
fn a_query_ranks_hits_and_serves_them_in_pages() {
    let service = SearchService::new(everything(library()), Some(r"C:\Users\alex".into()));
    let first = search(&service, "invoice").expect("page");
    assert_eq!(first.total, 61);
    assert_eq!(first.offset, 0);
    assert_eq!(first.items.len(), 50);
    assert!(!first.truncated);
    let top = &first.items[0];
    assert_eq!(top.path, r"C:\Users\alex\Invoices");
    assert_eq!(top.parent_path, r"C:\Users\alex");
    assert!(top.is_dir);
    assert_eq!(top.name_ranges, vec![[0, 7]]);
    assert_eq!(top.modified_at.as_deref(), Some("1970-01-01T00:00:00Z"));
    assert_eq!(
        first.capabilities,
        SearchCapabilities {
            backend: SearchBackendKind::Everything,
            folders_only: false,
        }
    );

    let second = service
        .page(&SearchPageRequest {
            query_id: first.query_id,
            offset: 50,
        })
        .expect("second page");
    assert_eq!(second.items.len(), 11);
    assert_eq!(second.offset, 50);
}

#[test]
fn newer_queries_and_closing_make_older_pages_stale() {
    let service = SearchService::new(everything(library()), None);
    let first = search(&service, "invoice").expect("page");
    let older = service.begin();
    assert!(!older.cancellation.is_cancelled());
    let newer = service.begin();
    assert!(older.cancellation.is_cancelled());
    assert_eq!(
        service.page(&SearchPageRequest {
            query_id: first.query_id,
            offset: 0,
        }),
        Err(SearchRejection::Stale)
    );
    assert_eq!(
        service.run(live(older), &request("invoice")),
        Err(SearchRejection::Stale)
    );
    let current = service
        .run(live(newer), &request("invoice"))
        .expect("current");
    service.close();
    assert_eq!(
        service.page(&SearchPageRequest {
            query_id: current.query_id,
            offset: 0,
        }),
        Err(SearchRejection::Stale)
    );
}

#[test]
fn rejections_cover_short_queries_and_backend_states() {
    let service = SearchService::new(everything(library()), None);
    assert_eq!(search(&service, "in"), Err(SearchRejection::TooShort));
    assert_eq!(
        service.run(
            RunTicket {
                ticket: service.begin(),
                cancelled: &|| true,
            },
            &request("invoice"),
        ),
        Err(SearchRejection::Stale)
    );

    let none = SearchService::new(SearchBackend::None, None);
    assert_eq!(search(&none, "invoice"), Err(SearchRejection::Unavailable));
    assert_eq!(none.capabilities().backend, SearchBackendKind::None);

    let not_ready = SearchService::new(
        SearchBackend::Everything(Arc::new(EverythingHandle::test_not_ready())),
        None,
    );
    assert_eq!(
        search(&not_ready, "invoice"),
        Err(SearchRejection::NotReady)
    );
    let failing = SearchService::new(
        SearchBackend::Everything(Arc::new(EverythingHandle::test_available_error())),
        None,
    );
    assert!(matches!(
        search(&failing, "invoice"),
        Err(SearchRejection::Failed { .. })
    ));
}

#[test]
fn scope_filters_hits_and_a_spent_budget_returns_a_truncated_page() {
    let service = SearchService::new(everything(library()), None);
    let ticket = service.begin();
    let folders = service
        .run(
            live(ticket),
            &SearchQueryRequest {
                scope: SearchScope::Folders,
                ..request("invoice")
            },
        )
        .expect("folders");
    assert_eq!(folders.total, 1);
    let ticket = service.begin();
    let files = service
        .run(
            live(ticket),
            &SearchQueryRequest {
                scope: SearchScope::Files,
                ..request("invoice")
            },
        )
        .expect("files");
    assert_eq!(files.total, 60);

    let spent = SearchService::with_budget(everything(library()), None, Duration::ZERO);
    let page = search(&spent, "invoice").expect("partial");
    assert!(page.truncated);
    assert_eq!(page.total, 0);
}

/// A folder tree source that can supersede the query while it is read.
struct Tree {
    tree: FolderSizeTree,
    names: FileNameStore,
    during_read: Mutex<Option<Box<dyn Fn() + Send>>>,
}

impl FolderTreeSource for Tree {
    fn read(&self, read: &mut dyn FnMut(IndexRead<'_>)) -> Result<(), SearchError> {
        if let Some(hook) = self.during_read.lock().unwrap().as_ref() {
            hook();
        }
        read(IndexRead {
            tree: &self.tree,
            names: &self.names,
            blocked: &[],
            generation: 0,
        });
        Ok(())
    }
}

fn folder_tree(root: &Path, children: &[&str]) -> FolderSizeTree {
    let mut tree = FolderSizeTree::default();
    assert!(tree.add_root(root));
    assert!(tree.merge_record(root, DirectoryListing::scanned(0, children)));
    tree
}

fn tree_at(root: &Path, children: &[&str]) -> Arc<Tree> {
    Arc::new(Tree {
        tree: folder_tree(root, children),
        names: FileNameStore::new(root.join(NAMES_FILE)),
        during_read: Mutex::new(None),
    })
}

/// A tree whose root folder holds `files` in a saved segment.
fn tree_with_files(root: &Path, files: &[&str]) -> Arc<Tree> {
    let tree = folder_tree(root, &["documents"]);
    let mut names = FileNameStore::new(root.join(NAMES_FILE));
    let root_slot = tree.lookup(root).expect("root slot");
    names.set_names(
        root_slot,
        PackedNames::from_names(files.iter().map(|file| file.as_bytes())),
    );
    let segment = compacted(Compaction {
        tree: &tree,
        names: &names,
        target: SegmentTarget {
            paths: paths_in(root),
            token: SEGMENT_TOKEN,
        },
        block_target: BLOCK_TARGET_BYTES,
    });
    let index = index_over(segment, &root.join(NAMES_FILE));
    Arc::new(Tree {
        tree: index.tree,
        names: index.names,
        during_read: Mutex::new(None),
    })
}

#[test]
fn the_tree_backend_finds_files_once_file_names_exist() {
    let dir = tempdir().expect("temp dir");
    std::fs::write(dir.path().join("documents.txt"), b"x").expect("file");
    let tree = tree_with_files(dir.path(), &["documents.txt", "notes.md"]);
    let service = SearchService::new(SearchBackend::Tree(tree), None);
    assert_eq!(
        service.capabilities(),
        SearchCapabilities {
            backend: SearchBackendKind::MacIndex,
            folders_only: false,
        }
    );
    let page = search(&service, "documents").expect("page");
    assert_eq!(page.total, 2);
    assert!(page.items[0].is_dir, "the exact folder ranks first");
    assert!(!page.items[1].is_dir);
    assert!(page.items[1].modified_at.is_some());
    assert!(!page.capabilities.folders_only);
    let ticket = service.begin();
    let files = service
        .run(
            live(ticket),
            &SearchQueryRequest {
                scope: SearchScope::Files,
                ..request("documents")
            },
        )
        .expect("files");
    assert_eq!(files.total, 1);
    assert!(files.items.iter().all(|hit| !hit.is_dir));
}

#[test]
fn tree_hits_are_stat_ed_when_their_page_is_served() {
    let dir = tempdir().expect("temp dir");
    std::fs::create_dir(dir.path().join("documents")).expect("folder");
    let tree = tree_at(dir.path(), &["documents", "downloads-gone"]);
    let service = SearchService::new(SearchBackend::Tree(tree), None);
    assert_eq!(
        service.capabilities(),
        SearchCapabilities {
            backend: SearchBackendKind::MacIndex,
            folders_only: true,
        }
    );
    let page = search(&service, "do").err();
    assert_eq!(page, Some(SearchRejection::TooShort));
    let page = search(&service, "docu").expect("page");
    assert_eq!(page.items.len(), 1);
    assert!(page.items[0].modified_at.is_some());
    let gone = search(&service, "downloads").expect("page");
    assert_eq!(gone.items[0].modified_at, None);
}

#[test]
fn a_query_superseded_after_collection_is_stale() {
    let dir = tempdir().expect("temp dir");
    let tree = tree_at(dir.path(), &["documents"]);
    let service = Arc::new(SearchService::new(SearchBackend::Tree(tree.clone()), None));
    let ticket = service.begin();
    // Scope `files` reads no file names from a folder-only tree and never
    // checks for cancellation, so only the final generation check fires.
    service.begin();
    assert_eq!(
        service.run(
            live(ticket),
            &SearchQueryRequest {
                scope: SearchScope::Files,
                ..request("documents")
            },
        ),
        Err(SearchRejection::Stale)
    );
    let weak = Arc::downgrade(&service);
    *tree.during_read.lock().unwrap() = Some(Box::new(move || {
        if let Some(service) = weak.upgrade() {
            service.begin();
        }
    }));
    assert_eq!(search(&service, "documents"), Err(SearchRejection::Stale));
}

#[test]
fn backends_come_from_the_size_service() {
    let sizes = SizeService::with_everything_handle(
        Duration::from_secs(1),
        Some(EverythingHandle::test_available(Default::default())),
    );
    let service = SearchService::new(SearchBackend::from_sizes(&sizes), None);
    assert_eq!(
        service.capabilities().backend,
        SearchBackendKind::Everything
    );
    assert!(sizes.everything_handle().is_some());
    let without = SizeService::with_everything_handle(Duration::from_secs(1), None);
    assert_eq!(
        SearchService::new(SearchBackend::from_sizes(&without), None)
            .capabilities()
            .backend,
        SearchBackendKind::None
    );
}

#[test]
fn rejections_and_requests_use_the_frontend_shapes() {
    assert_eq!(
        SearchRejection::from_executor("IPC latency request cancelled".into()),
        SearchRejection::Stale
    );
    assert_eq!(
        SearchRejection::from_executor("queue full".into()),
        SearchRejection::Failed {
            message: "queue full".into()
        }
    );
    assert_eq!(
        SearchRejection::from(SearchError::Unavailable),
        SearchRejection::Unavailable
    );
    assert_eq!(
        serde_json::to_value(SearchRejection::Failed {
            message: "boom".into()
        })
        .unwrap(),
        json!({ "kind": "failed", "message": "boom" })
    );
    assert_eq!(
        serde_json::to_value(SearchRejection::TooShort).unwrap(),
        json!({ "kind": "tooShort" })
    );
    let parsed: SearchQueryRequest = serde_json::from_value(json!({ "query": "abc" })).unwrap();
    assert_eq!(
        parsed,
        SearchQueryRequest {
            query: "abc".into(),
            include_hidden: false,
            scope: SearchScope::All,
        }
    );
    let scoped: SearchQueryRequest = serde_json::from_value(
        json!({ "query": "abc", "includeHidden": true, "scope": "folders" }),
    )
    .unwrap();
    assert_eq!(scoped.scope, SearchScope::Folders);
    assert!(scoped.include_hidden);
    let page: SearchPageRequest =
        serde_json::from_value(json!({ "queryId": 3, "offset": 50 })).unwrap();
    assert_eq!(page.query_id, 3);
}
