mod capture;
mod classification;
pub mod coordinator;
pub mod patch;
mod runtime;
mod tab_patch;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use self::coordinator::{WatchCoordinator, WatchId};
use crate::fs::{DirectoryEntry, SortDirection, SortKey};

pub use capture::{begin_capture, drain_captured_mutations, CaptureHandle};
#[cfg(feature = "test-utils")]
pub use capture::{capture_handle_for_tests, drain_captured_mutations_for_tests};
#[cfg(feature = "test-utils")]
pub use classification::{
    canonical_dir_for_tests, first_error_path_for_tests, handle_debounce_result_for_tests,
    noop_watch_error_for_tests, raw_mutations_for_event_for_tests,
};
use runtime::reconcile_tabs;
pub use runtime::{add_watch, create_runtime, remove_watch};
#[cfg(feature = "test-utils")]
pub use runtime::{
    forward_notify_result_for_tests, forward_notify_result_with_before_lock_for_tests,
    process_compacted_batch_for_tests, while_tabs_locked_for_tests,
};
pub use tab_patch::{diff_entries, snapshot_for_target, PatchLabel};
use tab_patch::{snapshot_for_watch_baseline, snapshot_from_entries_for_watch};

const RENAME_EVENT_PATH_COUNT: usize = 2;

fn is_paired_rename(event: &notify::Event) -> bool {
    event.paths.len() >= RENAME_EVENT_PATH_COUNT
}

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

impl DirPatch {
    fn has_changes(&self) -> bool {
        !self.changed.is_empty() || !self.removed.is_empty()
    }
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
    tabs: WatchedTabs,
    pub watch_counts: HashMap<PathBuf, usize>,
}

impl Drop for WatchRuntime {
    fn drop(&mut self) {
        self.coordinator.shutdown();
    }
}

pub type PatchEmitter = Arc<dyn Fn(DirPatch) + Send + Sync>;
pub type ErrorEmitter = Arc<dyn Fn(String, String) + Send + Sync>;

#[derive(Clone)]
pub struct WatchEmitters {
    pub patch: PatchEmitter,
    pub error: ErrorEmitter,
}

pub struct TabWatchArm {
    pub target: Option<WatchTarget>,
    pub listing_seed_entries: Option<Vec<DirectoryEntry>>,
    pub entries: Option<Vec<DirectoryEntry>>,
}

impl TabWatchArm {
    pub fn disarm() -> Self {
        Self {
            target: None,
            listing_seed_entries: None,
            entries: None,
        }
    }
}
type WatchedTabs = Arc<Mutex<HashMap<String, WatchedTab>>>;

#[derive(Clone)]
struct WatchedTab {
    watch_id: WatchId,
    target: WatchTarget,
    snapshot: HashMap<String, DirectoryEntry>,
}

impl WatchService {
    pub fn set_tab_watch(&self, arm: TabWatchArm, emitters: WatchEmitters) -> Result<(), String> {
        let TabWatchArm {
            target,
            listing_seed_entries,
            entries,
        } = arm;
        let mut guard = self.inner.lock().expect("watch service lock");
        let Some(target) = target else {
            return disarm_all_tabs(guard.as_mut());
        };
        if guard.is_none() {
            *guard = Some(create_runtime(emitters)?);
        }

        let runtime = guard.as_mut().expect("watch runtime");
        let snapshot = match (listing_seed_entries, entries) {
            (Some(entries), _) | (None, Some(entries)) => {
                snapshot_from_entries_for_watch(&target, entries)
            }
            (None, None) => snapshot_for_watch_baseline(&target)?,
        };
        arm_tab_watch(runtime, target, snapshot)
    }

    fn watched_tabs(&self) -> Option<WatchedTabs> {
        let guard = self.inner.lock().expect("watch service lock");
        guard.as_ref().map(|runtime| Arc::clone(&runtime.tabs))
    }

    pub fn reconcile(&self, emitters: WatchEmitters) {
        let Some(tabs) = self.watched_tabs() else {
            return;
        };
        reconcile_tabs(&tabs, "refresh", &emitters);
    }
}

struct PaneTabs {
    same_tab_previous: Option<WatchedTab>,
    stale_siblings: Vec<(String, WatchedTab)>,
}

fn pane_tabs(tabs: &WatchedTabs, tab_id: &str) -> PaneTabs {
    let tabs = tabs.lock().expect("watch tabs lock");
    let pane_name = pane_scope(tab_id);
    let stale_siblings = tabs
        .iter()
        .filter(|(other_id, _)| pane_scope(other_id) == pane_name && other_id.as_str() != tab_id)
        .map(|(other_id, watched)| (other_id.clone(), watched.clone()))
        .collect();
    PaneTabs {
        same_tab_previous: tabs.get(tab_id).cloned(),
        stale_siblings,
    }
}

fn arm_tab_watch(
    runtime: &mut WatchRuntime,
    target: WatchTarget,
    snapshot: HashMap<String, DirectoryEntry>,
) -> Result<(), String> {
    let pane = pane_tabs(&runtime.tabs, &target.tab_id);
    let previous_path = pane.same_tab_previous.map(|previous| previous.target.path);
    let path_changed = previous_path.as_deref() != Some(target.path.as_str());
    if path_changed {
        add_watch(runtime, Path::new(&target.path))?;
    }
    if let Some(previous_path) = previous_path.filter(|previous| *previous != target.path) {
        remove_watch(runtime, Path::new(&previous_path))?;
    }
    for (_, stale) in &pane.stale_siblings {
        remove_watch(runtime, Path::new(&stale.target.path))?;
    }

    let mut tabs = runtime.tabs.lock().expect("watch tabs lock");
    for (stale_tab_id, _) in pane.stale_siblings {
        tabs.remove(&stale_tab_id);
    }
    tabs.insert(
        target.tab_id.clone(),
        WatchedTab {
            watch_id: next_watch_id(),
            target,
            snapshot,
        },
    );
    Ok(())
}

fn disarm_all_tabs(runtime: Option<&mut WatchRuntime>) -> Result<(), String> {
    let Some(runtime) = runtime else {
        return Ok(());
    };
    let watched_paths: Vec<PathBuf> = {
        let tabs = runtime.tabs.lock().expect("watch tabs lock");
        tabs.values()
            .map(|tab| PathBuf::from(&tab.target.path))
            .collect()
    };
    for watched_path in watched_paths {
        remove_watch(runtime, &watched_path)?;
    }
    runtime.tabs.lock().expect("watch tabs lock").clear();
    Ok(())
}

fn next_watch_id() -> WatchId {
    WatchId(NEXT_WATCH_ID.fetch_add(1, Ordering::SeqCst))
}

pub fn pane_scope(tab_id: &str) -> &str {
    match tab_id.split_once('-') {
        Some((scope, _)) => scope,
        None => tab_id,
    }
}

#[cfg(feature = "test-utils")]
pub fn insert_tab_for_tests(
    runtime: &mut WatchRuntime,
    target: WatchTarget,
    snapshot: HashMap<String, DirectoryEntry>,
) -> WatchId {
    let watch_id = next_watch_id();
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

#[cfg(feature = "test-utils")]
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
