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

/// A command-scoped manifest view.  Load this once at the start of a Trash
/// command, make all lookups and mutations against the in-memory map, then
/// commit at most once after the filesystem work has completed.
///
/// Keeping the transaction explicit prevents a multi-select restore or purge
/// from repeatedly parsing and atomically rewriting the sidecar file.
#[derive(Debug, Clone)]
pub struct Transaction {
    path: PathBuf,
    manifest: Manifest,
    dirty: bool,
}

fn manifest_path(trash_dir: &Path) -> PathBuf {
    trash_dir.join(".axopane-trash-manifest.json")
}

/// Commits all correlations from one deletion batch in a single manifest
/// read/modify/write transaction.  Callers deliberately omit uncertain
/// correlations: unknown system Trash entries must stay discoverable without
/// an invented provenance record.
pub fn record_batch<I>(trash_dir: &Path, entries: I) -> Result<(), String>
where
    I: IntoIterator<Item = (String, String, i64)>,
{
    let mut transaction = Transaction::load(trash_dir)?;
    transaction.record_batch(entries);
    transaction.commit()
}

impl Transaction {
    pub fn load(trash_dir: &Path) -> Result<Self, String> {
        let path = manifest_path(trash_dir);
        let manifest = load_json_or_default(&path).map_err(|error| error.to_string())?;
        Ok(Self {
            path,
            manifest,
            dirty: false,
        })
    }

    pub fn lookup(&self, trashed_name: &str) -> Option<(String, i64)> {
        self.manifest
            .entries
            .get(trashed_name)
            .map(|entry| (entry.original_path.clone(), entry.deleted_at))
    }

    pub fn record_batch<I>(&mut self, entries: I)
    where
        I: IntoIterator<Item = (String, String, i64)>,
    {
        for (trashed_name, original_path, deleted_at) in entries {
            self.manifest.entries.insert(
                trashed_name,
                ManifestEntry {
                    original_path,
                    deleted_at,
                },
            );
            self.dirty = true;
        }
    }

    pub fn remove(&mut self, trashed_name: &str) {
        self.dirty |= self.manifest.entries.remove(trashed_name).is_some();
    }

    /// Writes at most once. A no-op transaction intentionally avoids a
    /// needless sidecar rewrite; callers that must materialise an empty
    /// manifest use [`Self::force_clear_commit`].
    pub fn commit(&mut self) -> Result<(), String> {
        if self.dirty {
            write_json_atomic(&self.path, &self.manifest).map_err(|error| error.to_string())?;
            self.dirty = false;
        }
        Ok(())
    }

    pub fn force_clear_commit(&mut self) -> Result<(), String> {
        self.manifest.entries.clear();
        self.dirty = true;
        self.commit()
    }

    pub fn force_commit(&mut self) -> Result<(), String> {
        self.dirty = true;
        self.commit()
    }
}

/// Name of the manifest file itself, so directory listings can skip it.
pub fn is_manifest_file(name: &str) -> bool {
    name == ".axopane-trash-manifest.json" || name == ".axopane-trash-manifest.json.tmp"
}
