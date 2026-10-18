use std::collections::HashMap;
use std::path::{Path, PathBuf};

use file_explorer_lib::size::index::tree::{FolderSizeTree, FolderTotal, LowerBound};

/// A path under the relative root `idx`, built from components so it is
/// valid on every OS.
fn p(parts: &[&str]) -> PathBuf {
    std::iter::once("idx")
        .chain(parts.iter().copied())
        .collect()
}

fn tree() -> FolderSizeTree {
    let mut tree = FolderSizeTree::default();
    assert!(tree.add_root(&p(&[])));
    tree
}

fn bytes(tree: &FolderSizeTree, parts: &[&str]) -> u64 {
    tree.total(&p(parts)).expect("indexed").bytes
}

fn sorted(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort();
    paths
}

fn record(tree: &mut FolderSizeTree, parts: &[&str], direct: u64, subdirs: &[&str]) {
    assert!(
        tree.merge_record(&p(parts), direct, subdirs, false),
        "{parts:?}"
    );
}

/// idx(1) / a(10) / b(100) / c(1000), plus idx / s(5).
fn chain() -> FolderSizeTree {
    let mut tree = tree();
    record(&mut tree, &[], 1, &["a", "s"]);
    record(&mut tree, &["a"], 10, &["b"]);
    record(&mut tree, &["a", "b"], 100, &["c"]);
    record(&mut tree, &["a", "b", "c"], 1000, &[]);
    record(&mut tree, &["s"], 5, &[]);
    tree
}

#[test]
fn delta_propagates_to_every_ancestor_and_reports_only_subscribed_changes() {
    let mut tree = chain();
    assert_eq!(bytes(&tree, &[]), 1116);
    tree.drain_changed(Vec::<PathBuf>::new());

    let added = tree.apply_listing::<&str>(&p(&["a", "b", "c"]), 1500, &[], false);
    assert_eq!(added, Some(vec![]));
    assert_eq!(bytes(&tree, &["a", "b", "c"]), 1500);
    assert_eq!(bytes(&tree, &["a", "b"]), 1600);
    assert_eq!(bytes(&tree, &["a"]), 1610);
    assert_eq!(bytes(&tree, &[]), 1616);
    assert_eq!(bytes(&tree, &["s"]), 5);

    let subscribed = [p(&["a"]), p(&["s"]), p(&["missing"])];
    assert_eq!(tree.drain_changed(&subscribed), vec![p(&["a"])]);
    assert!(tree.drain_changed(&subscribed).is_empty());

    // A re-read that changes nothing is not a change.
    tree.apply_listing::<&str>(&p(&["a", "b", "c"]), 1500, &[], false);
    assert!(tree.drain_changed(&subscribed).is_empty());
}

#[test]
fn record_merge_in_any_order_equals_the_walk_totals() {
    // (path, direct bytes, subdirectories), as a walk would report them.
    let records: Vec<(Vec<&str>, u64, Vec<&str>)> = vec![
        (vec!["x", "y", "z"], 7, vec![]),
        (vec!["q"], 3, vec![]),
        (vec![], 11, vec!["q", "x"]),
        (vec!["x"], 13, vec!["w", "y"]),
        (vec!["x", "w"], 17, vec![]),
        (vec!["x", "y"], 19, vec!["z"]),
    ];
    let mut tree = tree();
    let mut expected: HashMap<PathBuf, u64> = HashMap::new();
    for (parts, direct, subdirs) in &records {
        record(&mut tree, parts, *direct, subdirs);
        for depth in 0..=parts.len() {
            *expected.entry(p(&parts[..depth])).or_default() += direct;
        }
    }
    for (path, total) in &expected {
        let got = tree.total(path).expect("indexed");
        assert_eq!(got.bytes, *total, "{path:?}");
        assert!(got.fully_indexed && got.scanned);
    }
    assert_eq!(tree.node_count(), records.len());
    assert!(tree.unscanned_tops().is_empty());
}

#[test]
fn listed_subdirectories_stay_pending_until_their_own_record_arrives() {
    let mut tree = tree();
    record(&mut tree, &[], 1, &["a", "b"]);
    let root = tree.total(&p(&[])).expect("root");
    assert!(
        root.scanned && !root.fully_indexed,
        "children not walked yet"
    );
    assert!(!tree.total(&p(&["a"])).expect("placeholder").scanned);
    assert_eq!(sorted(tree.unscanned_tops()), vec![p(&["a"]), p(&["b"])]);

    // A deep record creates its missing ancestors as pending too.
    record(&mut tree, &["a", "deep", "er"], 4, &[]);
    assert_eq!(bytes(&tree, &["a"]), 4);
    assert!(!tree.total(&p(&["a", "deep"])).expect("ancestor").scanned);
    assert_eq!(sorted(tree.unscanned_tops()), vec![p(&["a"]), p(&["b"])]);

    record(&mut tree, &["a"], 1, &["deep"]);
    record(&mut tree, &["a", "deep"], 0, &["er"]);
    record(&mut tree, &["b"], 0, &[]);
    assert!(tree.total(&p(&[])).expect("root").fully_indexed);
    assert_eq!(bytes(&tree, &[]), 6);
}

