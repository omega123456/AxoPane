//! Patch variants for Phase 5's watch-driven directory-session updates
//! (Functional Requirement 7 / "Contract and Consistency Rules": patch modes
//! are `delta`, `replace-view`, and `metadata-only`, each carrying the
//! revisions required to reject stale or path-mismatched work).
//!
//! These are distinct from the legacy [`crate::watch::DirPatch`] (the v1
//! tab-scoped `dir://patch` event, which stays byte-for-byte compatible for
//! the un-migrated fraction of the app) — [`SessionPatch`] is the v2
//! session/watch/view-revisioned equivalent that `directory_session`
//! consumers (Phase 5's frontend collection, and later Phase 7's tree) can
//! validate against their own current baseline before applying.

use serde::{Deserialize, Serialize};

use crate::directory_session::model::SessionBaseline;
use crate::fs::DirectoryEntry;

/// One row-level insert/remove/update with a Rust-computed final position.
/// Only emitted when the position is unambiguous (a single child added,
/// removed, or renamed within an already-sorted view) — anything broader or
/// order-ambiguous is a [`SessionPatch::ReplaceView`] instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum RowDelta {
    /// A new row appears at `row_index` (the total row count grows by one).
    Inserted {
        row_index: u64,
        entry: DirectoryEntry,
    },
    /// The row previously at `row_index` is gone (the total row count
    /// shrinks by one; every later row's index implicitly shifts down by
    /// one).
    Removed { row_index: u64, path: String },
    /// The row at `row_index` is replaced in place (same position, new
    /// content) — e.g. a rename that does not change sort order, or a
    /// metadata change under a sort key that does not depend on the changed
    /// field.
    Updated {
        row_index: u64,
        entry: DirectoryEntry,
    },
}

/// A `metadata-only` patch: fields on an already-loaded row change (size,
/// item count, icon-relevant attributes) without any row ever moving pages
/// or position. Distinct from [`RowDelta::Updated`] so a consumer can apply
/// it as a narrow field merge rather than a full row replace, and so Phase 8
/// (item counts) can reuse this path without implying an ordering change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataDelta {
    pub path: String,
    pub entry: DirectoryEntry,
}

/// The complete v2 patch envelope. Every variant carries the
/// [`SessionBaseline`] the patch was computed against (pre-patch, i.e. the
/// baseline the consumer must currently hold to accept it) plus the new
/// `view_revision`/`total_rows` the patch establishes, so a consumer can
/// reject a patch computed against a baseline it no longer holds (stale
/// session/navigation/watch/view) or a path it is not currently displaying.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "mode"
)]
pub enum SessionPatch {
    /// Small, order-unambiguous change(s): apply `deltas` in order to a
    /// currently-loaded, covered page without reinstalling it.
    Delta {
        pane_id: String,
        tab_id: String,
        path: String,
        /// Baseline the patch was computed against (what the consumer must
        /// currently hold to accept this delta).
        previous_baseline: SessionBaseline,
        /// Baseline the patch establishes once applied.
        next_baseline: SessionBaseline,
        total_rows: u64,
        deltas: Vec<RowDelta>,
    },
    /// Broad or order-ambiguous change: every cached page must be
    /// invalidated and the visible range refetched from `next_baseline`.
    ReplaceView {
        pane_id: String,
        tab_id: String,
        path: String,
        previous_baseline: SessionBaseline,
        next_baseline: SessionBaseline,
        total_rows: u64,
    },
    /// Field-only change(s) to already-loaded rows; no row moves position or
    /// page membership, and no baseline advance is required since ordering
    /// did not change (the watch/view revision is carried unchanged so a
    /// consumer can still validate applicability).
    MetadataOnly {
        pane_id: String,
        tab_id: String,
        path: String,
        baseline: SessionBaseline,
        updates: Vec<MetadataDelta>,
    },
}

impl SessionPatch {
    pub fn pane_id(&self) -> &str {
        match self {
            SessionPatch::Delta { pane_id, .. }
            | SessionPatch::ReplaceView { pane_id, .. }
            | SessionPatch::MetadataOnly { pane_id, .. } => pane_id,
        }
    }

    pub fn tab_id(&self) -> &str {
        match self {
            SessionPatch::Delta { tab_id, .. }
            | SessionPatch::ReplaceView { tab_id, .. }
            | SessionPatch::MetadataOnly { tab_id, .. } => tab_id,
        }
    }

    pub fn path(&self) -> &str {
        match self {
            SessionPatch::Delta { path, .. }
            | SessionPatch::ReplaceView { path, .. }
            | SessionPatch::MetadataOnly { path, .. } => path,
        }
    }

    /// The baseline a consumer must currently hold for this patch to be
    /// applicable at all (before-state for `Delta`/`ReplaceView`, the
    /// unchanged current baseline for `MetadataOnly`).
    pub fn required_baseline(&self) -> SessionBaseline {
        match self {
            SessionPatch::Delta {
                previous_baseline, ..
            }
            | SessionPatch::ReplaceView {
                previous_baseline, ..
            } => *previous_baseline,
            SessionPatch::MetadataOnly { baseline, .. } => *baseline,
        }
    }
}

