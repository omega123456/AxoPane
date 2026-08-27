//! Covers the `log_frontend` command and its formatting/level helpers, which back
//! the frontend logger's IPC sink (`src/lib/app-log-commands.ts`).

use file_explorer_lib::ipc::commands::{format_frontend_log, frontend_log_level, log_frontend};
use file_explorer_lib::ipc::types::LogFrontendRequest;

#[test]
fn maps_known_levels_and_folds_trace_into_debug() {
    assert_eq!(frontend_log_level("error"), log::Level::Error);
    assert_eq!(frontend_log_level("warn"), log::Level::Warn);
    assert_eq!(frontend_log_level("info"), log::Level::Info);
    assert_eq!(frontend_log_level("debug"), log::Level::Debug);
    assert_eq!(frontend_log_level("trace"), log::Level::Debug);
}

#[test]
fn unknown_level_defaults_to_info() {
    assert_eq!(frontend_log_level("verbose"), log::Level::Info);
    assert_eq!(frontend_log_level(""), log::Level::Info);
}

#[test]
fn formats_line_with_explicit_category_and_details() {
    let line = format_frontend_log(Some("ipc"), "list_dir failed", Some(r#"{"path":"C:\\"}"#));
    assert_eq!(line, r#"[ipc] list_dir failed {"path":"C:\\"}"#);
}

#[test]
fn formats_line_defaulting_category_and_omitting_absent_details() {
    let line = format_frontend_log(None, "ready", None);
    assert_eq!(line, "[frontend] ready");
}

#[test]
fn log_frontend_accepts_a_request_without_panicking() {
    // No global logger is installed in tests, so this exercises the command body
    // (level mapping + line formatting + dispatch to `log::log!`) as a no-op sink.
    log_frontend(LogFrontendRequest {
        level: "info".to_string(),
        message: "hello".to_string(),
        category: Some("frontend".to_string()),
        details: Some(r#"{"a":1}"#.to_string()),
    });

    log_frontend(LogFrontendRequest {
        level: "warn".to_string(),
        message: "no details".to_string(),
        category: None,
        details: None,
    });
}
