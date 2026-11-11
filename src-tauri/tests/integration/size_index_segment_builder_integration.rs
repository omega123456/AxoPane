use std::fs;
use std::path::Path;

use file_explorer_lib::size::index::names::signature::{is_non_ascii, signature_of, DOT_BIT};
use file_explorer_lib::size::index::names::{names_in, PackedNames};
use file_explorer_lib::size::index::persist::temp_path;
use file_explorer_lib::size::index::segment::{
    BuildError, MappedSegment, NodeFlags, SegmentBuilder, SegmentCorrupt, SegmentEntry,
    SegmentPaths, SegmentTarget, CHUNK_BYTES, TREE_FORMAT_VERSION,
};
use tempfile::{tempdir, TempDir};

use crate::common::segment::{
    child_names, descend, file_names_of, files_in, names_to, paths_in, put_u32_at,
    resealed_single_chunk, CHECKSUM_BYTES, FIRST_SEQUENCE, FIRST_TREE_FORMAT, TREE_VERSION_OFFSET,
};

const TOKEN: u64 = 77;
const HEADER_BYTES: usize = 48;
const HIDDEN_RANGE_COUNT_OFFSET: usize = 32;
const HIDDEN_RANGE_BYTES: usize = 8;
const RANGE_BOUND_BYTES: usize = 4;
const HIDDEN_PLAN_RANGES: usize = 2;
const KELVIN: &str = "\u{212A}ELVIN";
const SCANNED: NodeFlags = NodeFlags {
    scanned: true,
    denied: false,
    skipped: false,
};
const DENIED: NodeFlags = NodeFlags {
    scanned: true,
    denied: true,
    skipped: false,
};
const SKIPPED: NodeFlags = NodeFlags {
    scanned: false,
    denied: false,
    skipped: true,
};
const UNSCANNED: NodeFlags = NodeFlags {
    scanned: false,
    denied: false,
    skipped: false,
};

struct Planned {
    depth: usize,
    name: &'static str,
    direct: u64,
    flags: NodeFlags,
    files: &'static [&'static str],
}

const fn planned(depth: usize, name: &'static str, direct: u64) -> Planned {
    Planned {
        depth,
        name,
        direct,
        flags: SCANNED,
        files: &[],
    }
}

fn plan() -> Vec<Planned> {
    vec![
        Planned {
            files: &["root.txt"],
            ..planned(0, "/data", 1)
        },
        Planned {
            files: &["a.md", "b.md"],
            ..planned(1, "alpha", 10)
        },
        planned(2, "deep", 100),
        Planned {
            files: &["leaf.bin"],
            ..planned(3, "leaf", 1000)
        },
        planned(1, "beta", 20),
        Planned {
            flags: DENIED,
            ..planned(2, "locked", 0)
        },
        Planned {
            flags: SKIPPED,
            ..planned(1, "skipped", 0)
        },
        Planned {
            flags: UNSCANNED,
            ..planned(1, "waiting", 0)
        },
        planned(0, "/other", 5),
        planned(1, "only", 7),
    ]
}

fn target(directory: &Path) -> SegmentTarget {
    SegmentTarget {
        paths: paths_in(directory),
        token: TOKEN,
    }
}

fn push_all(builder: &mut SegmentBuilder, entries: &[Planned]) {
    for (ordinal, entry) in entries.iter().enumerate() {
        let files = PackedNames::from_names(entry.files.iter().map(|name| name.as_bytes()));
        let pushed = builder.push(SegmentEntry {
            depth: entry.depth,
            name: entry.name,
            direct: entry.direct,
            flags: entry.flags,
            file_names: files.as_bytes(),
        });
        assert_eq!(pushed.expect("push"), ordinal as u32);
    }
}

struct Built {
    directory: TempDir,
    segment: MappedSegment,
}

fn built(entries: &[Planned]) -> Built {
    let directory = tempdir().expect("temp dir");
    let mut builder = SegmentBuilder::create(target(directory.path())).expect("builder");
    push_all(&mut builder, entries);
    let finished = builder.finish().expect("finish");
    assert_eq!(finished.node_count as usize, entries.len());
    let segment = MappedSegment::open(&finished.paths).expect("opens");
    Built { directory, segment }
}

