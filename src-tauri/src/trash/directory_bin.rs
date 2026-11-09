use std::fs;
use std::path::{Path, PathBuf};

use super::{manifest, TrashEntry};

pub struct OriginalLocation {
    pub original_path: String,
    pub deleted_at: Option<i64>,
}

impl From<manifest::ManifestEntry> for OriginalLocation {
    fn from(entry: manifest::ManifestEntry) -> Self {
        Self {
            original_path: entry.original_path,
            deleted_at: Some(entry.deleted_at),
        }
    }
}

pub trait TrashIndex: Sized {
    fn load(trash_dir: &Path) -> Result<Self, String>;
    fn original_location(&self, id: &str) -> Option<OriginalLocation>;
    fn fallback_deleted_at(metadata: &fs::Metadata) -> Option<i64>;
    fn manifest(&mut self) -> &mut manifest::Transaction;
    fn commit_restores(&mut self) -> Result<(), String>;
}

impl TrashIndex for manifest::Transaction {
    fn load(trash_dir: &Path) -> Result<Self, String> {
        manifest::Transaction::load(trash_dir)
    }

    fn original_location(&self, id: &str) -> Option<OriginalLocation> {
        self.lookup(id).map(OriginalLocation::from)
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
        return fs::remove_dir_all(path).map_err(|error| error.to_string());
    }
    fs::remove_file(path).map_err(|error| error.to_string())
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
    let original_location = index.original_location(name);
    let deleted_at = original_location.as_ref().and_then(|location| {
        location
            .deleted_at
            .or_else(|| metadata.as_ref().and_then(Index::fallback_deleted_at))
    });
    Some(TrashEntry {
        id: name.to_string(),
        name: name.to_string(),
        original_path: original_location.map(|location| location.original_path),
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
        let Some(OriginalLocation { original_path, .. }) = index.original_location(id) else {
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
