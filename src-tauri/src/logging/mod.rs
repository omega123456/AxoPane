//! Application logging: a custom `log::Log` sink that writes one file per day
//! under the app config dir, prunes files older than [`RETENTION_DAYS`], and
//! exposes a runtime-reconfigurable minimum level plus a reader for the current
//! day's file (consumed by the in-app log viewer).
//!
//! The pure helpers here (level parsing, file naming, pruning, line
//! formatting/parsing, day reading) are unit-tested directly. The global logger
//! install lives in [`logger::install_global`] and is the only process-global
//! side effect, mirroring the `lib::run` entrypoint carve-out.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::{Date, Month, OffsetDateTime};

pub mod logger;

pub use logger::FileLogger;

/// Number of daily log files to keep (today plus the previous six days).
pub const RETENTION_DAYS: i64 = 7;

/// Separator between the target and the (escaped) message in a serialized log
/// line. The surrounding spaces disambiguate it from the `::` inside Rust module
/// paths used as targets.
const TARGET_MESSAGE_SEP: &str = " :: ";

/// Minimum capture level, ordered most-severe (`Error`) to least (`Trace`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    /// Parse a case-insensitive level name. Returns `None` for unknown values.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "error" => Some(Self::Error),
            "warn" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            "trace" => Some(Self::Trace),
            _ => None,
        }
    }

    /// Lowercase wire name (matches the frontend `LogLevel` union).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }

    /// Uppercase token written into the log line (`[INFO]`).
    pub fn token(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
            Self::Trace => "TRACE",
        }
    }

    /// The global `log` filter that admits this level and everything more severe.
    pub fn to_level_filter(self) -> log::LevelFilter {
        match self {
            Self::Error => log::LevelFilter::Error,
            Self::Warn => log::LevelFilter::Warn,
            Self::Info => log::LevelFilter::Info,
            Self::Debug => log::LevelFilter::Debug,
            Self::Trace => log::LevelFilter::Trace,
        }
    }

    /// Map a `log::Level` onto our level enum.
    pub fn from_log_level(level: log::Level) -> Self {
        match level {
            log::Level::Error => Self::Error,
            log::Level::Warn => Self::Warn,
            log::Level::Info => Self::Info,
            log::Level::Debug => Self::Debug,
            log::Level::Trace => Self::Trace,
        }
    }

    /// Round-trip through the `u8` discriminant stored in the logger's atomic.
    pub fn from_index(index: u8) -> Self {
        match index {
            0 => Self::Error,
            1 => Self::Warn,
            3 => Self::Debug,
            4 => Self::Trace,
            _ => Self::Info,
        }
    }
}

/// A single parsed log line surfaced to the viewer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    /// Stable per-file index (line number), used as the React key.
    pub id: usize,
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

/// Shared logging handle managed by Tauri: the log directory and the live logger
/// (for runtime level changes).
pub struct LoggingState {
    pub dir: PathBuf,
    pub logger: Arc<FileLogger>,
}

/// Current local date, falling back to UTC when the local offset is unavailable
/// (the `time` crate can refuse to read it on some multithreaded platforms).
pub fn current_local_date() -> Date {
    current_local_datetime().date()
}

/// Current local timestamp, with the same UTC fallback as [`current_local_date`].
pub fn current_local_datetime() -> OffsetDateTime {
    OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc())
}

/// Format a timestamp as RFC 3339 (the frontend parses it with `new Date`).
pub fn format_timestamp(at: OffsetDateTime) -> String {
    at.format(&Rfc3339).unwrap_or_default()
}

/// Daily file name for `date`, e.g. `axopane-2026-06-30.log`.
pub fn daily_log_file_name(date: Date) -> String {
    format!(
        "axopane-{:04}-{:02}-{:02}.log",
        date.year(),
        u8::from(date.month()),
        date.day(),
    )
}

/// Inverse of [`daily_log_file_name`]; `None` for anything that isn't a daily log.
pub fn parse_log_file_date(name: &str) -> Option<Date> {
    let stem = name.strip_prefix("axopane-")?.strip_suffix(".log")?;
    let mut parts = stem.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u8 = parts.next()?.parse().ok()?;
    let day: u8 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Date::from_calendar_date(year, Month::try_from(month).ok()?, day).ok()
}

/// Delete daily log files whose date is at least [`RETENTION_DAYS`] older than
/// `today`. Unparseable names and IO errors are ignored.
pub fn prune_old_logs(dir: &Path, today: Date, retention_days: i64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(date) = parse_log_file_date(name) else {
            continue;
        };
        if (today - date).whole_days() >= retention_days {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Escape a message so the whole entry stays on a single line.
pub fn escape_message(message: &str) -> String {
    message
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Inverse of [`escape_message`].
pub fn unescape_message(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut chars = message.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Compose a single, round-trippable log line.
pub fn format_log_line(timestamp: &str, level: LogLevel, target: &str, message: &str) -> String {
    format!(
        "{timestamp} [{}] {target}{TARGET_MESSAGE_SEP}{}",
        level.token(),
        escape_message(message),
    )
}

/// Parse a line produced by [`format_log_line`]. Returns `None` (and the caller
/// skips the line) when the shape doesn't match.
pub fn parse_log_line(line: &str) -> Option<LogEntry> {
    let (timestamp, rest) = line.split_once(' ')?;
    let rest = rest.strip_prefix('[')?;
    let (token, rest) = rest.split_once(']')?;
    let level = LogLevel::parse(token)?;
    let rest = rest.strip_prefix(' ')?;
    let (target, message) = rest.split_once(TARGET_MESSAGE_SEP)?;
    Some(LogEntry {
        id: 0,
        timestamp: timestamp.to_string(),
        level: level.as_str().to_string(),
        target: target.to_string(),
        message: unescape_message(message),
    })
}

/// Read and parse the current day's log file. Missing file or unreadable lines
/// yield an empty/partial result rather than an error.
pub fn read_current_day_logs(dir: &Path, today: Date) -> Vec<LogEntry> {
    let path = dir.join(daily_log_file_name(today));
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    contents
        .lines()
        .filter_map(parse_log_line)
        .enumerate()
        .map(|(index, mut entry)| {
            entry.id = index;
            entry
        })
        .collect()
}
