use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use notify::{RecursiveMode, Watcher};

use super::classification::raw_mutations_for_event;
use super::coordinator::{
    self, CompactedBatch, MutationKind, RawMutation, WatchCoordinator, WatchId,
};
use super::tab_patch::{
    diff_entries, patch_changed_path, remove_path, resnapshot_tab, snapshot_for_watch_baseline,
};
use super::{ErrorEmitter, PatchEmitter, WatchRuntime, WatchTarget, WatchedTab, WatchedTabs};
use crate::error_text::StringifyError;
use crate::fs::DirectoryEntry;

pub fn create_runtime(
    emit_patch: PatchEmitter,
    emit_error: ErrorEmitter,
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
        .stringify_err()?;

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
pub(super) fn forward_notify_result(
    tabs: &WatchedTabs,
    raw_sender: &coordinator::RawLaneSender,
    result: Result<notify::Event, notify::Error>,
) {
    forward_notify_result_after_before_lock(tabs, raw_sender, result, || {});
}

fn forward_notify_result_after_before_lock<F>(
    tabs: &WatchedTabs,
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
        Ok(event) => forward_event(&event, registrations, raw_sender),
        Err(_) => mark_registrations_unresolved(registrations, raw_sender),
    }
}

fn forward_event(
    event: &notify::Event,
    registrations: Vec<(WatchId, WatchTarget)>,
    raw_sender: &coordinator::RawLaneSender,
) {
    for (watch_id, target) in registrations {
        let watched = WatchedTab {
            watch_id,
            target,
            snapshot: HashMap::new(),
        };
        for mutation in raw_mutations_for_event(event, &watched) {
            raw_sender.push(mutation);
        }
    }
}

/// An unresolvable notify error has no stable child identity; mark every
/// active watch dirty so the bounded coordinator schedules authoritative
/// recovery instead of retaining it.
fn mark_registrations_unresolved(
    registrations: Vec<(WatchId, WatchTarget)>,
    raw_sender: &coordinator::RawLaneSender,
) {
    for (watch_id, _) in registrations {
        raw_sender.push(RawMutation {
            watch_id,
            child_path: PathBuf::new(),
            kind: MutationKind::Unresolved,
        });
    }
}

pub(super) fn process_compacted_batch(
    tabs: &WatchedTabs,
    batch: CompactedBatch,
    patch_emitter: &PatchEmitter,
    error_emitter: &ErrorEmitter,
) {
    let Some(watched) = watched_tab_by_id(tabs, batch_watch_id(&batch)) else {
        return;
    };
    let next = match batch {
        CompactedBatch::Dirty { .. } => snapshot_for_watch_baseline(&watched.target),
        CompactedBatch::Targeted {
            changed, removed, ..
        } => snapshot_with_targeted_changes(&watched, &changed, &removed),
    };
    let next = match next {
        Ok(next) => next,
        Err(error) => {
            error_emitter(watched.target.path.clone(), error);
            return;
        }
    };
    let patch = diff_entries(
        &watched.target.tab_id,
        &watched.target.path,
        "watch",
        &watched.snapshot,
        &next,
    );
    let stored = store_snapshot(tabs, watched.watch_id, next);
    if stored && patch.has_changes() {
        patch_emitter(patch);
    }
}

fn batch_watch_id(batch: &CompactedBatch) -> WatchId {
    match batch {
        CompactedBatch::Targeted { watch_id, .. } | CompactedBatch::Dirty { watch_id, .. } => {
            *watch_id
        }
    }
}

fn watched_tab_by_id(tabs: &WatchedTabs, watch_id: WatchId) -> Option<WatchedTab> {
    let tabs = tabs.lock().expect("watch tabs lock");
    tabs.values()
        .find(|watched| watched.watch_id == watch_id)
        .cloned()
}

fn snapshot_with_targeted_changes(
    watched: &WatchedTab,
    changed: &[PathBuf],
    removed: &[PathBuf],
) -> Result<HashMap<String, DirectoryEntry>, String> {
    let mut next = watched.snapshot.clone();
    let mut changed_entries = Vec::new();
    let mut removed_paths = Vec::new();
    for path in removed {
        remove_path(&mut next, &mut removed_paths, path);
    }
    for path in changed {
        patch_changed_path(
            &watched.target,
            &mut next,
            &mut changed_entries,
            &mut removed_paths,
            path,
        )?;
    }
    Ok(next)
}

/// Returns `false` when the tab was unwatched while its batch was processed.
fn store_snapshot(
    tabs: &WatchedTabs,
    watch_id: WatchId,
    snapshot: HashMap<String, DirectoryEntry>,
) -> bool {
    let mut tabs = tabs.lock().expect("watch tabs lock");
    let Some(current) = tabs
        .values_mut()
        .find(|current| current.watch_id == watch_id)
    else {
        return false;
    };
    current.snapshot = snapshot;
    true
}

pub(super) fn reconcile_tabs(
    tabs: &WatchedTabs,
    reason: &str,
    patch_emitter: &PatchEmitter,
    error_emitter: &ErrorEmitter,
) {
    let mut tabs = tabs.lock().expect("watch tabs lock");
    for watched in tabs.values_mut() {
        resnapshot_tab(watched, reason, patch_emitter, error_emitter);
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

fn watch_path(watcher: &mut notify::RecommendedWatcher, path: &Path) -> Result<(), String> {
    watcher
        .watch(path, RecursiveMode::NonRecursive)
        .stringify_err()
}

fn unwatch_path(watcher: &mut notify::RecommendedWatcher, path: &Path) -> Result<(), String> {
    watcher.unwatch(path).stringify_err()
}

/// Test-only wrapper around the private `process_compacted_batch`, driving
/// it against a real [`WatchRuntime`]'s tab map so coverage/behavior tests
/// can exercise targeted-patch application, dirty-resnapshot, and the
/// unknown-watch-id no-op branch without a live OS watcher thread producing
/// the [`CompactedBatch`].
#[cfg(feature = "test-utils")]
pub fn process_compacted_batch_for_tests(
    runtime: &WatchRuntime,
    batch: CompactedBatch,
    emit_patch: PatchEmitter,
    emit_error: ErrorEmitter,
) {
    process_compacted_batch(&runtime.tabs, batch, &emit_patch, &emit_error);
}

/// Drives the same callback forwarding path used by `notify`, allowing
/// integration tests to verify that a callback is retained until it can take
/// the brief registration snapshot.
#[cfg(feature = "test-utils")]
pub fn forward_notify_result_for_tests(
    runtime: &WatchRuntime,
    result: Result<notify::Event, notify::Error>,
) {
    forward_notify_result(&runtime.tabs, &runtime.coordinator.raw_sender(), result);
}

#[cfg(feature = "test-utils")]
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
pub fn while_tabs_locked_for_tests(runtime: &WatchRuntime, action: impl FnOnce()) {
    let _tabs = runtime.tabs.lock().expect("watch tabs lock");
    action();
}