/// Input bundle for [`classify_patch`], grouped into one struct so the
/// function stays within the workspace's `too_many_arguments` lint budget.
pub struct ClassifyPatchInput<'a> {
    pub pane_id: &'a str,
    pub tab_id: &'a str,
    pub path: &'a str,
    pub previous_baseline: SessionBaseline,
    pub next_baseline: SessionBaseline,
    /// Full ordered row list before the mutation (already filtered/sorted by
    /// [`crate::directory_session::view::SessionView`]).
    pub previous_view: &'a [DirectoryEntry],
    /// Full ordered row list after the mutation.
    pub next_view: &'a [DirectoryEntry],
    /// Display paths the watch compaction reported as changed/created.
    pub changed_paths: &'a [String],
    /// Display paths the watch compaction reported as removed.
    pub removed_paths: &'a [String],
}

/// Classifies a compacted set of changed/removed child entries (already
/// resolved against the *previous* sorted view) into the smallest applicable
/// [`SessionPatch`] variant.
///
/// - Zero changes -> `None` (nothing to emit).
/// - Exactly one child added, removed, or updated *in place* (its sort key
///   does not move it) -> `Delta` with one [`RowDelta`].
/// - Anything else (multiple children, or a single change whose sort
///   position is ambiguous/moved) -> `ReplaceView`.
pub fn classify_patch(input: ClassifyPatchInput<'_>) -> Option<SessionPatch> {
    let ClassifyPatchInput {
        pane_id,
        tab_id,
        path,
        previous_baseline,
        next_baseline,
        previous_view,
        next_view,
        changed_paths,
        removed_paths,
    } = input;

    let total_touched = changed_paths.len() + removed_paths.len();
    if total_touched == 0 {
        return None;
    }

    if total_touched == 1 {
        if let Some(delta) =
            single_row_delta(previous_view, next_view, changed_paths, removed_paths)
        {
            return Some(SessionPatch::Delta {
                pane_id: pane_id.to_string(),
                tab_id: tab_id.to_string(),
                path: path.to_string(),
                previous_baseline,
                next_baseline,
                total_rows: next_view.len() as u64,
                deltas: vec![delta],
            });
        }
    }

    Some(SessionPatch::ReplaceView {
        pane_id: pane_id.to_string(),
        tab_id: tab_id.to_string(),
        path: path.to_string(),
        previous_baseline,
        next_baseline,
        total_rows: next_view.len() as u64,
    })
}

/// Attempts to express exactly one touched path as a single [`RowDelta`].
/// Returns `None` (forcing the caller to fall back to `ReplaceView`) if the
/// row count changed by anything other than exactly 0 or 1, or if the
/// touched row's position cannot be unambiguously determined from a direct
/// index comparison — e.g. a rename that also happens to change sort
/// position among other untouched rows shifting is intentionally treated as
/// ambiguous here rather than risk an incorrect computed index.
fn single_row_delta(
    previous_view: &[DirectoryEntry],
    next_view: &[DirectoryEntry],
    changed_paths: &[String],
    removed_paths: &[String],
) -> Option<RowDelta> {
    if !removed_paths.is_empty() {
        // Exactly one removed child and total_touched == 1 (checked by the
        // caller) means no changed entries at all.
        let removed_path = &removed_paths[0];
        if next_view.len() + 1 != previous_view.len() {
            return None;
        }
        let row_index = previous_view
            .iter()
            .position(|entry| &entry.path == removed_path)?;
        // The row must actually be gone from `next_view` at that same index
        // (or the list is now shorter than that index), otherwise something
        // more than a simple removal happened.
        if row_index < next_view.len() && next_view[row_index].path == *removed_path {
            return None;
        }
        return Some(RowDelta::Removed {
            row_index: row_index as u64,
            path: removed_path.clone(),
        });
    }

    let changed_path = &changed_paths[0];
    let previous_index = previous_view
        .iter()
        .position(|entry| &entry.path == changed_path);
    let next_index = next_view
        .iter()
        .position(|entry| &entry.path == changed_path)?;
    let next_entry = next_view[next_index].clone();

    match previous_index {
        None => {
            // A brand-new row. Row count must have grown by exactly one and
            // the new row must be at the reported index with everything
            // else in the same relative order.
            if next_view.len() != previous_view.len() + 1 {
                return None;
            }
            if !surrounding_rows_match(previous_view, next_view, next_index) {
                return None;
            }
            Some(RowDelta::Inserted {
                row_index: next_index as u64,
                entry: next_entry,
            })
        }
        Some(previous_index) => {
            // An in-place update only if the row count is unchanged and the
            // touched row stayed at the exact same index (no reordering).
            if previous_view.len() != next_view.len() || previous_index != next_index {
                return None;
            }
            Some(RowDelta::Updated {
                row_index: next_index as u64,
                entry: next_entry,
            })
        }
    }
}

/// Confirms every row other than the freshly-inserted one at `inserted_index`
/// still lines up between `previous_view` and `next_view` (used to rule out
/// "insert plus reorder" being mistaken for a pure insert).
fn surrounding_rows_match(
    previous_view: &[DirectoryEntry],
    next_view: &[DirectoryEntry],
    inserted_index: usize,
) -> bool {
    let mut previous_iter = previous_view.iter();
    for (index, next_entry) in next_view.iter().enumerate() {
        if index == inserted_index {
            continue;
        }
        match previous_iter.next() {
            Some(previous_entry) if previous_entry.path == next_entry.path => {}
            _ => return false,
        }
    }
    previous_iter.next().is_none()
}
