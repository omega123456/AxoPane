//! Deterministic test provider. File names select outcomes without OS calls.

use super::provider::{ProviderCapability, ThumbnailProvider};
use super::types::{ThumbnailCandidate, ThumbnailState};

#[derive(Default)]
pub struct FakeThumbnailProvider;

impl ThumbnailProvider for FakeThumbnailProvider {
    fn capability(&self) -> ProviderCapability {
        ProviderCapability::Fake
    }

    fn generate(&self, candidate: &ThumbnailCandidate) -> ThumbnailState {
        if candidate.is_directory {
            return ThumbnailState::Unavailable;
        }
        match candidate
            .fingerprint
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
        {
            name if name.contains("failed") => ThumbnailState::Failed,
            name if name.contains("unavailable") || name.contains("cancelled") => {
                ThumbnailState::Unavailable
            }
            _ => ThumbnailState::Ready {
                data_url: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADElEQVR42mNk+M/wHwAF/gL+ZfGHkAAAAABJRU5ErkJggg==".to_string(),
            },
        }
    }
}
