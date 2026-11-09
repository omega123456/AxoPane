use serde::{Deserialize, Serialize};

use crate::directory_session::model::SessionBaseline;
use crate::fs::DirectoryEntry;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum RowDelta {
    Inserted {
        row_index: u64,
        entry: DirectoryEntry,
    },
    Removed {
        row_index: u64,
        path: String,
    },
    Updated {
        row_index: u64,
        entry: DirectoryEntry,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataDelta {
    pub path: String,
    pub entry: DirectoryEntry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "mode"
)]
pub enum SessionPatch {
    Delta {
        pane_id: String,
        tab_id: String,
        path: String,
        previous_baseline: SessionBaseline,
        next_baseline: SessionBaseline,
        total_rows: u64,
        deltas: Vec<RowDelta>,
    },
    ReplaceView {
        pane_id: String,
        tab_id: String,
        path: String,
        previous_baseline: SessionBaseline,
        next_baseline: SessionBaseline,
        total_rows: u64,
    },
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

pub struct ClassifyPatchInput<'a> {
    pub pane_id: &'a str,
    pub tab_id: &'a str,
    pub path: &'a str,
    pub previous_baseline: SessionBaseline,
    pub next_baseline: SessionBaseline,
    pub previous_view: &'a [DirectoryEntry],
    pub next_view: &'a [DirectoryEntry],
    pub changed_paths: &'a [String],
    pub removed_paths: &'a [String],
}

pub fn classify_patch(input: ClassifyPatchInput<'_>) -> Option<SessionPatch> {
    let ClassifyPatchInput {
        pane_id,
        tab_id,
        path,
        previous_baseline,
        next_baseline,
        next_view,
        changed_paths,
        removed_paths,
        ..
    } = input;

    let total_touched = changed_paths.len() + removed_paths.len();
    if total_touched == 0 {
        return None;
    }

    if total_touched == 1 {
        if let Some(delta) = single_row_delta(&input) {
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

fn single_row_delta(input: &ClassifyPatchInput<'_>) -> Option<RowDelta> {
    let ClassifyPatchInput {
        previous_view,
        next_view,
        changed_paths,
        removed_paths,
        ..
    } = *input;
    if !removed_paths.is_empty() {
        let removed_path = &removed_paths[0];
        if next_view.len() + 1 != previous_view.len() {
            return None;
        }
        let row_index = previous_view
            .iter()
            .position(|entry| &entry.path == removed_path)?;
        let still_present = next_view
            .get(row_index)
            .is_some_and(|entry| entry.path == *removed_path);
        if still_present {
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
            let stayed_in_place =
                previous_view.len() == next_view.len() && previous_index == next_index;
            if !stayed_in_place {
                return None;
            }
            Some(RowDelta::Updated {
                row_index: next_index as u64,
                entry: next_entry,
            })
        }
    }
}

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
