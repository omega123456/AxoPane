use std::collections::HashMap;
use std::path::Path;

use super::{DirEntryPatch, DirPatch, WatchEmitters, WatchTarget, WatchedTab};
use crate::error_text::StringifyError;
use crate::fs::{self, DirectoryEntry, ListDirOptions, ListDirResponse, SortDirection, SortKey};

pub(super) struct SnapshotDraft {
    pub(super) next: HashMap<String, DirectoryEntry>,
    pub(super) changed: Vec<DirEntryPatch>,
    pub(super) removed: Vec<String>,
}

impl SnapshotDraft {
    pub(super) fn from_previous(previous: &HashMap<String, DirectoryEntry>) -> Self {
        Self {
            next: previous.clone(),
            changed: Vec::new(),
            removed: Vec::new(),
        }
    }

    pub(super) fn remove(&mut self, path: &Path) {
        let display_path = fs::display_path_from_path(path);
        self.next.remove(&display_path);
        if !self.removed.contains(&display_path) {
            self.removed.push(display_path);
        }
    }

    pub(super) fn apply_change(&mut self, target: &WatchTarget, path: &Path) -> Result<(), String> {
        let display_path = fs::display_path_from_path(path);

        if !path.exists() {
            self.remove(path);
            return Ok(());
        }

        let entry = fs::directory_entry_from_path_without_item_count(path).stringify_err()?;
        if !matches_target_filter(&entry, target) {
            self.remove(path);
            return Ok(());
        }

        self.next.insert(entry.path.clone(), entry.clone());
        self.changed.push(DirEntryPatch {
            path: entry.path.clone(),
            entry: Some(entry),
        });
        self.removed
            .retain(|removed_path| *removed_path != display_path);
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub struct PatchLabel<'label> {
    pub tab_id: &'label str,
    pub path: &'label str,
    pub reason: &'label str,
}

impl<'label> PatchLabel<'label> {
    pub(super) fn for_target(target: &'label WatchTarget, reason: &'label str) -> Self {
        Self {
            tab_id: &target.tab_id,
            path: &target.path,
            reason,
        }
    }
}

pub(super) fn matches_target_filter(entry: &DirectoryEntry, target: &WatchTarget) -> bool {
    entry.is_visible_with(target.show_hidden)
        && entry.name_matches_filter(&target.filter.to_lowercase())
}

pub fn snapshot_for_target(
    target: &WatchTarget,
) -> Result<HashMap<String, DirectoryEntry>, String> {
    let response = list_dir_for_snapshot(&ListDirOptions {
        path: target.path.clone(),
        sort_key: target.sort_key,
        sort_direction: target.sort_direction,
        filter: target.filter.clone(),
        show_hidden: target.show_hidden,
        include_item_counts: target.include_item_counts,
    })?;

    Ok(snapshot_from_entries(response.entries, false))
}

pub(super) fn snapshot_for_watch_baseline(
    target: &WatchTarget,
) -> Result<HashMap<String, DirectoryEntry>, String> {
    let response = list_dir_for_snapshot(&ListDirOptions {
        path: target.path.clone(),
        sort_key: baseline_sort_key(target),
        sort_direction: SortDirection::Asc,
        filter: target.filter.clone(),
        show_hidden: target.show_hidden,
        include_item_counts: baseline_include_item_counts(target),
    })?;

    Ok(snapshot_from_entries_for_watch(target, response.entries))
}

pub(super) fn snapshot_from_entries_for_watch(
    target: &WatchTarget,
    entries: Vec<DirectoryEntry>,
) -> HashMap<String, DirectoryEntry> {
    snapshot_from_entries(entries, !baseline_include_item_counts(target))
}

fn baseline_sort_key(target: &WatchTarget) -> SortKey {
    if target.sort_key == SortKey::Items {
        return SortKey::Name;
    }
    target.sort_key
}

fn baseline_include_item_counts(target: &WatchTarget) -> bool {
    target.include_item_counts && target.sort_key != SortKey::Items
}

fn snapshot_from_entries(
    entries: Vec<DirectoryEntry>,
    strip_item_counts: bool,
) -> HashMap<String, DirectoryEntry> {
    entries
        .into_iter()
        .map(|mut entry| {
            entry.icon_data_url = None;
            if strip_item_counts {
                entry.item_count = None;
            }
            (entry.path.clone(), entry)
        })
        .collect()
}

fn list_dir_for_snapshot(options: &ListDirOptions) -> Result<ListDirResponse, String> {
    fs::list_dir(options).stringify_err()
}

pub(super) fn resnapshot_tab(watched: &mut WatchedTab, reason: &str, emitters: &WatchEmitters) {
    let next_snapshot = match snapshot_for_watch_baseline(&watched.target) {
        Ok(next_snapshot) => next_snapshot,
        Err(error) => {
            (emitters.error)(watched.target.path.clone(), error);
            return;
        }
    };
    let label = PatchLabel::for_target(&watched.target, reason);
    let patch = diff_entries(label, &watched.snapshot, &next_snapshot);
    watched.snapshot = next_snapshot;
    if patch.has_changes() {
        (emitters.patch)(patch);
    }
}

pub fn diff_entries(
    label: PatchLabel<'_>,
    previous: &HashMap<String, DirectoryEntry>,
    next: &HashMap<String, DirectoryEntry>,
) -> DirPatch {
    let mut changed = Vec::new();
    let mut removed = Vec::new();

    for (entry_path, entry) in next {
        let differs = previous.get(entry_path) != Some(entry);
        if differs {
            changed.push(DirEntryPatch {
                path: entry_path.clone(),
                entry: Some(entry.clone()),
            });
        }
    }

    for entry_path in previous.keys() {
        if !next.contains_key(entry_path) {
            removed.push(entry_path.clone());
        }
    }

    DirPatch {
        tab_id: label.tab_id.to_string(),
        path: label.path.to_string(),
        reason: label.reason.to_string(),
        changed,
        removed,
    }
}
