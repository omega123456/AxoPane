use std::sync::Arc;

use tauri::{AppHandle, Emitter};

/// A callback that forwards each payload to the webview as `event`.
pub(super) fn frontend_emitter<Payload>(
    handle: &AppHandle,
    event: &'static str,
) -> Arc<dyn Fn(Payload) + Send + Sync>
where
    Payload: serde::Serialize + Clone + 'static,
{
    let handle = handle.clone();
    Arc::new(move |payload| {
        let _ = handle.emit(event, payload);
    })
}

/// Live folder-size updates, converted to the `size://state` event shape.
#[cfg(target_os = "macos")]
pub(super) fn size_state_emitter(handle: &AppHandle) -> crate::size::index::SizeEmitter {
    let handle = handle.clone();
    Arc::new(move |updates| {
        let batch = updates
            .into_iter()
            .map(crate::ipc::commands::size_state_event_from_update)
            .collect::<Vec<_>>();
        let _ = handle.emit(crate::ipc::events::SIZE_STATE, batch);
    })
}
