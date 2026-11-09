use super::types::{ThumbnailCandidate, ThumbnailState};
use std::sync::Arc;

pub type ThumbnailPreviewCallback = Arc<dyn Fn(ThumbnailState) + Send + Sync>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCapability {
    Native,
    Fake,
    Unsupported,
}

pub trait ThumbnailProvider: Send + Sync {
    fn capability(&self) -> ProviderCapability;
    fn generate(
        &self,
        candidate: &ThumbnailCandidate,
        preview: ThumbnailPreviewCallback,
    ) -> ThumbnailState;

    fn cancel(&self, _candidate: &ThumbnailCandidate) {}

    fn shutdown(&self) {}
}
