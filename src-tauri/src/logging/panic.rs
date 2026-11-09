use std::backtrace::Backtrace;
use std::panic::{self, PanicHookInfo};
use std::sync::Arc;
use std::thread;

use super::{current_local_datetime, FileLogger, LogLevel, LogMessage};

pub const PANIC_TARGET: &str = "panic";
pub const UNNAMED_THREAD: &str = "<unnamed>";
pub const NON_TEXT_PAYLOAD: &str = "<non-text panic payload>";
const UNKNOWN_LOCATION: &str = "<unknown location>";

pub fn install_panic_logging(logger: Arc<FileLogger>) {
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        record_panic(&logger, info);
        previous_hook(info);
    }));
}

fn record_panic(logger: &FileLogger, info: &PanicHookInfo<'_>) {
    let report = format!(
        "thread '{}' panicked at {}: {}\n{}",
        thread::current().name().unwrap_or(UNNAMED_THREAD),
        location_of(info),
        message_of(info),
        Backtrace::force_capture()
    );
    let entry = LogMessage {
        level: LogLevel::Error,
        target: PANIC_TARGET,
        message: &report,
    };
    logger.write_entry(current_local_datetime(), entry);
}

fn location_of(info: &PanicHookInfo<'_>) -> String {
    info.location()
        .map_or_else(|| UNKNOWN_LOCATION.to_string(), ToString::to_string)
}

fn message_of<'info>(info: &'info PanicHookInfo<'_>) -> &'info str {
    let payload = info.payload();
    if let Some(message) = payload.downcast_ref::<&str>() {
        return message;
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message;
    }
    NON_TEXT_PAYLOAD
}
