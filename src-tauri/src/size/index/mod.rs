//! Folder-size index. The pure tree and persistence modules are
//! cross-platform; the macOS service builds on them.
//!
//! The service keeps one folder-only tree of `/`, minus other volumes,
//! `/System/Volumes` and devfs
//! ([`crate::size::fast_walk::INDEX_EXCLUDED_ROOTS`]), never crossing onto a
//! device other than the root's or the Data volume's. Symlinks are never
//! stored or followed; a size request through one is routed by its resolved
//! target. A coordination thread applies every tree mutation: scan records, single-directory re-reads driven by a recursive,
//! directory-level FSEvents stream, and privacy changes. Lookups take a
//! shared read lock on the caller's thread. Two worker threads walk with
//! [`crate::size::fast_walk`]: a gentle background lane (one thread,
//! background QoS, re-admitted every ~1,000 directories, paused by the power
//! gate and while a boost waits) and a boost lane that runs one user-visible
//! walk at a time, newest first.
//!
//! The stream is used only for size bookkeeping: it never emits `dir://patch`
//! and never drives pane or tree refresh.

pub mod persist;
#[cfg(target_os = "macos")]
pub mod platform;
pub mod tree;

#[cfg(target_os = "macos")]
mod lanes;
#[cfg(target_os = "macos")]
mod machine;
#[cfg(target_os = "macos")]
mod privacy;
#[cfg(target_os = "macos")]
mod service;
#[cfg(target_os = "macos")]
mod snapshot;
#[cfg(target_os = "macos")]
mod status;
#[cfg(target_os = "macos")]
mod subscriptions;

#[cfg(all(target_os = "macos", feature = "test-utils"))]
pub use service::LaneSnapshot;
#[cfg(target_os = "macos")]
pub use service::{
    Clock, FolderSizeIndex, IndexOptions, SizeEmitter, StatusEmitter, TreeUnavailable,
    READMIT_EVERY_DIRS, SNAPSHOT_FILE,
};
