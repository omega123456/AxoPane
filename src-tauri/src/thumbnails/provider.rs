//! Native thumbnail provider boundary. Scheduling belongs to the next phase.

use super::types::{ThumbnailCandidate, ThumbnailState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCapability {
    Native,
    Fake,
    Unsupported,
}

pub trait ThumbnailProvider: Send + Sync {
    fn capability(&self) -> ProviderCapability;
    fn generate(&self, candidate: &ThumbnailCandidate) -> ThumbnailState;

    /// A no-op at this foundation layer. Native implementations keep this hook
    /// so the scheduler can give cancellation ownership to the provider later.
    fn cancel(&self, _candidate: &ThumbnailCandidate) {}

    fn shutdown(&self) {}
}
