// `#[allow(dead_code)]`: these two submodules are fully used from
// `directory_session`/`lib.rs` in the real crate build. The `allow` only
// matters for `tests/watch_private_integration.rs`, which `include!`s this
// entire file verbatim into an isolated single-purpose test binary that
// never references either module — without it, that binary's own
// `-D warnings` dead-code lint would fail on code that is very much alive in
// the real crate.
#[allow(dead_code)]
pub mod coordinator;
#[allow(dead_code)]
pub mod patch;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use notify::event::{EventKind, ModifyKind, RenameMode};
use notify::{RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};

use self::coordinator::{
    CompactedBatch, MutationKind, RawMutation, WatchCoordinator, WatchId,
};
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

static NEXT_WATCH_ID: AtomicU64 = AtomicU64::new(1);

pub struct WatchRuntime {
    pub watcher: notify::RecommendedWatcher,
    coordinator: WatchCoordinator,
    tabs: Arc<Mutex<HashMap<String, WatchedTab>>>,
    pub watch_counts: HashMap<PathBuf, usize>,
}

impl Drop for WatchRuntime {
    fn drop(&mut self) {
        self.coordinator.shutdown();
    }
}

#[derive(Clone)]
struct WatchedTab {
    watch_id: WatchId,
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
        listing_seed_entries: Option<Vec<DirectoryEntry>>,
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
            let snapshot = match (listing_seed_entries, entries) {
                (Some(entries), _) | (None, Some(entries)) => {
                    snapshot_from_entries_for_watch(&target, entries)
                }
                (None, None) => snapshot_for_watch_baseline(&target)?,
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
            let watch_id = WatchId(NEXT_WATCH_ID.fetch_add(1, Ordering::SeqCst));
            // This is a brand-new generation, so it cannot have coordinator
            // state to clear. Do not enqueue `Replace(watch_id)` here: the
            // native watch is already armed, and a child event can reach the
            // raw lane before that control message is drained. Clearing the
            // new ID afterward would discard the first real create/delete
            // batch and leave the pane stale until an explicit refresh.
            tabs.insert(
                target.tab_id.clone(),
                WatchedTab {
                    watch_id,
                    target,
                    snapshot,
                },
            );
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

    /// Rechecks every currently watched tab and emits a patch only for paths
    /// whose snapshot changed. Used as a window-focus reliability net for OS
    /// watcher events that may have been dropped while the app was inactive.
    pub fn reconcile(
        &self,
        emit_patch: Arc<dyn Fn(DirPatch) + Send + Sync>,
        emit_error: Arc<dyn Fn(String, String) + Send + Sync>,
    ) {
        let tabs = {
            let guard = self.inner.lock().expect("watch service lock");
            let Some(runtime) = guard.as_ref() else {
                return;
            };
            Arc::clone(&runtime.tabs)
        };

        reconcile_tabs(&tabs, "refresh", &emit_patch, &emit_error);
    }
}

pub fn create_runtime(
    emit_patch: Arc<dyn Fn(DirPatch) + Send + Sync>,
    emit_error: Arc<dyn Fn(String, String) + Send + Sync>,
) -> Result<WatchRuntime, String> {
    let tabs = Arc::new(Mutex::new(HashMap::<String, WatchedTab>::new()));
    let tabs_for_compactor = Arc::clone(&tabs);
    let coordinator = WatchCoordinator::spawn(Arc::new(move |batch| {
        process_compacted_batch(&tabs_for_compactor, batch, &emit_patch, &emit_error);
    }));
    let raw_sender = coordinator.raw_sender();
    let tabs_for_callback = Arc::clone(&tabs);

    let watcher =
        notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
            forward_notify_result(&tabs_for_callback, &raw_sender, result);
        })
        .map_err(|error| error.to_string())?;

    Ok(WatchRuntime {
        watcher,
        coordinator,
        tabs,
        watch_counts: HashMap::new(),
    })
}

/// Forwards a notify callback through the bounded coordinator. The callback
/// takes a short registration snapshot before classifying events, then drops
/// the tabs lock before touching the coordinator. This deliberately waits for
/// that brief snapshot rather than using `try_lock`: losing an OS mutation is
/// worse than briefly delaying a callback, and tab locks never cover I/O.
fn forward_notify_result(
    tabs: &Arc<Mutex<HashMap<String, WatchedTab>>>,
    raw_sender: &coordinator::RawLaneSender,
    result: Result<notify::Event, notify::Error>,
) {
    forward_notify_result_after_before_lock(tabs, raw_sender, result, || {});
}