#[test]
fn listing_adds_new_children_as_pending_and_prunes_removed_ones() {
    let mut tree = chain();
    record(&mut tree, &["a"], 10, &["b", "old"]);
    record(&mut tree, &["a", "old"], 40, &["deep"]);
    record(&mut tree, &["a", "old", "deep"], 400, &[]);
    assert_eq!(bytes(&tree, &[]), 1556);

    let added = tree
        .apply_listing(&p(&["a"]), 12, &["b", "new", "new"], false)
        .expect("listed");
    assert_eq!(added, vec![p(&["a", "new"])]);
    assert!(tree.total(&p(&["a", "old"])).is_none());
    assert!(tree.total(&p(&["a", "old", "deep"])).is_none());
    assert_eq!(bytes(&tree, &["a"]), 1112);
    assert_eq!(bytes(&tree, &[]), 1118);

    let new = tree.total(&p(&["a", "new"])).expect("added");
    assert!(!new.scanned && !new.fully_indexed);
    assert!(!tree.total(&p(&[])).expect("root").fully_indexed);
    record(&mut tree, &["a", "new"], 2, &[]);
    assert!(tree.total(&p(&[])).expect("root").fully_indexed);

    assert_eq!(
        tree.apply_listing::<&str>(&p(&["nope"]), 0, &[], false),
        None
    );
}

#[test]
fn prune_subtracts_exactly_once() {
    let mut tree = chain();
    let before = tree.node_count();
    assert!(tree.prune(&p(&["a", "b"])));
    assert_eq!(bytes(&tree, &["a"]), 10);
    assert_eq!(bytes(&tree, &[]), 16);
    assert_eq!(tree.node_count(), before - 2);

    assert!(!tree.prune(&p(&["a", "b"])));
    assert_eq!(bytes(&tree, &[]), 16);

    // Freed slots are reused without disturbing totals.
    record(&mut tree, &["a", "n1", "n2"], 3, &[]);
    assert_eq!(bytes(&tree, &[]), 19);
    assert_eq!(tree.node_count(), before);
}

#[test]
fn rename_moves_the_subtree_and_its_totals() {
    let mut tree = chain();
    tree.drain_changed(Vec::<PathBuf>::new());
    assert!(tree.rename(&p(&["a", "b"]), &p(&["s", "moved"])));
    assert!(tree.total(&p(&["a", "b"])).is_none());
    assert_eq!(bytes(&tree, &["a"]), 10);
    assert_eq!(bytes(&tree, &["s"]), 1105);
    assert_eq!(bytes(&tree, &["s", "moved", "c"]), 1000);
    assert_eq!(bytes(&tree, &[]), 1116);
    assert_eq!(
        tree.drain_changed([p(&["s", "moved"]), p(&["a"])]),
        vec![p(&["s", "moved"]), p(&["a"])]
    );

    // Case-only rename keeps the node.
    assert!(tree.rename(&p(&["s", "moved"]), &p(&["s", "Moved"])));
    assert_eq!(bytes(&tree, &["s", "Moved"]), 1100);
    assert!(tree.total(&p(&["s", "moved"])).is_some(), "fallback lookup");

    // Renaming onto an existing folder replaces it.
    record(&mut tree, &["t"], 50, &[]);
    assert!(tree.rename(&p(&["t"]), &p(&["a"])));
    assert_eq!(bytes(&tree, &["a"]), 50);
    assert_eq!(bytes(&tree, &[]), 1156);
}

#[test]
fn rename_rejects_impossible_moves() {
    let mut tree = chain();
    let before = bytes(&tree, &[]);
    assert!(!tree.rename(&p(&["missing"]), &p(&["x"])));
    assert!(!tree.rename(&p(&["a"]), &p(&["missing", "x"])));
    assert!(!tree.rename(&p(&["a"]), &p(&["a", "b", "inside"])));
    assert!(!tree.rename(&p(&["a", "b"]), &p(&["a"])));
    assert!(!tree.rename(&p(&[]), &p(&["s", "root"])));
    assert!(!tree.rename(&p(&["a"]), Path::new("")));
    assert_eq!(bytes(&tree, &[]), before);
}

