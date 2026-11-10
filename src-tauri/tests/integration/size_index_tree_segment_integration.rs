use std::fmt::Debug;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use file_explorer_lib::size::index::segment::{
    MappedSegment, NodeFlags, SegmentBuilder, SegmentEntry, SegmentTarget,
};
use file_explorer_lib::size::index::tree::{
    DirectoryListing, FolderSizeTree, TreeOverlayStats, OVERLAY_CAP_DEAD_RANGES,
    OVERLAY_CAP_FOLDERS,
};
use tempfile::{tempdir, TempDir};

use crate::common::segment::{
    heap_of, paths_in, strings, walked, walked_segment, SegmentPlace, Walked,
};

const TOKEN: u64 = 5;
const SEGMENT_HEADER_BYTES: usize = 48;
const WIDE_CHILDREN: usize = 6_000;
const WIDE_ROOT: &str = "wide-root";
const FOLDERS_IN_THE_WALK: usize = 8;

/// Every folder the tests create, move or remove, relative to the root.
const KNOWN_FOLDERS: [&[&str]; 14] = [
    &[],
    &["a"],
    &["a", "b"],
    &["a", "b", "c"],
    &["a", "old"],
    &["a", "old", "deep"],
    &["a", "old", "deep", "later"],
    &["a", "new"],
    &["a", "new", "inner"],
    &["pending"],
    &["s"],
    &["s", "moved"],
    &["s", "x", "y"],
    &["missing"],
];

/// root(1) / a(10) / b(100) / c(1000, denied), a / old(40) / deep(400),
/// root / s(5), and root / pending which the walk never visited.
fn walk(root: &Path) -> Vec<Walked> {
    let folder_a = root.join("a");
    vec![
        Walked {
            subdirs: strings(&["a", "s", "pending"]),
            ..walked(root.to_path_buf(), 1)
        },
        Walked {
            subdirs: strings(&["b", "old"]),
            ..walked(folder_a.clone(), 10)
        },
        Walked {
            subdirs: strings(&["c"]),
            ..walked(folder_a.join("b"), 100)
        },
        Walked {
            denied: true,
            ..walked(folder_a.join("b").join("c"), 1000)
        },
        Walked {
            subdirs: strings(&["deep"]),
            ..walked(folder_a.join("old"), 40)
        },
        walked(folder_a.join("old").join("deep"), 400),
        walked(root.join("s"), 5),
    ]
}

/// The same walk held twice: once on the heap, once in a mapped segment.
struct Twins {
    _directory: TempDir,
    root: PathBuf,
    heap: FolderSizeTree,
    mapped: FolderSizeTree,
}

fn twins() -> Twins {
    let directory = tempdir().expect("temp dir");
    let root = directory.path().join("walk-root");
    let records = walk(&root);
    let place = SegmentPlace {
        directory: directory.path(),
        root: &root,
        token: TOKEN,
    };
    let segment = walked_segment(place, &records);
    let mut mapped = FolderSizeTree::default();
    let ids = mapped.attach_base(Arc::new(segment)).expect("attached");
    assert_eq!(ids, 0..FOLDERS_IN_THE_WALK as u32);
    let mut heap = heap_of(&root, &records);
    heap.drain_changed(Vec::<PathBuf>::new());
    Twins {
        heap,
        mapped,
        root,
        _directory: directory,
    }
}

fn sorted(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort();
    paths
}

impl Twins {
    fn path(&self, parts: &[&str]) -> PathBuf {
        parts
            .iter()
            .fold(self.root.clone(), |path, part| path.join(part))
    }

    fn both<T: PartialEq + Debug>(&mut self, operation: impl Fn(&mut FolderSizeTree) -> T) -> T {
        let on_heap = operation(&mut self.heap);
        let on_segment = operation(&mut self.mapped);
        assert_eq!(on_segment, on_heap);
        self.assert_same_answers();
        on_segment
    }

    fn assert_same_answers(&self) {
        for parts in KNOWN_FOLDERS {
            let path = self.path(parts);
            assert_eq!(
                self.mapped.total(&path),
                self.heap.total(&path),
                "{parts:?}"
            );
            assert_eq!(
                sorted(self.mapped.indexed_subtrees_under(&path)),
                sorted(self.heap.indexed_subtrees_under(&path)),
                "{parts:?}"
            );
        }
        assert_eq!(self.mapped.node_count(), self.heap.node_count());
        assert_eq!(
            sorted(self.mapped.unscanned_tops()),
            sorted(self.heap.unscanned_tops())
        );
        assert_eq!(
            self.mapped.export_slots().len(),
            self.heap.export_slots().len()
        );
    }

