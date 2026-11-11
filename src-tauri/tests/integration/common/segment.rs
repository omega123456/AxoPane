use std::path::{Path, PathBuf};
use std::sync::Arc;

use file_explorer_lib::size::index::names::block::checksum;
use file_explorer_lib::size::index::names::{names_in, FileNameStore, PackedNames, SavedNames};
use file_explorer_lib::size::index::segment::{
    BaseRewrite, BuildError, GraftTarget, MappedSegment, RewriteProgress, RewriteSource,
    SegmentBuilder, SegmentPaths, SegmentTarget, SpillOptions, SpillRecord, WalkSpill, RUN_BYTES,
};
use file_explorer_lib::size::index::tree::{DirectoryListing, FolderSizeTree};

pub const FIRST_SEQUENCE: u64 = 1;
pub const CHECKSUM_BYTES: usize = 8;
pub const TREE_VERSION_OFFSET: usize = 8;
pub const FIRST_TREE_FORMAT: u32 = 1;

const TREE_HEADER_BYTES: usize = 48;
const TREE_HEADER_FIELD_BYTES: usize = TREE_HEADER_BYTES - CHECKSUM_BYTES;
const U32_BYTES: usize = 4;

const WALK_SPILL_FILE: &str = "walk.spill";

#[derive(Clone)]
pub struct Walked {
    pub path: PathBuf,
    pub direct: u64,
    pub denied: bool,
    pub subdirs: Vec<String>,
    pub files: Vec<String>,
}

pub fn walked(path: PathBuf, direct: u64) -> Walked {
    Walked {
        path,
        direct,
        denied: false,
        subdirs: Vec::new(),
        files: Vec::new(),
    }
}

pub fn strings(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

pub fn listing(record: &Walked) -> DirectoryListing<'_, String> {
    DirectoryListing {
        direct_bytes: record.direct,
        subdirs: &record.subdirs,
        denied: record.denied,
    }
}

pub fn heap_of(root: &Path, records: &[Walked]) -> FolderSizeTree {
    let mut tree = FolderSizeTree::default();
    assert!(tree.add_root(root));
    for record in records {
        assert!(tree.merge_record(&record.path, listing(record)));
    }
    tree
}

pub fn append(spill: &mut WalkSpill, record: &Walked) -> Result<(), BuildError> {
    let files = PackedNames::from_names(&record.files);
    spill.append(SpillRecord {
        path: &record.path,
        listing: listing(record),
        file_names: files.as_bytes(),
    })
}

pub struct SegmentPlace<'place> {
    pub directory: &'place Path,
    pub root: &'place Path,
    pub token: u64,
}

pub fn walked_segment(place: SegmentPlace<'_>, records: &[Walked]) -> MappedSegment {
    let root_name = place.root.to_string_lossy().into_owned();
    spilled_segment(place, records, root_name)
}

pub fn graft_walk_segment(place: SegmentPlace<'_>, records: &[Walked]) -> MappedSegment {
    let top_name = place.root.file_name().expect("a named graft top");
    let root_name = top_name.to_string_lossy().into_owned();
    spilled_segment(place, records, root_name)
}

fn spilled_segment(
    place: SegmentPlace<'_>,
    records: &[Walked],
    root_name: String,
) -> MappedSegment {
    let mut spill = WalkSpill::create(SpillOptions {
        runs_path: place.directory.join(WALK_SPILL_FILE),
        root: place.root.to_path_buf(),
        run_bytes: RUN_BYTES,
    });
    for record in records {
        append(&mut spill, record).expect("append");
    }
    let built = spill.build(GraftTarget {
        segment: SegmentTarget {
            paths: paths_in(place.directory),
            token: place.token,
        },
        root_name,
    });
    MappedSegment::open(&built.expect("build").paths).expect("opens")
}

pub fn paths_in(directory: &Path) -> SegmentPaths {
    SegmentPaths::numbered(directory, FIRST_SEQUENCE)
}

