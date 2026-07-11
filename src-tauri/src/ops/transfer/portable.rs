use super::{AdapterSelection, PortableReason, TransferAdapter, TransferRequirements};

#[derive(Debug, Default)]
pub struct PortableAdapter;

impl TransferAdapter for PortableAdapter {
    fn select(&self, _requirements: TransferRequirements) -> AdapterSelection {
        AdapterSelection::Portable(PortableReason::UnsupportedPlatform)
    }
}