    fn drain_both(&mut self) -> Vec<PathBuf> {
        let subscribed: Vec<PathBuf> = KNOWN_FOLDERS.iter().map(|parts| self.path(parts)).collect();
        self.both(|tree| tree.drain_changed(&subscribed))
    }
}

#[test]
fn a_tree_attached_to_a_segment_answers_like_the_heap_tree() {
    let mut twins = twins();
    twins.assert_same_answers();
    assert_eq!(twins.mapped.node_count(), FOLDERS_IN_THE_WALK);
    assert_eq!(twins.mapped.overlay_stats(), TreeOverlayStats::default());
    assert!(twins.mapped.heap_ids().is_empty());
    assert!(!twins.mapped.is_dirty() && !twins.mapped.structure_changed());
    assert!(!twins.mapped.is_corrupt());
    assert!(twins.drain_both().is_empty());

    let root_id = twins.mapped.lookup(&twins.root).expect("root");
    assert_eq!(
        twins.mapped.subtree_slots(root_id),
        (0..FOLDERS_IN_THE_WALK as u32).collect::<Vec<u32>>(),
        "segment ids are the depth-first order"
    );
    assert_eq!(
        twins.mapped.export_slots(),
        twins.mapped.subtree_slots(root_id)
    );

    let shouted = twins.path(&["A", "OLD", "Deep"]);
    assert_eq!(
        twins.mapped.lookup(&shouted),
        twins.mapped.lookup(&twins.path(&["a", "old", "deep"]))
    );
    assert_eq!(twins.mapped.total(&shouted), twins.heap.total(&shouted));
}

#[test]
fn a_reread_patches_the_mapped_folder_and_every_ancestor() {
    let mut twins = twins();
    let deep = twins.path(&["a", "old", "deep"]);
    let added =
        twins.both(|tree| tree.apply_listing::<&str>(&deep, DirectoryListing::scanned(900, &[])));
    assert_eq!(added, Some(vec![]));
    let root_total = twins.mapped.total(&twins.root).expect("root");
    assert_eq!(root_total.bytes, 1 + 10 + 100 + 1000 + 40 + 900 + 5);
    assert_eq!(twins.mapped.overlay_stats().patched_folders, 4);
    assert!(twins.mapped.is_dirty());
    assert!(!twins.mapped.structure_changed(), "sizes only");

    let changed = twins.drain_both();
    assert_eq!(
        sorted(changed),
        sorted(vec![
            twins.root.clone(),
            twins.path(&["a"]),
            twins.path(&["a", "old"]),
            deep.clone()
        ])
    );
    assert!(twins.drain_both().is_empty());
    assert_eq!(
        twins.mapped.overlay_stats().patched_folders,
        4,
        "size patches outlive the change report"
    );
}

#[test]
fn a_reread_that_changes_nothing_leaves_no_overlay_behind() {
    let mut twins = twins();
    let old = twins.path(&["a", "old"]);
    let added =
        twins.both(|tree| tree.apply_listing(&old, DirectoryListing::scanned(40, &["deep"])));
    assert_eq!(added, Some(vec![]));
    assert_eq!(twins.mapped.overlay_stats(), TreeOverlayStats::default());
    assert!(!twins.mapped.is_dirty());
}

#[test]
fn new_folders_below_mapped_parents_live_in_the_overlay() {
    let mut twins = twins();
    let folder_a = twins.path(&["a"]);
    let added = twins.both(|tree| {
        tree.apply_listing(
            &folder_a,
            DirectoryListing::scanned(12, &["b", "new", "old"]),
        )
    });
    assert_eq!(added, Some(vec![twins.path(&["a", "new"])]));
    let stats = twins.mapped.overlay_stats();
    assert_eq!(stats.heap_folders, 1);
    assert_eq!(twins.mapped.heap_ids().len(), 1);
    assert!(twins.mapped.structure_changed());

    let inner = twins.path(&["a", "new", "inner"]);
    let deep_new = twins.path(&["s", "x", "y"]);
    twins.both(|tree| tree.merge_record::<&str>(&inner, DirectoryListing::scanned(7, &[])));
    twins.both(|tree| tree.merge_record::<&str>(&deep_new, DirectoryListing::scanned(3, &[])));
    let new_slot = twins.mapped.lookup(&inner).expect("new folder");
    assert!(twins.mapped.heap_ids().contains(&new_slot));
    assert_eq!(twins.mapped.overlay_stats().heap_folders, 4);

    let listed_again = twins.both(|tree| {
        tree.apply_listing(
            &folder_a,
            DirectoryListing::scanned(12, &["b", "new", "old"]),
        )
    });
    assert_eq!(listed_again, Some(vec![]));
    assert!(!twins.drain_both().is_empty());
}