pub fn file_names_of(segment: &MappedSegment, id: u32) -> Vec<String> {
    let names = segment.file_names();
    let block_index = names.blocks_covering(id..id + 1).start;
    if block_index >= names.block_count() {
        return Vec::new();
    }
    let touched = names
        .touch_block(block_index)
        .expect("names block verifies");
    let packed = touched.view.names_of(id).unwrap_or_default();
    names_in(packed)
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect()
}

pub fn names_to(segment: &MappedSegment, id: u32) -> Vec<String> {
    let mut names = Vec::new();
    let mut next = Some(id);
    while let Some(current) = next {
        names.push(segment.name(current).expect("name").to_string());
        next = segment.row(current).expect("row").parent;
    }
    names.reverse();
    names
}

pub fn child_names(segment: &MappedSegment, id: u32) -> Vec<String> {
    let children = segment.children(id).expect("children");
    children
        .iter()
        .map(|child| segment.name(child).expect("child name").to_string())
        .collect()
}

pub fn descend(segment: &MappedSegment, top: u32, names: &[&str]) -> Option<u32> {
    names.iter().try_fold(top, |id, name| {
        segment.child_named(id, name).expect("child lookup")
    })
}

pub fn files_in(directory: &Path) -> Vec<String> {
    let mut files: Vec<String> = std::fs::read_dir(directory)
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    files.sort();
    files
}

pub struct Compaction<'index> {
    pub tree: &'index FolderSizeTree,
    pub names: &'index FileNameStore,
    pub target: SegmentTarget,
    pub block_target: usize,
}

pub fn compacted(compaction: Compaction<'_>) -> MappedSegment {
    let Compaction {
        tree,
        names,
        target,
        block_target,
    } = compaction;
    let mut builder = SegmentBuilder::with_block_target(target, block_target).expect("builder");
    let mut rewrite = BaseRewrite::of(tree);
    let source = RewriteSource { tree, names };
    let mut progress = RewriteProgress::MoreFolders;
    while progress == RewriteProgress::MoreFolders {
        progress = rewrite.copy_step(source, &mut builder).expect("copied");
    }
    MappedSegment::open(&builder.finish().expect("finish").paths).expect("opens")
}

pub struct SegmentBackedIndex {
    pub tree: FolderSizeTree,
    pub names: FileNameStore,
}

pub fn index_over(segment: MappedSegment, names_path: &Path) -> SegmentBackedIndex {
    let segment = Arc::new(segment);
    let mut tree = FolderSizeTree::default();
    let slots = tree.attach_base(Arc::clone(&segment)).expect("attached");
    let mut base = SavedNames::default();
    base.attach_segment(slots, segment.shared_file_names());
    let mut names = FileNameStore::new(names_path.to_path_buf());
    names.restore(base);
    SegmentBackedIndex { tree, names }
}

pub fn put_u32_at(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + U32_BYTES].copy_from_slice(&value.to_le_bytes());
}

pub fn resealed_single_chunk(mut bytes: Vec<u8>) -> Vec<u8> {
    let table_start = bytes.len() - CHECKSUM_BYTES;
    let chunk_checksum = checksum(&bytes[TREE_HEADER_BYTES..table_start]);
    bytes[table_start..].copy_from_slice(&chunk_checksum.to_le_bytes());
    let mut covered = bytes[..TREE_HEADER_FIELD_BYTES].to_vec();
    covered.extend_from_slice(&bytes[table_start..]);
    let header_checksum = checksum(&covered);
    bytes[TREE_HEADER_FIELD_BYTES..TREE_HEADER_BYTES]
        .copy_from_slice(&header_checksum.to_le_bytes());
    bytes
}

pub fn rewrite_as_format_one(tree_file: &Path) {
    let mut bytes = std::fs::read(tree_file).expect("read tree file");
    put_u32_at(&mut bytes, TREE_VERSION_OFFSET, FIRST_TREE_FORMAT);
    std::fs::write(tree_file, resealed_single_chunk(bytes)).expect("write tree file");
}