#[test]
fn skipped_roots_make_ancestors_lower_bounds_with_three_names_and_a_count() {
    let mut tree = tree();
    let names = ["Downloads", "Desktop", "Documents", "Containers", "Cloud"];
    record(&mut tree, &["home"], 8, &names);
    for name in names {
        assert!(tree.set_skipped(&p(&["home", name]), true));
    }
    let home = tree.total(&p(&["home"])).expect("home");
    assert_eq!(home.bytes, 8);
    assert!(home.fully_indexed, "skipped roots are not pending");
    assert_eq!(
        home.lower_bound,
        Some(LowerBound {
            skipped_names: vec!["Cloud".into(), "Containers".into(), "Desktop".into()],
            skipped_more: 2,
            denied_count: 0,
        })
    );
    let root = tree.total(&p(&[])).expect("root");
    assert_eq!(root.lower_bound.expect("bound").skipped_more, 2);

    let desktop = tree.total(&p(&["home", "Desktop"])).expect("desktop");
    assert!(desktop.skipped && desktop.lower_bound.is_none());

    // Visiting clears the skip; the root waits for its scan, then resolves.
    for name in ["Downloads", "Desktop", "Documents", "Containers"] {
        assert!(tree.set_skipped(&p(&["home", name]), false));
        record(&mut tree, &["home", name], 1, &[]);
    }
    let home = tree.total(&p(&["home"])).expect("home");
    assert_eq!(
        home.lower_bound.expect("one left").skipped_names,
        vec!["Cloud"]
    );
    tree.set_skipped(&p(&["home", "Cloud"]), false);
    assert!(!tree.total(&p(&["home"])).expect("home").fully_indexed);
    record(&mut tree, &["home", "Cloud"], 1, &[]);
    let home = tree.total(&p(&["home"])).expect("home");
    assert_eq!(home.lower_bound, None);
    assert_eq!(home.bytes, 13);

    assert!(!tree.set_skipped(&p(&["home", "never"]), false));
    assert!(!tree.set_skipped(Path::new("elsewhere"), true));
}

#[test]
fn nested_skipped_roots_are_named_once_each() {
    let mut tree = tree();
    record(&mut tree, &["lib"], 0, &["Group"]);
    tree.set_skipped(&p(&["lib", "Group"]), true);
    tree.set_skipped(&p(&["lib", "Group", "inner"]), true);
    let bound = tree
        .total(&p(&["lib"]))
        .and_then(|t| t.lower_bound)
        .expect("bound");
    assert_eq!(bound.skipped_names, vec!["Group", "inner"]);
    assert_eq!(bound.skipped_more, 0);
}

#[test]
fn denied_directories_propagate_and_clear_when_readable() {
    let mut tree = chain();
    record(&mut tree, &["a", "b"], 100, &["c", "locked"]);
    assert!(tree.merge_record::<&str>(&p(&["a", "b", "locked"]), 0, &[], true));
    let locked = tree.total(&p(&["a", "b", "locked"])).expect("locked");
    assert!(locked.denied && locked.scanned && locked.lower_bound.is_none());
    for parts in [&["a", "b"][..], &["a"][..], &[][..]] {
        let bound = tree.total(&p(parts)).and_then(|t| t.lower_bound);
        assert_eq!(
            bound,
            Some(LowerBound {
                denied_count: 1,
                ..LowerBound::default()
            }),
            "{parts:?}"
        );
    }

    tree.apply_listing::<&str>(&p(&["a", "b", "locked"]), 9, &[], false);
    assert_eq!(
        tree.total(&p(&["a"])),
        Some(FolderTotal {
            bytes: 1119,
            fully_indexed: true,
            scanned: true,
            denied: false,
            skipped: false,
            lower_bound: None,
        })
    );
}

#[test]
fn lookup_is_exact_first_then_case_insensitive() {
    let mut tree = tree();
    record(&mut tree, &[], 0, &["Foo", "foo", "Bar"]);
    record(&mut tree, &["Foo"], 1, &[]);
    record(&mut tree, &["foo"], 2, &[]);
    record(&mut tree, &["Bar", "Ünï"], 3, &[]);

    assert_eq!(bytes(&tree, &["Foo"]), 1);
    assert_eq!(bytes(&tree, &["foo"]), 2);
    assert!(matches!(bytes(&tree, &["FOO"]), 1 | 2));
    assert_eq!(bytes(&tree, &["bar", "üNÏ"]), 3);
    assert_eq!(
        tree.total(&["IDX", "BAR"].iter().collect::<PathBuf>())
            .map(|t| t.bytes),
        Some(3)
    );
    assert!(tree.total(&p(&["baz"])).is_none());
    assert!(tree.total(Path::new("other")).is_none());
    assert!(tree.total(&p(&["..", "Foo"])).is_none());
    assert!(!tree.merge_record::<&str>(&p(&["..", "x"]), 1, &[], false));
    assert!(!tree.merge_record::<&str>(Path::new("other"), 1, &[], false));

    // A case-variant record still lands under the existing root.
    assert!(tree.merge_record::<&str>(
        &["IDX", "Bar"].iter().collect::<PathBuf>(),
        4,
        &["Ünï"],
        false
    ));
    assert_eq!(bytes(&tree, &["Bar"]), 7);
}

