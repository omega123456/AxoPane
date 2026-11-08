use std::collections::HashMap;
use std::path::Path;

use super::{DirEntryPatch, DirPatch, ErrorEmitter, PatchEmitter, WatchTarget, WatchedTab};
use crate::error_text::StringifyError;
use crate::fs::{self, DirectoryEntry, ListDirOptions, ListDirResponse, SortDirection, SortKey};

pub(super) fn remove_path(
    next: &mut HashMap<String, DirectoryEntry>,
    removed: &mut Vec<String>,
    path: &Path,
) {
    let display_path = fs::display_path_from_path(path);
    next.remove(&display_path);
    if !removed.contains(&display_path) {
        removed.push(display_path);
    }
}

pub(super) fn patch_changed_path(
    target: &WatchTarget,
    next: &mut HashMap<String, DirectoryEntry>,
    changed: &mut Vec<DirEntryPatch>,
    removed: &mut Vec<String>,
    path: &Path,
) -> Result<(), String> {
    let display_path = fs::display_path_from_path(path);

    if !path.exists() {
        remove_path(next, removed, path);
        return Ok(());
    }

    let entry = fs::directory_entry_from_path_without_item_count(path).stringify_err()?;
    if !matches_target_filter(&entry, target) {
        remove_path(next, removed, path);
        return Ok(());
    }

    next.insert(entry.path.clone(), entry.clone());
    changed.push(DirEntryPatch {
        path: entry.path.clone(),
        entry: Some(entry),
    });
    removed.retain(|removed_path| *removed_path != display_path);
    Ok(())
}

pub(super) fn matches_target_filter(entry: &DirectoryEntry, target: &WatchTarget) -> bool {
    if !target.show_hidden && (entry.is_hidden || entry.is_system) {
        return false;
    }

    target.filter.is_empty()
        || entry
            .name
            .to_lowercase()
            .contains(&target.filter.to_lowercase())
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

/// Builds a snapshot map keyed by entry path from an already-fetched entries
/// list, avoiding a second directory enumeration when the caller (e.g. the
/// frontend's `list_dir` response) has already produced the listing.
///
/// `icon_data_url` is normalized to `None` regardless of what the caller
/// supplies: `list_dir` (and therefore every subsequent re-diff snapshot)
/// always reports entries with `icon_data_url: None` — icons are resolved
/// separately and asynchronously by the frontend and are never part of the
/// filesystem listing itself. The frontend, however, seeds this baseline from
/// entries that may already carry a resolved `iconDataUrl` (from its own
/// icon cache). Without this normalization those icon-bearing entries would
/// never compare equal to a subsequent `list_dir`-sourced snapshot, so the
/// very first diff after arming would spuriously report every such entry as
/// "changed" even though nothing changed on disk. Normalizing here keeps the
/// guarantee that the first diff after seeding is empty.
pub(super) fn snapshot_from_entries_for_watch(
    target: &WatchTarget,
    entries: Vec<DirectoryEntry>,
) -> HashMap<String, DirectoryEntry> {
    snapshot_from_entries(entries, !baseline_include_item_counts(target))
}

fn baseline_sort_key(target: &WatchTarget) -> SortKey {
    if target.sort_key == SortKey::Items {
        SortKey::Name
    } else {
        target.sort_key
    }
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

/// Resnapshots `watched`, stores the new baseline and emits the difference.
pub(super) fn resnapshot_tab(
    watched: &mut WatchedTab,
    reason: &str,
    patch_emitter: &PatchEmitter,
    error_emitter: &ErrorEmitter,
) {
    let next_snapshot = match snapshot_for_watch_baseline(&watched.target) {
        Ok(next_snapshot) => next_snapshot,
        Err(error) => {
            error_emitter(watched.target.path.clone(), error);
            return;
        }
    };
    let patch = diff_entries(
        &watched.target.tab_id,
        &watched.target.path,
        reason,
        &watched.snapshot,
        &next_snapshot,
    );
    watched.snapshot = next_snapshot;
    if patch.has_changes() {
        patch_emitter(patch);
    }
}

pub fn diff_entries(
    tab_id: &str,
    path: &str,
    reason: &str,
    previous: &HashMap<String, DirectoryEntry>,
    next: &HashMap<String, DirectoryEntry>,
) -> DirPatch {
    let mut changed = Vec::new();
    let mut removed = Vec::new();

    for (entry_path, entry) in next {
        if previous.get(entry_path) != Some(entry) {
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
        tab_id: tab_id.to_string(),
        path: path.to_string(),
        reason: reason.to_string(),
        changed,
        removed,
    }
}
