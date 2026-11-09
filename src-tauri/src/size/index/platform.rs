use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::persist::ChangeFlags;

pub const DATA_VOLUME_PREFIX: &str = "/System/Volumes/Data";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEvent {
    pub path: PathBuf,
    pub id: u64,
    pub flags: ChangeFlags,
    pub history_done: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSignal {
    WillSleep,
    DidWake,
    Conditions {
        low_power: bool,
        thermal_pressure: bool,
    },
}

pub type EventSink = Arc<dyn Fn(Vec<RawEvent>) + Send + Sync>;
pub type PowerSink = Arc<dyn Fn(PowerSignal) + Send + Sync>;
pub type StreamHandle = Box<dyn Send>;

#[derive(Clone, Copy)]
pub struct StreamRequest<'request> {
    pub roots: &'request [PathBuf],
    pub excluded: &'request [PathBuf],
    pub since: Option<u64>,
}

pub trait IndexPlatform: Send + Sync {
    fn current_event_id(&self) -> u64;
    fn volume_uuid(&self, path: &Path) -> Option<String>;
    fn os_build(&self) -> Option<String>;
    fn start_stream(&self, request: StreamRequest<'_>, sink: EventSink) -> Option<StreamHandle>;
    fn full_disk_access(&self) -> bool;
    fn observe_power(&self, sink: PowerSink) -> PowerSignal;
}

pub fn normalize(path: &Path) -> PathBuf {
    match path.strip_prefix(DATA_VOLUME_PREFIX) {
        Ok(rest) if rest.components().next().is_some() => Path::new("/").join(rest),
        _ => path.to_path_buf(),
    }
}

pub fn protected_roots(home: &Path) -> Vec<PathBuf> {
    [
        "Desktop",
        "Documents",
        "Downloads",
        "Library/Mobile Documents",
        "Library/CloudStorage",
        "Library/Containers",
        "Library/Group Containers",
    ]
    .iter()
    .map(|relative| home.join(relative))
    .collect()
}

pub fn stream_exclusions(home: &Path) -> Vec<PathBuf> {
    [
        PathBuf::from("/Volumes"),
        PathBuf::from("/dev"),
        PathBuf::from("/private/var/folders"),
        PathBuf::from("/private/var/db"),
        PathBuf::from("/private/var/log"),
        home.join("Library/Caches"),
    ]
    .to_vec()
}

#[cfg(feature = "test-utils")]
pub use fake::FakePlatform;

#[cfg(feature = "test-utils")]
#[path = "platform/fake.rs"]
mod fake;

#[cfg(not(feature = "test-utils"))]
pub use mac::{home_dir, MacPlatform};

#[cfg(not(feature = "test-utils"))]
#[path = "platform/mac.rs"]
mod mac;
