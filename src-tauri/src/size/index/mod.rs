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
#[cfg(all(target_os = "macos", not(feature = "test-utils")))]
pub use service::MacIndexSetup;
#[cfg(target_os = "macos")]
pub use service::{
    Clock, FolderSizeIndex, IndexOptions, SizeEmitter, StatusEmitter, TreeUnavailable,
    READMIT_EVERY_DIRS, SNAPSHOT_FILE,
};
