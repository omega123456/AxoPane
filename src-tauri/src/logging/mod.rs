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

struct LevelMetadata {
    level: LogLevel,
    name: &'static str,
    token: &'static str,
    filter: log::LevelFilter,
    log_level: log::Level,
}

const LEVEL_METADATA_BY_DISCRIMINANT: [LevelMetadata; 5] = [
    LevelMetadata {
        level: LogLevel::Error,
        name: "error",
        token: "ERROR",
        filter: log::LevelFilter::Error,
        log_level: log::Level::Error,
    },
    LevelMetadata {
        level: LogLevel::Warn,
        name: "warn",
        token: "WARN",
        filter: log::LevelFilter::Warn,
        log_level: log::Level::Warn,
    },
    LevelMetadata {
        level: LogLevel::Info,
        name: "info",
        token: "INFO",
        filter: log::LevelFilter::Info,
        log_level: log::Level::Info,
    },
    LevelMetadata {
        level: LogLevel::Debug,
        name: "debug",
        token: "DEBUG",
        filter: log::LevelFilter::Debug,
        log_level: log::Level::Debug,
    },
    LevelMetadata {
        level: LogLevel::Trace,
        name: "trace",
        token: "TRACE",
        filter: log::LevelFilter::Trace,
        log_level: log::Level::Trace,
    },
];

const FALLBACK_LEVEL: LogLevel = LogLevel::Info;

impl LogLevel {
    fn metadata(self) -> &'static LevelMetadata {
        &LEVEL_METADATA_BY_DISCRIMINANT[self as usize]
    }

    fn find(predicate: impl Fn(&LevelMetadata) -> bool) -> Option<Self> {
        LEVEL_METADATA_BY_DISCRIMINANT
            .iter()
            .find(|metadata| predicate(metadata))
            .map(|metadata| metadata.level)
    }

    /// Parse a case-insensitive level name. Returns `None` for unknown values.
    pub fn parse(value: &str) -> Option<Self> {
        let name = value.trim().to_ascii_lowercase();
        Self::find(|metadata| metadata.name == name)
    }

    /// Lowercase wire name (matches the frontend `LogLevel` union).
    pub fn as_str(self) -> &'static str {
        self.metadata().name
    }

    /// Uppercase token written into the log line (`[INFO]`).
    pub fn token(self) -> &'static str {
        self.metadata().token
    }

    /// The global `log` filter that admits this level and everything more severe.
    pub fn to_level_filter(self) -> log::LevelFilter {
        self.metadata().filter
    }

    /// Map a `log::Level` onto our level enum.
    pub fn from_log_level(level: log::Level) -> Self {
        Self::find(|metadata| metadata.log_level == level).unwrap_or(FALLBACK_LEVEL)
    }

    /// Round-trip through the `u8` discriminant stored in the logger's atomic.
    pub fn from_index(index: u8) -> Self {
        LEVEL_METADATA_BY_DISCRIMINANT
            .get(usize::from(index))
            .map_or(FALLBACK_LEVEL, |metadata| metadata.level)
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
#[derive(Clone)]
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
