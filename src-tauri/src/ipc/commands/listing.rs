use crate::directory_session::EnumerationAdmission;
use crate::fs;
use crate::ipc::command_services::NavigationServices;
use std::sync::Arc;

#[cfg(not(feature = "test-utils"))]
use super::latency_filesystem;
use super::logged_error;
use crate::ipc::types::{
    BeginNavigationRequest, BeginNavigationResponse, GetSessionRangeRequest, ListDirRequest,
    ListDirResponse, ListTreeChildrenRequest, ListTreeChildrenResponse, ReleaseSessionRequest,
    ReleaseSessionResponse, ReviseSessionViewRequest, SessionRangeResponse, SessionRejection,
};
use tauri::State;

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn list_dir(
    payload: ListDirRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<ListDirResponse, String> {
    let key = payload.path.clone();
    executor
        .latency_cancellable(
            crate::ipc::executor::LatencyRequest::new(key),
            move |cancel| list_dir_until_cancelled(&payload, cancel),
        )
        .await
}

#[cfg(not(feature = "test-utils"))]
fn list_dir_until_cancelled(
    payload: &ListDirRequest,
    cancel: &crate::ipc::executor::Cancellation,
) -> Result<ListDirResponse, String> {
    let outcome = fs::list_dir_with_cancellation(payload, || cancel.is_cancelled())
        .map_err(|error| list_dir_failure(&payload.path, error))?;
    match outcome {
        fs::ListDirOutcome::Complete(response) => Ok(response),
        fs::ListDirOutcome::Cancelled => Err(LIST_DIR_CANCELLED.to_string()),
    }
}

#[cfg(not(feature = "test-utils"))]
const LIST_DIR_CANCELLED: &str = "directory listing cancelled";

fn list_dir_failure(path: &str, error: impl std::fmt::Display) -> String {
    logged_error(format!("Failed to load \"{path}\": {error}"))
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn list_dir(payload: ListDirRequest) -> Result<ListDirResponse, String> {
    list_dir_impl(payload)
}

#[cfg(feature = "test-utils")]
fn list_dir_impl(payload: ListDirRequest) -> Result<ListDirResponse, String> {
    fs::list_dir(&payload).map_err(|error| list_dir_failure(&payload.path, error))
}

/// Establishes (or replaces) the directory session for a pane.
#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn begin_directory_session(
    payload: BeginNavigationRequest,
    services: NavigationServices<'_>,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<BeginNavigationResponse, String> {
    let resource_key = services.registry.resolve_resource_key(&payload.path);
    let key = resource_key.clone().unwrap_or_else(|| payload.path.clone());
    let sessions = Arc::clone(services.sessions.inner());
    executor
        .latency(key, move || {
            let admission = EnumerationAdmission {
                coordinator: None,
                resource_key,
            };
            sessions.begin_navigation(payload, admission)
        })
        .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn begin_directory_session(
    payload: BeginNavigationRequest,
    services: NavigationServices<'_>,
    coordinator: State<'_, Arc<crate::resource_coordinator::ResourceCoordinator>>,
) -> Result<BeginNavigationResponse, String> {
    let resource_key = services.registry.resolve_resource_key(&payload.path);
    let admission = EnumerationAdmission {
        coordinator: Some(&coordinator),
        resource_key,
    };
    services.sessions.begin_navigation(payload, admission)
}

/// Idempotent page-index range fetch for an established v2 directory
/// session. Any valid page index may be retried for the same baseline; a
/// stale baseline field is rejected (not served).
///
/// The error is the typed [`SessionRejection`] (not a stringified message) so
/// the frontend can branch on `kind` (e.g. "stale, silently re-navigate" vs.
/// "page out of range, a real bug") instead of string-matching.
#[tauri::command]
pub fn get_directory_session_range(
    payload: GetSessionRangeRequest,
    state: State<'_, Arc<crate::directory_session::DirectorySessionService>>,
) -> Result<SessionRangeResponse, SessionRejection> {
    state.get_range(&payload)
}

/// Revises the sort/filter/show-hidden/item-count view of an already-active
/// v2 directory session without re-enumerating the filesystem (the frontend
/// calls this instead of re-running `begin_directory_session` for a plain
/// sort/filter change on the same directory). Returns the same shape as
/// `begin_directory_session` (a fresh baseline + first page) so callers can
/// treat "new session" and "revised view" uniformly.
#[tauri::command]
pub fn revise_directory_session_view(
    payload: ReviseSessionViewRequest,
    state: State<'_, Arc<crate::directory_session::DirectorySessionService>>,
) -> Result<BeginNavigationResponse, SessionRejection> {
    state.revise_view(payload)
}

/// Idempotent release of a v2 directory session. Safe to call more than once
/// (e.g. tab close racing app teardown) — a release for an
/// already-superseded/absent session simply reports `released: false`.
#[tauri::command]
pub fn release_directory_session(
    payload: ReleaseSessionRequest,
    state: State<'_, Arc<crate::directory_session::DirectorySessionService>>,
) -> ReleaseSessionResponse {
    state.release_session(&payload)
}

#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn list_tree_children(
    payload: ListTreeChildrenRequest,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<ListTreeChildrenResponse, String> {
    let key = payload.path.clone();
    latency_filesystem(&executor, key, move || list_tree_children_impl(payload)).await?
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn list_tree_children(
    payload: ListTreeChildrenRequest,
) -> Result<ListTreeChildrenResponse, String> {
    list_tree_children_impl(payload)
}

fn list_tree_children_impl(
    payload: ListTreeChildrenRequest,
) -> Result<ListTreeChildrenResponse, String> {
    fs::list_tree_children(&payload).map_err(|error| {
        logged_error(format!(
            "Failed to load tree children for \"{}\": {error}",
            payload.path
        ))
    })
}
