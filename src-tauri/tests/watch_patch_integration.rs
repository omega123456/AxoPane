//! Coverage for `watch::patch`'s revisioned delta / replace-view /
//! metadata-only patch construction (Phase 5's watch-driven directory-session
//! updates). Exercises `classify_patch` end-to-end through its public
//! surface, plus the `SessionPatch` accessor methods.

use file_explorer_lib::directory_session::model::{
    NavigationRevision, SessionBaseline, SessionId, ViewRevision, WatchRevision,
};
use file_explorer_lib::fs::DirectoryEntry;
use file_explorer_lib::watch::patch::{
    classify_patch, ClassifyPatchInput, MetadataDelta, RowDelta, SessionPatch,
};

fn baseline(n: u64) -> SessionBaseline {
    SessionBaseline {
        session_id: SessionId(n),
        navigation_revision: NavigationRevision(n),
        watch_revision: WatchRevision(n),
        view_revision: ViewRevision(n),
    }
}

fn entry(path: &str, name: &str) -> DirectoryEntry {
    DirectoryEntry {
        id: path.to_string(),
        name: name.to_string(),
        path: path.to_string(),
        is_dir: false,
        icon_data_url: None,
        size_bytes: Some(1),
        item_count: None,
        type_label: "TXT file".to_string(),
        modified_at: None,
        created_at: None,
        attributes: Vec::new(),
        is_hidden: false,
        is_system: false,
    }
}

#[test]
fn classify_patch_returns_none_when_nothing_touched() {
    let previous = vec![entry("/a", "a")];
    let result = classify_patch(ClassifyPatchInput {
        pane_id: "left",
        tab_id: "tab-1",
        path: "/dir",
        previous_baseline: baseline(0),
        next_baseline: baseline(1),
        previous_view: &previous,
        next_view: &previous,
        changed_paths: &[],
        removed_paths: &[],
    });
    assert!(result.is_none());
}

#[test]
fn classify_patch_produces_delta_for_single_insert() {
    let previous = vec![entry("/a", "a"), entry("/c", "c")];
    let next = vec![entry("/a", "a"), entry("/b", "b"), entry("/c", "c")];
    let patch = classify_patch(ClassifyPatchInput {
        pane_id: "left",
        tab_id: "tab-1",
        path: "/dir",
        previous_baseline: baseline(0),
        next_baseline: baseline(1),
        previous_view: &previous,
        next_view: &next,
        changed_paths: &["/b".to_string()],
        removed_paths: &[],
    })
    .expect("expected a patch");

    match &patch {
        SessionPatch::Delta {
            deltas, total_rows, ..
        } => {
            assert_eq!(*total_rows, 3);
            assert_eq!(
                deltas,
                &[RowDelta::Inserted {
                    row_index: 1,
                    entry: entry("/b", "b"),
                }]
            );
        }
        other => panic!("expected Delta, got {other:?}"),
    }

    assert_eq!(patch.pane_id(), "left");
    assert_eq!(patch.tab_id(), "tab-1");
    assert_eq!(patch.path(), "/dir");
    assert_eq!(patch.required_baseline(), baseline(0));
}

#[test]
fn classify_patch_produces_delta_for_single_removal() {
    let previous = vec![entry("/a", "a"), entry("/b", "b"), entry("/c", "c")];
    let next = vec![entry("/a", "a"), entry("/c", "c")];
    let patch = classify_patch(ClassifyPatchInput {
        pane_id: "right",
        tab_id: "tab-2",
        path: "/dir",
        previous_baseline: baseline(1),
        next_baseline: baseline(2),
        previous_view: &previous,
        next_view: &next,
        changed_paths: &[],
        removed_paths: &["/b".to_string()],
    })
    .expect("expected a patch");

    match &patch {
        SessionPatch::Delta {
            deltas, total_rows, ..
        } => {
            assert_eq!(*total_rows, 2);
            assert_eq!(
                deltas,
                &[RowDelta::Removed {
                    row_index: 1,
                    path: "/b".to_string(),
                }]
            );
        }
        other => panic!("expected Delta, got {other:?}"),
    }
}

