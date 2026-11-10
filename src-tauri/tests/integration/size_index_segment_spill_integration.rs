use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use file_explorer_lib::size::index::names::PackedNames;
use file_explorer_lib::size::index::segment::{
    BuildError, GraftTarget, MappedSegment, SegmentTarget, SpillOptions, SpillRecord, WalkSpill,
    RUN_BYTES,
};
use file_explorer_lib::size::index::tree::{DirectoryListing, FolderSizeTree};
use tempfile::{tempdir, TempDir};

use crate::common::segment::{child_names, descend, file_names_of, files_in, names_to, paths_in};

const TOKEN: u64 = 9;
const ROOT_ID: u32 = 0;
const TINY_RUN_BYTES: usize = 512;
const ONE_RECORD_PER_RUN: usize = 1;
const TOP_FOLDERS: [&str; 6] = ["a", "a b", "a.b", "B", "é", "zz"];
const BULK_FOLDERS: usize = 300;

#[derive(Clone)]
struct Walked {
    path: PathBuf,
    direct: u64,
    denied: bool,
    subdirs: Vec<String>,
    files: Vec<String>,
}

fn walked(path: PathBuf, direct: u64) -> Walked {
    Walked {
        path,
        direct,
        denied: false,
        subdirs: Vec::new(),
        files: Vec::new(),
    }
}

fn strings(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

fn walked_top_folder(root: &Path, position: usize) -> Vec<Walked> {
    let top = root.join(TOP_FOLDERS[position]);
    let weight = position as u64 + 1;
    vec![
        Walked {
            subdirs: strings(&["y", "x"]),
            files: vec![format!("{}.file", TOP_FOLDERS[position])],
            ..walked(top.clone(), 10 * weight)
        },
        Walked {
            subdirs: strings(&["deep"]),
            ..walked(top.join("x"), 100 * weight)
        },
        Walked {
            denied: position.is_multiple_of(2),
            files: strings(&["d1", "d2", "d3"]),
            ..walked(top.join("x").join("deep"), 1000 * weight)
        },
    ]
}

fn walked_bulk(root: &Path) -> Vec<Walked> {
    let bulk = root.join("bulk");
    let names: Vec<String> = (0..BULK_FOLDERS)
        .map(|index| format!("folder-{index:04}"))
        .collect();
    let mut records = vec![Walked {
        subdirs: names.clone(),
        ..walked(bulk.clone(), 1)
    }];
    records.extend(names.iter().enumerate().map(|(index, name)| Walked {
        files: vec![format!("{name}.dat")],
        ..walked(bulk.join(name), index as u64)
    }));
    records
}

fn walked_tree(root: &Path) -> Vec<Walked> {
    let mut subdirs = strings(&TOP_FOLDERS);
    subdirs.extend(strings(&["unvisited", "bulk"]));
    let mut records = vec![Walked {
        subdirs,
        files: strings(&["r1.txt", "r2.txt"]),
        ..walked(root.to_path_buf(), 5)
    }];
    for position in 0..TOP_FOLDERS.len() {
        records.extend(walked_top_folder(root, position));
    }
    records.extend(walked_bulk(root));
    records
}

fn listing(record: &Walked) -> DirectoryListing<'_, String> {
    DirectoryListing {
        direct_bytes: record.direct,
        subdirs: &record.subdirs,
        denied: record.denied,
    }
}

fn heap_of(root: &Path, records: &[Walked]) -> FolderSizeTree {
    let mut tree = FolderSizeTree::default();
    assert!(tree.add_root(root));
    for record in records {
        assert!(tree.merge_record(&record.path, listing(record)));
    }
    tree
}

struct Fixture {
    directory: TempDir,
    root: PathBuf,
}

fn fixture() -> Fixture {
    let directory = tempdir().expect("temp dir");
    let root = directory.path().join("walk-root");
    Fixture { directory, root }
}

impl Fixture {
    fn spill(&self, run_bytes: usize) -> WalkSpill {
        WalkSpill::create(SpillOptions {
            runs_path: self.directory.path().join("walk.spill"),
            root: self.root.clone(),
            run_bytes,
        })
    }

    fn graft_target(&self) -> GraftTarget {
        GraftTarget {
            segment: SegmentTarget {
                paths: paths_in(self.directory.path()),
                token: TOKEN,
            },
            root_name: self.root.to_string_lossy().into_owned(),
        }
    }

    fn spilled(&self, records: &[Walked], run_bytes: usize) -> WalkSpill {
        let mut spill = self.spill(run_bytes);
        for record in records {
            append(&mut spill, record).expect("append");
        }
        spill
    }

    fn grafted(&self, records: &[Walked], run_bytes: usize) -> MappedSegment {
        let built = self.spilled(records, run_bytes).build(self.graft_target());
        MappedSegment::open(&built.expect("build").paths).expect("opens")
    }

    fn relative(&self, path: &Path) -> Vec<String> {
        let below_root = path.strip_prefix(&self.root).expect("below root");
        below_root
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect()
    }