#[test]
fn rows_carry_rolled_up_totals_counters_and_subtree_ranges() {
    let fixture = built(&plan());
    let segment = &fixture.segment;
    assert_eq!(segment.node_count(), 10);
    assert_eq!(segment.token(), TOKEN);
    assert_eq!(segment.top_level().expect("top level"), [0, 8]);

    let data = segment.row(0).expect("data row");
    assert_eq!(data.parent, None);
    assert_eq!(data.direct, 1);
    assert_eq!(data.total, 1 + 10 + 100 + 1000 + 20);
    assert_eq!((data.pending, data.skipped, data.denied), (1, 1, 1));
    assert_eq!(data.flags, SCANNED);
    assert_eq!(data.subtree_end, 8);

    let alpha = segment.row(1).expect("alpha row");
    assert_eq!(alpha.parent, Some(0));
    assert_eq!(alpha.total, 1110);
    assert_eq!((alpha.pending, alpha.skipped, alpha.denied), (0, 0, 0));
    assert_eq!(segment.subtree(1).expect("alpha subtree"), 1..4);

    let beta = segment.row(4).expect("beta row");
    assert_eq!((beta.total, beta.denied), (20, 1));
    assert_eq!(segment.row(5).expect("locked row").flags, DENIED);
    assert_eq!(segment.row(6).expect("skipped row").flags, SKIPPED);
    let waiting = segment.row(7).expect("waiting row");
    assert_eq!((waiting.flags, waiting.pending), (UNSCANNED, 1));

    let other = segment.row(8).expect("other row");
    assert_eq!(
        (other.parent, other.total, other.subtree_end),
        (None, 12, 10)
    );
}

#[test]
fn names_children_and_paths_resolve_through_the_mapping() {
    let fixture = built(&plan());
    let segment = &fixture.segment;
    assert_eq!(segment.name(0).expect("root name"), "/data");
    assert_eq!(names_to(segment, 3), ["/data", "alpha", "deep", "leaf"]);
    assert_eq!(
        child_names(segment, 0),
        ["alpha", "beta", "skipped", "waiting"]
    );
    assert!(segment.children(3).expect("leaf children").is_empty());
    let data_children = segment.children(0).expect("data children");
    assert_eq!(data_children.len(), 4);
    assert_eq!(data_children.get(1), Some(4));
    assert_eq!(data_children.get(4), None);

    assert_eq!(descend(segment, 0, &["alpha", "deep", "leaf"]), Some(3));
    assert_eq!(descend(segment, 0, &["beta", "locked"]), Some(5));
    assert_eq!(descend(segment, 0, &["waiting"]), Some(7));
    assert_eq!(descend(segment, 8, &["only"]), Some(9));
    assert_eq!(descend(segment, 0, &["missing"]), None);
    assert_eq!(descend(segment, 0, &["Alpha"]), None);
    assert_eq!(descend(segment, 0, &["aaa"]), None);
    assert_eq!(descend(segment, 0, &["zzz"]), None);
}

#[test]
fn file_names_share_the_tree_ordinals() {
    let fixture = built(&plan());
    let segment = &fixture.segment;
    assert_eq!(segment.file_names().header().folder_count, 10);
    assert_eq!(file_names_of(segment, 0), ["root.txt"]);
    assert_eq!(file_names_of(segment, 1), ["a.md", "b.md"]);
    assert_eq!(file_names_of(segment, 3), ["leaf.bin"]);
    assert!(file_names_of(segment, 4).is_empty());
}

#[test]
fn finishing_leaves_only_the_two_segment_files() {
    let Built { directory, .. } = built(&plan());
    assert_eq!(
        files_in(directory.path()),
        ["file-names-000001.bin", "folder-tree-000001.bin"]
    );
}

#[test]
fn an_empty_segment_opens_with_no_nodes() {
    let fixture = built(&[]);
    let segment = &fixture.segment;
    assert_eq!(segment.node_count(), 0);
    assert!(segment.top_level().expect("top level").is_empty());
    assert_eq!(segment.row(0), Err(SegmentCorrupt));
    assert_eq!(segment.name(0), Err(SegmentCorrupt));
}

