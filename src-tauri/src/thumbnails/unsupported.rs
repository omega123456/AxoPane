use super::provider::{ProviderCapability, ThumbnailProvider};
use super::types::{ThumbnailCandidate, ThumbnailState};

#[derive(Default)]
pub struct UnsupportedThumbnailProvider;

impl ThumbnailProvider for UnsupportedThumbnailProvider {
    fn capability(&self) -> ProviderCapability {
        ProviderCapability::Unsupported
    }

    fn generate(&self, _candidate: &ThumbnailCandidate) -> ThumbnailState {
        ThumbnailState::Unavailable
    }
}