#[test]
fn removing_mapped_subtrees_reports_dead_ranges_instead_of_slots() {
    let mut twins = twins();
    let folder_a = twins.path(&["a"]);
    let old = twins
        .mapped
        .lookup(&twins.path(&["a", "old"]))
        .expect("old");
    twins.both(|tree| tree.apply_listing(&folder_a, DirectoryListing::scanned(10, &["b"])));
    assert_eq!(twins.mapped.take_removed_ranges(), vec![old..old + 2]);
    assert!(twins.mapped.take_removed_slots().is_empty());
    assert!(twins.mapped.take_removed_ranges().is_empty(), "drained");
    assert_eq!(twins.mapped.overlay_stats().dead_ranges, 1);
    assert!(twins.mapped.subtree_slots(old).is_empty(), "a dead id");

    let pending = twins.path(&["pending"]);
    let side = twins.path(&["s"]);
    assert!(twins.both(|tree| tree.prune(&pending)));
    assert!(twins.both(|tree| tree.prune(&side)));
    assert_eq!(
        twins.mapped.overlay_stats().dead_ranges,
        1,
        "neighbouring removals share one range"
    );

    assert!(twins.both(|tree| tree.prune(&folder_a)));
    assert_eq!(twins.mapped.node_count(), 1);
    assert_eq!(
        twins.mapped.overlay_stats().dead_ranges,
        1,
        "a removed subtree swallows the ranges inside it"
    );
    assert!(!twins.both(|tree| tree.prune(&folder_a)), "already gone");
}

#[test]
fn removing_a_mapped_subtree_frees_the_overlay_folders_below_it() {
    let mut twins = twins();
    let later = twins.path(&["a", "old", "deep", "later"]);
    twins.both(|tree| tree.merge_record::<&str>(&later, DirectoryListing::scanned(2, &[])));
    let later_slot = twins.mapped.lookup(&later).expect("overlay folder");
    twins.mapped.take_removed_slots();

    let old = twins.path(&["a", "old"]);
    assert!(twins.both(|tree| tree.prune(&old)));
    assert_eq!(twins.mapped.take_removed_slots(), vec![later_slot]);
    assert_eq!(twins.mapped.take_removed_ranges().len(), 1);
    assert_eq!(twins.mapped.overlay_stats().heap_folders, 0);
    assert_eq!(twins.mapped.lookup(&later), None);
}

#[test]
fn skipping_excluding_and_including_mapped_folders_matches_the_heap() {
    let mut twins = twins();
    let side = twins.path(&["s"]);
    let old = twins.path(&["a", "old"]);
    let pending = twins.path(&["pending"]);
    assert!(twins.both(|tree| tree.set_skipped(&side, true)));
    let root_total = twins.mapped.total(&twins.root).expect("root");
    let bound = root_total.lower_bound.expect("skipped below");
    assert_eq!(bound.skipped_paths, vec![side.clone()]);
    assert_eq!(bound.denied_count, 1);
    assert!(twins.both(|tree| tree.set_skipped(&side, false)));

    let excluded = twins.both(|tree| tree.exclude_subtree(&old).map(|folder| folder.changed));
    assert_eq!(excluded, Some(true));
    let again = twins.both(|tree| tree.exclude_subtree(&old).map(|folder| folder.changed));
    assert_eq!(again, Some(false));
    assert_eq!(twins.mapped.take_removed_ranges().len(), 1);
    assert!(twins.both(|tree| tree.include_subtree(&old)));
    assert!(!twins.both(|tree| tree.include_subtree(&old)));

    let root = twins.root.clone();
    assert!(twins.both(|tree| tree.prune_unscanned(&root)));
    assert_eq!(twins.mapped.total(&pending), None);
    assert_eq!(
        twins.mapped.total(&old),
        None,
        "included again, never rescanned"
    );
}

