use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use file_explorer_lib::size::index::names::writer::RewriteStats;
use file_explorer_lib::size::index::names::{FileNameStore, PackedNames};
use file_explorer_lib::size::index::segment::{
    BaseRewrite, MappedSegment, RewriteFailure, RewriteProgress, RewriteSource, SegmentBuilder,
    SegmentPaths, SegmentTarget,
};
use file_explorer_lib::size::index::tree::{DirectoryListing, FolderSizeTree};
use tempfile::{tempdir, TempDir};

use crate::common::segment::{compacted, index_over, Compaction, SegmentBackedIndex};

const TOKEN: u64 = 31;
const FOLDER_COUNT: usize = 40;
const ONE_FOLDER_PER_BLOCK: usize = 64;
const A_FEW_FOLDERS_PER_BLOCK: usize = 400;
const NAMES_HEADER_BYTES: u64 = 48;
const BASE_SEQUENCE: u64 = 1;
const REWRITE_SEQUENCE: u64 = 2;
const NAMES_FILE: &str = "unused-names.bin";

fn folder_name(number: usize) -> String {
    format!("f{number:03}")
}

fn names_for(number: usize) -> PackedNames {
    PackedNames::from_names([
        format!("report-{number}.pdf"),
        format!("notes-{number}.md"),
        format!(".hidden-{number}"),
    ])
}

fn strings(names: &PackedNames) -> Vec<String> {
    names
        .names()
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect()
}

struct Fixture {
    directory: TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempdir().expect("temp dir");
        let root = directory.path().join("rewrite-root");
        Self { directory, root }
    }

    fn folder(&self, number: usize) -> PathBuf {
        self.root.join(folder_name(number))
    }

    fn paths(&self, sequence: u64) -> SegmentPaths {
        SegmentPaths::numbered(self.directory.path(), sequence)
    }

    fn heap_index(&self) -> (FolderSizeTree, FileNameStore) {
        let mut tree = FolderSizeTree::default();
        assert!(tree.add_root(&self.root));
        let children: Vec<String> = (0..FOLDER_COUNT).map(folder_name).collect();
        let listing = DirectoryListing::scanned(1, &children);
        assert!(tree.merge_record(&self.root, listing));
        let mut names = FileNameStore::new(self.directory.path().join(NAMES_FILE));
        let no_children: Vec<String> = Vec::new();
        for number in 0..FOLDER_COUNT {
            let path = self.folder(number);
            let slot = tree
                .merge_record_slot(&path, DirectoryListing::scanned(1, &no_children))
                .expect("folder");
            names.set_names(slot, names_for(number));
        }
        (tree, names)
    }

    fn base(&self, block_target: usize) -> SegmentBackedIndex {
        let (tree, names) = self.heap_index();
        let segment = compacted(Compaction {
            tree: &tree,
            names: &names,
            target: SegmentTarget {
                paths: self.paths(BASE_SEQUENCE),
                token: TOKEN,
            },
            block_target,
        });
        index_over(segment, &self.directory.path().join(NAMES_FILE))
    }

    fn reopened_base(&self) -> SegmentBackedIndex {
        let segment = MappedSegment::open(&self.paths(BASE_SEQUENCE)).expect("opens");
        index_over(segment, &self.directory.path().join(NAMES_FILE))
    }
}

struct Rewritten {
    index: SegmentBackedIndex,
    stats: RewriteStats,
}

fn rewrite(fixture: &Fixture, index: &SegmentBackedIndex) -> Result<Rewritten, RewriteFailure> {
    let target = SegmentTarget {
        paths: fixture.paths(REWRITE_SEQUENCE),
        token: TOKEN,
    };
    let mut builder = SegmentBuilder::for_compaction(target).expect("builder");
    let mut rewrite = BaseRewrite::of(&index.tree);
    let source = RewriteSource {
        tree: &index.tree,
        names: &index.names,
    };
    let mut progress = RewriteProgress::MoreFolders;
    while progress == RewriteProgress::MoreFolders {
        progress = rewrite.copy_step(source, &mut builder)?;
    }
    let built = builder.finish().expect("finish");
    let segment = MappedSegment::open(&built.paths).expect("opens");
    let names_path = fixture.directory.path().join(NAMES_FILE);
    Ok(Rewritten {
        index: index_over(segment, &names_path),
        stats: built.names,
    })
}

