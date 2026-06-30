//! Pure helpers of the file logging stack: level mapping, daily file naming,
//! retention pruning, line formatting/parsing, and current-day reading.

use std::fs;

use file_explorer_lib::logging::{
    current_local_date, current_local_datetime, daily_log_file_name, escape_message,
    format_log_line, format_timestamp, parse_log_file_date, parse_log_line, prune_old_logs,
    read_current_day_logs, unescape_message, LogLevel, RETENTION_DAYS,
};
use tempfile::tempdir;
use time::{Date, Month, OffsetDateTime, Time};

fn date(year: i32, month: Month, day: u8) -> Date {
    Date::from_calendar_date(year, month, day).expect("valid date")
}

#[test]
fn level_parse_is_case_insensitive_and_rejects_unknown() {
    assert_eq!(LogLevel::parse(" Error "), Some(LogLevel::Error));
    assert_eq!(LogLevel::parse("WARN"), Some(LogLevel::Warn));
    assert_eq!(LogLevel::parse("info"), Some(LogLevel::Info));
    assert_eq!(LogLevel::parse("debug"), Some(LogLevel::Debug));
    assert_eq!(LogLevel::parse("trace"), Some(LogLevel::Trace));
    assert_eq!(LogLevel::parse("verbose"), None);
    assert_eq!(LogLevel::parse(""), None);
}

#[test]
fn level_string_and_token_and_filter_round_trip() {
    for level in [
        LogLevel::Error,
        LogLevel::Warn,
        LogLevel::Info,
        LogLevel::Debug,
        LogLevel::Trace,
    ] {
        assert_eq!(LogLevel::parse(level.as_str()), Some(level));
        assert_eq!(level.token(), level.as_str().to_uppercase());
        assert_eq!(LogLevel::from_index(level as u8), level);
    }
    assert_eq!(LogLevel::Error.to_level_filter(), log::LevelFilter::Error);
    assert_eq!(LogLevel::Trace.to_level_filter(), log::LevelFilter::Trace);
}

#[test]
fn level_ordering_is_severity_descending() {
    assert!(LogLevel::Error < LogLevel::Warn);
    assert!(LogLevel::Warn < LogLevel::Info);
    assert!(LogLevel::Info < LogLevel::Debug);
    assert!(LogLevel::Debug < LogLevel::Trace);
}

#[test]
fn level_maps_from_log_crate_levels() {
    assert_eq!(LogLevel::from_log_level(log::Level::Error), LogLevel::Error);
    assert_eq!(LogLevel::from_log_level(log::Level::Warn), LogLevel::Warn);
    assert_eq!(LogLevel::from_log_level(log::Level::Info), LogLevel::Info);
    assert_eq!(LogLevel::from_log_level(log::Level::Debug), LogLevel::Debug);
    assert_eq!(LogLevel::from_log_level(log::Level::Trace), LogLevel::Trace);
}

#[test]
fn from_index_defaults_unknown_to_info() {
    assert_eq!(LogLevel::from_index(2), LogLevel::Info);
    assert_eq!(LogLevel::from_index(99), LogLevel::Info);
}

#[test]
fn daily_file_name_and_parse_round_trip() {
    let day = date(2026, Month::June, 7);
    assert_eq!(daily_log_file_name(day), "axopane-2026-06-07.log");
    assert_eq!(parse_log_file_date("axopane-2026-06-07.log"), Some(day));
}

#[test]
fn parse_log_file_date_rejects_non_daily_names() {
    assert_eq!(parse_log_file_date("notes.txt"), None);
    assert_eq!(parse_log_file_date("axopane-2026-06.log"), None);
    assert_eq!(parse_log_file_date("axopane-2026-06-07-1.log"), None);
    assert_eq!(parse_log_file_date("axopane-2026-13-07.log"), None);
    assert_eq!(parse_log_file_date("axopane-bad-mm-dd.log"), None);
    assert_eq!(parse_log_file_date("axopane-2026-06-07.txt"), None);
}

