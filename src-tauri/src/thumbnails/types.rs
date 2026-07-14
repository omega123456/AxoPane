//! Stable, scheduler-independent thumbnail identities.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::ipc::types::ThumbnailQuality;
use base64::Engine;

pub const PREVIEW_CSS_PIXELS: u32 = 112;
pub const MAX_PREVIEW_DIMENSION: u32 = 224;
pub const MAX_PREVIEW_PIXELS: u32 = MAX_PREVIEW_DIMENSION * MAX_PREVIEW_DIMENSION;
pub const MAX_PNG_BYTES: usize = 256 * 1024;
pub const MAX_DATA_URL_BYTES: usize = 350 * 1024;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ThumbnailFingerprint {
    pub path: PathBuf,
    pub modified_unix_seconds: u64,
    pub size_bytes: u64,
    pub physical_size: u32,
}

impl ThumbnailFingerprint {
    pub fn from_metadata(path: &Path, modified_unix_seconds: u64, size_bytes: u64) -> Self {
        Self {
            path: path.to_path_buf(),
            modified_unix_seconds,
            size_bytes,
            physical_size: MAX_PREVIEW_DIMENSION,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThumbnailCandidate {
    pub fingerprint: ThumbnailFingerprint,
    pub is_directory: bool,
}

impl ThumbnailCandidate {
    pub fn new(fingerprint: ThumbnailFingerprint, is_directory: bool) -> Self {
        Self {
            fingerprint,
            is_directory,
        }
    }

    /// Re-reads the listing identity once before native generation. Cache hits
    /// trust the watched directory listing and do not touch the filesystem.
    pub fn matches_current_metadata(&self) -> bool {
        let Ok(metadata) = std::fs::metadata(&self.fingerprint.path) else {
            return false;
        };
        let Ok(modified_unix_seconds) = metadata
            .modified()
            .and_then(|modified| {
                modified
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(std::io::Error::other)
            })
            .map(|duration| duration.as_secs())
        else {
            return false;
        };
        self.fingerprint.modified_unix_seconds == modified_unix_seconds
            && self.fingerprint.size_bytes == metadata.len()
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ThumbnailCacheKey(pub ThumbnailFingerprint);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThumbnailDataUrl(Arc<str>);

impl ThumbnailDataUrl {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThumbnailState {
    Ready {
        data_url: ThumbnailDataUrl,
        quality: ThumbnailQuality,
    },
    Unavailable,
    Failed,
}

impl ThumbnailState {
    pub fn weight(&self) -> usize {
        match self {
            Self::Ready { data_url, .. } => data_url.len(),
            Self::Unavailable | Self::Failed => 1,
        }
    }
}

/// Accepts only the transport format and hard payload cap used by the UI.
/// Native providers must call this after bounded raster conversion and PNG
/// encoding, before handing a preview to the cache or scheduler.
pub fn validated_png_data_url(data_url: String) -> Result<ThumbnailState, ThumbnailState> {
    let Some(encoded) = data_url.strip_prefix("data:image/png;base64,") else {
        return Err(ThumbnailState::Failed);
    };
    if data_url.len() > MAX_DATA_URL_BYTES {
        return Err(ThumbnailState::Failed);
    }
    let Ok(png) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
        return Err(ThumbnailState::Failed);
    };
    if png.len() > MAX_PNG_BYTES
        || png.len() < 24
        || &png[..8] != b"\x89PNG\r\n\x1a\n"
        || &png[12..16] != b"IHDR"
    {
        return Err(ThumbnailState::Failed);
    }
    let width = u32::from_be_bytes(png[16..20].try_into().map_err(|_| ThumbnailState::Failed)?);
    let height = u32::from_be_bytes(png[20..24].try_into().map_err(|_| ThumbnailState::Failed)?);
    if width == 0
        || height == 0
        || width > MAX_PREVIEW_DIMENSION
        || height > MAX_PREVIEW_DIMENSION
        || width
            .checked_mul(height)
            .is_none_or(|pixels| pixels > MAX_PREVIEW_PIXELS)
    {
        return Err(ThumbnailState::Failed);
    }
    Ok(ThumbnailState::Ready {
        data_url: ThumbnailDataUrl(Arc::from(data_url)),
        quality: ThumbnailQuality::High,
    })
}
