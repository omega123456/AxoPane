pub mod exclusions;
pub mod names;
pub mod persist;
#[cfg(target_os = "macos")]
pub mod platform;
pub mod segment;
pub mod tree;

#[cfg(target_os = "macos")]
mod host;
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
    names_path_beside, Clock, FolderSizeIndex, IndexOptions, SizeEmitter, StatusEmitter,
    TreeUnavailable, READMIT_EVERY_DIRS, SNAPSHOT_FILE,
};

pub struct IndexRead<'view> {
    pub tree: &'view FolderSizeTree,
    pub names: &'view FileNameStore,
    pub blocked: &'view [PathBuf],
}
