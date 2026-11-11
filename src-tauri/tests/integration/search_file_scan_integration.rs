use std::cell::Cell;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use file_explorer_lib::search::file_scan::BLOCKS_PER_READ;
use file_explorer_lib::search::path_style::PathStyle;
use file_explorer_lib::search::results::{rank_candidates, CANDIDATE_CAP};
use file_explorer_lib::search::source::FolderTreeSource;
use file_explorer_lib::search::tree_scan::{candidates, TreeSearchRun};
use file_explorer_lib::search::{CandidateBatch, Control, SearchError, SearchOptions, SearchScope};
use file_explorer_lib::size::index::names::file::NAMES_FILE;
use file_explorer_lib::size::index::names::{FileNameStore, PackedNames};
use file_explorer_lib::size::index::segment::{MappedSegment, SegmentTarget};
use file_explorer_lib::size::index::tree::{DirectoryListing, FolderSizeTree, Graft};
use tempfile::{tempdir, TempDir};

use crate::common::segment::{
    compacted, graft_walk_segment, index_over, paths_in, Compaction, SegmentPlace, Walked,
};
use crate::search_tree_scan_integration::{build, never, parsed, run, sorted, Source, VISIBLE};

pub(crate) const SMALL_BLOCK_TARGET: usize = 64;
const LARGE_BLOCK_TARGET: usize = 256 * 1024;
const SEGMENT_TOKEN: u64 = 7;
const GRAFT_TOKEN: u64 = 8;
const PATH_SEPARATOR: char = '/';
const GRAFT_DIRECTORY_PREFIX: &str = "graft";
const HEADER_BYTES: u64 = 48;
const CORRUPTION: [u8; 4] = [0xEE; 4];

const FILLER_FOLDERS: usize = 40;
const DOT_ROOT: &str = ".dot-root";
const DOT_ROOT_CACHE: &str = ".dot-root/.cache";
const DOT_ROOT_DOCS: &str = ".dot-root/docs";

pub(crate) struct Folder {
    pub(crate) path: String,
    pub(crate) files: Vec<String>,
}

pub(crate) fn folder(path: &str, files: &[&str]) -> Folder {
    Folder {
        path: path.to_owned(),
        files: files.iter().map(|file| (*file).to_owned()).collect(),
    }
}

pub(crate) fn library() -> Vec<Folder> {
    vec![
        folder(
            "/Users/alex/project",
            &["README.md", "notes.txt", ".hidden.md"],
        ),
        folder("/Users/alex/project/docs", &["guide.md", "diagram.png"]),
        folder("/Users/alex/project/docs/deep/deeper", &["buried.md"]),
        folder("/Users/alex/project/.agent/plans", &["plan.md"]),
        folder("/Users/alex/other", &["readme.md", "stray.md"]),
        folder("/Users/alex/Documents", &["invoice-2026.pdf", "letter.md"]),
        folder("/Users/alex/Documents/Work", &["invoice-old.pdf"]),
        folder("/Users/alex/Invoices", &[]),
        folder("/Applications", &["Calendar.app"]),
    ]
}

fn wide_library() -> Vec<Folder> {
    let mut folders = library();
    let filler = (0..FILLER_FOLDERS).map(|index| Folder {
        path: format!("/Users/alex/archive/box-{index:02}"),
        files: vec![format!("record-{index:02}.md")],
    });
    folders.extend(filler);
    folders
}

pub(crate) const WITH_HIDDEN: SearchOptions = SearchOptions {
    include_hidden: true,
    scope: SearchScope::All,
};

pub(crate) const FILES: SearchOptions = SearchOptions {
    include_hidden: false,
    scope: SearchScope::Files,
};

pub(crate) const FOLDERS: SearchOptions = SearchOptions {
    include_hidden: false,
    scope: SearchScope::Folders,
};

pub(crate) struct Fixture {
    dir: TempDir,
    pub(crate) source: Source,
}

impl Fixture {
    pub(crate) fn new(folders: &[Folder]) -> Self {
        Self::with_block_target(folders, LARGE_BLOCK_TARGET)
    }

    pub(crate) fn with_block_target(folders: &[Folder], block_target: usize) -> Self {
        Self::over_tree(tree_of(folders), folders, block_target)
    }