#[test]
fn only_overlay_folders_can_be_moved() {
    let mut twins = twins();
    let mapped_folder = twins.path(&["a", "b"]);
    assert!(!twins
        .mapped
        .rename(&mapped_folder, &twins.path(&["s", "b"])));
    assert_eq!(twins.mapped.overlay_stats(), TreeOverlayStats::default());

    let new = twins.path(&["a", "new"]);
    let moved = twins.path(&["s", "moved"]);
    twins.both(|tree| tree.merge_record::<&str>(&new, DirectoryListing::scanned(9, &[])));
    assert!(twins.both(|tree| tree.rename(&new, &moved)));
    assert_eq!(twins.mapped.total(&moved).expect("moved").bytes, 9);
    assert_eq!(
        twins.mapped.total(&twins.path(&["s"])).expect("s").bytes,
        14
    );
}

#[test]
fn a_tree_that_already_holds_folders_refuses_a_base_segment() {
    let directory = tempdir().expect("temp dir");
    let root = directory.path().join("walk-root");
    let place = SegmentPlace {
        directory: directory.path(),
        root: &root,
        token: TOKEN,
    };
    let segment = Arc::new(walked_segment(place, &walk(&root)));
    let mut tree = FolderSizeTree::default();
    assert!(tree.add_root(&root));
    assert_eq!(tree.attach_base(Arc::clone(&segment)), None);

    let mut attached = FolderSizeTree::default();
    assert!(attached.attach_base(Arc::clone(&segment)).is_some());
    assert_eq!(attached.attach_base(segment), None);
    assert!(!attached.add_root(&root), "the segment root is the root");
}

#[test]
fn the_overlay_cap_counts_folders_and_dead_ranges() {
    let at_cap = TreeOverlayStats {
        heap_folders: OVERLAY_CAP_FOLDERS - 1,
        patched_folders: 1,
        dead_ranges: OVERLAY_CAP_DEAD_RANGES,
    };
    assert!(!at_cap.exceeds_cap());
    let too_many_folders = TreeOverlayStats {
        patched_folders: 2,
        ..at_cap
    };
    assert!(too_many_folders.exceeds_cap());
    let too_many_ranges = TreeOverlayStats {
        dead_ranges: OVERLAY_CAP_DEAD_RANGES + 1,
        ..at_cap
    };
    assert!(too_many_ranges.exceeds_cap());
}

fn scanned_entry(depth: usize, name: &str) -> SegmentEntry<'_> {
    SegmentEntry {
        depth,
        name,
        direct: 1,
        flags: NodeFlags {
            scanned: true,
            ..NodeFlags::default()
        },
        file_names: &[],
    }
}

fn wide_segment(directory: &Path) -> PathBuf {
    let target = SegmentTarget {
        paths: paths_in(directory),
        token: TOKEN,
    };
    let mut builder = SegmentBuilder::create(target).expect("builder");
    builder.push(scanned_entry(0, WIDE_ROOT)).expect("root");
    for index in 0..WIDE_CHILDREN {
        let name = format!("child-{index:05}");
        builder.push(scanned_entry(1, &name)).expect("child");
    }
    builder.finish().expect("finish").paths.tree
}

#[test]
fn a_damaged_segment_chunk_marks_the_tree_corrupt_and_reads_as_empty() {
    let directory = tempdir().expect("temp dir");
    let tree_file = wide_segment(directory.path());
    let mut bytes = fs::read(&tree_file).expect("read");
    bytes[SEGMENT_HEADER_BYTES] ^= 0xff;
    fs::write(&tree_file, bytes).expect("write");

    let segment = MappedSegment::open(&paths_in(directory.path())).expect("header still valid");
    let mut tree = FolderSizeTree::default();
    tree.attach_base(Arc::new(segment)).expect("attached");
    assert!(!tree.is_corrupt(), "nothing damaged was read yet");

    let last_child = Path::new(WIDE_ROOT).join("child-05999");
    assert!(
        tree.lookup(&last_child).is_some(),
        "names are in a clean chunk"
    );
    let total = tree.total(Path::new(WIDE_ROOT)).expect("root");
    assert_eq!(total.bytes, 0, "the damaged size columns read as empty");
    assert!(tree.is_corrupt());
}