#[test]
fn ids_outside_the_segment_are_reported_as_corrupt() {
    let fixture = built(&plan());
    let segment = &fixture.segment;
    assert_eq!(segment.row(10), Err(SegmentCorrupt));
    assert_eq!(segment.subtree(10), Err(SegmentCorrupt));
    assert_eq!(segment.name(10), Err(SegmentCorrupt));
    assert_eq!(segment.name(u32::MAX), Err(SegmentCorrupt));
    assert_eq!(segment.signature(10), Err(SegmentCorrupt));
    assert!(segment.children(10).is_err());
    assert!(segment.child_named(10, "alpha").is_err());
}

fn rejected(entries: &[Planned], last: SegmentEntry<'_>) -> BuildError {
    let directory = tempdir().expect("temp dir");
    let mut builder = SegmentBuilder::create(target(directory.path())).expect("builder");
    push_all(&mut builder, entries);
    let error = builder.push(last).expect_err("rejected");
    assert_eq!(builder.node_count() as usize, entries.len());
    builder.discard();
    assert!(files_in(directory.path()).is_empty());
    error
}

fn entry(depth: usize, name: &str) -> SegmentEntry<'_> {
    SegmentEntry {
        depth,
        name,
        direct: 0,
        flags: SCANNED,
        file_names: &[],
    }
}

#[test]
fn the_builder_rejects_entries_that_break_depth_first_name_order() {
    let roots = [planned(0, "/data", 0), planned(1, "beta", 0)];
    let skipped_level = rejected(&roots, entry(3, "deep"));
    let unsorted_sibling = rejected(&roots, entry(1, "alpha"));
    let repeated_sibling = rejected(&roots, entry(1, "beta"));
    let unnamed = rejected(&roots, entry(1, ""));
    let orphan = rejected(&[], entry(1, "alpha"));
    let unterminated_names = rejected(
        &roots,
        SegmentEntry {
            file_names: b"not-terminated",
            ..entry(1, "gamma")
        },
    );
    for error in [
        skipped_level,
        unsorted_sibling,
        repeated_sibling,
        unnamed,
        orphan,
        unterminated_names,
    ] {
        assert!(matches!(error, BuildError::Malformed), "{error:?}");
    }
}

#[test]
fn top_level_entries_may_arrive_in_any_name_order() {
    let fixture = built(&[planned(0, "/zeta", 1), planned(0, "/alpha", 2)]);
    let segment = &fixture.segment;
    assert_eq!(segment.top_level().expect("top level"), [0, 1]);
    assert_eq!(segment.name(1).expect("second root"), "/alpha");
}

#[test]
fn creating_a_builder_fails_when_the_directory_cannot_be_made() {
    let directory = tempdir().expect("temp dir");
    let blocker = directory.path().join("blocker");
    fs::write(&blocker, b"file").expect("blocker");
    let created = SegmentBuilder::create(target(&blocker));
    assert!(matches!(created, Err(BuildError::Io(_))));
}

#[test]
fn a_blocked_names_file_fails_creation_and_leaves_no_row_log() {
    let directory = tempdir().expect("temp dir");
    let names_temp = temp_path(&paths_in(directory.path()).names);
    fs::create_dir(&names_temp).expect("blocking directory");
    let created = SegmentBuilder::create(target(directory.path()));
    assert!(matches!(created, Err(BuildError::Io(_))));
    assert_eq!(files_in(directory.path()), ["file-names-000001.tmp"]);
}

const WIDE_CHILDREN: usize = 6_000;

fn wide_names() -> Vec<String> {
    (0..WIDE_CHILDREN)
        .map(|index| format!("child-{index:05}"))
        .collect()
}

