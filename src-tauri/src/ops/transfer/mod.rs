//! Capability-gated file-copy selection.
//!
//! Platform adapters only describe a native implementation when every fact it
//! relies on is confirmed. The actual byte transfer remains the portable,
//! cancellation-aware adapter under `test-utils`.

pub mod macos;
pub mod portable;
pub mod windows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferRequirements {
    pub sparse: bool,
    pub offload: bool,
    pub clone: bool,
    pub preserve_metadata: bool,
    pub preserve_link: bool,
    pub progress: bool,
    pub cancellation: bool,
}

impl Default for TransferRequirements {
    fn default() -> Self {
        Self {
            sparse: false,
            offload: false,
            clone: false,
            preserve_metadata: false,
            preserve_link: false,
            progress: true,
            cancellation: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortableReason {
    UnsupportedPlatform,
    ApiUnavailable,
    UnsupportedBuild,
    UnsupportedFilesystem,
    NetworkServerUnsupported,
    SparseUnsupported,
    OffloadUnsupported,
    CloneUnsupported,
    MetadataUnsupported,
    LinkUnsupported,
    ProgressUnsupported,
    CancellationUnsupported,
    NativeError(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterSelection {
    Portable(PortableReason),
    WindowsCopyFile2,
    MacosCopyfile,
}

pub trait TransferAdapter {
    fn select(&self, requirements: TransferRequirements) -> AdapterSelection;
}