#[test]
fn multiple_roots_including_absolute_os_paths() {
    let absolute = std::env::temp_dir().join("axopane-index-root");
    let mut tree = FolderSizeTree::default();
    assert!(tree.add_root(&p(&[])));
    assert!(tree.add_root(&absolute));
    assert!(!tree.add_root(&absolute));
    assert!(tree.merge_record::<&str>(&absolute.join("child"), 6, &[], false));
    record(&mut tree, &["child"], 2, &[]);
    assert_eq!(tree.total(&absolute).map(|t| t.bytes), Some(6));
    assert_eq!(bytes(&tree, &[]), 2);

    let upper = PathBuf::from(absolute.to_string_lossy().to_uppercase());
    assert_eq!(tree.total(&upper.join("CHILD")).map(|t| t.bytes), Some(6));

    assert!(tree.prune(&absolute));
    assert!(tree.total(&absolute.join("child")).is_none());
    assert_eq!(tree.node_count(), 2);
}

#[test]
fn a_filesystem_root_is_an_indexed_folder_with_its_own_total() {
    let root = Path::new(std::path::MAIN_SEPARATOR_STR);
    let mut tree = FolderSizeTree::default();
    assert!(tree.add_root(root));
    assert!(tree.merge_record(root, 1, &["Users", "usr"], false));
    assert!(tree.merge_record::<&str>(&root.join("Users"), 20, &[], false));
    assert_eq!(tree.total(root).map(|t| t.fully_indexed), Some(false));
    assert!(tree.merge_record::<&str>(&root.join("usr"), 300, &[], false));
    let total = tree.total(root).expect("root");
    assert_eq!((total.bytes, total.fully_indexed), (321, true));
    assert_eq!(tree.total(&root.join("usr")).map(|t| t.bytes), Some(300));
    assert!(tree.prune(&root.join("usr")));
    assert_eq!(tree.total(root).map(|t| t.bytes), Some(21));
}

#[test]
fn prune_unscanned_drops_folders_a_finished_walk_never_reached() {
    let mut tree = chain();
    record(&mut tree, &["a"], 10, &["b", "offdevice", "Desktop"]);
    tree.set_skipped(&p(&["a", "Desktop"]), true);
    record(&mut tree, &["a", "b"], 100, &["c", "vanished"]);
    assert!(!tree.total(&p(&[])).expect("root").fully_indexed);

    assert!(tree.prune_unscanned(&p(&["a"])));
    assert!(tree.total(&p(&["a", "offdevice"])).is_none());
    assert!(tree.total(&p(&["a", "b", "vanished"])).is_none());
    assert!(tree.total(&p(&["a", "Desktop"])).expect("kept").skipped);
    assert!(tree.total(&p(&[])).expect("root").fully_indexed);
    assert_eq!(bytes(&tree, &[]), 1116);

    // Index roots are never pruned, even when unscanned.
    let mut fresh = FolderSizeTree::default();
    fresh.add_root(&p(&[]));
    assert!(fresh.prune_unscanned(&p(&[])));
    assert_eq!(fresh.unscanned_tops(), vec![p(&[])]);
    assert!(!fresh.prune_unscanned(&p(&["gone"])));
}

#[test]
fn indexed_subtrees_lists_the_largest_complete_subtrees() {
    let mut tree = chain();
    record(&mut tree, &["a"], 10, &["b", "b2", "p"]);
    record(&mut tree, &["a", "b2"], 0, &[]);
    record(&mut tree, &["a", "p", "done"], 0, &[]);
    let mut got = tree.indexed_subtrees_under(&p(&[]));
    got.sort();
    assert_eq!(
        got,
        vec![
            p(&["a", "b"]),
            p(&["a", "b2"]),
            p(&["a", "p", "done"]),
            p(&["s"])
        ]
    );
    assert!(tree.indexed_subtrees_under(&p(&["nope"])).is_empty());
}

#[test]
fn dirty_tracks_changes_since_the_last_snapshot() {
    let mut tree = chain();
    assert!(tree.is_dirty());
    tree.mark_clean();
    tree.apply_listing::<&str>(&p(&["s"]), 5, &[], false);
    assert!(!tree.is_dirty(), "no-op re-read");
    tree.prune(&p(&["s"]));
    assert!(tree.is_dirty());
}