fn names_at(index: &SegmentBackedIndex, path: &Path) -> Vec<String> {
    let slot = index.tree.lookup(path).expect("folder");
    let names = index.names.names_of(slot).expect("readable");
    names.map(|packed| strings(&packed)).unwrap_or_default()
}

fn assert_every_folder_keeps_its_names(fixture: &Fixture, index: &SegmentBackedIndex) {
    for number in (0..FOLDER_COUNT).filter(|&number| number != EDITED) {
        assert_eq!(
            names_at(index, &fixture.folder(number)),
            strings(&names_for(number)),
            "{}",
            folder_name(number)
        );
    }
}

const EDITED: usize = 10;

#[test]
fn an_unchanged_base_is_rewritten_by_copying_every_block() {
    let fixture = Fixture::new();
    let base = fixture.base(ONE_FOLDER_PER_BLOCK);
    let block_count = base.names.block_count() as u64;
    assert!(block_count > 10, "{block_count} blocks");

    let rewritten = rewrite(&fixture, &base).expect("rewritten");

    assert_eq!(rewritten.stats.blocks_copied, block_count);
    assert_eq!(rewritten.stats.blocks_reencoded, 0);
    assert_every_folder_keeps_its_names(&fixture, &rewritten.index);
    assert_eq!(
        names_at(&rewritten.index, &fixture.folder(EDITED)),
        strings(&names_for(EDITED))
    );
}

#[test]
fn only_blocks_touched_by_the_overlay_are_reencoded() {
    let fixture = Fixture::new();
    let mut base = fixture.base(ONE_FOLDER_PER_BLOCK);
    let block_count = base.names.block_count() as u64;
    let edited = base.tree.lookup(&fixture.folder(EDITED)).expect("edited");
    base.names
        .set_names(edited, PackedNames::from_names(["fresh.txt"]));

    let rewritten = rewrite(&fixture, &base).expect("rewritten");

    assert!(rewritten.stats.blocks_reencoded >= 1);
    assert!(rewritten.stats.blocks_copied >= block_count - 2);
    assert_eq!(
        names_at(&rewritten.index, &fixture.folder(EDITED)),
        ["fresh.txt"]
    );
    assert_every_folder_keeps_its_names(&fixture, &rewritten.index);
}

#[test]
fn a_folder_inserted_inside_a_block_falls_back_to_reencoding_it() {
    let fixture = Fixture::new();
    let mut base = fixture.base(A_FEW_FOLDERS_PER_BLOCK);
    let inserted = fixture.folder(EDITED).join("inserted");
    let no_children: Vec<String> = Vec::new();
    let listing = DirectoryListing::scanned(1, &["inserted"]);
    base.tree
        .apply_listing(&fixture.folder(EDITED), listing)
        .expect("listed");
    let slot = base
        .tree
        .merge_record_slot(&inserted, DirectoryListing::scanned(5, &no_children))
        .expect("inserted");
    base.names
        .set_names(slot, PackedNames::from_names(["inner.txt"]));

    let rewritten = rewrite(&fixture, &base).expect("rewritten");

    assert!(rewritten.stats.blocks_reencoded >= 1);
    assert_eq!(names_at(&rewritten.index, &inserted), ["inner.txt"]);
    assert_eq!(
        names_at(&rewritten.index, &fixture.folder(EDITED)),
        strings(&names_for(EDITED))
    );
    assert_every_folder_keeps_its_names(&fixture, &rewritten.index);
}

#[test]
fn a_block_whose_bytes_are_damaged_is_not_copied() {
    let fixture = Fixture::new();
    drop(fixture.base(ONE_FOLDER_PER_BLOCK));
    let mut file = OpenOptions::new()
        .write(true)
        .open(fixture.paths(BASE_SEQUENCE).names)
        .expect("names file");
    file.seek(SeekFrom::Start(NAMES_HEADER_BYTES))
        .expect("seek");
    file.write_all(&[0xEE; 4]).expect("damage");
    drop(file);
    let base = fixture.reopened_base();

    let failure = rewrite(&fixture, &base).err();

    assert!(matches!(failure, Some(RewriteFailure::NamesUnreadable)));
    assert!(base.names.take_unreported_failure());
}