#[test]
fn classify_patch_produces_delta_for_in_place_update() {
    let previous = vec![entry("/a", "a"), entry("/b", "b")];
    let mut updated_b = entry("/b", "b");
    updated_b.size_bytes = Some(99);
    let next = vec![entry("/a", "a"), updated_b.clone()];

    let patch = classify_patch(ClassifyPatchInput {
        pane_id: "left",
        tab_id: "tab-1",
        path: "/dir",
        previous_baseline: baseline(0),
        next_baseline: baseline(1),
        previous_view: &previous,
        next_view: &next,
        changed_paths: &["/b".to_string()],
        removed_paths: &[],
    })
    .expect("expected a patch");

    match &patch {
        SessionPatch::Delta { deltas, .. } => {
            assert_eq!(
                deltas,
                &[RowDelta::Updated {
                    row_index: 1,
                    entry: updated_b,
                }]
            );
        }
        other => panic!("expected Delta, got {other:?}"),
    }
}

#[test]
fn classify_patch_falls_back_to_replace_view_for_multiple_changes() {
    let previous = vec![entry("/a", "a"), entry("/b", "b")];
    let next = vec![entry("/a", "a"), entry("/b", "b"), entry("/c", "c")];
    let patch = classify_patch(ClassifyPatchInput {
        pane_id: "left",
        tab_id: "tab-1",
        path: "/dir",
        previous_baseline: baseline(0),
        next_baseline: baseline(1),
        previous_view: &previous,
        next_view: &next,
        changed_paths: &["/c".to_string(), "/b".to_string()],
        removed_paths: &[],
    })
    .expect("expected a patch");

    match &patch {
        SessionPatch::ReplaceView { total_rows, .. } => assert_eq!(*total_rows, 3),
        other => panic!("expected ReplaceView, got {other:?}"),
    }
    assert_eq!(patch.pane_id(), "left");
    assert_eq!(patch.tab_id(), "tab-1");
    assert_eq!(patch.path(), "/dir");
}

#[test]
fn classify_patch_falls_back_to_replace_view_when_removal_row_count_mismatches() {
    // Row count shrank by 2 while only 1 removal was reported: ambiguous.
    let previous = vec![entry("/a", "a"), entry("/b", "b"), entry("/c", "c")];
    let next = vec![entry("/a", "a")];
    let patch = classify_patch(ClassifyPatchInput {
        pane_id: "left",
        tab_id: "tab-1",
        path: "/dir",
        previous_baseline: baseline(0),
        next_baseline: baseline(1),
        previous_view: &previous,
        next_view: &next,
        changed_paths: &[],
        removed_paths: &["/b".to_string()],
    })
    .expect("expected a patch");

    assert!(matches!(patch, SessionPatch::ReplaceView { .. }));
}

#[test]
fn classify_patch_falls_back_to_replace_view_when_removed_row_still_present() {
    // Reported "removed" path is still present at the same index in
    // next_view (a different row was actually removed): something more
    // than a simple removal happened.
    let previous = vec![entry("/a", "a"), entry("/b", "b"), entry("/c", "c")];
    let next = vec![entry("/a", "a"), entry("/b", "b")];
    let patch = classify_patch(ClassifyPatchInput {
        pane_id: "left",
        tab_id: "tab-1",
        path: "/dir",
        previous_baseline: baseline(0),
        next_baseline: baseline(1),
        previous_view: &previous,
        next_view: &next,
        changed_paths: &[],
        removed_paths: &["/a".to_string()],
    })
    .expect("expected a patch");

    assert!(matches!(patch, SessionPatch::ReplaceView { .. }));
}

#[test]
fn classify_patch_falls_back_to_replace_view_when_insert_row_count_mismatches() {
    // Row count grew by 2 for a single reported changed path: ambiguous.
    let previous = vec![entry("/a", "a")];
    let next = vec![entry("/a", "a"), entry("/b", "b"), entry("/c", "c")];
    let patch = classify_patch(ClassifyPatchInput {
        pane_id: "left",
        tab_id: "tab-1",
        path: "/dir",
        previous_baseline: baseline(0),
        next_baseline: baseline(1),
        previous_view: &previous,
        next_view: &next,
        changed_paths: &["/b".to_string()],
        removed_paths: &[],
    })
    .expect("expected a patch");

    assert!(matches!(patch, SessionPatch::ReplaceView { .. }));
}

