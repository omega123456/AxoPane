use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::ipc::types::ThumbnailQuality;
use base64::Engine;

pub const PREVIEW_CSS_PIXELS: u32 = 112;
pub const MAX_PREVIEW_DIMENSION: u32 = 224;
pub const MAX_PREVIEW_PIXELS: u32 = MAX_PREVIEW_DIMENSION * MAX_PREVIEW_DIMENSION;
pub const MAX_PNG_BYTES: usize = 256 * 1024;
pub const MAX_DATA_URL_BYTES: usize = 350 * 1024;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const PNG_IHDR_TYPE: &[u8] = b"IHDR";
const PNG_IHDR_TYPE_RANGE: std::ops::Range<usize> = 12..16;
const PNG_WIDTH_RANGE: std::ops::Range<usize> = 16..20;
const PNG_HEIGHT_RANGE: std::ops::Range<usize> = 20..24;

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

fn has_png_header(png: &[u8]) -> bool {
    let long_enough = png.len() >= PNG_HEIGHT_RANGE.end;
    long_enough && png.starts_with(PNG_SIGNATURE) && &png[PNG_IHDR_TYPE_RANGE] == PNG_IHDR_TYPE
}

fn is_valid_preview_axis(length: u32) -> bool {
    (1..=MAX_PREVIEW_DIMENSION).contains(&length)
}

pub fn is_within_preview_bounds(width: u32, height: u32) -> bool {
    let fits_area = width
        .checked_mul(height)
        .is_some_and(|pixels| pixels <= MAX_PREVIEW_PIXELS);
    is_valid_preview_axis(width) && is_valid_preview_axis(height) && fits_area
}

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
    if png.len() > MAX_PNG_BYTES {
        return Err(ThumbnailState::Failed);
    }
    if !has_png_header(&png) {
        return Err(ThumbnailState::Failed);
    }
    let width = u32::from_be_bytes(
        png[PNG_WIDTH_RANGE]
            .try_into()
            .map_err(|_| ThumbnailState::Failed)?,
    );
    let height = u32::from_be_bytes(
        png[PNG_HEIGHT_RANGE]
            .try_into()
            .map_err(|_| ThumbnailState::Failed)?,
    );
    if !is_within_preview_bounds(width, height) {
        return Err(ThumbnailState::Failed);
    }
    Ok(ThumbnailState::Ready {
        data_url: ThumbnailDataUrl(Arc::from(data_url)),
        quality: ThumbnailQuality::High,
    })
}
