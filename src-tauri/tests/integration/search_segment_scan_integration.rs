use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use file_explorer_lib::search::path_style::PathStyle;
use file_explorer_lib::search::query::parse;
use file_explorer_lib::search::tree_scan::{candidates_in_chunks, TreeSearchRun};
use file_explorer_lib::search::{Control, SearchOptions};
use file_explorer_lib::size::index::names::PackedNames;
use file_explorer_lib::size::index::tree::DirectoryListing;
use tempfile::{tempdir, TempDir};

use crate::common::segment::{index_over, walked, walked_segment, SegmentPlace, Walked};
use crate::search_file_scan_integration::{
    library, packed, ranked, Fixture, Folder, FILES, FOLDERS, WITH_HIDDEN,
};
use crate::search_tree_scan_integration::{never, sorted, Source, VISIBLE};

const TOKEN: u64 = 11;
const ROOT: &str = "/";
const UNUSED_NAMES_FILE: &str = "no-pair-file.bin";
const TINY_CHUNK: usize = 3;
const QUERIES: [&str; 7] = [
    "*.md",
    "project/*.md",
    "inv",
    "docs",
    "/Users/alex/project/",
    "alex/",
    "readme",
];
const EVERY_SCOPE: [&SearchOptions; 4] = [&VISIBLE, &WITH_HIDDEN, &FILES, &FOLDERS];

/// One walk record per folder and ancestor, as a full scan under `/` reports them.
fn walk_of(folders: &[Folder]) -> Vec<Walked> {
    let mut records: BTreeMap<String, Walked> = BTreeMap::new();
    let mut record_of = |path: &str| {
        let fresh = walked(PathBuf::from(path), 0);
        records.entry(path.to_owned()).or_insert(fresh).clone()
    };
    let mut links = vec![record_of(ROOT)];
    for folder in folders {
        let mut parent = ROOT.to_owned();
        for name in folder.path.trim_start_matches('/').split('/') {
            let child = PathStyle::Posix.join(&parent, name);
            record_of(&child);
            links.push(Walked {
                subdirs: vec![name.to_owned()],
                ..record_of(&parent)
            });
            parent = child;
        }
    }
    for link in links {
        let record = records
            .get_mut(&*link.path.to_string_lossy())
            .expect("record");
        record.subdirs.extend(link.subdirs);
    }
    for folder in folders {
        let record = records.get_mut(&folder.path).expect("folder record");
        record.files.clone_from(&folder.files);
    }
    records.into_values().collect()
}

/// The library held in one mapped segment, with nothing in the overlays yet.
struct SegmentBacked {
    _directory: TempDir,
    source: Source,
}

fn segment_backed(folders: &[Folder]) -> SegmentBacked {
    let directory = tempdir().expect("temp dir");
    let place = SegmentPlace {
        directory: directory.path(),
        root: Path::new(ROOT),
        token: TOKEN,
    };
    let segment = walked_segment(place, &walk_of(folders));
    let index = index_over(segment, &directory.path().join(UNUSED_NAMES_FILE));
    SegmentBacked {
        source: Source::with_names(index.tree, index.names),
        _directory: directory,
    }
}

impl SegmentBacked {
    fn slot(&self, path: &str) -> u32 {
        let tree = self.source.tree.lock().unwrap();
        tree.lookup(Path::new(path)).expect("folder in tree")
    }

    fn files(&self, raw: &str) -> Vec<String> {
        let mut found = ranked(&self.source, raw, &FILES);
        found.sort_unstable();
        found
    }

    fn prune(&self, path: &str) {
        let mut tree = self.source.tree.lock().unwrap();
        let mut names = self.source.names.lock().unwrap();
        assert!(tree.prune(Path::new(path)));
        for slots in tree.take_removed_ranges() {
            names.tombstone_range(slots);
        }
    }
}

