pub fn bootstrap_message() -> &'static str {
    "phase-1-common"
}

pub fn wait_until(
    timeout: std::time::Duration,
    failure_message: &str,
    mut condition: impl FnMut() -> bool,
) {
    let deadline = std::time::Instant::now() + timeout;
    while !condition() {
        assert!(std::time::Instant::now() < deadline, "{failure_message}");
        std::thread::yield_now();
    }
}
