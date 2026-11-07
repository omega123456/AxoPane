//! Trash browsing over a plain directory (`~/.Trash` on macOS, the fake trash
//! under `test-utils`). Each backend supplies the [`TrashIndex`] that records
//! where trashed items came from.

use std::fs;
use std::path::{Path, PathBuf};

use super::{manifest, TrashEntry};

/// Original-location bookkeeping loaded once per trash command.
pub trait TrashIndex: Sized {
    fn load(trash_dir: &Path) -> Result<Self, String>;
    /// The original path of a trashed item and, when recorded, when it was trashed.
    fn original_location(&self, id: &str) -> Option<(String, Option<i64>)>;
    /// When the index has no deletion time, the backend's best guess from metadata.
    fn fallback_deleted_at(metadata: &fs::Metadata) -> Option<i64>;
    fn manifest(&mut self) -> &mut manifest::Transaction;
    fn commit_restores(&mut self) -> Result<(), String>;
}

impl TrashIndex for manifest::Transaction {
    fn load(trash_dir: &Path) -> Result<Self, String> {
        manifest::Transaction::load(trash_dir)
    }

    fn original_location(&self, id: &str) -> Option<(String, Option<i64>)> {
        self.lookup(id)
            .map(|(original_path, deleted_at)| (original_path, Some(deleted_at)))
    }

    fn fallback_deleted_at(_metadata: &fs::Metadata) -> Option<i64> {
        None
    }

    fn manifest(&mut self) -> &mut manifest::Transaction {
        self
    }

    fn commit_restores(&mut self) -> Result<(), String> {
        self.commit()
    }
}

fn remove_entry(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|error| error.to_string())
    } else {
        fs::remove_file(path).map_err(|error| error.to_string())
    }
}

fn readable_metadata(path: &Path) -> Option<fs::Metadata> {
    fs::metadata(path)
        .inspect_err(|error| {
            log::warn!(
                "Trash entry '{}' is listed without metadata: {error}",
                path.display()
            );
        })
        .ok()
}

fn trash_entry<Index: TrashIndex>(entry: &fs::DirEntry, index: &Index) -> Option<TrashEntry> {
    let path = entry.path();
    let name = path.file_name().and_then(|value| value.to_str())?;
    if manifest::is_manifest_file(name) {
        return None;
    }
    let file_type = entry
        .file_type()
        .inspect_err(|error| {
            log::warn!(
                "Skipping trash entry '{}': failed to read file type: {error}",
                path.display()
            );
        })
        .ok()?;
    let metadata = readable_metadata(&path);
    let is_dir = file_type.is_dir();
    let (original_path, deleted_at) = match index.original_location(name) {
        Some((original_path, recorded_deleted_at)) => (
            Some(original_path),
            recorded_deleted_at.or_else(|| metadata.as_ref().and_then(Index::fallback_deleted_at)),
        ),
        None => (None, None),
    };
    Some(TrashEntry {
        id: name.to_string(),
        name: name.to_string(),
        original_path,
        size_bytes: metadata
            .as_ref()
            .and_then(|metadata| (!is_dir).then_some(metadata.len())),
        is_dir,
        deleted_at,
    })
}

pub fn list_trash_in<Index: TrashIndex>(trash_dir: &Path) -> Result<Vec<TrashEntry>, String> {
    if !trash_dir.exists() {
        return Ok(Vec::new());
    }
    let entries =
        fs::read_dir(trash_dir).map_err(|error| format!("{}: {error}", trash_dir.display()))?;
    let index = Index::load(trash_dir)?;
    let readable_entries = entries.filter_map(|entry| {
        entry
            .inspect_err(|error| {
                log::warn!(
                    "Skipping unreadable trash entry in '{}': {error}",
                    trash_dir.display()
                );
            })
            .ok()
    });
    Ok(readable_entries
        .filter_map(|entry| trash_entry(&entry, &index))
        .collect())
}

pub fn restore_from_trash_in<Index: TrashIndex>(
    ids: &[String],
    trash_dir: &Path,
) -> Result<(), String> {
    let mut index = Index::load(trash_dir)?;
    for id in ids {
        let Some((original_path, _)) = index.original_location(id) else {
            return Err(format!(
                "'{id}' has no known original location and cannot be restored"
            ));
        };
        let target = PathBuf::from(&original_path);
        if target.exists() {
            return Err(format!(
                "cannot restore '{id}': an item already exists at {original_path}"
            ));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::rename(trash_dir.join(id), &target).map_err(|error| error.to_string())?;
        index.manifest().remove(id);
    }
    index.commit_restores()
}

pub fn delete_from_trash_in<Index: TrashIndex>(
    ids: &[String],
    trash_dir: &Path,
) -> Result<(), String> {
    for id in ids {
        let path = trash_dir.join(id);
        if !path.exists() {
            return Err(format!("'{id}' is no longer in the trash"));
        }
        remove_entry(&path)?;
    }
    // Purging does not need provenance to remove an item. Preserve that
    // behavior if a corrupt manifest is discovered only after the filesystem
    // mutations, then perform one sidecar transaction for the whole command.
    let mut index = Index::load(trash_dir)?;
    for id in ids {
        index.manifest().remove(id);
    }
    index.manifest().force_commit()
}

pub fn empty_trash_in<Index: TrashIndex>(trash_dir: &Path) -> Result<(), String> {
    if !trash_dir.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(trash_dir).map_err(|error| error.to_string())?;
    let mut index = Index::load(trash_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !manifest::is_manifest_file(name) {
            remove_entry(&path)?;
        }
    }
    index.manifest().force_clear_commit()
}