fn built_wide() -> Built {
    let directory = tempdir().expect("temp dir");
    let mut builder = SegmentBuilder::create(target(directory.path())).expect("builder");
    builder.push(entry(0, "/wide")).expect("root");
    for (index, name) in wide_names().iter().enumerate() {
        let pushed = builder.push(SegmentEntry {
            direct: index as u64,
            ..entry(1, name)
        });
        pushed.expect("child");
    }
    let finished = builder.finish().expect("finish");
    let segment = MappedSegment::open(&finished.paths).expect("opens");
    Built { directory, segment }
}

#[test]
fn a_wide_folder_resolves_every_child_by_binary_search() {
    let fixture = built_wide();
    let segment = &fixture.segment;
    let expected_total: u64 = (0..WIDE_CHILDREN as u64).sum();
    assert_eq!(segment.row(0).expect("root").total, expected_total);
    assert_eq!(
        segment.subtree(0).expect("root subtree").len(),
        WIDE_CHILDREN + 1
    );
    for (index, name) in wide_names().iter().enumerate() {
        let child = segment.child_named(0, name).expect("lookup");
        assert_eq!(child, Some(index as u32 + 1));
    }
    assert_eq!(segment.child_named(0, "child-99999").expect("lookup"), None);
}

fn flip_byte(path: &Path, offset: usize) {
    let mut bytes = fs::read(path).expect("read");
    bytes[offset] ^= 0xff;
    fs::write(path, bytes).expect("write");
}

fn reopened(directory: &Path) -> Option<MappedSegment> {
    MappedSegment::open(&paths_in(directory))
}

#[test]
fn a_damaged_chunk_fails_only_the_reads_that_touch_it() {
    let Built { directory, segment } = built_wide();
    drop(segment);
    let tree = paths_in(directory.path()).tree;
    let file_len = fs::metadata(&tree).expect("metadata").len() as usize;
    assert!(file_len > 3 * CHUNK_BYTES, "spans several chunks");
    flip_byte(&tree, HEADER_BYTES);

    let damaged = reopened(directory.path()).expect("header still valid");
    assert_eq!(damaged.row(0), Err(SegmentCorrupt));
    assert_eq!(damaged.row(0), Err(SegmentCorrupt));
    let last = WIDE_CHILDREN as u32;
    assert_eq!(
        damaged.name(last).expect("name in a clean chunk"),
        "child-05999"
    );
    assert_eq!(
        damaged.name(last).expect("verified chunk reread"),
        "child-05999"
    );
}

#[test]
fn a_damaged_header_or_checksum_table_refuses_to_open() {
    let Built { directory, segment } = built(&plan());
    drop(segment);
    let tree = paths_in(directory.path()).tree;
    let original = fs::read(&tree).expect("read");

    for offset in [0, 9, 13, HEADER_BYTES - 1, original.len() - 1] {
        flip_byte(&tree, offset);
        assert!(reopened(directory.path()).is_none(), "offset {offset}");
        fs::write(&tree, &original).expect("restore");
    }
    fs::write(&tree, &original[..original.len() - 1]).expect("truncate");
    assert!(reopened(directory.path()).is_none());
    fs::write(&tree, &original[..HEADER_BYTES / 2]).expect("truncate header");
    assert!(reopened(directory.path()).is_none());
    fs::write(&tree, &original).expect("restore");
    assert!(reopened(directory.path()).is_some());
}

#[test]
fn a_segment_needs_both_files_with_the_same_token() {
    let directory = tempdir().expect("temp dir");
    assert!(reopened(directory.path()).is_none());

    let Built {
        directory: first, ..
    } = built(&plan());
    let other = tempdir().expect("temp dir");
    let mut builder = SegmentBuilder::create(SegmentTarget {
        paths: paths_in(other.path()),
        token: TOKEN + 1,
    })
    .expect("builder");
    push_all(&mut builder, &plan());
    builder.finish().expect("finish");

    let mixed = SegmentPaths {
        tree: paths_in(first.path()).tree,
        names: paths_in(other.path()).names,
    };
    assert!(MappedSegment::open(&mixed).is_none());
    fs::remove_file(paths_in(first.path()).names).expect("remove names");
    assert!(reopened(first.path()).is_none());
}