    fn id_of(&self, segment: &MappedSegment, path: &Path) -> u32 {
        let names = self.relative(path);
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        descend(segment, ROOT_ID, &names).expect("path is in the segment")
    }
}

fn append(spill: &mut WalkSpill, record: &Walked) -> Result<(), BuildError> {
    let files = PackedNames::from_names(&record.files);
    spill.append(SpillRecord {
        path: &record.path,
        listing: listing(record),
        file_names: files.as_bytes(),
    })
}

fn every_folder(records: &[Walked]) -> BTreeSet<PathBuf> {
    let listed = records.iter().flat_map(|record| {
        let children = record.subdirs.iter();
        children.map(move |subdir| record.path.join(subdir))
    });
    let visited = records.iter().map(|record| record.path.clone());
    visited.chain(listed).collect()
}

struct Comparison<'compared> {
    fixture: &'compared Fixture,
    segment: &'compared MappedSegment,
    heap: &'compared FolderSizeTree,
}

impl Comparison<'_> {
    fn assert_same_total(&self, path: &Path) {
        let row = self.segment.row(self.fixture.id_of(self.segment, path));
        let row = row.expect("row");
        let total = self.heap.total(path).expect("heap total");
        let denied_below = row.denied - u32::from(row.flags.denied);
        let heap_denied_below = total.lower_bound.map_or(0, |bound| bound.denied_count);
        assert_eq!(row.total, total.bytes, "{path:?}");
        assert_eq!(row.pending == 0, total.fully_indexed, "{path:?}");
        assert_eq!(row.flags.scanned, total.scanned, "{path:?}");
        assert_eq!(row.flags.denied, total.denied, "{path:?}");
        assert_eq!(row.flags.skipped, total.skipped, "{path:?}");
        assert_eq!(denied_below, heap_denied_below, "{path:?}");
    }

    fn assert_same_folders(&self, folders: &BTreeSet<PathBuf>) {
        assert_eq!(self.segment.node_count() as usize, self.heap.node_count());
        assert_eq!(self.segment.node_count() as usize, folders.len());
        for folder in folders {
            self.assert_same_total(folder);
        }
    }

    fn assert_depth_first_name_order(&self, folders: &BTreeSet<PathBuf>) {
        let mut expected: Vec<Vec<String>> = folders
            .iter()
            .map(|folder| self.fixture.relative(folder))
            .collect();
        expected.sort();
        for (id, relative) in expected.iter().enumerate() {
            let names = names_to(self.segment, id as u32);
            assert_eq!(&names[1..], relative.as_slice());
            let below = expected
                .iter()
                .filter(|other| other.starts_with(relative))
                .count();
            let subtree = self.segment.subtree(id as u32).expect("subtree");
            assert_eq!(subtree.len(), below, "{relative:?}");
        }
    }

    fn assert_same_file_names(&self, records: &[Walked]) {
        for record in records {
            let id = self.fixture.id_of(self.segment, &record.path);
            assert_eq!(file_names_of(self.segment, id), record.files);
        }
    }
}

fn assert_segment_answers_like_the_heap(fixture: &Fixture, run_bytes: usize) {
    let records = walked_tree(&fixture.root);
    let arrival_order: Vec<Walked> = records.iter().rev().cloned().collect();
    let segment = fixture.grafted(&arrival_order, run_bytes);
    let heap = heap_of(&fixture.root, &records);
    let folders = every_folder(&records);
    let comparison = Comparison {
        fixture,
        segment: &segment,
        heap: &heap,
    };
    comparison.assert_same_folders(&folders);
    comparison.assert_depth_first_name_order(&folders);
    comparison.assert_same_file_names(&records);
    let root_name = segment.name(ROOT_ID).expect("root name");
    assert_eq!(Path::new(root_name), fixture.root);
}

#[test]
fn a_segment_sorted_in_memory_answers_like_the_heap_tree() {
    let fixture = fixture();
    let spill = fixture.spilled(&walked_tree(&fixture.root), RUN_BYTES);
    assert_eq!(spill.run_file_count(), 0);
    drop(spill);
    assert_segment_answers_like_the_heap(&fixture, RUN_BYTES);
}

#[test]
fn a_segment_merged_from_many_sorted_runs_answers_like_the_heap_tree() {
    let fixture = fixture();
    let records = walked_tree(&fixture.root);
    let spill = fixture.spilled(&records, TINY_RUN_BYTES);
    assert_eq!(spill.record_count(), records.len() as u64);
    assert!(spill.run_file_count() > 3, "{}", spill.run_file_count());
    drop(spill);
    assert_eq!(files_in(fixture.directory.path()), Vec::<String>::new());

    assert_segment_answers_like_the_heap(&fixture, TINY_RUN_BYTES);
    assert_eq!(
        files_in(fixture.directory.path()),
        ["file-names-000001.bin", "folder-tree-000001.bin"]
    );
}