#[test]
fn a_segment_backed_index_finds_what_the_heap_index_finds() {
    let fixture = segment_backed(&library());
    let heap = Fixture::new(&library());
    for raw in QUERIES {
        for options in EVERY_SCOPE {
            assert_eq!(
                ranked(&fixture.source, raw, options),
                ranked(&heap.source, raw, options),
                "{raw:?} {options:?}"
            );
        }
    }
    let names = fixture.source.names.lock().unwrap();
    assert!(names.is_available());
    assert!(names.has_saved_names());
    assert_eq!(names.block_count(), 1);
    let files_in_the_library: usize = library().iter().map(|folder| folder.files.len()).sum();
    assert_eq!(
        names.count_saved_and_overlay_names(),
        Ok(files_in_the_library as u64)
    );
}

#[test]
fn the_folder_pass_walks_segment_ids_and_then_overlay_ids_in_chunks() {
    let fixture = segment_backed(&library());
    let added = Path::new("/Users/alex/project/docs-next");
    {
        let mut tree = fixture.source.tree.lock().unwrap();
        assert!(tree.merge_record::<&str>(added, DirectoryListing::scanned(0, &[])));
    }
    let control = Control {
        cancelled: &never,
        expired: &never,
    };
    let run = TreeSearchRun {
        source: &fixture.source,
        options: &FOLDERS,
        control: &control,
    };
    let query = parse("docs", PathStyle::Posix, Some("/Users/alex")).expect("query");
    let reads_before = fixture.source.reads();
    let batch = candidates_in_chunks(&run, &query, TINY_CHUNK).expect("batch");
    assert_eq!(
        sorted(&batch),
        [
            "/Users/alex/Documents",
            "/Users/alex/project/docs",
            "/Users/alex/project/docs-next"
        ]
    );
    let folder_count = fixture.source.tree.lock().unwrap().node_count();
    let segment_chunks = (folder_count - 1).div_ceil(TINY_CHUNK);
    let overlay_chunks = 1;
    let path_reads = 1;
    assert_eq!(
        fixture.source.reads() - reads_before,
        segment_chunks + overlay_chunks + path_reads
    );
}

#[test]
fn overlay_names_and_dead_ranges_win_over_segment_blocks() {
    let fixture = segment_backed(&library());
    let docs = fixture.slot("/Users/alex/project/docs");
    let deeper = fixture.slot("/Users/alex/project/docs/deep/deeper");
    {
        let mut names = fixture.source.names.lock().unwrap();
        assert_eq!(names.blocks_for_slots(&[docs, deeper]), [0]);
        names.set_names(docs, packed(&["rewritten.md"]));
    }
    assert_eq!(
        fixture.files("project/*.md"),
        [
            "/Users/alex/project/README.md",
            "/Users/alex/project/docs/deep/deeper/buried.md",
            "/Users/alex/project/docs/rewritten.md",
        ]
    );

    fixture.prune("/Users/alex/project/docs");
    assert_eq!(
        fixture.files("project/*.md"),
        ["/Users/alex/project/README.md"]
    );
    let names = fixture.source.names.lock().unwrap();
    assert_eq!(names.names_of(docs), Ok(Some(PackedNames::default())));
    assert_eq!(names.names_of(deeper), Ok(Some(PackedNames::default())));
    assert_eq!(
        names.overlay_stats().folders,
        0,
        "the dead range dropped it"
    );
    let other = fixture.slot("/Users/alex/other");
    assert_eq!(
        names.names_of(other),
        Ok(Some(packed(&["readme.md", "stray.md"])))
    );
}

#[test]
fn files_of_new_overlay_folders_are_found_next_to_segment_files() {
    let fixture = segment_backed(&library());
    let added = Path::new("/Users/alex/project/drafts");
    {
        let mut tree = fixture.source.tree.lock().unwrap();
        let slot = tree
            .merge_record_slot::<&str>(added, DirectoryListing::scanned(0, &[]))
            .expect("added");
        let mut names = fixture.source.names.lock().unwrap();
        names.set_names(slot, packed(&["draft.md"]));
    }
    assert_eq!(
        fixture.files("project/*.md"),
        [
            "/Users/alex/project/README.md",
            "/Users/alex/project/docs/deep/deeper/buried.md",
            "/Users/alex/project/docs/guide.md",
            "/Users/alex/project/drafts/draft.md",
        ]
    );
    assert_eq!(
        fixture.files("/Users/alex/project/drafts/"),
        ["/Users/alex/project/drafts/draft.md"]
    );
}