#[test]
fn classify_patch_falls_back_to_replace_view_when_insert_reorders_surrounding_rows() {
    // A single new path but the surrounding rows do not line up 1:1 in
    // order: treated as ambiguous rather than a pure insert.
    let previous = vec![entry("/a", "a"), entry("/z", "z")];
    let next = vec![entry("/z", "z"), entry("/m", "m"), entry("/a", "a")];
    let patch = classify_patch(ClassifyPatchInput {
        pane_id: "left",
        tab_id: "tab-1",
        path: "/dir",
        previous_baseline: baseline(0),
        next_baseline: baseline(1),
        previous_view: &previous,
        next_view: &next,
        changed_paths: &["/m".to_string()],
        removed_paths: &[],
    })
    .expect("expected a patch");

    assert!(matches!(patch, SessionPatch::ReplaceView { .. }));
}

#[test]
fn classify_patch_falls_back_to_replace_view_when_update_moves_row() {
    // Changed path exists in both views but at a different index: an
    // in-place update cannot be expressed unambiguously.
    let previous = vec![entry("/a", "a"), entry("/b", "b"), entry("/c", "c")];
    let next = vec![entry("/b", "b"), entry("/a", "a"), entry("/c", "c")];
    let patch = classify_patch(ClassifyPatchInput {
        pane_id: "left",
        tab_id: "tab-1",
        path: "/dir",
        previous_baseline: baseline(0),
        next_baseline: baseline(1),
        previous_view: &previous,
        next_view: &next,
        changed_paths: &["/b".to_string()],
        removed_paths: &[],
    })
    .expect("expected a patch");

    assert!(matches!(patch, SessionPatch::ReplaceView { .. }));
}

#[test]
fn classify_patch_falls_back_to_replace_view_when_changed_path_missing_from_next_view() {
    // Reported "changed" path is not even present in next_view (e.g. it was
    // both changed and then immediately removed by a later event) — cannot
    // resolve to a single delta.
    let previous = vec![entry("/a", "a"), entry("/b", "b")];
    let next = vec![entry("/a", "a")];
    let patch = classify_patch(ClassifyPatchInput {
        pane_id: "left",
        tab_id: "tab-1",
        path: "/dir",
        previous_baseline: baseline(0),
        next_baseline: baseline(1),
        previous_view: &previous,
        next_view: &next,
        changed_paths: &["/b".to_string()],
        removed_paths: &[],
    })
    .expect("expected a patch");

    assert!(matches!(patch, SessionPatch::ReplaceView { .. }));
}

#[test]
fn session_patch_accessors_cover_replace_view_and_metadata_only() {
    let replace = SessionPatch::ReplaceView {
        pane_id: "left".to_string(),
        tab_id: "tab-1".to_string(),
        path: "/dir".to_string(),
        previous_baseline: baseline(0),
        next_baseline: baseline(1),
        total_rows: 5,
    };
    assert_eq!(replace.pane_id(), "left");
    assert_eq!(replace.tab_id(), "tab-1");
    assert_eq!(replace.path(), "/dir");
    assert_eq!(replace.required_baseline(), baseline(0));

    let metadata_only = SessionPatch::MetadataOnly {
        pane_id: "right".to_string(),
        tab_id: "tab-2".to_string(),
        path: "/other".to_string(),
        baseline: baseline(3),
        updates: vec![MetadataDelta {
            path: "/other/file".to_string(),
            entry: entry("/other/file", "file"),
        }],
    };
    assert_eq!(metadata_only.pane_id(), "right");
    assert_eq!(metadata_only.tab_id(), "tab-2");
    assert_eq!(metadata_only.path(), "/other");
    assert_eq!(metadata_only.required_baseline(), baseline(3));
}

#[test]
fn session_patch_serializes_with_camel_case_mode_tag() {
    let patch = SessionPatch::Delta {
        pane_id: "left".to_string(),
        tab_id: "tab-1".to_string(),
        path: "/dir".to_string(),
        previous_baseline: baseline(0),
        next_baseline: baseline(1),
        total_rows: 1,
        deltas: vec![RowDelta::Inserted {
            row_index: 0,
            entry: entry("/a", "a"),
        }],
    };
    let json = serde_json::to_string(&patch).expect("serialize");
    assert!(json.contains("\"mode\":\"delta\""));
    assert!(json.contains("\"kind\":\"inserted\""));

    let round_tripped: SessionPatch = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(round_tripped, patch);
}
