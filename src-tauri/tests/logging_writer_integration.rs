//! The `FileLogger` sink: construction, level gating, daily rollover with
//! retention pruning, and the `log::Log` trait surface.

use std::fs;

use file_explorer_lib::logging::{
    current_local_date, daily_log_file_name, read_current_day_logs, FileLogger, LogLevel,
};
use tempfile::tempdir;
use time::{Date, Month, OffsetDateTime, Time};

fn at(year: i32, month: Month, day: u8) -> OffsetDateTime {
    OffsetDateTime::new_utc(
        Date::from_calendar_date(year, month, day).expect("date"),
        Time::from_hms(12, 0, 0).expect("time"),
    )
}

#[test]
fn new_creates_directory_and_reports_initial_level() {
    let base = tempdir().expect("dir");
    let dir = base.path().join("logs");
    let logger = FileLogger::new(&dir, LogLevel::Warn).expect("logger");
    assert!(dir.exists());
    assert_eq!(logger.level(), LogLevel::Warn);
}

#[test]
fn set_level_changes_capture_gate() {
    let dir = tempdir().expect("dir");
    let logger = FileLogger::new(dir.path(), LogLevel::Info).expect("logger");

    assert!(logger.enabled_for(log::Level::Error));
    assert!(logger.enabled_for(log::Level::Info));
    assert!(!logger.enabled_for(log::Level::Debug));

    logger.set_level(LogLevel::Debug);
    assert_eq!(logger.level(), LogLevel::Debug);
    assert!(logger.enabled_for(log::Level::Debug));
    assert!(!logger.enabled_for(log::Level::Trace));

    logger.set_level(LogLevel::Error);
    assert!(!logger.enabled_for(log::Level::Warn));
}

#[test]
fn write_entry_rolls_files_per_day_and_prunes_stale() {
    let dir = tempdir().expect("dir");
    let day_one = at(2026, Month::June, 30);
    let day_two = at(2026, Month::July, 1);

    let stale = daily_log_file_name(day_one.date() - time::Duration::days(40));
    fs::write(dir.path().join(&stale), b"old").expect("seed stale");

    let logger = FileLogger::new(dir.path(), LogLevel::Trace).expect("logger");
    logger.write_entry(day_one, LogLevel::Info, "core", "first day");
    logger.write_entry(day_two, LogLevel::Warn, "core", "second day");

    assert!(dir
        .path()
        .join(daily_log_file_name(day_one.date()))
        .exists());
    assert!(dir
        .path()
        .join(daily_log_file_name(day_two.date()))
        .exists());
    assert!(!dir.path().join(&stale).exists(), "stale file pruned");

    let day_one_logs = read_current_day_logs(dir.path(), day_one.date());
    assert_eq!(day_one_logs.len(), 1);
    assert_eq!(day_one_logs[0].message, "first day");
}

#[test]
fn log_trait_respects_level_and_flush_is_safe() {
    let dir = tempdir().expect("dir");
    let logger = FileLogger::new(dir.path(), LogLevel::Info).expect("logger");

    let kept = log::Record::builder()
        .level(log::Level::Error)
        .target("ipc")
        .args(format_args!("kept message"))
        .build();
    let dropped = log::Record::builder()
        .level(log::Level::Debug)
        .target("ipc")
        .args(format_args!("dropped message"))
        .build();

    assert!(log::Log::enabled(&*logger, kept.metadata()));
    assert!(!log::Log::enabled(&*logger, dropped.metadata()));

    log::Log::log(&*logger, &kept);
    log::Log::log(&*logger, &dropped);
    log::Log::flush(&*logger);

    let entries = read_current_day_logs(dir.path(), current_local_date());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].message, "kept message");
    assert_eq!(entries[0].level, "error");
}
