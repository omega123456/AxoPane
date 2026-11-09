use std::sync::Arc;

use tauri::{AppHandle, Emitter};

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
