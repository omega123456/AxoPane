use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use notify::event::{EventKind, ModifyKind, RenameMode};
use notify::{RecursiveMode, Watcher};

use super::classification::is_direct_child;
use super::{coordinator, RENAME_EVENT_PATH_COUNT};
use crate::error_text::StringifyError;

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
    receiver: crossbeam_channel::Receiver<Result<notify::Event, notify::Error>>,
    overflowed: Arc<AtomicBool>,
}

/// Begins buffering filesystem events for direct children of `path`. Returns
/// a [`CaptureHandle`] whose buffered mutation paths can be read with
/// [`drain_captured_mutations`] once enumeration completes.
pub fn begin_capture(path: &Path) -> Result<CaptureHandle, String> {
    let (sender, receiver) = crossbeam_channel::bounded(coordinator::RAW_LANE_CAPACITY);
    let overflowed = Arc::new(AtomicBool::new(false));
    let overflowed_for_callback = Arc::clone(&overflowed);
    let mut watcher = notify::recommended_watcher(move |result| {
        // Non-blocking push: a full lane during capture marks the capture
        // overflowed (forcing a resnapshot) instead of blocking the
        // platform callback thread, matching the same non-blocking
        // contract the long-lived `WatchCoordinator` compactor lane uses.
        if sender.try_send(result).is_err() {
            overflowed_for_callback.store(true, Ordering::SeqCst);
        }
    })
    .stringify_err()?;
    watcher
        .watch(path, RecursiveMode::NonRecursive)
        .stringify_err()?;

    Ok(CaptureHandle {
        watcher,
        watched_path: path.to_path_buf(),
        receiver,
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

    if handle.overflowed.load(Ordering::SeqCst) {
        return None;
    }

    let mut mutated = HashSet::new();
    while let Ok(result) = handle.receiver.try_recv() {
        let event = result.ok()?;
        mutated.extend(captured_event_paths(&event, &handle.watched_path)?);
    }

    Some(mutated)
}

/// `None` means the event cannot be trusted as a precise path list.
fn captured_event_paths(event: &notify::Event, watched_path: &Path) -> Option<Vec<PathBuf>> {
    if event.need_rescan() {
        return None;
    }
    match event.kind {
        EventKind::Access(_) => Some(Vec::new()),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => renamed_paths(event),
        EventKind::Any
        | EventKind::Other
        | EventKind::Modify(ModifyKind::Name(_) | ModifyKind::Any | ModifyKind::Other) => None,
        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_) => Some(
            event
                .paths
                .iter()
                .filter(|changed_path| is_direct_child(changed_path, watched_path))
                .cloned()
                .collect(),
        ),
    }
}

fn renamed_paths(event: &notify::Event) -> Option<Vec<PathBuf>> {
    if event.paths.len() < RENAME_EVENT_PATH_COUNT {
        return None;
    }
    Some(vec![event.paths[0].clone(), event.paths[1].clone()])
}

#[cfg(feature = "test-utils")]
pub fn capture_handle_for_tests(path: &Path) -> Result<CaptureHandle, String> {
    begin_capture(path)
}

#[cfg(feature = "test-utils")]
pub fn drain_captured_mutations_for_tests(handle: CaptureHandle) -> Option<HashSet<PathBuf>> {
    drain_captured_mutations(handle)
}
