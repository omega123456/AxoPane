//! Tracks the original location of items moved into the OS trash.
//!
//! Neither macOS's `~/.Trash` folder nor our `test-utils` fake trash
//! directory record where an item came from, so restoring an item to its
//! original path is impossible without our own bookkeeping. This module
//! keeps a small JSON sidecar file (`.axopane-trash-manifest.json`) inside
//! the trash directory itself, keyed by the name the item ends up with
//! once it lands in the trash (which may differ from its original name if
//! Finder de-duplicated a collision).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::persist::{load_json_or_default, write_json_atomic};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Manifest {
    #[serde(default)]
    entries: HashMap<String, ManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestEntry {
    original_path: String,
    deleted_at: i64,
}

fn manifest_path(trash_dir: &Path) -> PathBuf {
    trash_dir.join(".axopane-trash-manifest.json")
}

pub fn record(
    trash_dir: &Path,
    trashed_name: &str,
    original_path: &str,
    deleted_at: i64,
) -> Result<(), String> {
    let path = manifest_path(trash_dir);
    let mut manifest: Manifest = load_json_or_default(&path).map_err(|error| error.to_string())?;
    manifest.entries.insert(
        trashed_name.to_string(),
        ManifestEntry {
            original_path: original_path.to_string(),
            deleted_at,
        },
    );
    write_json_atomic(&path, &manifest).map_err(|error| error.to_string())
}

pub fn lookup(trash_dir: &Path, trashed_name: &str) -> Option<(String, i64)> {
    let path = manifest_path(trash_dir);
    let manifest: Manifest = load_json_or_default(&path).ok()?;
    manifest
        .entries
        .get(trashed_name)
        .map(|entry| (entry.original_path.clone(), entry.deleted_at))
}

pub fn remove(trash_dir: &Path, trashed_name: &str) -> Result<(), String> {
    let path = manifest_path(trash_dir);
    let mut manifest: Manifest = load_json_or_default(&path).map_err(|error| error.to_string())?;
    manifest.entries.remove(trashed_name);
    write_json_atomic(&path, &manifest).map_err(|error| error.to_string())
}

pub fn clear(trash_dir: &Path) -> Result<(), String> {
    write_json_atomic(&manifest_path(trash_dir), &Manifest::default())
        .map_err(|error| error.to_string())
}

/// Name of the manifest file itself, so directory listings can skip it.
pub fn is_manifest_file(name: &str) -> bool {
    name == ".axopane-trash-manifest.json" || name == ".axopane-trash-manifest.json.tmp"
}
