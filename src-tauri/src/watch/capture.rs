use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use notify::event::{EventKind, ModifyKind, RenameMode};
use notify::{RecursiveMode, Watcher};

use super::classification::is_direct_child;
use super::{coordinator, is_paired_rename};
use crate::error_text::StringifyError;

pub struct CaptureHandle {
    watcher: notify::RecommendedWatcher,
    watched_path: PathBuf,
    receiver: crossbeam_channel::Receiver<Result<notify::Event, notify::Error>>,
    overflowed: Arc<AtomicBool>,
}

pub fn begin_capture(path: &Path) -> Result<CaptureHandle, String> {
    let (sender, receiver) = crossbeam_channel::bounded(coordinator::RAW_LANE_CAPACITY);
    let overflowed = Arc::new(AtomicBool::new(false));
    let overflowed_for_callback = Arc::clone(&overflowed);
    let mut watcher = notify::recommended_watcher(move |result| {
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
    if !is_paired_rename(event) {
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
