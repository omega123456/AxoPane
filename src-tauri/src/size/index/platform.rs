//! The folder-size index's only contact with macOS services: the FSEvents
//! stream, event IDs and the volume UUID, the OS build, power and thermal
//! notifications, the Full Disk Access probe and the home directory.
//!
//! Under `feature = "test-utils"` only [`FakePlatform`] exists, so tests
//! never reach a real OS service.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::persist::ChangeFlags;

/// Where the Data volume is mounted; FSEvents may report paths under it.
pub const DATA_VOLUME_PREFIX: &str = "/System/Volumes/Data";

/// One directory-level FSEvents record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEvent {
    pub path: PathBuf,
    pub id: u64,
    pub flags: ChangeFlags,
    /// Marks the end of history replay; carries no change.
    pub history_done: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSignal {
    WillSleep,
    DidWake,
    /// Current Low Power Mode and serious-or-critical thermal state.
    Conditions {
        low_power: bool,
        thermal_pressure: bool,
    },
}

pub type EventSink = Arc<dyn Fn(Vec<RawEvent>) + Send + Sync>;
pub type PowerSink = Arc<dyn Fn(PowerSignal) + Send + Sync>;
/// A running event stream; dropping it stops the stream.
pub type StreamHandle = Box<dyn Send>;

pub trait IndexPlatform: Send + Sync {
    fn current_event_id(&self) -> u64;
    fn volume_uuid(&self, path: &Path) -> Option<String>;
    /// The macOS build (`kern.osversion`); a change means a rescan.
    fn os_build(&self) -> Option<String>;
    /// Starts a directory-level stream over `roots` (minus `excluded`),
    /// replaying from `since` when given. `None` if it could not start.
    fn start_stream(
        &self,
        roots: &[PathBuf],
        excluded: &[PathBuf],
        since: Option<u64>,
        sink: EventSink,
    ) -> Option<StreamHandle>;
    /// Non-prompting probe; any failure means no Full Disk Access.
    fn full_disk_access(&self) -> bool;
    /// Starts power observation and returns the current conditions.
    fn observe_power(&self, sink: PowerSink) -> PowerSignal;
}

/// Maps a `/System/Volumes/Data`-prefixed path onto its firmlinked,
/// pane-visible path.
pub fn normalize(path: &Path) -> PathBuf {
    match path.strip_prefix(DATA_VOLUME_PREFIX) {
        Ok(rest) if rest.components().next().is_some() => Path::new("/").join(rest),
        _ => path.to_path_buf(),
    }
}

/// Folders background work never reads until visited.
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

/// Paths left out of the stream over `/`: other volumes and devfs, then the
/// high-churn locations. `/System/Volumes` stays in, because
/// Data-volume changes may be reported under [`DATA_VOLUME_PREFIX`]; the
/// index drops the rest of it after [`normalize`]. FSEvents takes at most
/// eight exclusions.
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
