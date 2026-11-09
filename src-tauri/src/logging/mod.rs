use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::{Date, Month, OffsetDateTime};

pub mod logger;
pub mod panic;

pub use logger::FileLogger;

pub const RETENTION_DAYS: i64 = 7;

const TARGET_MESSAGE_SEP: &str = " :: ";

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

    pub fn parse(value: &str) -> Option<Self> {
        let name = value.trim().to_ascii_lowercase();
        Self::find(|metadata| metadata.name == name)
    }

    pub fn as_str(self) -> &'static str {
        self.metadata().name
    }

    pub fn token(self) -> &'static str {
        self.metadata().token
    }

    pub fn to_level_filter(self) -> log::LevelFilter {
        self.metadata().filter
    }

    pub fn from_log_level(level: log::Level) -> Self {
        Self::find(|metadata| metadata.log_level == level).unwrap_or(FALLBACK_LEVEL)
    }

    pub fn from_index(index: u8) -> Self {
        LEVEL_METADATA_BY_DISCRIMINANT
            .get(usize::from(index))
            .map_or(FALLBACK_LEVEL, |metadata| metadata.level)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: usize,
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

#[derive(Clone)]
pub struct LoggingState {
    pub dir: PathBuf,
    pub logger: Arc<FileLogger>,
}

pub fn current_local_date() -> Date {
    current_local_datetime().date()
}

pub fn current_local_datetime() -> OffsetDateTime {
    OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc())
}

pub fn format_timestamp(at: OffsetDateTime) -> String {
    at.format(&Rfc3339).unwrap_or_default()
}

pub fn daily_log_file_name(date: Date) -> String {
    format!(
        "axopane-{:04}-{:02}-{:02}.log",
        date.year(),
        u8::from(date.month()),
        date.day(),
    )
}

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
        let age_in_days = (today - date).whole_days();
        if age_in_days >= retention_days {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

pub fn escape_message(message: &str) -> String {
    message
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

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

#[derive(Clone, Copy)]
pub struct LogMessage<'message> {
    pub level: LogLevel,
    pub target: &'message str,
    pub message: &'message str,
}

pub fn format_log_line(timestamp: &str, record: LogMessage<'_>) -> String {
    let LogMessage {
        level,
        target,
        message,
    } = record;
    format!(
        "{timestamp} [{}] {target}{TARGET_MESSAGE_SEP}{}",
        level.token(),
        escape_message(message),
    )
}

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
