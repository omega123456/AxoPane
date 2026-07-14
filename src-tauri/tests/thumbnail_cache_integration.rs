#[path = "common/mod.rs"]
mod common;

use std::path::Path;

use base64::Engine;
use file_explorer_lib::thumbnails::cache::{ThumbnailCache, NEGATIVE_TTL_SECONDS};
use file_explorer_lib::thumbnails::types::{
    validated_png_data_url, ThumbnailCacheKey, ThumbnailFingerprint, ThumbnailState,
    MAX_DATA_URL_BYTES,
};

fn key(name: &str, modified: u64) -> ThumbnailCacheKey {
    ThumbnailCacheKey(ThumbnailFingerprint::from_metadata(
        Path::new(name),
        modified,
        8,
    ))
}

fn ready_payload(extra_bytes: usize) -> String {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend_from_slice(&1_u32.to_be_bytes());
    png.extend_from_slice(&1_u32.to_be_bytes());
    png.resize(png.len() + extra_bytes, 0);
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

#[test]
fn fingerprint_includes_path_metadata_and_fixed_size_class() {
    let first = ThumbnailFingerprint::from_metadata(Path::new("same.png"), 1, 20);
    let changed = ThumbnailFingerprint::from_metadata(Path::new("same.png"), 2, 20);
    let resized = ThumbnailFingerprint::from_metadata(Path::new("same.png"), 1, 21);

    assert_ne!(first, changed);
    assert_ne!(first, resized);
    assert_eq!(first.physical_size, 224);
    assert_eq!(common::bootstrap_message(), "phase-1-common");
}

#[test]
fn negative_records_expire_against_the_injected_clock() {
    let key = key("unknown.bin", 1);
    let mut cache = ThumbnailCache::new();
    cache.insert(key.clone(), ThumbnailState::Unavailable, 12);

    assert_eq!(cache.get(&key, 12), Some(ThumbnailState::Unavailable));
    assert_eq!(cache.get(&key, 12 + NEGATIVE_TTL_SECONDS), None);
}

#[test]
fn oversized_ready_payload_is_not_admitted() {
    let key = key("large.png", 1);
    let mut cache = ThumbnailCache::new();
    cache.insert(
        key.clone(),
        ThumbnailState::Ready {
            data_url: format!("data:image/png;base64,{}", "a".repeat(MAX_DATA_URL_BYTES)),
        },
        0,
    );
    assert_eq!(cache.get(&key, 0), None);
}

#[test]
fn png_data_urls_are_validated_before_use() {
    assert!(validated_png_data_url(ready_payload(0)).is_ok());
    assert_eq!(
        validated_png_data_url("data:image/png;base64,AA==".into()),
        Err(ThumbnailState::Failed)
    );
    assert_eq!(
        validated_png_data_url("data:image/jpeg;base64,AA==".into()),
        Err(ThumbnailState::Failed)
    );
}

#[test]
fn cache_evicts_least_recently_used_records_at_the_fixed_entry_limit() {
    let mut cache = ThumbnailCache::new();
    for index in 0..257 {
        cache.insert(
            key(&format!("{index}.png"), 1),
            ThumbnailState::Unavailable,
            0,
        );
    }

    assert_eq!(cache.len(), 256);
    assert_eq!(cache.get(&key("0.png", 1), 0), None);
    assert_eq!(
        cache.get(&key("256.png", 1), 0),
        Some(ThumbnailState::Unavailable)
    );
}

#[test]
fn cache_also_evicts_when_encoded_preview_weight_exceeds_sixteen_mebibytes() {
    let mut cache = ThumbnailCache::new();
    let payload = ready_payload(60_000);
    for index in 0..256 {
        cache.insert(
            key(&format!("weighted-{index}.png"), 1),
            ThumbnailState::Ready {
                data_url: payload.clone(),
            },
            0,
        );
    }

    assert!(cache.len() < 256);
    assert_eq!(cache.get(&key("weighted-0.png", 1), 0), None);
}
