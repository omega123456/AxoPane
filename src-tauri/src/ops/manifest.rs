//! Progressive transfer manifests.
//!
//! A manifest is deliberately discovered while a fallback copy runs: moves
//! that can be renamed must not pay for a recursive pre-scan merely to make a
//! progress denominator exact.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    pub source: PathBuf,
    pub relative_path: PathBuf,
    pub bytes: u64,
    pub is_link: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ProgressiveManifest {
    entries: Vec<ManifestEntry>,
    discovered_bytes: u64,
}

impl ProgressiveManifest {
    pub fn record(&mut self, root: &Path, path: PathBuf, bytes: u64, is_link: bool) {
        let relative_path = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        self.discovered_bytes = self.discovered_bytes.saturating_add(bytes);
        self.entries.push(ManifestEntry {
            source: path,
            relative_path,
            bytes,
            is_link,
        });
    }

    pub fn entries(&self) -> &[ManifestEntry] {
        &self.entries
    }
    pub fn discovered_bytes(&self) -> u64 {
        self.discovered_bytes
    }
}
