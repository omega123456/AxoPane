mod clipboard;
mod entries;
mod icons;
mod item_counts;
mod listing;
mod logging;
mod native_menu;
mod persistence;
mod queue;
mod search;
mod shell;
mod sizes;
mod trash;
mod volumes;
mod watch;

pub use clipboard::*;
pub use entries::*;
pub use icons::*;
pub use item_counts::*;
pub use listing::*;
pub use logging::*;
pub use native_menu::*;
pub use persistence::*;
pub use queue::*;
pub use search::*;
pub use shell::*;
pub use sizes::*;
pub use trash::*;
pub use volumes::*;
pub use watch::*;

/// Runs bounded, single-request filesystem work through the owned IPC
/// latency executor. Command handlers only validate/capture payloads before
/// entering this boundary; the executor has a fixed queue/workers, shared
/// coordinator admission, deadline, and deterministic shutdown contract.
///
/// Long-running transfers deliberately do not use this helper; they belong to
/// `OpsService`, which owns cancellation, progress and shutdown instead.
#[cfg(not(feature = "test-utils"))]
async fn latency_filesystem<T: Send + 'static>(
    executor: &crate::ipc::executor::IpcExecutor,
    resource_key: String,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    executor.latency(resource_key, move || Ok(work())).await
}

/// Variant for bounded mutations that can observe cancellation before they
/// touch an OS API and at their own safe internal checkpoints.
#[cfg(not(feature = "test-utils"))]
async fn latency_filesystem_cancellable<T: Send + 'static>(
    executor: &crate::ipc::executor::IpcExecutor,
    resource_key: String,
    work: impl FnOnce(&crate::ipc::executor::Cancellation) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    executor
        .latency_cancellable(
            crate::ipc::executor::LatencyRequest::new(resource_key),
            work,
        )
        .await
}

#[cfg(not(feature = "test-utils"))]
struct CancellableMutation {
    resource_key: String,
    cancelled_message: &'static str,
}

/// Runs `work` through the latency executor unless the request was
/// cancelled before it reached a worker, in which case it fails with
/// `cancelled_message`.
#[cfg(not(feature = "test-utils"))]
async fn latency_filesystem_unless_cancelled<T: Send + 'static>(
    executor: &crate::ipc::executor::IpcExecutor,
    mutation: CancellableMutation,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let CancellableMutation {
        resource_key,
        cancelled_message,
    } = mutation;
    latency_filesystem_cancellable(executor, resource_key, move |cancel| {
        if cancel.is_cancelled() {
            return Err(cancelled_message.to_string());
        }
        work()
    })
    .await
}

/// Logs `message` as an error and hands it back for the IPC response.
fn logged_error(message: String) -> String {
    log::error!("{message}");
    message
}