    fn over_tree(tree: FolderSizeTree, folders: &[Folder], block_target: usize) -> Self {
        let dir = tempdir().expect("temp dir");
        let mut names = FileNameStore::new(dir.path().join(NAMES_FILE));
        for folder in folders {
            names.set_names(slot(&tree, &folder.path), packed(&folder.files));
        }
        let unsaved = UnsavedIndex { tree, names };
        Self {
            source: saved_in_a_segment(dir.path(), unsaved, block_target),
            dir,
        }
    }

    pub(crate) fn slot(&self, path: &str) -> u32 {
        slot(&self.source.tree.lock().unwrap(), path)
    }

    fn blocks_read(&self) -> u64 {
        self.source.names.lock().unwrap().counters().blocks_read
    }

    fn block_count(&self) -> u64 {
        self.source.names.lock().unwrap().block_count() as u64
    }

    pub(crate) fn graft(&self, top: &str, folders: &[Folder]) -> Range<u32> {
        let directory = self.graft_directory(top);
        let place = SegmentPlace {
            directory: &directory,
            root: Path::new(top),
            token: GRAFT_TOKEN,
        };
        let segment = Arc::new(graft_walk_segment(place, &walk_records(folders)));
        let file_names = segment.shared_file_names();
        let mut tree = self.source.tree.lock().unwrap();
        let mut names = self.source.names.lock().unwrap();
        let ids = tree
            .install_graft(Graft {
                path: Path::new(top),
                segment,
                kept: &[],
            })
            .expect("graft installed");
        names.attach_segment(ids.clone(), file_names);
        for removed in tree.take_removed_slots() {
            names.tombstone(removed);
        }
        for removed in tree.take_removed_ranges() {
            names.tombstone_range(removed);
        }
        ids
    }

    fn graft_directory(&self, top: &str) -> PathBuf {
        let flattened = top.replace(PATH_SEPARATOR, "-");
        let directory = self
            .dir
            .path()
            .join(format!("{GRAFT_DIRECTORY_PREFIX}{flattened}"));
        std::fs::create_dir_all(&directory).expect("graft directory");
        directory
    }

    fn ranked(&self, raw: &str, options: &SearchOptions) -> Vec<String> {
        ranked(&self.source, raw, options)
    }
}

fn walk_records(folders: &[Folder]) -> Vec<Walked> {
    folders
        .iter()
        .map(|walked| Walked {
            path: PathBuf::from(&walked.path),
            direct: 0,
            denied: false,
            subdirs: child_names(folders, &walked.path),
            files: walked.files.clone(),
        })
        .collect()
}

