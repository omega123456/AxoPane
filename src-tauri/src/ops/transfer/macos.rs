use super::{AdapterSelection, PortableReason, TransferAdapter, TransferRequirements};

#[derive(Debug, Clone, Copy, Default)]
pub struct MacosCapabilities {
    pub api_available: bool,
    pub filesystem_supports_copyfile: bool,
    pub clone: bool,
    pub sparse: bool,
    pub metadata: bool,
    pub links: bool,
    pub progress: bool,
    pub cancellation: bool,
}

impl TransferAdapter for MacosCapabilities {
    fn select(&self, r: TransferRequirements) -> AdapterSelection {
        let reason = if !self.api_available {
            Some(PortableReason::ApiUnavailable)
        } else if !self.filesystem_supports_copyfile {
            Some(PortableReason::UnsupportedFilesystem)
        } else if r.clone && !self.clone {
            Some(PortableReason::CloneUnsupported)
        } else if r.sparse && !self.sparse {
            Some(PortableReason::SparseUnsupported)
        } else if r.preserve_metadata && !self.metadata {
            Some(PortableReason::MetadataUnsupported)
        } else if r.preserve_link && !self.links {
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
            .unwrap_or(AdapterSelection::MacosCopyfile)
    }
}
