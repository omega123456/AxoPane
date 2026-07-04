use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::event::{EventKind, ModifyKind, RenameMode};
use notify::RecursiveMode;
use notify_debouncer_full::{
    new_debouncer, DebounceEventResult, DebouncedEvent, Debouncer, FileIdMap,
};
use serde::{Deserialize, Serialize};

use crate::fs::{self, DirectoryEntry, ListDirOptions, ListDirResponse, SortDirection, SortKey};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DirEntryPatch {
    pub path: String,
    pub entry: Option<DirectoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DirPatch {
    pub tab_id: String,
    pub path: String,
    pub reason: String,
    pub changed: Vec<DirEntryPatch>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WatchTarget {
    pub tab_id: String,
    pub path: String,
    pub sort_key: SortKey,
    pub sort_direction: SortDirection,
    pub filter: String,
    pub show_hidden: bool,
    /// Whether the watcher should ask `list_dir` to count child-directory contents
    /// (mirrors the Items column visibility). When `false`, snapshots/resnapshots
    /// skip opening subdirectories purely to count their entries.
    pub include_item_counts: bool,
}

#[derive(Default)]
pub struct WatchService {
    inner: Mutex<Option<WatchRuntime>>,
}

pub struct WatchRuntime {
    pub debouncer: Debouncer<notify::RecommendedWatcher, FileIdMap>,
    tabs: Arc<Mutex<HashMap<String, WatchedTab>>>,
    pub watch_counts: HashMap<PathBuf, usize>,
}

#[derive(Clone)]
struct WatchedTab {
    target: WatchTarget,
    snapshot: HashMap<String, DirectoryEntry>,
}

impl WatchService {
    /// Arms (or disarms) the watch for a tab.
    ///
    /// When `entries` is `Some`, the baseline snapshot is seeded directly from
    /// those entries (the listing the frontend already fetched) instead of
    /// re-reading the directory, eliminating a redundant enumeration. The
    /// entries supplied must be the post-filter/sort listing for `target.path`
    /// so the first diff against subsequent filesystem events is empty.
    pub fn set_tab_watch(
        &self,
        target: Option<WatchTarget>,
        entries: Option<Vec<DirectoryEntry>>,
        emit_patch: Arc<dyn Fn(DirPatch) + Send + Sync>,
        emit_error: Arc<dyn Fn(String, String) + Send + Sync>,
    ) -> Result<(), String> {
        let mut guard = self.inner.lock().expect("watch service lock");

        if let Some(target) = target {
            if guard.is_none() {
                let runtime = create_runtime(Arc::clone(&emit_patch), Arc::clone(&emit_error))?;
                *guard = Some(runtime);
            }

            let runtime = guard.as_mut().expect("watch runtime");
            let path = PathBuf::from(&target.path);
            let snapshot = match entries {
                Some(entries) => snapshot_from_entries(entries),
                None => snapshot_for_target(&target)?,
            };
            let pane_name = pane_scope(&target.tab_id).to_string();

            let (same_tab_previous, stale_tabs) = {
                let tabs = runtime.tabs.lock().expect("watch tabs lock");
                let same_tab_previous = tabs.get(&target.tab_id).cloned();
                let mut stale_tabs = Vec::new();
                for (tab_id, watched) in tabs.iter() {
                    if pane_scope(tab_id) == pane_name && *tab_id != target.tab_id {
                        stale_tabs.push((tab_id.clone(), watched.clone()));
                    }
                }
                (same_tab_previous, stale_tabs)
            };

            let should_add_watch = match same_tab_previous.as_ref() {
                Some(previous) => previous.target.path != target.path,
                None => true,
            };
            if should_add_watch {
                add_watch(runtime, &path)?;
            }

            if let Some(previous) = same_tab_previous {
                if previous.target.path != target.path {
                    remove_watch(runtime, Path::new(&previous.target.path))?;
                }
            }

            for (_, stale) in &stale_tabs {
                remove_watch(runtime, Path::new(&stale.target.path))?;
            }

            let mut tabs = runtime.tabs.lock().expect("watch tabs lock");
            for (stale_tab_id, _) in stale_tabs {
                tabs.remove(&stale_tab_id);
            }
            tabs.insert(target.tab_id.clone(), WatchedTab { target, snapshot });
        } else if let Some(runtime) = guard.as_mut() {
            let watched_paths = {
                let tabs = runtime.tabs.lock().expect("watch tabs lock");
                tabs.values()
                    .map(|tab| PathBuf::from(&tab.target.path))
                    .collect::<Vec<_>>()
            };
            for watched_path in watched_paths {
                remove_watch(runtime, &watched_path)?;
            }
            runtime.tabs.lock().expect("watch tabs lock").clear();
        }

        Ok(())
    }
}

pub fn create_runtime(
    emit_patch: Arc<dyn Fn(DirPatch) + Send + Sync>,
    emit_error: Arc<dyn Fn(String, String) + Send + Sync>,
) -> Result<WatchRuntime, String> {
    let tabs = Arc::new(Mutex::new(HashMap::<String, WatchedTab>::new()));
    let patch_emitter = emit_patch;
    let error_emitter = emit_error;
    let tabs_for_callback = tabs.clone();

    let debouncer = new_debouncer(
        Duration::from_millis(150),
        None,
        move |result: DebounceEventResult| {
            handle_debounce_result(&tabs_for_callback, result, &patch_emitter, &error_emitter)
        },
    )
    .map_err(|error| error.to_string())?;

    Ok(WatchRuntime {
        debouncer,
        tabs,
        watch_counts: HashMap::new(),
    })
}

pub fn pane_scope(tab_id: &str) -> &str {
    match tab_id.split_once('-') {
        Some((scope, _)) => scope,
        None => tab_id,
    }
}

#[cfg(feature = "test-utils")]
#[inline(never)]
fn noop_watch_error(_: String, _: String) {}

fn handle_debounce_result(
    tabs: &Arc<Mutex<HashMap<String, WatchedTab>>>,
    result: DebounceEventResult,
    patch_emitter: &Arc<dyn Fn(DirPatch) + Send + Sync>,
    error_emitter: &Arc<dyn Fn(String, String) + Send + Sync>,
) {
    match result {
        Ok(events) => {
            let changed_paths: HashSet<PathBuf> = events
                .iter()
                .flat_map(|event| event.paths.iter().cloned())
                .filter(|path| path.parent().is_some())
                .collect();

            let mut tabs = tabs.lock().expect("watch tabs lock");
            for watched in tabs.values_mut() {
                if !matches_watched_parent(&changed_paths, &watched.target.path) {
                    continue;
                }

                match patch_for_events(&watched.target, &watched.snapshot, &events) {
                    Ok(PatchResult::Targeted { patch, snapshot }) => {
                        watched.snapshot = snapshot;
                        if !patch.changed.is_empty() || !patch.removed.is_empty() {
                            patch_emitter(patch);
                        }
                    }
                    Ok(PatchResult::NeedsResnapshot) => {
                        match snapshot_for_target(&watched.target) {
                            Ok(next_snapshot) => {
                                let patch = diff_entries(
                                    &watched.target.tab_id,
                                    &watched.target.path,
                                    "watch",
                                    &watched.snapshot,
                                    &next_snapshot,
                                );
                                watched.snapshot = next_snapshot;
                                if !patch.changed.is_empty() || !patch.removed.is_empty() {
                                    patch_emitter(patch);
                                }
                            }
                            Err(error) => error_emitter(watched.target.path.clone(), error),
                        }
                    }
                    Err(error) => error_emitter(watched.target.path.clone(), error),
                }
            }
        }
        Err(errors) => {
            for error in errors {
                error_emitter(first_error_path(&error), error.to_string());
            }
        }
    }
}

#[cfg(feature = "test-utils")]
#[allow(dead_code)]
pub fn insert_tab_for_tests(
    runtime: &mut WatchRuntime,
    target: WatchTarget,
    snapshot: HashMap<String, DirectoryEntry>,
) {
    runtime
        .tabs
        .lock()
        .expect("watch tabs lock")
        .insert(target.tab_id.clone(), WatchedTab { target, snapshot });
}

/// Returns the current baseline snapshot stored for `tab_id`, or `None` if the
/// tab isn't (or is no longer) watched. Lets tests assert on `set_tab_watch`'s
/// seeded baseline directly, without forcing a synchronous re-diff.
#[cfg(feature = "test-utils")]
#[allow(dead_code)]
pub fn tab_snapshot_for_tests(
    service: &WatchService,
    tab_id: &str,
) -> Option<HashMap<String, DirectoryEntry>> {
    let guard = service.inner.lock().expect("watch service lock");
    guard.as_ref().and_then(|runtime| {
        runtime
            .tabs
            .lock()
            .expect("watch tabs lock")
            .get(tab_id)
            .map(|tab| tab.snapshot.clone())
    })
}

#[cfg(feature = "test-utils")]
#[allow(dead_code)]
pub fn handle_debounce_result_for_tests(
    runtime: &WatchRuntime,
    result: DebounceEventResult,
    emit_patch: Arc<dyn Fn(DirPatch) + Send + Sync>,
    emit_error: Arc<dyn Fn(String, String) + Send + Sync>,
) {
    handle_debounce_result(&runtime.tabs, result, &emit_patch, &emit_error);
}

#[cfg(feature = "test-utils")]
#[allow(dead_code)]
pub fn noop_watch_error_for_tests(path: String, error: String) {
    noop_watch_error(path, error);
}

#[cfg(feature = "test-utils")]
#[allow(dead_code)]
pub fn canonical_dir_for_tests(path: &Path) -> PathBuf {
    canonical_dir(path)
}

#[cfg(feature = "test-utils")]
#[allow(dead_code)]
pub fn first_error_path_for_tests(error: &notify::Error) -> String {
    first_error_path(error)
}

/// Resolve a directory to its canonical, symlink-free form for comparison.
///
/// The OS watchers report event paths in canonical form (notably macOS FSEvents
/// returns `/private/var/...` for a `/var/...` symlink), while a watched target
/// path may still be the symlinked form the caller supplied. Canonicalizing both
/// sides before comparing keeps parent-directory matching correct across
/// platforms. Falls back to the lexical path when canonicalization fails (e.g. a
/// directory that has since been removed).
fn canonical_dir(path: &Path) -> PathBuf {
    match fs::canonicalize_dir(path) {
        Ok(path) => path,
        Err(_) => path.to_path_buf(),
    }
}

fn matches_watched_parent(changed_paths: &HashSet<PathBuf>, watched_path: &str) -> bool {
    let watched_parent = canonical_dir(Path::new(watched_path));
    for changed_path in changed_paths {
        if let Some(parent) = changed_path.parent() {
            if canonical_dir(parent) == watched_parent {
                return true;
            }
        }
    }
    false
}

enum PatchResult {
    Targeted {
        patch: DirPatch,
        snapshot: HashMap<String, DirectoryEntry>,
    },
    NeedsResnapshot,
}

fn patch_for_events(
    target: &WatchTarget,
    previous: &HashMap<String, DirectoryEntry>,
    events: &[DebouncedEvent],
) -> Result<PatchResult, String> {
    let watched_parent = Path::new(&target.path);
    let mut next = previous.clone();
    let mut changed = Vec::new();
    let mut removed = Vec::new();

    for event in events {
        if event.need_rescan() {
            return Ok(PatchResult::NeedsResnapshot);
        }

        match event.kind {
            EventKind::Access(_) => {}
            EventKind::Any | EventKind::Other => return Ok(PatchResult::NeedsResnapshot),
            EventKind::Create(_) => {
                for path in event
                    .paths
                    .iter()
                    .filter(|path| is_direct_child(path, watched_parent))
                {
                    patch_changed_path(target, &mut next, &mut changed, &mut removed, path)?;
                }
            }
            EventKind::Remove(_) => {
                for path in event
                    .paths
                    .iter()
                    .filter(|path| is_direct_child(path, watched_parent))
                {
                    remove_path(&mut next, &mut removed, path);
                }
            }
            EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Metadata(_)) => {
                for path in event
                    .paths
                    .iter()
                    .filter(|path| is_direct_child(path, watched_parent))
                {
                    patch_changed_path(target, &mut next, &mut changed, &mut removed, path)?;
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                if event.paths.len() < 2 {
                    return Ok(PatchResult::NeedsResnapshot);
                }
                let from = &event.paths[0];
                let to = &event.paths[1];
                if is_direct_child(from, watched_parent) {
                    remove_path(&mut next, &mut removed, from);
                }
                if is_direct_child(to, watched_parent) {
                    patch_changed_path(target, &mut next, &mut changed, &mut removed, to)?;
                }
            }
            EventKind::Modify(ModifyKind::Name(_)) | EventKind::Modify(ModifyKind::Any) => {
                return Ok(PatchResult::NeedsResnapshot);
            }
            EventKind::Modify(ModifyKind::Other) => return Ok(PatchResult::NeedsResnapshot),
        }
    }

    Ok(PatchResult::Targeted {
        patch: DirPatch {
            tab_id: target.tab_id.clone(),
            path: target.path.clone(),
            reason: "watch".to_string(),
            changed,
            removed,
        },
        snapshot: next,
    })
}

/// Both the event path's parent and the watched parent are canonicalized so
/// symlinked and canonical forms of the same directory compare equal across
/// platforms (see [`canonical_dir`]).
fn is_direct_child(path: &Path, watched_parent: &Path) -> bool {
    match path.parent() {
        Some(parent) => canonical_dir(parent) == canonical_dir(watched_parent),
        None => false,
    }
}

fn remove_path(next: &mut HashMap<String, DirectoryEntry>, removed: &mut Vec<String>, path: &Path) {
    let display_path = fs::display_path_from_path(path);
    next.remove(&display_path);
    if !removed.contains(&display_path) {
        removed.push(display_path);
    }
}

fn patch_changed_path(
    target: &WatchTarget,
    next: &mut HashMap<String, DirectoryEntry>,
    changed: &mut Vec<DirEntryPatch>,
    removed: &mut Vec<String>,
    path: &Path,
) -> Result<(), String> {
    let display_path = fs::display_path_from_path(path);

    if !path.exists() {
        next.remove(&display_path);
        if !removed.contains(&display_path) {
            removed.push(display_path);
        }
        return Ok(());
    }

    let entry = fs::directory_entry_from_path(path).map_err(|error| error.to_string())?;
    if !matches_target_filter(&entry, target) {
        next.remove(&display_path);
        if !removed.contains(&display_path) {
            removed.push(display_path);
        }
        return Ok(());
    }

    next.insert(entry.path.clone(), entry.clone());
    changed.push(DirEntryPatch {
        path: entry.path.clone(),
        entry: Some(entry),
    });
    let mut retained = Vec::with_capacity(removed.len());
    for removed_path in removed.drain(..) {
        if removed_path != display_path {
            retained.push(removed_path);
        }
    }
    *removed = retained;
    Ok(())
}

fn matches_target_filter(entry: &DirectoryEntry, target: &WatchTarget) -> bool {
    if !target.show_hidden && (entry.is_hidden || entry.is_system) {
        return false;
    }

    target.filter.is_empty()
        || entry
            .name
            .to_lowercase()
            .contains(&target.filter.to_lowercase())
}

#[inline(never)]
fn first_error_path(error: &notify::Error) -> String {
    match error.paths.first() {
        Some(path) => path.to_string_lossy().into_owned(),
        None => String::new(),
    }
}

pub fn add_watch(runtime: &mut WatchRuntime, path: &Path) -> Result<(), String> {
    match runtime.watch_counts.get_mut(path) {
        Some(count) => {
            *count += 1;
            Ok(())
        }
        None => {
            watch_path(&mut runtime.debouncer, path)?;
            runtime.watch_counts.insert(path.to_path_buf(), 1);
            Ok(())
        }
    }
}

pub fn remove_watch(runtime: &mut WatchRuntime, path: &Path) -> Result<(), String> {
    let Some(count) = runtime.watch_counts.get_mut(path) else {
        return Ok(());
    };

    if *count > 1 {
        *count -= 1;
        return Ok(());
    }

    unwatch_path(&mut runtime.debouncer, path)?;
    runtime.watch_counts.remove(path);
    Ok(())
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

    Ok(snapshot_from_entries(response.entries))
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
fn snapshot_from_entries(entries: Vec<DirectoryEntry>) -> HashMap<String, DirectoryEntry> {
    entries
        .into_iter()
        .map(|mut entry| {
            entry.icon_data_url = None;
            (entry.path.clone(), entry)
        })
        .collect()
}

fn list_dir_for_snapshot(options: &ListDirOptions) -> Result<ListDirResponse, String> {
    fs::list_dir(options).map_err(|error| error.to_string())
}

fn watch_path(
    debouncer: &mut Debouncer<notify::RecommendedWatcher, FileIdMap>,
    path: &Path,
) -> Result<(), String> {
    debouncer
        .watch(path, RecursiveMode::NonRecursive)
        .map_err(|error| error.to_string())
}

fn unwatch_path(
    debouncer: &mut Debouncer<notify::RecommendedWatcher, FileIdMap>,
    path: &Path,
) -> Result<(), String> {
    debouncer.unwatch(path).map_err(|error| error.to_string())
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