#[test]
fn numbered_paths_differ_per_sequence_and_per_file() {
    let directory = Path::new("cache");
    let first = SegmentPaths::numbered(directory, FIRST_SEQUENCE);
    let second = SegmentPaths::numbered(directory, FIRST_SEQUENCE + 1);
    assert_ne!(first.tree, first.names);
    assert_ne!(first, second);
    assert_eq!(first.tree, directory.join("folder-tree-000001.bin"));
    assert_eq!(second.names, directory.join("file-names-000002.bin"));
}

const MANY_FOLDERS: usize = 4_000;
const NAMES_PER_BIG_FOLDER: usize = 40;

fn many_folder_names(folder: usize) -> PackedNames {
    PackedNames::from_names(
        (0..NAMES_PER_BIG_FOLDER)
            .map(|file| format!("folder-{folder:05}-file-{file:03}-with-a-long-distinct-name.txt")),
    )
}

fn build_many(builder: &mut SegmentBuilder) {
    let root_names = PackedNames::default();
    builder
        .push(SegmentEntry {
            depth: 0,
            name: "/",
            direct: 0,
            flags: SCANNED,
            file_names: root_names.as_bytes(),
        })
        .expect("root");
    for folder in 0..MANY_FOLDERS {
        let name = format!("folder-{folder:05}");
        let names = many_folder_names(folder);
        builder
            .push(SegmentEntry {
                depth: 1,
                name: &name,
                direct: folder as u64,
                flags: SCANNED,
                file_names: names.as_bytes(),
            })
            .expect("folder");
    }
}

#[test]
fn a_large_build_writes_many_verified_blocks_whose_signatures_match_their_names() {
    let directory = tempdir().expect("temp dir");
    let paths = paths_in(directory.path());
    let mut builder = SegmentBuilder::create(SegmentTarget {
        paths: paths.clone(),
        token: TOKEN,
    })
    .expect("builder");
    build_many(&mut builder);
    let built = builder.finish().expect("built");
    assert!(built.names.blocks_rebuilt > 16, "several blocks");
    assert_eq!(built.names.blocks_copied, 0);

    let segment = MappedSegment::open(&paths).expect("opens");
    let names = segment.file_names();
    assert_eq!(names.block_count() as u64, built.names.blocks_rebuilt);
    for index in 0..names.block_count() {
        let touched = names.touch_block(index).expect("verifies");
        assert!(touched.newly_verified);
        for folder in touched.view.folders() {
            let expected: Vec<u32> = names_in(folder.packed).map(signature_of).collect();
            assert_eq!(folder.signatures.iter().collect::<Vec<u32>>(), expected);
        }
    }
    assert_eq!(
        file_names_of(&segment, 1234),
        many_folder_names(1233)
            .names()
            .map(|name| String::from_utf8_lossy(name).into_owned())
            .collect::<Vec<String>>()
    );
}

fn hidden_plan() -> Vec<Planned> {
    vec![
        planned(0, "/data", 1),
        planned(1, ".git", 2),
        planned(2, ".inner", 3),
        planned(2, "hooks", 4),
        planned(2, "objects", 5),
        planned(3, "pack", 6),
        planned(1, "Résumé", 7),
        planned(1, "src", 8),
        planned(2, ".cache", 9),
        planned(3, "deep", 10),
        planned(2, "main", 11),
        planned(1, KELVIN, 12),
    ]
}

#[test]
fn every_folder_carries_its_name_signature_and_top_most_dot_folders_are_hidden_ranges() {
    let entries = hidden_plan();
    let fixture = built(&entries);
    let segment = &fixture.segment;
    for (ordinal, entry) in (0..).zip(&entries) {
        let stored = segment.signature(ordinal).expect("signature");
        assert_eq!(
            stored,
            signature_of(entry.name.as_bytes()),
            "{}",
            entry.name
        );
    }
    assert_ne!(segment.signature(1).expect(".git") & DOT_BIT, 0);
    assert!(is_non_ascii(segment.signature(6).expect("Résumé")));
    assert!(is_non_ascii(segment.signature(11).expect("kelvin")));
    assert!(!is_non_ascii(segment.signature(7).expect("src")));

    let git_subtree = segment.subtree(1).expect(".git subtree");
    let cache_subtree = segment.subtree(8).expect(".cache subtree");
    assert_eq!((git_subtree.clone(), cache_subtree.clone()), (1..6, 8..10));
    assert_eq!(
        segment.hidden_ranges().expect("hidden ranges"),
        [git_subtree, cache_subtree]
    );
}