fn child_names(folders: &[Folder], parent: &str) -> Vec<String> {
    let children = folders
        .iter()
        .map(|folder| Path::new(&folder.path))
        .filter(|path| path.parent() == Some(Path::new(parent)));
    children
        .filter_map(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect()
}

fn tree_of(folders: &[Folder]) -> FolderSizeTree {
    let paths: Vec<&str> = folders.iter().map(|folder| folder.path.as_str()).collect();
    build(&paths)
}

fn slot(tree: &FolderSizeTree, path: &str) -> u32 {
    tree.lookup(Path::new(path)).expect("folder in tree")
}

pub(crate) fn packed<Name: AsRef<str>>(files: &[Name]) -> PackedNames {
    PackedNames::from_names(files.iter().map(|file| file.as_ref().as_bytes()))
}

struct UnsavedIndex {
    tree: FolderSizeTree,
    names: FileNameStore,
}

fn source_over(segment: MappedSegment, directory: &Path) -> Source {
    let index = index_over(segment, &directory.join(NAMES_FILE));
    Source::with_names(index.tree, index.names)
}

fn saved_in_a_segment(directory: &Path, unsaved: UnsavedIndex, block_target: usize) -> Source {
    let segment = compacted(Compaction {
        tree: &unsaved.tree,
        names: &unsaved.names,
        target: SegmentTarget {
            paths: paths_in(directory),
            token: SEGMENT_TOKEN,
        },
        block_target,
    });
    source_over(segment, directory)
}

pub(crate) fn ranked(source: &Source, raw: &str, options: &SearchOptions) -> Vec<String> {
    let batch = run(source, raw, options);
    rank_candidates(batch.candidates, &parsed(raw), PathStyle::Posix)
        .into_iter()
        .map(|hit| hit.path)
        .collect()
}

fn sorted_ranked(fixture: &Fixture, raw: &str, options: &SearchOptions) -> Vec<String> {
    let mut paths = fixture.ranked(raw, options);
    paths.sort_unstable();
    paths
}

fn run_with(
    source: &Source,
    raw: &str,
    control: &Control<'_>,
) -> Result<CandidateBatch, SearchError> {
    candidates(
        &TreeSearchRun {
            source,
            options: &FILES,
            control,
        },
        &parsed(raw),
    )
}

#[test]
fn folder_segment_queries_find_files_at_any_depth_under_matching_folders() {
    let fixture = Fixture::new(&library());
    assert_eq!(
        sorted_ranked(&fixture, "project/*.md", &WITH_HIDDEN),
        [
            "/Users/alex/project/.agent/plans/plan.md",
            "/Users/alex/project/.hidden.md",
            "/Users/alex/project/README.md",
            "/Users/alex/project/docs/deep/deeper/buried.md",
            "/Users/alex/project/docs/guide.md",
        ]
    );
    assert_eq!(
        sorted_ranked(&fixture, "project/*.md", &VISIBLE),
        [
            "/Users/alex/project/README.md",
            "/Users/alex/project/docs/deep/deeper/buried.md",
            "/Users/alex/project/docs/guide.md",
        ]
    );
    assert_eq!(
        sorted_ranked(&fixture, "plans/*.md", &VISIBLE),
        Vec::<String>::new(),
        "a hidden matching folder hides its files"
    );
}

#[test]
fn plain_names_find_files_anywhere() {
    let fixture = Fixture::new(&library());
    let paths = sorted_ranked(&fixture, "readme", &VISIBLE);
    assert_eq!(
        paths,
        [
            "/Users/alex/other/readme.md",
            "/Users/alex/project/README.md"
        ]
    );
    let batch = run(&fixture.source, "readme", &VISIBLE);
    assert!(batch.candidates.iter().all(|candidate| !candidate.is_dir));
    assert!(batch
        .candidates
        .iter()
        .all(|candidate| candidate.modified.is_none()));
    assert!(!batch.truncated);
}

#[test]
fn anchored_and_browse_queries_include_files() {
    let fixture = Fixture::new(&library());
    assert_eq!(
        fixture.ranked("~/Documents/inv", &VISIBLE),
        [
            "/Users/alex/Documents/invoice-2026.pdf",
            "/Users/alex/Documents/Work/invoice-old.pdf",
        ]
    );
    assert_eq!(
        sorted_ranked(&fixture, "~/Documents/", &VISIBLE),
        [
            "/Users/alex/Documents/Work",
            "/Users/alex/Documents/invoice-2026.pdf",
            "/Users/alex/Documents/letter.md",
        ]
    );
    assert_eq!(
        sorted_ranked(&fixture, "project/", &VISIBLE),
        [
            "/Users/alex/project/README.md",
            "/Users/alex/project/docs",
            "/Users/alex/project/notes.txt",
        ]
    );
    assert!(sorted_ranked(&fixture, "project/", &WITH_HIDDEN)
        .contains(&"/Users/alex/project/.hidden.md".to_owned()));
    assert!(fixture.ranked("/Nowhere/", &VISIBLE).is_empty());
}

#[test]
fn scopes_pick_files_folders_or_both_ranked_together() {
    let fixture = Fixture::new(&library());
    let files = run(&fixture.source, "invoice", &FILES);
    assert_eq!(
        sorted(&files),
        [
            "/Users/alex/Documents/Work/invoice-old.pdf",
            "/Users/alex/Documents/invoice-2026.pdf",
        ]
    );
    let before_folders = fixture.blocks_read();
    let folders = run(&fixture.source, "invoice", &FOLDERS);
    assert_eq!(sorted(&folders), ["/Users/alex/Invoices"]);
    assert_eq!(
        fixture.blocks_read(),
        before_folders,
        "folder scope reads no file names"
    );
    let both = fixture.ranked("invoice", &VISIBLE);
    assert_eq!(
        both,
        [
            "/Users/alex/Invoices",
            "/Users/alex/Documents/invoice-2026.pdf",
            "/Users/alex/Documents/Work/invoice-old.pdf",
        ]
    );
}

#[test]
fn file_scope_folder_segments_still_resolve_matching_subtrees() {
    let fixture = Fixture::new(&library());
    assert_eq!(
        sorted_ranked(&fixture, "deep/*.md", &FILES),
        ["/Users/alex/project/docs/deep/deeper/buried.md"]
    );
    assert!(fixture.ranked("missing/*.md", &FILES).is_empty());
}

#[test]
fn path_scoped_queries_read_only_blocks_covering_matching_subtrees() {
    let folders = wide_library();
    let fixture = Fixture::with_block_target(&folders, SMALL_BLOCK_TARGET);
    let project = fixture.slot("/Users/alex/project");
    let project_slots = fixture.source.tree.lock().unwrap().subtree_slots(project);
    let names = fixture.source.names.lock().unwrap();
    let block_count = names.block_count() as u64;
    let covering = names.blocks_for_slots(&project_slots).len() as u64;
    drop(names);
    assert!(covering > 0);
    assert!(covering < block_count / 2, "{covering} of {block_count}");

    let before = fixture.blocks_read();
    assert_eq!(fixture.ranked("project/*.md", &FILES).len(), 3);
    assert_eq!(fixture.blocks_read() - before, covering);

    let before_global = fixture.blocks_read();
    assert_eq!(fixture.ranked("record", &FILES).len(), FILLER_FOLDERS);
    assert_eq!(fixture.blocks_read() - before_global, block_count);
    assert!(
        block_count as usize > BLOCKS_PER_READ,
        "global reads are chunked"
    );
}

fn library_with_a_full_hidden_block() -> Vec<Folder> {
    let mut folders = library();
    let plans = folders
        .iter_mut()
        .find(|folder| folder.path.ends_with("/.agent/plans"))
        .expect("plans folder");
    plans
        .files
        .extend(["roadmap-2026.md", "retro-notes.md"].map(String::from));
    folders
}

fn blocks_read_by(fixture: &Fixture, raw: &str, options: &SearchOptions) -> u64 {
    let before = fixture.blocks_read();
    fixture.ranked(raw, options);
    fixture.blocks_read() - before
}

#[test]
fn with_hidden_files_off_blocks_inside_hidden_subtrees_are_never_read() {
    let fixture =
        Fixture::with_block_target(&library_with_a_full_hidden_block(), SMALL_BLOCK_TARGET);
    let hidden_blocks = {
        let tree = fixture.source.tree.lock().unwrap();
        let names = fixture.source.names.lock().unwrap();
        names.blocks_within(&tree.hidden_ranges())
    };
    assert_eq!(hidden_blocks.len(), 1, "the .agent subtree fills one block");
    let block_count = fixture.block_count();

    assert_eq!(blocks_read_by(&fixture, "*.md", &FILES), block_count - 1);
    let with_hidden_files = SearchOptions {
        scope: SearchScope::Files,
        ..WITH_HIDDEN
    };
    assert_eq!(
        blocks_read_by(&fixture, "*.md", &with_hidden_files),
        block_count
    );
    assert!(fixture
        .ranked("roadmap", &with_hidden_files)
        .contains(&"/Users/alex/project/.agent/plans/roadmap-2026.md".to_owned()));
    assert!(fixture.ranked("roadmap", &FILES).is_empty());
    assert_eq!(
        blocks_read_by(&fixture, "/Users/alex/project/.agent/plans/", &FILES),
        1,
        "listings read the blocks of the listed folders"
    );
}

fn dot_rooted_library() -> Vec<Folder> {
    vec![
        folder(
            DOT_ROOT,
            &[
                "root-notes-one.txt",
                "root-notes-two.txt",
                "root-notes-three.txt",
            ],
        ),
        folder(
            DOT_ROOT_CACHE,
            &["cached-alpha.md", "cached-beta.md", "cached-gamma.md"],
        ),
        folder(DOT_ROOT_DOCS, &["guide.md"]),
    ]
}

fn dot_rooted_tree(folders: &[Folder]) -> FolderSizeTree {
    let mut tree = FolderSizeTree::default();
    assert!(tree.add_root(Path::new(DOT_ROOT)));
    for folder in folders {
        let children = child_names(folders, &folder.path);
        let listing = DirectoryListing::scanned(0, &children);
        assert!(tree.merge_record(Path::new(&folder.path), listing));
    }
    tree
}

#[test]
fn below_a_dot_named_root_hidden_subtrees_are_still_skipped_whole() {
    let folders = dot_rooted_library();
    let fixture = Fixture::over_tree(dot_rooted_tree(&folders), &folders, SMALL_BLOCK_TARGET);
    let hidden_blocks = {
        let tree = fixture.source.tree.lock().unwrap();
        let names = fixture.source.names.lock().unwrap();
        names.blocks_within(&tree.hidden_ranges())
    };
    assert_eq!(hidden_blocks.len(), 1, "the .cache subtree fills one block");
    let block_count = fixture.block_count();

    assert_eq!(blocks_read_by(&fixture, "*.md", &FILES), block_count - 1);
    assert_eq!(
        sorted_ranked(&fixture, "*.md", &FILES),
        [".dot-root/docs/guide.md"],
        "the root shows and its hidden subtree does not"
    );
    let with_hidden_files = SearchOptions {
        scope: SearchScope::Files,
        ..WITH_HIDDEN
    };
    assert_eq!(
        blocks_read_by(&fixture, "*.md", &with_hidden_files),
        block_count
    );
    assert_eq!(
        sorted_ranked(&fixture, "*.md", &with_hidden_files),
        [
            ".dot-root/.cache/cached-alpha.md",
            ".dot-root/.cache/cached-beta.md",
            ".dot-root/.cache/cached-gamma.md",
            ".dot-root/docs/guide.md",
        ]
    );
}

#[test]
fn blocks_inside_the_dead_range_of_a_replaced_subtree_are_never_read() {
    let fixture = Fixture::with_block_target(&wide_library(), SMALL_BLOCK_TARGET);
    let block_count_before = fixture.block_count();
    let replacement = [
        folder("/Users/alex/archive", &[]),
        folder("/Users/alex/archive/box-new", &["record-new.md"]),
    ];
    fixture.graft("/Users/alex/archive", &replacement);
    let dead_blocks = {
        let names = fixture.source.names.lock().unwrap();
        names.blocks_within(names.dead_slots())
    };
    assert!(dead_blocks.len() > 1, "{dead_blocks:?}");
    let block_count = fixture.block_count();
    assert!(
        block_count > block_count_before,
        "the graft adds its own block"
    );

    let with_hidden_files = SearchOptions {
        scope: SearchScope::Files,
        ..WITH_HIDDEN
    };
    assert_eq!(
        fixture.ranked("record", &with_hidden_files),
        ["/Users/alex/archive/box-new/record-new.md"]
    );
    assert_eq!(
        blocks_read_by(&fixture, "record", &with_hidden_files),
        block_count - dead_blocks.len() as u64
    );
}

#[test]
fn cancellation_and_budget_expiry_stop_file_scanning() {
    let folders = wide_library();
    let fixture = Fixture::with_block_target(&folders, SMALL_BLOCK_TARGET);
    let yes = || true;
    let cancelled = Control {
        cancelled: &yes,
        expired: &never,
    };
    assert_eq!(
        run_with(&fixture.source, "record", &cancelled),
        Err(SearchError::Cancelled)
    );
    assert_eq!(fixture.blocks_read(), 0);

    let checks = Cell::new(0);
    let after_overlay = || {
        checks.set(checks.get() + 1);
        checks.get() > 1
    };
    let expiring = Control {
        cancelled: &never,
        expired: &after_overlay,
    };
    let partial = run_with(&fixture.source, "record", &expiring).expect("partial");
    assert!(partial.truncated);
    assert!(partial.candidates.is_empty());
    assert_eq!(fixture.blocks_read(), 0);

    let checks = Cell::new(0);
    let after_first_blocks = || {
        checks.set(checks.get() + 1);
        checks.get() > 2
    };
    let expiring = Control {
        cancelled: &never,
        expired: &after_first_blocks,
    };
    let partial = run_with(&fixture.source, "record", &expiring).expect("partial");
    assert!(partial.truncated);
    assert_eq!(fixture.blocks_read(), BLOCKS_PER_READ as u64);
}

fn bulk_source(count: usize) -> (TempDir, Source) {
    let mut files: Vec<String> = (0..count)
        .map(|index| format!("inv-{index:05}.txt"))
        .collect();
    files.push("inv".to_owned());
    let tree = build(&["/bulk"]);
    let dir = tempdir().expect("temp dir");
    let mut names = FileNameStore::new(dir.path().join(NAMES_FILE));
    names.set_names(
        slot(&tree, "/bulk"),
        PackedNames::from_names(files.iter().map(String::as_bytes)),
    );
    let source = saved_in_a_segment(dir.path(), UnsavedIndex { tree, names }, LARGE_BLOCK_TARGET);
    (dir, source)
}

#[test]
fn file_hits_share_the_candidate_cap_and_keep_the_best_tiers() {
    let (_dir, source) = bulk_source(2 * CANDIDATE_CAP + 500);
    let batch = run(&source, "inv", &FILES);
    assert!(batch.truncated);
    assert_eq!(batch.candidates.len(), CANDIDATE_CAP);
    assert!(batch
        .candidates
        .iter()
        .any(|candidate| candidate.path == "/bulk/inv"));

    let listing = run(&source, "/bulk/", &FILES);
    assert!(listing.truncated);
    assert_eq!(listing.candidates.len(), CANDIDATE_CAP);
}

#[test]
fn fresh_overlay_names_and_tombstones_win_over_blocks() {
    let fixture = Fixture::new(&library());
    let other = fixture.slot("/Users/alex/other");
    let docs = fixture.slot("/Users/alex/project/docs");
    {
        let mut names = fixture.source.names.lock().unwrap();
        names.set_names(other, packed(&["fresh-readme.md"]));
        names.tombstone(docs);
    }
    let paths = sorted_ranked(&fixture, "readme", &VISIBLE);
    assert_eq!(
        paths,
        [
            "/Users/alex/other/fresh-readme.md",
            "/Users/alex/project/README.md",
        ]
    );
    assert!(fixture.ranked("guide", &FILES).is_empty());
    assert_eq!(
        sorted_ranked(&fixture, "other/*.md", &FILES),
        ["/Users/alex/other/fresh-readme.md"]
    );
}

#[test]
fn names_held_only_in_the_overlay_are_searchable() {
    let folders = library();
    let tree = tree_of(&folders);
    let dir = tempdir().expect("temp dir");
    let mut names = FileNameStore::new(dir.path().join(NAMES_FILE));
    for folder in &folders {
        names.set_names(slot(&tree, &folder.path), packed(&folder.files));
    }
    let source = Source::with_names(tree, names);
    assert!(source.file_names_available());
    let mut global = ranked(&source, "readme", &VISIBLE);
    global.sort_unstable();
    assert_eq!(
        global,
        [
            "/Users/alex/other/readme.md",
            "/Users/alex/project/README.md"
        ]
    );
    let mut scoped = ranked(&source, "project/*.md", &VISIBLE);
    scoped.sort_unstable();
    assert_eq!(scoped.len(), 3, "{scoped:?}");
}

#[test]
fn privacy_blocked_roots_hide_their_files_but_not_themselves() {
    let mut fixture = Fixture::new(&library());
    fixture.source.blocked = vec![PathBuf::from("/Users/alex/Documents")];
    assert!(fixture.ranked("invoice", &FILES).is_empty());
    assert!(fixture.ranked("~/Documents/", &FILES).is_empty());
    assert_eq!(
        fixture.ranked("Documents", &FOLDERS),
        ["/Users/alex/Documents"]
    );
}

#[test]
fn a_corrupt_block_falls_back_to_folders_and_marks_results_truncated() {
    let fixture = Fixture::new(&library());
    let Fixture { dir, source } = fixture;
    drop(source);
    let segment_paths = paths_in(dir.path());
    corrupt(&segment_paths.names);
    let segment = MappedSegment::open(&segment_paths).expect("the header still opens");
    let source = source_over(segment, dir.path());
    assert!(source.file_names_available());

    let batch = run(&source, "invoice", &VISIBLE);
    assert!(batch.truncated);
    assert_eq!(sorted(&batch), ["/Users/alex/Invoices"]);
    assert!(!source.file_names_available());
    assert!(source.names.lock().unwrap().take_unreported_failure());

    let after = run(&source, "invoice", &VISIBLE);
    assert!(
        !after.truncated,
        "unavailable names are skipped, not retried"
    );
    assert_eq!(sorted(&after), ["/Users/alex/Invoices"]);
}

fn corrupt(path: &Path) {
    let mut file = OpenOptions::new().write(true).open(path).expect("open");
    file.seek(SeekFrom::Start(HEADER_BYTES)).expect("seek");
    file.write_all(&CORRUPTION).expect("corrupt");
}

#[test]
fn read_errors_during_the_file_pass_propagate() {
    let mut fixture = Fixture::new(&library());
    fixture.source.fail = Some(SearchError::NotReady);
    let control = Control {
        cancelled: &never,
        expired: &never,
    };
    assert_eq!(
        run_with(&fixture.source, "readme", &control),
        Err(SearchError::NotReady)
    );
    assert!(!fixture.source.file_names_available());
}
