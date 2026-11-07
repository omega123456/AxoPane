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

impl WindowsCapabilities {
    fn missing_capability(&self, requirements: TransferRequirements) -> Option<PortableReason> {
        if !self.api_available {
            return Some(PortableReason::ApiUnavailable);
        }
        if !self.supported_build {
            return Some(PortableReason::UnsupportedBuild);
        }
        let remote_server_lacks_copyfile2 =
            !self.local_filesystem && !self.server_supports_copyfile2;
        if remote_server_lacks_copyfile2 {
            return Some(PortableReason::NetworkServerUnsupported);
        }
        let sparse_missing = requirements.sparse && !self.sparse;
        if sparse_missing {
            return Some(PortableReason::SparseUnsupported);
        }
        let offload_missing = requirements.offload && !self.offload;
        if offload_missing {
            return Some(PortableReason::OffloadUnsupported);
        }
        let metadata_missing = requirements.preserve_metadata && !self.metadata;
        if metadata_missing {
            return Some(PortableReason::MetadataUnsupported);
        }
        let links_missing = requirements.preserve_link && !self.reparse_links;
        if links_missing {
            return Some(PortableReason::LinkUnsupported);
        }
        let progress_missing = requirements.progress && !self.progress;
        if progress_missing {
            return Some(PortableReason::ProgressUnsupported);
        }
        let cancellation_missing = requirements.cancellation && !self.cancellation;
        if cancellation_missing {
            return Some(PortableReason::CancellationUnsupported);
        }
        None
    }
}

impl TransferAdapter for WindowsCapabilities {
    fn select(&self, requirements: TransferRequirements) -> AdapterSelection {
        self.missing_capability(requirements)
            .map(AdapterSelection::Portable)
            .unwrap_or(AdapterSelection::WindowsCopyFile2)
    }
}