#[test]
fn a_dot_named_top_level_folder_opens_no_range_but_its_nested_dot_folders_do() {
    let fixture = built(&[
        planned(0, "/data", 1),
        planned(1, "visible", 2),
        planned(0, ".top", 3),
        planned(1, ".nested", 4),
        planned(2, ".deeper", 5),
        planned(1, "child", 6),
    ]);
    let nested_subtree = fixture.segment.subtree(3).expect("nested subtree");
    assert_eq!(nested_subtree, 3..5);
    assert_eq!(
        fixture.segment.hidden_ranges().expect("hidden"),
        [nested_subtree]
    );
    assert!(built(&plan())
        .segment
        .hidden_ranges()
        .expect("none")
        .is_empty());
}

struct RewrittenTree {
    directory: TempDir,
    original: Vec<u8>,
}

impl RewrittenTree {
    fn of(entries: &[Planned]) -> Self {
        let Built { directory, segment } = built(entries);
        drop(segment);
        let original = fs::read(paths_in(directory.path()).tree).expect("read");
        Self {
            directory,
            original,
        }
    }

    fn reopened_with(&self, change: impl Fn(&mut Vec<u8>)) -> Option<MappedSegment> {
        let mut bytes = self.original.clone();
        change(&mut bytes);
        let tree = paths_in(self.directory.path()).tree;
        fs::write(tree, resealed_single_chunk(bytes)).expect("write");
        reopened(self.directory.path())
    }

    fn hidden_entry_offset(&self, index: usize) -> usize {
        let table_end = self.original.len() - CHECKSUM_BYTES;
        table_end - (HIDDEN_PLAN_RANGES - index) * HIDDEN_RANGE_BYTES
    }

    fn assert_hidden_table_corrupt(&self, change: impl Fn(&mut Vec<u8>)) {
        let damaged = self.reopened_with(change).expect("checksums still match");
        assert_eq!(damaged.hidden_ranges(), Err(SegmentCorrupt));
        assert_eq!(damaged.signature(0), Ok(signature_of(b"/data")));
    }
}

#[test]
fn a_format_one_tree_file_is_refused_even_with_valid_checksums() {
    assert_eq!(TREE_FORMAT_VERSION, FIRST_TREE_FORMAT + 1);
    let rewritten = RewrittenTree::of(&plan());
    let unchanged = rewritten.reopened_with(|_| {});
    assert!(unchanged.is_some(), "resealing alone keeps the file valid");

    let format_one =
        rewritten.reopened_with(|bytes| put_u32_at(bytes, TREE_VERSION_OFFSET, FIRST_TREE_FORMAT));
    assert!(format_one.is_none());
}

#[test]
fn a_hidden_ranges_table_that_breaks_its_invariants_reads_as_corrupt() {
    let rewritten = RewrittenTree::of(&hidden_plan());
    let first = rewritten.hidden_entry_offset(0);
    let second = rewritten.hidden_entry_offset(1);
    let inside_the_git_subtree = 4;
    rewritten.assert_hidden_table_corrupt(|bytes| {
        put_u32_at(bytes, first + RANGE_BOUND_BYTES, inside_the_git_subtree)
    });
    rewritten.assert_hidden_table_corrupt(|bytes| {
        let first_entry = bytes[first..second].to_vec();
        bytes.copy_within(second..second + HIDDEN_RANGE_BYTES, first);
        bytes[second..second + HIDDEN_RANGE_BYTES].copy_from_slice(&first_entry);
    });
    rewritten.assert_hidden_table_corrupt(|bytes| put_u32_at(bytes, second, u32::MAX));

    let too_many = |bytes: &mut Vec<u8>| put_u32_at(bytes, HIDDEN_RANGE_COUNT_OFFSET, u32::MAX);
    assert!(rewritten.reopened_with(too_many).is_none());
}
