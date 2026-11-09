#[cfg(feature = "test-utils")]
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use notify::event::{EventKind, ModifyKind, RenameMode};

#[cfg(feature = "test-utils")]
use super::coordinator::WatchId;
use super::coordinator::{MutationKind, RawMutation};
#[cfg(feature = "test-utils")]
use super::tab_patch::{resnapshot_tab, SnapshotDraft};
use super::WatchedTab;
#[cfg(feature = "test-utils")]
use super::{is_paired_rename, DirPatch, WatchEmitters, WatchRuntime, WatchTarget, WatchedTabs};
use crate::fs;
#[cfg(feature = "test-utils")]
use crate::fs::DirectoryEntry;

#[cfg(feature = "test-utils")]
#[inline(never)]
fn noop_watch_error(_: String, _: String) {}

#[cfg(feature = "test-utils")]
fn process_results(
    tabs: &WatchedTabs,
    batch: Vec<Result<notify::Event, notify::Error>>,
    emitters: &WatchEmitters,
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
        refresh_tabs_from_events(tabs, &events, emitters);
    }

    for error in errors {
        (emitters.error)(first_error_path(&error), error.to_string());
    }
}

#[cfg(feature = "test-utils")]
fn refresh_tabs_from_events(
    tabs: &WatchedTabs,
    events: &[notify::Event],
    emitters: &WatchEmitters,
) {
    let changed_paths: HashSet<PathBuf> = events
        .iter()
        .flat_map(|event| event.paths.iter().cloned())
        .filter(|path| path.parent().is_some())
        .collect();

    let mut tabs = tabs.lock().expect("watch tabs lock");
    for watched in tabs.values_mut() {
        if matches_watched_parent(&changed_paths, &watched.target.path) {
            refresh_tab_from_events(watched, events, emitters);
        }
    }
}

#[cfg(feature = "test-utils")]
fn refresh_tab_from_events(
    watched: &mut WatchedTab,
    events: &[notify::Event],
    emitters: &WatchEmitters,
) {
    match patch_for_events(&watched.target, &watched.snapshot, events) {
        Ok(PatchResult::Targeted { patch, snapshot }) => {
            watched.snapshot = snapshot;
            if patch.has_changes() {
                (emitters.patch)(patch);
            }
        }
        Ok(PatchResult::NeedsResnapshot) => {
            resnapshot_tab(watched, "watch", emitters);
        }
        Err(error) => (emitters.error)(watched.target.path.clone(), error),
    }
}

pub(super) fn raw_mutations_for_event(
    event: &notify::Event,
    watched: &WatchedTab,
) -> Vec<RawMutation> {
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
pub(super) fn matches_watched_parent(changed_paths: &HashSet<PathBuf>, watched_path: &str) -> bool {
    let watched_parent = canonical_dir(Path::new(watched_path));
    changed_paths
        .iter()
        .filter_map(|changed_path| changed_path.parent())
        .any(|parent| canonical_dir(parent) == watched_parent)
}

#[cfg(feature = "test-utils")]
pub(super) enum PatchResult {
    Targeted {
        patch: DirPatch,
        snapshot: HashMap<String, DirectoryEntry>,
    },
    NeedsResnapshot,
}

#[cfg(feature = "test-utils")]
#[derive(PartialEq, Eq)]
enum EventApplication {
    Applied,
    NeedsResnapshot,
}

#[cfg(feature = "test-utils")]
impl SnapshotDraft {
    fn apply_changes<'path>(
        &mut self,
        target: &WatchTarget,
        paths: impl Iterator<Item = &'path PathBuf>,
    ) -> Result<(), String> {
        for path in paths {
            self.apply_change(target, path)?;
        }
        Ok(())
    }

    fn apply_event(
        &mut self,
        target: &WatchTarget,
        event: &notify::Event,
    ) -> Result<EventApplication, String> {
        if event.need_rescan() {
            return Ok(EventApplication::NeedsResnapshot);
        }
        let watched_parent = Path::new(&target.path);
        let direct_children = event
            .paths
            .iter()
            .filter(|path| is_direct_child(path, watched_parent));
        match event.kind {
            EventKind::Access(_) => {}
            EventKind::Create(_)
            | EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Metadata(_)) => {
                self.apply_changes(target, direct_children)?;
            }
            EventKind::Remove(_) => direct_children.for_each(|path| self.remove(path)),
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                return self.apply_rename(target, event);
            }
            EventKind::Any
            | EventKind::Other
            | EventKind::Modify(ModifyKind::Name(_) | ModifyKind::Any | ModifyKind::Other) => {
                return Ok(EventApplication::NeedsResnapshot);
            }
        }
        Ok(EventApplication::Applied)
    }

    fn apply_rename(
        &mut self,
        target: &WatchTarget,
        event: &notify::Event,
    ) -> Result<EventApplication, String> {
        if !is_paired_rename(event) {
            return Ok(EventApplication::NeedsResnapshot);
        }
        let watched_parent = Path::new(&target.path);
        let from = &event.paths[0];
        let to = &event.paths[1];
        if is_direct_child(from, watched_parent) {
            self.remove(from);
        }
        if is_direct_child(to, watched_parent) {
            self.apply_change(target, to)?;
        }
        Ok(EventApplication::Applied)
    }
}

