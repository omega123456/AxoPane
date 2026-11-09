use super::provider::{ProviderCapability, ThumbnailPreviewCallback, ThumbnailProvider};
use super::types::{ThumbnailCandidate, ThumbnailState};

const UNAVAILABLE_NAME_MARKERS: [&str; 2] = ["unavailable", "cancelled"];

fn is_unavailable_fixture_name(name: &str) -> bool {
    UNAVAILABLE_NAME_MARKERS
        .iter()
        .any(|marker| name.contains(marker))
}

#[derive(Default)]
pub struct FakeThumbnailProvider;

impl ThumbnailProvider for FakeThumbnailProvider {
    fn capability(&self) -> ProviderCapability {
        ProviderCapability::Fake
    }

    fn generate(
        &self,
        candidate: &ThumbnailCandidate,
        _preview: ThumbnailPreviewCallback,
    ) -> ThumbnailState {
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
            name if is_unavailable_fixture_name(name) => {
                ThumbnailState::Unavailable
            }
            _ => super::types::validated_png_data_url("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADElEQVR42mNk+M/wHwAF/gL+ZfGHkAAAAABJRU5ErkJggg==".to_string())
                .unwrap_or(ThumbnailState::Failed),
        }
    }
}
