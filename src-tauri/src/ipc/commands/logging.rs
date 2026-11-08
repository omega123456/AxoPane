use crate::persist::{PersistedStore, PersistenceState};
#[cfg(not(feature = "test-utils"))]
use std::sync::Arc;

#[cfg(not(feature = "test-utils"))]
use super::latency_filesystem;
use crate::ipc::types::{LogFrontendRequest, SetLogLevelRequest};
use tauri::State;

/// Map a frontend level string to a [`log::Level`].
///
/// The frontend permits `trace`, which is folded into `Debug`. Unknown values
/// default to `Info`.
pub fn frontend_log_level(level: &str) -> log::Level {
    match level {
        "error" => log::Level::Error,
        "warn" => log::Level::Warn,
        "debug" | "trace" => log::Level::Debug,
        _ => log::Level::Info,
    }
}

/// Compose a single log line from a frontend log request. `category` defaults to
/// `frontend`; serialized `details` (when present) are appended to the message.
pub fn format_frontend_log(category: Option<&str>, message: &str, details: Option<&str>) -> String {
    let category = category.unwrap_or("frontend");
    match details {
        Some(details) => format!("[{category}] {message} {details}"),
        None => format!("[{category}] {message}"),
    }
}

/// Forward a frontend log line into the application logger (the custom file sink
/// in [`crate::logging`]).
///
/// The frontend logger (`app-log-commands.ts`) routes every line here so test
/// runs stay quiet (the line goes to the IPC sink, not the test console) and so
/// frontend diagnostics share the backend's stdout/webview log targets.
#[tauri::command]
pub fn log_frontend(payload: LogFrontendRequest) {
    let line = format_frontend_log(
        payload.category.as_deref(),
        &payload.message,
        payload.details.as_deref(),
    );
    log::log!(frontend_log_level(&payload.level), "{line}");
}

/// Apply a new minimum capture level: update the live logger and persist the
/// choice into the app config so it survives restarts. Pure core shared by the
/// command wrapper and tests.
pub fn apply_log_level(
    config: &PersistedStore<crate::persist::Config>,
    logger: &crate::logging::FileLogger,
    level: crate::logging::LogLevel,
) {
    logger.set_level(level);
    let mut current = config.current();
    current.log_level = level.as_str().to_string();
    config.replace(current);
}

/// Read and parse the current day's log file for the in-app log viewer.
#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub async fn read_logs(
    logging: State<'_, Arc<crate::logging::LoggingState>>,
    executor: State<'_, Arc<crate::ipc::executor::IpcExecutor>>,
) -> Result<Vec<crate::logging::LogEntry>, String> {
    let logging = Arc::clone(logging.inner());
    latency_filesystem(&executor, "logs".into(), move || {
        crate::logging::read_current_day_logs(&logging.dir, crate::logging::current_local_date())
    })
    .await
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn read_logs(
    logging: State<'_, crate::logging::LoggingState>,
) -> Vec<crate::logging::LogEntry> {
    crate::logging::read_current_day_logs(&logging.dir, crate::logging::current_local_date())
}

/// Set the minimum backend capture level (applied immediately and persisted).
#[cfg(not(feature = "test-utils"))]
#[tauri::command]
pub fn set_log_level(
    payload: SetLogLevelRequest,
    logging: State<'_, Arc<crate::logging::LoggingState>>,
    persistence: State<'_, PersistenceState>,
) -> Result<(), String> {
    let parsed = crate::logging::LogLevel::parse(&payload.level)
        .ok_or_else(|| format!("invalid log level: {}", payload.level))?;
    apply_log_level(&persistence.config, &logging.logger, parsed);
    Ok(())
}

#[cfg(feature = "test-utils")]
#[tauri::command]
pub fn set_log_level(
    payload: SetLogLevelRequest,
    logging: State<'_, crate::logging::LoggingState>,
    persistence: State<'_, PersistenceState>,
) -> Result<(), String> {
    let parsed = crate::logging::LogLevel::parse(&payload.level)
        .ok_or_else(|| format!("invalid log level: {}", payload.level))?;
    apply_log_level(&persistence.config, &logging.logger, parsed);
    Ok(())
}
