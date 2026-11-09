use std::any::Any;
use std::panic;
use std::sync::Arc;
use std::thread;

use file_explorer_lib::logging::panic::{
    install_panic_logging, NON_TEXT_PAYLOAD, PANIC_TARGET, UNNAMED_THREAD,
};
use file_explorer_lib::logging::{current_local_date, read_current_day_logs, FileLogger, LogLevel};
use tempfile::tempdir;

const WORKER_NAME: &str = "size-index";
const NON_TEXT_VALUE: u32 = 7;

fn join_panicking(worker: thread::JoinHandle<()>) -> Box<dyn Any + Send> {
    worker.join().expect_err("worker panics")
}

#[test]
fn panics_are_logged_with_thread_location_message_and_backtrace() {
    let dir = tempdir().expect("dir");
    let logger = FileLogger::new(dir.path(), LogLevel::Error).expect("logger");
    install_panic_logging(Arc::clone(&logger));

    let named = thread::Builder::new()
        .name(WORKER_NAME.to_string())
        .spawn(|| panic!("index exploded"))
        .expect("spawn");
    join_panicking(named);
    let formatted_value = NON_TEXT_VALUE;
    join_panicking(thread::spawn(move || {
        panic!("slot {formatted_value} missing")
    }));
    join_panicking(thread::spawn(|| panic::panic_any(NON_TEXT_VALUE)));
    let _ = panic::take_hook();

    let entries = read_current_day_logs(dir.path(), current_local_date());
    assert_eq!(entries.len(), 3);
    assert!(entries.iter().all(|entry| entry.level == "error"));
    assert!(entries.iter().all(|entry| entry.target == PANIC_TARGET));
    let named_report = &entries[0].message;
    assert!(named_report.starts_with(&format!("thread '{WORKER_NAME}' panicked at ")));
    assert!(named_report.contains("logging_panic_integration.rs"));
    assert!(named_report.contains("index exploded"));
    assert!(named_report.lines().count() > 1);
    let unnamed_report = &entries[1].message;
    assert!(unnamed_report.starts_with(&format!("thread '{UNNAMED_THREAD}' panicked at ")));
    assert!(unnamed_report.contains(&format!("slot {NON_TEXT_VALUE} missing")));
    assert!(entries[2].message.contains(NON_TEXT_PAYLOAD));
}
