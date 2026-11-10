use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use file_explorer_lib::size::index::segment::{
    GraftTarget, MappedSegment, NodeFlags, SegmentBuilder, SegmentEntry, SegmentTarget,
    SpillOptions, WalkSpill, RUN_BYTES,
};
use file_explorer_lib::size::index::tree::{DirectoryListing, FolderSizeTree, Forest, Graft};
use tempfile::{tempdir, TempDir};

use crate::common::segment::{
    append, heap_of, listing, paths_in, strings, walked, walked_segment, SegmentPlace, Walked,
};

const BASE_TOKEN: u64 = 11;
const GRAFT_TOKEN: u64 = 12;
const GRAFT_SPILL_FILE: &str = "graft.spill";

fn base_walk(root: &Path) -> Vec<Walked> {
    let folder_a = root.join("a");
    vec![
        Walked {
            subdirs: strings(&["a", "s"]),
            ..walked(root.to_path_buf(), 1)
        },
        Walked {
            subdirs: strings(&["b", "old"]),
            ..walked(folder_a.clone(), 10)
        },
        walked(folder_a.join("b"), 100),
        Walked {
            subdirs: strings(&["deep"]),
            ..walked(folder_a.join("old"), 40)
        },
        walked(folder_a.join("old").join("deep"), 400),
        walked(root.join("s"), 5),
    ]
}

fn rewalk_of_a(root: &Path) -> Vec<Walked> {
    let folder_a = root.join("a");
    vec![
        Walked {
            subdirs: strings(&["b", "new"]),
            ..walked(folder_a.clone(), 20)
        },
        walked(folder_a.join("b"), 200),
        walked(folder_a.join("new"), 2_000),
    ]
}

fn rewalk_of_a_keeping_old(root: &Path) -> Vec<Walked> {
    let folder_a = root.join("a");
    vec![
        Walked {
            subdirs: strings(&["b", "old"]),
            ..walked(folder_a.clone(), 20)
        },
        walked(folder_a.join("b"), 200),
    ]
}

struct Fixture {
    directory: TempDir,
    root: PathBuf,
    mapped: FolderSizeTree,
}

fn fixture() -> Fixture {
    let directory = tempdir().expect("temp dir");
    let root = directory.path().join("graft-root");
    let base = walked_segment(
        SegmentPlace {
            directory: &directory.path().join("base"),
            root: &root,
            token: BASE_TOKEN,
        },
        &base_walk(&root),
    );
    let mut mapped = FolderSizeTree::default();
    mapped.attach_base(Arc::new(base)).expect("attached");
    Fixture {
        directory,
        root,
        mapped,
    }
}

impl Fixture {
    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    fn graft_segment(&self, under: &str, records: &[Walked]) -> Arc<MappedSegment> {
        let walk_root = self.path(under);
        let directory = self
            .directory
            .path()
            .join(format!("graft-{}", under.replace('/', "-")));
        let mut spill = WalkSpill::create(SpillOptions {
            runs_path: directory.join(GRAFT_SPILL_FILE),
            root: walk_root.clone(),
            run_bytes: RUN_BYTES,
        });
        for record in records {
            append(&mut spill, record).expect("append");
        }
        let root_name = walk_root.file_name().expect("named").to_string_lossy();
        let built = spill
            .build(GraftTarget {
                segment: SegmentTarget {
                    paths: paths_in(&directory),
                    token: GRAFT_TOKEN,
                },
                root_name: root_name.into_owned(),
            })
            .expect("build");
        Arc::new(MappedSegment::open(&built.paths).expect("opens"))
    }

    fn total(&self, relative: &str) -> Option<u64> {
        self.mapped
            .total(&self.path(relative))
            .map(|total| total.bytes)
    }

    fn live_ids_in_ranges(&self, relative: &str) -> BTreeSet<u32> {
        let id = self.mapped.lookup(&self.path(relative)).expect("folder");
        let ranges = self.mapped.subtree_ranges(id);
        ranges
            .ranges()
            .iter()
            .flat_map(Clone::clone)
            .filter(|&member| self.is_live(member))
            .collect()
    }

    fn is_live(&self, id: u32) -> bool {
        !self.mapped.subtree_slots(id).is_empty()
    }

    fn subtree_ids(&self, relative: &str) -> BTreeSet<u32> {
        let id = self.mapped.lookup(&self.path(relative)).expect("folder");
        self.mapped.subtree_slots(id).into_iter().collect()
    }
}

fn heap_after_rewalk(root: &Path, rewalk: &[Walked]) -> FolderSizeTree {
    let mut heap = heap_of(root, &base_walk(root));
    assert!(heap.prune(&root.join("a")));
    for record in rewalk {
        heap.merge_record(&record.path, listing(record));
    }
    heap
}