#[test]
fn folders_listed_but_never_walked_stay_unscanned_leaves() {
    let fixture = fixture();
    let segment = fixture.grafted(&walked_tree(&fixture.root), RUN_BYTES);
    let unvisited = fixture.id_of(&segment, &fixture.root.join("unvisited"));
    let row = segment.row(unvisited).expect("row");
    assert!(!row.flags.scanned);
    assert_eq!((row.pending, row.total), (1, 0));
    assert!(segment.children(unvisited).expect("children").is_empty());
    assert!(segment.row(ROOT_ID).expect("root").pending > 1);
}

#[test]
fn a_record_without_walked_parents_gets_unscanned_parents_like_the_heap_tree() {
    let fixture = fixture();
    let orphan = fixture.root.join("a").join("b").join("c");
    let records = [
        Walked {
            subdirs: strings(&["a"]),
            ..walked(fixture.root.clone(), 1)
        },
        walked(orphan.clone(), 40),
        walked(fixture.root.join("a").join("z"), 2),
    ];
    let segment = fixture.grafted(&records, RUN_BYTES);
    let heap = heap_of(&fixture.root, &records);
    let mut folders = every_folder(&records);
    folders.insert(fixture.root.join("a").join("b"));
    let comparison = Comparison {
        fixture: &fixture,
        segment: &segment,
        heap: &heap,
    };
    comparison.assert_same_folders(&folders);
    comparison.assert_depth_first_name_order(&folders);
    assert_eq!(segment.row(ROOT_ID).expect("root").total, 43);
}

fn relisted_root(fixture: &Fixture) -> [Walked; 3] {
    [
        Walked {
            subdirs: strings(&["old", "kept"]),
            files: strings(&["first.txt"]),
            ..walked(fixture.root.clone(), 1)
        },
        Walked {
            subdirs: strings(&["new", "kept", "new"]),
            files: strings(&["second.txt"]),
            ..walked(fixture.root.clone(), 2)
        },
        walked(fixture.root.join("kept"), 30),
    ]
}

fn assert_last_listing_wins(run_bytes: usize) {
    let fixture = fixture();
    let records = relisted_root(&fixture);
    let segment = fixture.grafted(&records, run_bytes);
    let heap = heap_of(&fixture.root, &records);
    let folders: BTreeSet<PathBuf> = ["", "kept", "new"]
        .iter()
        .map(|name| fixture.root.join(name))
        .collect();
    let comparison = Comparison {
        fixture: &fixture,
        segment: &segment,
        heap: &heap,
    };
    comparison.assert_same_folders(&folders);
    assert_eq!(child_names(&segment, ROOT_ID), ["kept", "new"]);
    assert_eq!(file_names_of(&segment, ROOT_ID), ["second.txt"]);
    assert_eq!(segment.row(ROOT_ID).expect("root").total, 32);
}

#[test]
fn the_last_record_for_a_folder_replaces_earlier_ones_in_one_run() {
    assert_last_listing_wins(RUN_BYTES);
}

#[test]
fn the_last_record_for_a_folder_replaces_earlier_ones_across_runs() {
    assert_last_listing_wins(ONE_RECORD_PER_RUN);
}

#[test]
fn a_walk_without_a_root_record_builds_nothing() {
    let fixture = fixture();
    let empty = fixture.spill(RUN_BYTES).build(fixture.graft_target());
    assert!(matches!(empty, Err(BuildError::NoRoot)), "{empty:?}");

    let child_only = [walked(fixture.root.join("child"), 1)];
    let rootless = fixture
        .spilled(&child_only, RUN_BYTES)
        .build(fixture.graft_target());
    assert!(matches!(rootless, Err(BuildError::NoRoot)), "{rootless:?}");
    assert!(files_in(fixture.directory.path()).is_empty());
}

#[test]
fn records_outside_the_walk_root_are_rejected() {
    let fixture = fixture();
    let mut spill = fixture.spill(RUN_BYTES);
    let elsewhere = walked(fixture.directory.path().join("elsewhere"), 1);
    let above_root = walked(fixture.root.join(".."), 1);
    for outside in [elsewhere, above_root] {
        let rejected = append(&mut spill, &outside);
        assert!(
            matches!(rejected, Err(BuildError::Malformed)),
            "{rejected:?}"
        );
    }
    assert_eq!(spill.record_count(), 0);
}

#[test]
fn unterminated_file_names_fail_the_build_and_leave_no_files() {
    let fixture = fixture();
    let mut spill = fixture.spill(ONE_RECORD_PER_RUN);
    let root = walked(fixture.root.clone(), 1);
    spill
        .append(SpillRecord {
            path: &root.path,
            listing: listing(&root),
            file_names: b"not-terminated",
        })
        .expect("append");
    let built = spill.build(fixture.graft_target());
    assert!(matches!(built, Err(BuildError::Malformed)), "{built:?}");
    assert!(files_in(fixture.directory.path()).is_empty());
}
