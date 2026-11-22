pub mod exclusions;
pub mod names;
pub mod persist;
#[cfg(target_os = "macos")]
pub mod platform;
pub mod segment;
pub mod tree;
pub mod verified;

#[cfg(target_os = "macos")]
mod compact;
#[cfg(target_os = "macos")]
mod grafts;
#[cfg(target_os = "macos")]
mod host;
#[cfg(target_os = "macos")]
mod lanes;
#[cfg(target_os = "macos")]
mod machine;
#[cfg(target_os = "macos")]
mod privacy;
#[cfg(target_os = "macos")]
mod scan_batch;
#[cfg(target_os = "macos")]
mod service;
#[cfg(target_os = "macos")]
mod snapshot;
#[cfg(target_os = "macos")]
mod split;
#[cfg(target_os = "macos")]
mod status;
#[cfg(target_os = "macos")]
mod subscriptions;

use std::path::PathBuf;

use names::FileNameStore;
use tree::FolderSizeTree;

#[cfg(target_os = "macos")]
pub use host::{IndexHost, IndexHostSetup, IndexOptionsFactory};
#[cfg(all(target_os = "macos", feature = "test-utils"))]
pub use service::LaneSnapshot;
#[cfg(all(target_os = "macos", not(feature = "test-utils")))]
pub use service::MacIndexSetup;
#[cfg(target_os = "macos")]
pub use service::{
    Clock, FOREST_BATCH_RECORDS, FolderSizeIndex, HEAP_BATCH_RECORDS, IndexOptions,
    READMIT_EVERY_DIRS, SNAPSHOT_FILE, SizeEmitter, StatusEmitter, TreeUnavailable, WalkBatching,
    names_path_beside,
};

pub struct IndexRead<'view> {
    pub tree: &'view FolderSizeTree,
    pub names: &'view FileNameStore,
    pub blocked: &'view [PathBuf],
    pub generation: u64,
}