#[test]
fn a_graft_replaces_a_mapped_subtree_and_answers_like_the_heap_tree() {
    let mut fixture = fixture();
    let rewalk = rewalk_of_a(&fixture.root);
    let segment = fixture.graft_segment("a", &rewalk);
    let folder_a = fixture.path("a");
    let ids = fixture
        .mapped
        .install_graft(Graft {
            path: &folder_a,
            segment,
            kept: &[],
        })
        .expect("installed");
    assert_eq!(ids.len(), 3);
    let heap = heap_after_rewalk(&fixture.root, &rewalk);

    for relative in ["", "a", "a/b", "a/new", "s"] {
        let path = fixture.path(relative);
        assert_eq!(
            fixture.mapped.total(&path),
            heap.total(&path),
            "{relative:?}"
        );
    }
    assert_eq!(fixture.total("a/old"), None, "the old subtree is gone");
    assert_eq!(fixture.mapped.node_count(), heap.node_count());
    assert!(fixture.mapped.is_dirty() && fixture.mapped.structure_changed());
    assert!(!fixture.mapped.take_removed_ranges().is_empty());
}

#[test]
fn a_boost_graft_moves_the_subtrees_it_kept_under_the_new_walk() {
    let mut fixture = fixture();
    let segment = fixture.graft_segment("a", &rewalk_of_a_keeping_old(&fixture.root));
    let folder_a = fixture.path("a");
    let kept = vec![fixture.path("a/old")];
    let old_before = fixture.mapped.lookup(&kept[0]).expect("old");

    fixture
        .mapped
        .install_graft(Graft {
            path: &folder_a,
            segment,
            kept: &kept,
        })
        .expect("installed");

    assert_eq!(fixture.mapped.lookup(&kept[0]), Some(old_before));
    assert_eq!(fixture.total("a/old"), Some(440));
    assert_eq!(fixture.total("a"), Some(20 + 200 + 440));
    assert_eq!(fixture.total(""), Some(1 + 660 + 5));
    assert_eq!(fixture.total("a/old/deep"), Some(400));
    assert!(fixture.mapped.unscanned_tops().is_empty());
}

#[test]
fn subtree_ranges_cover_exactly_the_live_subtree_after_grafts_and_edits() {
    let mut fixture = fixture();
    let untouched = fixture.live_ids_in_ranges("");
    assert_eq!(untouched, fixture.subtree_ids(""));
    let root_id = fixture.mapped.lookup(&fixture.root).expect("root");
    assert_eq!(
        fixture.mapped.subtree_ranges(root_id).ranges().len(),
        1,
        "an untouched segment is one range"
    );

    let segment = fixture.graft_segment("a", &rewalk_of_a_keeping_old(&fixture.root));
    let folder_a = fixture.path("a");
    let kept = vec![fixture.path("a/old")];
    fixture
        .mapped
        .install_graft(Graft {
            path: &folder_a,
            segment,
            kept: &kept,
        })
        .expect("installed");
    assert!(fixture.mapped.prune(&fixture.path("s")));

    for relative in ["", "a", "a/old"] {
        assert_eq!(
            fixture.live_ids_in_ranges(relative),
            fixture.subtree_ids(relative),
            "{relative:?}"
        );
    }
    assert!(fixture.mapped.subtree_ranges(u32::MAX).ranges().is_empty());
}

#[test]
fn a_graft_at_a_new_path_hangs_under_its_existing_parent() {
    let mut fixture = fixture();
    let fresh = vec![walked(fixture.path("s/fresh"), 7)];
    let segment = fixture.graft_segment("s/fresh", &fresh);
    let path = fixture.path("s/fresh");

    fixture
        .mapped
        .install_graft(Graft {
            path: &path,
            segment,
            kept: &[],
        })
        .expect("installed");

    assert_eq!(fixture.total("s/fresh"), Some(7));
    assert_eq!(fixture.total("s"), Some(12));
}

fn two_top_segment(directory: &Path) -> Arc<MappedSegment> {
    let target = SegmentTarget {
        paths: paths_in(directory),
        token: GRAFT_TOKEN,
    };
    let mut builder = SegmentBuilder::create(target).expect("builder");
    for top in ["first", "second"] {
        builder
            .push(SegmentEntry {
                depth: 0,
                name: top,
                direct: 1,
                flags: NodeFlags::default(),
                file_names: &[],
            })
            .expect("top");
    }
    let built = builder.finish().expect("finish");
    Arc::new(MappedSegment::open(&built.paths).expect("opens"))
}

