use super::provider::{ProviderCapability, ThumbnailPreviewCallback, ThumbnailProvider};
use super::types::{ThumbnailCandidate, ThumbnailState};

#[derive(Default)]
pub struct UnsupportedThumbnailProvider;

impl ThumbnailProvider for UnsupportedThumbnailProvider {
    fn capability(&self) -> ProviderCapability {
        ProviderCapability::Unsupported
    }

    fn generate(
        &self,
        _candidate: &ThumbnailCandidate,
        _preview: ThumbnailPreviewCallback,
    ) -> ThumbnailState {
        ThumbnailState::Unavailable
    }
}
