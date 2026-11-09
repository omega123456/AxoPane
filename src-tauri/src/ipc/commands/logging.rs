use crate::persist::{PersistedStore, PersistenceState};
#[cfg(not(feature = "test-utils"))]
use std::sync::Arc;

#[cfg(not(feature = "test-utils"))]
use super::latency_filesystem;
use crate::ipc::types::{LogFrontendRequest, SetLogLevelRequest};
use tauri::State;

pub fn frontend_log_level(level: &str) -> log::Level {
    match level {
        "error" => log::Level::Error,
        "warn" => log::Level::Warn,
        "debug" | "trace" => log::Level::Debug,
        _ => log::Level::Info,
    }
}

pub fn format_frontend_log(category: Option<&str>, message: &str, details: Option<&str>) -> String {
    let category = category.unwrap_or("frontend");
    match details {
        Some(details) => format!("[{category}] {message} {details}"),
        None => format!("[{category}] {message}"),
    }
}

#[tauri::command]
pub fn log_frontend(payload: LogFrontendRequest) {
    let line = format_frontend_log(
        payload.category.as_deref(),
        &payload.message,
        payload.details.as_deref(),
    );
    log::log!(frontend_log_level(&payload.level), "{line}");
}

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