#[cfg(feature = "test-utils")]
pub(super) fn patch_for_events(
    target: &WatchTarget,
    previous: &HashMap<String, DirectoryEntry>,
    events: &[notify::Event],
) -> Result<PatchResult, String> {
    let mut draft = SnapshotDraft::from_previous(previous);
    for event in events {
        if draft.apply_event(target, event)? == EventApplication::NeedsResnapshot {
            return Ok(PatchResult::NeedsResnapshot);
        }
    }

    Ok(PatchResult::Targeted {
        patch: DirPatch {
            tab_id: target.tab_id.clone(),
            path: target.path.clone(),
            reason: "watch".to_string(),
            changed: draft.changed,
            removed: draft.removed,
        },
        snapshot: draft.next,
    })
}

/// Both the event path's parent and the watched parent are canonicalized so
/// symlinked and canonical forms of the same directory compare equal across
/// platforms (see [`canonical_dir`]).
pub(super) fn is_direct_child(path: &Path, watched_parent: &Path) -> bool {
    match path.parent() {
        Some(parent) => canonical_dir(parent) == canonical_dir(watched_parent),
        None => false,
    }
}

#[cfg(feature = "test-utils")]
#[inline(never)]
pub(super) fn first_error_path(error: &notify::Error) -> String {
    match error.paths.first() {
        Some(path) => path.to_string_lossy().into_owned(),
        None => String::new(),
    }
}

#[cfg(feature = "test-utils")]
pub fn handle_debounce_result_for_tests(
    runtime: &WatchRuntime,
    batch: Vec<Result<notify::Event, notify::Error>>,
    emitters: WatchEmitters,
) {
    process_results(&runtime.tabs, batch, &emitters);
}

#[cfg(feature = "test-utils")]
pub fn noop_watch_error_for_tests(path: String, error: String) {
    noop_watch_error(path, error);
}

#[cfg(feature = "test-utils")]
pub fn canonical_dir_for_tests(path: &Path) -> PathBuf {
    canonical_dir(path)
}

#[cfg(feature = "test-utils")]
pub fn first_error_path_for_tests(error: &notify::Error) -> String {
    first_error_path(error)
}

/// Test-only wrapper around the private `raw_mutations_for_event` so
/// coverage/behavior tests can exercise the real `notify::Event` ->
/// `RawMutation` classification (Access/Remove/Unresolved/Changed, direct-
/// child filtering, `need_rescan`) without needing a live OS watcher thread
/// to deliver the event. `watch_id`/`target` are the same fields
/// `WatchedTab` stores; this constructs one internally since `WatchedTab`
/// itself is module-private.
#[cfg(feature = "test-utils")]
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
