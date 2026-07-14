#[path = "common/mod.rs"]
mod common;

use std::path::Path;

use file_explorer_lib::thumbnails::platform_provider;
use file_explorer_lib::thumbnails::provider::ProviderCapability;
use file_explorer_lib::thumbnails::types::{
    ThumbnailCandidate, ThumbnailFingerprint, ThumbnailState,
};

fn candidate(name: &str, is_directory: bool) -> ThumbnailCandidate {
    ThumbnailCandidate::new(
        ThumbnailFingerprint::from_metadata(Path::new(name), 1, 4),
        is_directory,
    )
}

fn generate(
    provider: &dyn file_explorer_lib::thumbnails::provider::ThumbnailProvider,
    candidate: &ThumbnailCandidate,
) -> ThumbnailState {
    provider.generate(candidate, std::sync::Arc::new(|_| {}))
}

#[test]
fn test_utils_always_selects_the_safe_fake_provider() {
    let provider = platform_provider();
    assert_eq!(provider.capability(), ProviderCapability::Fake);
    assert_eq!(
        generate(provider.as_ref(), &candidate("directory.png", true)),
        ThumbnailState::Unavailable
    );
    assert_eq!(common::bootstrap_message(), "phase-1-common");
}

#[test]
fn fake_provider_has_deterministic_terminal_outcomes_without_native_calls() {
    let provider = platform_provider();
    assert!(matches!(
        generate(provider.as_ref(), &candidate("preview.png", false)),
        ThumbnailState::Ready { .. }
    ));
    assert_eq!(
        generate(provider.as_ref(), &candidate("unavailable.png", false)),
        ThumbnailState::Unavailable
    );
    assert_eq!(
        generate(provider.as_ref(), &candidate("cancelled.png", false)),
        ThumbnailState::Unavailable
    );
    assert_eq!(
        generate(provider.as_ref(), &candidate("failed.png", false)),
        ThumbnailState::Failed
    );
    provider.cancel(&candidate("preview.png", false));
    provider.shutdown();
}
