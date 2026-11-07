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

impl MacosCapabilities {
    fn missing_capability(&self, requirements: TransferRequirements) -> Option<PortableReason> {
        if !self.api_available {
            return Some(PortableReason::ApiUnavailable);
        }
        if !self.filesystem_supports_copyfile {
            return Some(PortableReason::UnsupportedFilesystem);
        }
        let clone_missing = requirements.clone && !self.clone;
        if clone_missing {
            return Some(PortableReason::CloneUnsupported);
        }
        let sparse_missing = requirements.sparse && !self.sparse;
        if sparse_missing {
            return Some(PortableReason::SparseUnsupported);
        }
        let metadata_missing = requirements.preserve_metadata && !self.metadata;
        if metadata_missing {
            return Some(PortableReason::MetadataUnsupported);
        }
        let links_missing = requirements.preserve_link && !self.links;
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

impl TransferAdapter for MacosCapabilities {
    fn select(&self, requirements: TransferRequirements) -> AdapterSelection {
        self.missing_capability(requirements)
            .map(AdapterSelection::Portable)
            .unwrap_or(AdapterSelection::MacosCopyfile)
    }
}