fn forward_notify_result_after_before_lock<F>(
    tabs: &Arc<Mutex<HashMap<String, WatchedTab>>>,
    raw_sender: &coordinator::RawLaneSender,
    result: Result<notify::Event, notify::Error>,
    before_lock: F,
) where
    F: FnOnce(),
{
    before_lock();
    let registrations = {
        let tabs = tabs.lock().expect("watch tabs lock");
        tabs.values()
            .map(|watched| (watched.watch_id, watched.target.clone()))
            .collect::<Vec<_>>()
    };

    match result {
        Ok(event) => {
            for (watch_id, target) in registrations {
                let watched = WatchedTab {
                    watch_id,
                    target,
                    snapshot: HashMap::new(),
                };
                for mutation in raw_mutations_for_event(&event, &watched) {
                    raw_sender.push(mutation);
                }
            }
        }
        Err(_) => {
            // An unresolvable notify error has no stable child identity;
            // mark every active watch dirty so the bounded coordinator
            // schedules authoritative recovery instead of retaining it.
            for (watch_id, _) in registrations {
                raw_sender.push(RawMutation {
                    watch_id,
                    child_path: PathBuf::new(),
                    kind: MutationKind::Unresolved,
                });
            }
        }
    }
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

#[cfg(feature = "test-utils")]
fn process_results(
    tabs: &Arc<Mutex<HashMap<String, WatchedTab>>>,
    batch: Vec<Result<notify::Event, notify::Error>>,
    patch_emitter: &Arc<dyn Fn(DirPatch) + Send + Sync>,
    error_emitter: &Arc<dyn Fn(String, String) + Send + Sync>,
) {
    let mut events = Vec::new();
    let mut errors = Vec::new();
    for item in batch {
        match item {
            Ok(event) => events.push(event),
            Err(error) => errors.push(error),
        }
    }

    if !events.is_empty() {
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
                    match snapshot_for_watch_baseline(&watched.target) {
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

    for error in errors {
        error_emitter(first_error_path(&error), error.to_string());
    }
}

fn raw_mutations_for_event(event: &notify::Event, watched: &WatchedTab) -> Vec<RawMutation> {
    let kind = match event.kind {
        EventKind::Access(_) => return Vec::new(),
        EventKind::Remove(_) => MutationKind::Removed,
        EventKind::Any
        | EventKind::Other
        | EventKind::Modify(ModifyKind::Any | ModifyKind::Other)
        | EventKind::Modify(ModifyKind::Name(_))
            if event.kind != EventKind::Modify(ModifyKind::Name(RenameMode::Both)) =>
        {
            MutationKind::Unresolved
        }
        _ if event.need_rescan() => MutationKind::Unresolved,
        _ => MutationKind::Changed,
    };
    // macOS FSEvents can report a direct-child mutation as a modification of
    // the watched directory itself. That path does not identify one child,
    // so treating it as an ordinary changed path would filter it out below
    // and silently lose the update. Mark it unresolved instead: the bounded
    // coordinator will perform one authoritative non-recursive resnapshot.
    if event
        .paths
        .iter()
        .any(|path| canonical_dir(path) == canonical_dir(Path::new(&watched.target.path)))
    {
        return vec![RawMutation {
            watch_id: watched.watch_id,
            child_path: PathBuf::new(),
            kind: MutationKind::Unresolved,
        }];
    }
    if kind == MutationKind::Unresolved {
        return vec![RawMutation {
            watch_id: watched.watch_id,
            child_path: PathBuf::new(),
            kind,
        }];
    }
    event
        .paths
        .iter()
        .filter(|path| is_direct_child(path, Path::new(&watched.target.path)))
        .map(|child_path| RawMutation {
            watch_id: watched.watch_id,
            child_path: child_path.clone(),
            kind,
        })
        .collect()
}

fn process_compacted_batch(
    tabs: &Arc<Mutex<HashMap<String, WatchedTab>>>,
    batch: CompactedBatch,
    patch_emitter: &Arc<dyn Fn(DirPatch) + Send + Sync>,
    error_emitter: &Arc<dyn Fn(String, String) + Send + Sync>,
) {
    let Some(watched) = ({
        let tabs = tabs.lock().expect("watch tabs lock");
        tabs.values()
            .find(|watched| {
                watched.watch_id
                    == match &batch {
                        CompactedBatch::Targeted { watch_id, .. }
                        | CompactedBatch::Dirty { watch_id, .. } => *watch_id,
                    }
            })
            .cloned()
    }) else {
        return;
    };
    let next = match batch {
        CompactedBatch::Dirty { .. } => snapshot_for_watch_baseline(&watched.target),
        CompactedBatch::Targeted {
            changed, removed, ..
        } => {
            let mut next = watched.snapshot.clone();
            let mut changed_entries = Vec::new();
            let mut removed_paths = Vec::new();
            for path in removed {
                remove_path(&mut next, &mut removed_paths, &path);
            }
            for path in changed {
                if let Err(error) = patch_changed_path(
                    &watched.target,
                    &mut next,
                    &mut changed_entries,
                    &mut removed_paths,
                    &path,
                ) {
                    error_emitter(watched.target.path.clone(), error);
                    return;
                }
            }
            Ok(next)
        }
    };
    match next {
        Ok(next) => {
            let patch = diff_entries(
                &watched.target.tab_id,
                &watched.target.path,
                "watch",
                &watched.snapshot,
                &next,
            );
            let applied = {
                let mut tabs = tabs.lock().expect("watch tabs lock");
                let Some(current) = tabs
                    .values_mut()
                    .find(|current| current.watch_id == watched.watch_id)
                else {
                    return;
                };
                current.snapshot = next;
                true
            };
            if applied && (!patch.changed.is_empty() || !patch.removed.is_empty()) {
                patch_emitter(patch);
            }
        }
        Err(error) => error_emitter(watched.target.path.clone(), error),
    }
}

fn reconcile_tabs(
    tabs: &Arc<Mutex<HashMap<String, WatchedTab>>>,
    reason: &str,
    patch_emitter: &Arc<dyn Fn(DirPatch) + Send + Sync>,
    error_emitter: &Arc<dyn Fn(String, String) + Send + Sync>,
) {
    let mut tabs = tabs.lock().expect("watch tabs lock");
    for watched in tabs.values_mut() {
        match snapshot_for_watch_baseline(&watched.target) {
            Ok(next_snapshot) => {
                let patch = diff_entries(
                    &watched.target.tab_id,
                    &watched.target.path,
                    reason,
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
}

#[cfg(feature = "test-utils")]
#[allow(dead_code)]
pub fn insert_tab_for_tests(
    runtime: &mut WatchRuntime,
    target: WatchTarget,
    snapshot: HashMap<String, DirectoryEntry>,
) -> WatchId {
    let watch_id = WatchId(NEXT_WATCH_ID.fetch_add(1, Ordering::SeqCst));
    runtime.tabs.lock().expect("watch tabs lock").insert(
        target.tab_id.clone(),
        WatchedTab {
            watch_id,
            target,
            snapshot,
        },
    );
    watch_id
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
    batch: Vec<Result<notify::Event, notify::Error>>,
    emit_patch: Arc<dyn Fn(DirPatch) + Send + Sync>,
    emit_error: Arc<dyn Fn(String, String) + Send + Sync>,
) {
    process_results(&runtime.tabs, batch, &emit_patch, &emit_error);
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

#[cfg(feature = "test-utils")]
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

#[cfg(feature = "test-utils")]
enum PatchResult {
    Targeted {
        patch: DirPatch,
        snapshot: HashMap<String, DirectoryEntry>,
    },
    NeedsResnapshot,
}

#[cfg(feature = "test-utils")]
fn patch_for_events(
    target: &WatchTarget,
    previous: &HashMap<String, DirectoryEntry>,
    events: &[notify::Event],
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

    let entry = fs::directory_entry_from_path_without_item_count(path)
        .map_err(|error| error.to_string())?;
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

#[cfg(feature = "test-utils")]
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
            watch_path(&mut runtime.watcher, path)?;
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

    unwatch_path(&mut runtime.watcher, path)?;
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

    Ok(snapshot_from_entries(response.entries, false))
}

fn snapshot_for_watch_baseline(
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
fn snapshot_from_entries_for_watch(
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
    fs::list_dir(options).map_err(|error| error.to_string())
}

fn watch_path(watcher: &mut notify::RecommendedWatcher, path: &Path) -> Result<(), String> {
    watcher
        .watch(path, RecursiveMode::NonRecursive)
        .map_err(|error| error.to_string())
}

fn unwatch_path(watcher: &mut notify::RecommendedWatcher, path: &Path) -> Result<(), String> {
    watcher.unwatch(path).map_err(|error| error.to_string())
}

// ---------------------------------------------------------------------
// Watch-first session capture (Phase 3: directory_session)
// ---------------------------------------------------------------------
//
// `DirectorySessionService` needs to register a native watch and start
// buffering direct-child mutations *before* it enumerates a directory, so a
// create/delete that races the enumeration is never silently lost (it is
// either folded into the initial snapshot or triggers a resnapshot). This is
// intentionally independent from `WatchService`/`WatchRuntime` above (which
// remains the long-lived, tab-scoped v1 watch runtime): a session capture is
// short-lived (its own single-purpose `notify` watcher, torn down once the
// caller finishes reconciling), so it does not need to share state with the
// v1 per-tab watch map.

/// A short-lived native watch used only to capture direct-child mutation
/// paths for the duration of establishing a directory-session baseline. Not
/// intended for long-term use — [`DirectorySessionService`](crate::directory_session)
/// tears this down as soon as the session/watch/view baseline is published
/// and re-arms a normal watch afterward if needed.
///
/// Uses a bounded lane (capacity [`coordinator::RAW_LANE_CAPACITY`], matching
/// the long-lived compactor's raw-lane bound) rather than an unbounded
/// channel: the platform `notify` callback must never be able to grow memory
/// without bound during a pathological event burst that races enumeration.
/// Overflow (a full lane) is reported as `None` from
/// [`drain_captured_mutations`], which callers already treat as "resnapshot
/// required" — exactly the correct behavior for "too much happened to trust
/// a partial event list".
pub struct CaptureHandle {
    watcher: notify::RecommendedWatcher,
    watched_path: PathBuf,
    rx: crossbeam_channel::Receiver<Result<notify::Event, notify::Error>>,
    overflowed: Arc<std::sync::atomic::AtomicBool>,
}

/// Begins buffering filesystem events for direct children of `path`. Returns
/// a [`CaptureHandle`] whose buffered mutation paths can be read with
/// [`drain_captured_mutations`] once enumeration completes.
pub fn begin_capture(path: &Path) -> Result<CaptureHandle, String> {
    let (tx, rx) = crossbeam_channel::bounded(coordinator::RAW_LANE_CAPACITY);
    let overflowed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let overflowed_for_callback = Arc::clone(&overflowed);
    let mut watcher = notify::recommended_watcher(move |result| {
        // Non-blocking push: a full lane during capture marks the capture
        // overflowed (forcing a resnapshot) instead of blocking the
        // platform callback thread, matching the same non-blocking
        // contract the long-lived `WatchCoordinator` compactor lane uses.
        if tx.try_send(result).is_err() {
            overflowed_for_callback.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    })
    .map_err(|error| error.to_string())?;
    watcher
        .watch(path, RecursiveMode::NonRecursive)
        .map_err(|error| error.to_string())?;

    Ok(CaptureHandle {
        watcher,
        watched_path: path.to_path_buf(),
        rx,
        overflowed,
    })
}

/// Stops the capture watch and returns the set of direct-child paths that
/// changed (created/removed/modified/renamed) while it was active. A
/// `need_rescan`/`Any`/`Other` event, a lane overflow, or a rename this
/// function cannot fully resolve, is reported as `None` — the caller must
/// treat that as "resnapshot required" rather than trusting the partial path
/// list.
pub fn drain_captured_mutations(mut handle: CaptureHandle) -> Option<HashSet<PathBuf>> {
    let _ = handle.watcher.unwatch(&handle.watched_path);

    if handle.overflowed.load(std::sync::atomic::Ordering::SeqCst) {
        return None;
    }

    let mut mutated = HashSet::new();
    while let Ok(result) = handle.rx.try_recv() {
        let event = match result {
            Ok(event) => event,
            Err(_) => return None,
        };

        if event.need_rescan() {
            return None;
        }

        match event.kind {
            EventKind::Access(_) => {}
            EventKind::Any | EventKind::Other => return None,
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                if event.paths.len() < 2 {
                    return None;
                }
                mutated.insert(event.paths[0].clone());
                mutated.insert(event.paths[1].clone());
            }
            EventKind::Modify(ModifyKind::Name(_)) | EventKind::Modify(ModifyKind::Any) => {
                return None;
            }
            EventKind::Modify(ModifyKind::Other) => return None,
            EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_) => {
                for changed_path in &event.paths {
                    if is_direct_child(changed_path, &handle.watched_path) {
                        mutated.insert(changed_path.clone());
                    }
                }
            }
        }
    }

    Some(mutated)
}

/// Test-only wrapper around the private `raw_mutations_for_event` so
/// coverage/behavior tests can exercise the real `notify::Event` ->
/// `RawMutation` classification (Access/Remove/Unresolved/Changed, direct-
/// child filtering, `need_rescan`) without needing a live OS watcher thread
/// to deliver the event. `watch_id`/`target` are the same fields
/// `WatchedTab` stores; this constructs one internally since `WatchedTab`
/// itself is module-private.
#[cfg(feature = "test-utils")]
#[allow(dead_code)]
pub fn raw_mutations_for_event_for_tests(
    event: &notify::Event,
    watch_id: WatchId,
    target: WatchTarget,
) -> Vec<RawMutation> {
    let watched = WatchedTab {
        watch_id,
        target,
        snapshot: HashMap::new(),
    };
    raw_mutations_for_event(event, &watched)
}

/// Test-only wrapper around the private `process_compacted_batch`, driving
/// it against a real [`WatchRuntime`]'s tab map so coverage/behavior tests
/// can exercise targeted-patch application, dirty-resnapshot, and the
/// unknown-watch-id no-op branch without a live OS watcher thread producing
/// the [`CompactedBatch`].
#[cfg(feature = "test-utils")]
#[allow(dead_code)]
pub fn process_compacted_batch_for_tests(
    runtime: &WatchRuntime,
    batch: CompactedBatch,
    emit_patch: Arc<dyn Fn(DirPatch) + Send + Sync>,
    emit_error: Arc<dyn Fn(String, String) + Send + Sync>,
) {
    process_compacted_batch(&runtime.tabs, batch, &emit_patch, &emit_error);
}

/// Drives the same callback forwarding path used by `notify`, allowing
/// integration tests to verify that a callback is retained until it can take
/// the brief registration snapshot.
#[cfg(feature = "test-utils")]
#[allow(dead_code)]
pub fn forward_notify_result_for_tests(
    runtime: &WatchRuntime,
    result: Result<notify::Event, notify::Error>,
) {
    forward_notify_result(&runtime.tabs, &runtime.coordinator.raw_sender(), result);
}

#[cfg(feature = "test-utils")]
#[allow(dead_code)]
pub fn forward_notify_result_with_before_lock_for_tests(
    runtime: &WatchRuntime,
    result: Result<notify::Event, notify::Error>,
    before_lock: impl FnOnce(),
) {
    forward_notify_result_after_before_lock(
        &runtime.tabs,
        &runtime.coordinator.raw_sender(),
        result,
        before_lock,
    );
}

#[cfg(feature = "test-utils")]
#[allow(dead_code)]
pub fn while_tabs_locked_for_tests(runtime: &WatchRuntime, action: impl FnOnce()) {
    let _tabs = runtime.tabs.lock().expect("watch tabs lock");
    action();
}

#[cfg(feature = "test-utils")]
#[allow(dead_code)]
pub fn capture_handle_for_tests(path: &Path) -> Result<CaptureHandle, String> {
    begin_capture(path)
}

#[cfg(feature = "test-utils")]
#[allow(dead_code)]
pub fn drain_captured_mutations_for_tests(handle: CaptureHandle) -> Option<HashSet<PathBuf>> {
    drain_captured_mutations(handle)
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
