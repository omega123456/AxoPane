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
pub struct ManifestEntry {
    pub original_path: String,
    pub deleted_at: i64,
}

#[derive(Debug, Clone)]
pub struct Transaction {
    path: PathBuf,
    manifest: Manifest,
    dirty: bool,
}

fn manifest_path(trash_dir: &Path) -> PathBuf {
    trash_dir.join(".axopane-trash-manifest.json")
}

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

    pub fn lookup(&self, trashed_name: &str) -> Option<ManifestEntry> {
        self.manifest.entries.get(trashed_name).cloned()
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

pub fn is_manifest_file(name: &str) -> bool {
    name == ".axopane-trash-manifest.json" || name == ".axopane-trash-manifest.json.tmp"
}
