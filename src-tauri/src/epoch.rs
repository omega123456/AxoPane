use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn since_epoch(time: SystemTime) -> Duration {
    time.duration_since(UNIX_EPOCH).unwrap_or_default()
}

pub fn now_since_epoch() -> Duration {
    since_epoch(SystemTime::now())
}