#[test]
fn prune_deletes_only_files_outside_retention_window() {
    let dir = tempdir().expect("dir");
    let today = date(2026, Month::June, 30);
    let keep_today = daily_log_file_name(today);
    let keep_edge = daily_log_file_name(today - time::Duration::days(RETENTION_DAYS - 1));
    let drop_old = daily_log_file_name(today - time::Duration::days(RETENTION_DAYS));
    let drop_older = daily_log_file_name(today - time::Duration::days(30));

    for name in [&keep_today, &keep_edge, &drop_old, &drop_older] {
        fs::write(dir.path().join(name), b"line").expect("write");
    }
    fs::write(dir.path().join("unrelated.txt"), b"x").expect("write junk");

    prune_old_logs(dir.path(), today, RETENTION_DAYS);

    assert!(dir.path().join(&keep_today).exists());
    assert!(dir.path().join(&keep_edge).exists());
    assert!(!dir.path().join(&drop_old).exists());
    assert!(!dir.path().join(&drop_older).exists());
    assert!(dir.path().join("unrelated.txt").exists());
}

#[test]
fn prune_ignores_missing_directory() {
    let dir = tempdir().expect("dir");
    let missing = dir.path().join("nope");
    prune_old_logs(&missing, date(2026, Month::June, 30), RETENTION_DAYS);
    assert!(!missing.exists());
}

#[test]
fn escape_unescape_round_trips_control_characters() {
    let raw = "line one\nline two\rtab\\done";
    let escaped = escape_message(raw);
    assert!(!escaped.contains('\n'));
    assert!(!escaped.contains('\r'));
    assert_eq!(unescape_message(&escaped), raw);
}

#[test]
fn unescape_preserves_dangling_and_unknown_escapes() {
    assert_eq!(unescape_message("ends\\"), "ends\\");
    assert_eq!(unescape_message("a\\tb"), "a\\tb");
}

#[test]
fn format_and_parse_line_round_trip_preserves_fields() {
    let line = format_log_line(
        "2026-06-30T12:00:00Z",
        LogLevel::Warn,
        "file_explorer_lib::ipc",
        "queue stalled :: retrying\nnext",
    );
    let entry = parse_log_line(&line).expect("parse");
    assert_eq!(entry.id, 0);
    assert_eq!(entry.timestamp, "2026-06-30T12:00:00Z");
    assert_eq!(entry.level, "warn");
    assert_eq!(entry.target, "file_explorer_lib::ipc");
    assert_eq!(entry.message, "queue stalled :: retrying\nnext");
}

#[test]
fn parse_line_rejects_malformed_input() {
    assert!(parse_log_line("").is_none());
    assert!(parse_log_line("no-brackets here").is_none());
    assert!(parse_log_line("2026 [BOGUS] t :: m").is_none());
    assert!(parse_log_line("2026 [INFO] no-separator-message").is_none());
    assert!(parse_log_line("2026 missing-bracket t :: m").is_none());
}

#[test]
fn read_current_day_skips_malformed_lines_and_assigns_ids() {
    let dir = tempdir().expect("dir");
    let today = date(2026, Month::June, 30);
    let contents = format!(
        "{}\nthis is junk\n{}\n",
        format_log_line("2026-06-30T00:00:00Z", LogLevel::Info, "a", "first"),
        format_log_line("2026-06-30T00:00:01Z", LogLevel::Error, "b", "second"),
    );
    fs::write(dir.path().join(daily_log_file_name(today)), contents).expect("write");

    let entries = read_current_day_logs(dir.path(), today);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].id, 0);
    assert_eq!(entries[0].message, "first");
    assert_eq!(entries[1].id, 1);
    assert_eq!(entries[1].level, "error");
}

#[test]
fn read_current_day_returns_empty_when_no_file() {
    let dir = tempdir().expect("dir");
    assert!(read_current_day_logs(dir.path(), date(2026, Month::June, 30)).is_empty());
}

#[test]
fn current_clock_helpers_are_consistent() {
    let now = current_local_datetime();
    assert_eq!(now.date(), current_local_date());
    let fixed = OffsetDateTime::new_utc(
        date(2026, Month::June, 30),
        Time::from_hms(12, 0, 0).expect("time"),
    );
    assert_eq!(format_timestamp(fixed), "2026-06-30T12:00:00Z");
}
