use super::{AdapterSelection, PortableReason, TransferAdapter, TransferRequirements};

#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsCapabilities {
    pub api_available: bool,
    pub supported_build: bool,
    pub local_filesystem: bool,
    pub server_supports_copyfile2: bool,
    pub sparse: bool,
    pub offload: bool,
    pub metadata: bool,
    pub reparse_links: bool,
    pub progress: bool,
    pub cancellation: bool,
}

impl TransferAdapter for WindowsCapabilities {
    fn select(&self, r: TransferRequirements) -> AdapterSelection {
        let reason = if !self.api_available {
            Some(PortableReason::ApiUnavailable)
        } else if !self.supported_build {
            Some(PortableReason::UnsupportedBuild)
        } else if !self.local_filesystem && !self.server_supports_copyfile2 {
            Some(PortableReason::NetworkServerUnsupported)
        } else if r.sparse && !self.sparse {
            Some(PortableReason::SparseUnsupported)
        } else if r.offload && !self.offload {
            Some(PortableReason::OffloadUnsupported)
        } else if r.preserve_metadata && !self.metadata {
            Some(PortableReason::MetadataUnsupported)
        } else if r.preserve_link && !self.reparse_links {
            Some(PortableReason::LinkUnsupported)
        } else if r.progress && !self.progress {
            Some(PortableReason::ProgressUnsupported)
        } else if r.cancellation && !self.cancellation {
            Some(PortableReason::CancellationUnsupported)
        } else {
            None
        };
        reason
            .map(AdapterSelection::Portable)
            .unwrap_or(AdapterSelection::WindowsCopyFile2)
    }
}