#[test]
fn a_graft_outside_every_root_or_with_several_tops_is_refused() {
    let mut fixture = fixture();
    let segment = fixture.graft_segment("a", &rewalk_of_a(&fixture.root));
    let outside = PathBuf::from("/elsewhere/a");
    let refused_outside = fixture.mapped.install_graft(Graft {
        path: &outside,
        segment,
        kept: &[],
    });
    assert!(refused_outside.is_none());

    let folder_a = fixture.path("a");
    let several = two_top_segment(&fixture.directory.path().join("two-tops"));
    let refused_several = fixture.mapped.install_graft(Graft {
        path: &folder_a,
        segment: several,
        kept: &[],
    });
    assert!(refused_several.is_none());
    assert_eq!(fixture.total("a"), Some(550), "the tree is untouched");
}

#[test]
fn changes_inside_a_graft_reach_every_ancestor_above_it() {
    let mut fixture = fixture();
    let rewalk = rewalk_of_a(&fixture.root);
    let segment = fixture.graft_segment("a", &rewalk);
    let folder_a = fixture.path("a");
    fixture
        .mapped
        .install_graft(Graft {
            path: &folder_a,
            segment,
            kept: &[],
        })
        .expect("installed");
    let before = fixture.total("").expect("root");

    let grown: Vec<String> = Vec::new();
    let listing = DirectoryListing::scanned(2_500, &grown);
    fixture
        .mapped
        .apply_listing(&fixture.path("a/new"), listing);

    assert_eq!(fixture.total("a"), Some(20 + 200 + 2_500));
    assert_eq!(fixture.total(""), Some(before + 500));
}

impl Fixture {
    fn forest_segment(&self, records: &[Walked]) -> Arc<MappedSegment> {
        let directory = self.directory.path().join("forest");
        let mut spill = WalkSpill::create(SpillOptions {
            runs_path: directory.join(GRAFT_SPILL_FILE),
            root: self.root.clone(),
            run_bytes: RUN_BYTES,
        });
        append(&mut spill, &walked(self.root.clone(), 0)).expect("root");
        for record in records {
            append(&mut spill, record).expect("append");
        }
        let built = spill
            .build(GraftTarget {
                segment: SegmentTarget {
                    paths: paths_in(&directory),
                    token: GRAFT_TOKEN,
                },
                root_name: self.root.to_string_lossy().into_owned(),
            })
            .expect("build");
        Arc::new(MappedSegment::open(&built.paths).expect("opens"))
    }
}

#[test]
fn a_forest_grafts_each_top_under_its_own_parent_and_buries_the_rest() {
    let mut fixture = fixture();
    let records = vec![
        Walked {
            subdirs: strings(&["n"]),
            ..walked(fixture.path("a/b"), 300)
        },
        walked(fixture.path("a/b/n"), 7),
        walked(fixture.path("s"), 55),
    ];
    let segment = fixture.forest_segment(&records);
    let tops = vec![
        fixture.path("a/b"),
        fixture.path("missing"),
        fixture.path("s"),
    ];
    let folder_a = fixture.mapped.lookup(&fixture.path("a"));
    let nodes_before = fixture.mapped.node_count();

    let ids = fixture
        .mapped
        .install_forest(Forest {
            root: &fixture.root,
            tops: &tops,
            segment,
        })
        .expect("installed");

    assert_eq!(fixture.total("a/b"), Some(307));
    assert_eq!(fixture.total("s"), Some(55));
    assert_eq!(fixture.total("a"), Some(10 + 307 + 440));
    assert_eq!(fixture.total(""), Some(1 + 757 + 55));
    assert_eq!(fixture.mapped.lookup(&fixture.path("a")), folder_a);
    assert_eq!(fixture.total("missing"), None);
    assert_eq!(fixture.mapped.node_count(), nodes_before + 1);
    assert!(fixture.mapped.unscanned_tops().is_empty());
    let removed = fixture.mapped.take_removed_ranges();
    assert!(
        removed.iter().any(|dead| dead.start == ids.start),
        "the synthetic root is buried"
    );
}

#[test]
fn a_forest_outside_every_root_is_refused() {
    let mut fixture = fixture();
    let segment = fixture.forest_segment(&[walked(fixture.path("s"), 1)]);
    let elsewhere = PathBuf::from("/elsewhere");
    let tops = vec![elsewhere.join("s")];
    let refused = fixture.mapped.install_forest(Forest {
        root: &elsewhere,
        tops: &tops,
        segment,
    });
    assert!(refused.is_none());
    assert_eq!(fixture.total("s"), Some(5));
}
