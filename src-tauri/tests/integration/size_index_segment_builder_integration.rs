use std::fs;
use std::path::Path;

use file_explorer_lib::size::index::names::PackedNames;
use file_explorer_lib::size::index::persist::temp_path;
use file_explorer_lib::size::index::segment::{
    BuildError, MappedSegment, NodeFlags, SegmentBuilder, SegmentCorrupt, SegmentEntry,
    SegmentPaths, SegmentTarget, CHUNK_BYTES,
};
use tempfile::{tempdir, TempDir};

use crate::common::segment::{
    child_names, descend, file_names_of, files_in, names_to, paths_in, FIRST_SEQUENCE,
};

const TOKEN: u64 = 77;
const HEADER_BYTES: usize = 48;
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
