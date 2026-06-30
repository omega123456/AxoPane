//! The `read_logs` / `set_log_level` IPC commands and the shared
//! `apply_log_level` core (level applied to the live logger and persisted).

use std::fs;
use std::sync::Arc;

use file_explorer_lib::ipc::commands::{apply_log_level, read_logs, set_log_level};
use file_explorer_lib::logging::{
    current_local_date, daily_log_file_name, format_log_line, FileLogger, LogLevel, LoggingState,
};
use file_explorer_lib::ipc::types::SetLogLevelRequest;
use file_explorer_lib::persist::PersistenceState;
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::Manager;
use tempfile::{tempdir, TempDir};

type MockApp = tauri::App<tauri::test::MockRuntime>;

fn build_app() -> (TempDir, TempDir, Arc<FileLogger>, MockApp) {
    let log_dir = tempdir().expect("log dir");
    let config_dir = tempdir().expect("config dir");
    let logger = FileLogger::new(log_dir.path(), LogLevel::Info).expect("logger");
    let persistence = PersistenceState::load(config_dir.path()).expect("persistence");

    let app = mock_builder()
        .manage(LoggingState {
            dir: log_dir.path().to_path_buf(),
            logger: Arc::clone(&logger),
        })
        .manage(persistence)
        .build(mock_context(noop_assets()))
        .expect("build app");

    (log_dir, config_dir, logger, app)
}

#[test]
fn apply_log_level_updates_logger_and_persists_config() {
    let log_dir = tempdir().expect("log dir");
    let config_dir = tempdir().expect("config dir");
    let logger = FileLogger::new(log_dir.path(), LogLevel::Info).expect("logger");
    let persistence = PersistenceState::load(config_dir.path()).expect("persistence");

    apply_log_level(&persistence.config, &logger, LogLevel::Debug);

    assert_eq!(logger.level(), LogLevel::Debug);
    assert_eq!(persistence.config.current().log_level, "debug");
}

#[test]
fn read_logs_returns_current_day_entries() {
    let (log_dir, _config_dir, _logger, app) = build_app();
    let today = current_local_date();
    let line = format_log_line("2026-06-30T00:00:00Z", LogLevel::Info, "core", "hello");
    fs::write(
        log_dir.path().join(daily_log_file_name(today)),
        format!("{line}\n"),
    )
    .expect("seed log");

    let entries = read_logs(app.state::<LoggingState>());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].message, "hello");
}

#[test]
fn set_log_level_command_applies_and_rejects_invalid() {
    let (_log_dir, _config_dir, logger, app) = build_app();

    set_log_level(
        SetLogLevelRequest {
            level: "debug".to_string(),
        },
        app.state::<LoggingState>(),
        app.state::<PersistenceState>(),
    )
    .expect("set level");
    assert_eq!(logger.level(), LogLevel::Debug);
    assert_eq!(
        app.state::<PersistenceState>().config.current().log_level,
        "debug"
    );

    let error = set_log_level(
        SetLogLevelRequest {
            level: "verbose".to_string(),
        },
        app.state::<LoggingState>(),
        app.state::<PersistenceState>(),
    )
    .expect_err("invalid level rejected");
    assert!(error.contains("verbose"));
}
