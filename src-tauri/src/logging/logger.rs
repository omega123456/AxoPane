//! The file-backed [`log::Log`] implementation and its global install.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use time::{Date, OffsetDateTime};

use super::{
    current_local_date, current_local_datetime, daily_log_file_name, format_log_line,
    format_timestamp, prune_old_logs, LogLevel, RETENTION_DAYS,
};

/// Append-only writer that keeps one open file per calendar day and rolls over
/// (pruning stale files) when the day changes.
struct DailyWriter {
    open: Option<(Date, File)>,
}

impl DailyWriter {
    fn new() -> Self {
        Self { open: None }
    }

    fn write_line(&mut self, dir: &Path, date: Date, line: &str) -> std::io::Result<()> {
        let needs_roll = !matches!(self.open, Some((open_date, _)) if open_date == date);
        if needs_roll {
            std::fs::create_dir_all(dir)?;
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join(daily_log_file_name(date)))?;
            self.open = Some((date, file));
            prune_old_logs(dir, date, RETENTION_DAYS);
        }
        if let Some((_, file)) = self.open.as_mut() {
            writeln!(file, "{line}")?;
            file.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) {
        if let Some((_, file)) = self.open.as_mut() {
            let _ = file.flush();
        }
    }
}

/// A `log::Log` sink that writes to a daily file and to stdout, with a
/// runtime-adjustable minimum level.
pub struct FileLogger {
    dir: PathBuf,
    level: AtomicU8,
    writer: Mutex<DailyWriter>,
}

impl FileLogger {
    /// Create the logger, ensuring the log directory exists and pruning any
    /// already-stale daily files. The returned handle can be shared (one copy
    /// installed globally, one kept in [`super::LoggingState`]).
    pub fn new(dir: &Path, level: LogLevel) -> std::io::Result<Arc<Self>> {
        std::fs::create_dir_all(dir)?;
        prune_old_logs(dir, current_local_date(), RETENTION_DAYS);
        Ok(Arc::new(Self {
            dir: dir.to_path_buf(),
            level: AtomicU8::new(level as u8),
            writer: Mutex::new(DailyWriter::new()),
        }))
    }

    /// The current minimum capture level.
    pub fn level(&self) -> LogLevel {
        LogLevel::from_index(self.level.load(Ordering::Relaxed))
    }

    /// Change the minimum capture level; takes effect immediately for both the
    /// file sink and the global `log` filter.
    pub fn set_level(&self, level: LogLevel) {
        self.level.store(level as u8, Ordering::Relaxed);
        log::set_max_level(level.to_level_filter());
    }

    /// Whether a record at `level` passes the current filter.
    pub fn enabled_for(&self, level: log::Level) -> bool {
        LogLevel::from_log_level(level) <= self.level()
    }

    /// Write a single entry with an injected timestamp (the deterministic core
    /// used by tests and by [`FileLogger::write_record`]).
    pub fn write_entry(&self, at: OffsetDateTime, level: LogLevel, target: &str, message: &str) {
        let line = format_log_line(&format_timestamp(at), level, target, message);
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_line(&self.dir, at.date(), &line);
        }
        println!("{line}");
    }

    /// Extract and write a `log::Record` using the wall clock.
    pub fn write_record(&self, record: &log::Record<'_>) {
        self.write_entry(
            current_local_datetime(),
            LogLevel::from_log_level(record.level()),
            record.target(),
            &record.args().to_string(),
        );
    }
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        self.enabled_for(metadata.level())
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            self.write_record(record);
        }
    }

    fn flush(&self) {
        if let Ok(mut writer) = self.writer.lock() {
            writer.flush();
        }
    }
}

/// Install `logger` as the process-global `log` sink and set the initial filter.
///
/// This is the single process-global side effect of the logging stack, so it is
/// excluded from the test build (which never installs a global logger), matching
/// the `lib::run` entrypoint carve-out.
#[cfg(not(feature = "test-utils"))]
pub fn install_global(logger: Arc<FileLogger>, level: LogLevel) {
    struct Global(Arc<FileLogger>);

    impl log::Log for Global {
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            self.0.enabled(metadata)
        }
        fn log(&self, record: &log::Record<'_>) {
            self.0.log(record);
        }
        fn flush(&self) {
            self.0.flush();
        }
    }

    log::set_max_level(level.to_level_filter());
    let _ = log::set_boxed_logger(Box::new(Global(logger)));
}
