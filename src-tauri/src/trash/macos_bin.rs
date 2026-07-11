//! macOS Trash browsing.
//!
//! `~/.Trash` is a plain directory, but macOS records no original-location
//! metadata we can read back generically, so we maintain our own
//! [`manifest`](super::manifest) alongside it, written whenever this app
//! moves something to the trash.

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::{dsstore, manifest, TrashEntry};

/// Loads and parses `trash_dir/.DS_Store` once per operation, for the
/// [`dsstore`] fallback used when an item has no manifest entry (e.g. it was
/// trashed via Finder directly, not through this app). Best-effort: any
/// read/parse failure yields an empty map rather than failing the caller.
fn put_back_index(trash_dir: &Path) -> HashMap<String, dsstore::PutBack> {
    fs::read(trash_dir.join(".DS_Store"))
        .map(|bytes| dsstore::parse_put_back(&bytes))
        .unwrap_or_default()
}

/// Metadata loaded once for the lifetime of one public Trash command.
struct CommandIndexes {
    manifest: manifest::Transaction,
    put_back: HashMap<String, dsstore::PutBack>,
}

impl CommandIndexes {
    fn load(trash_dir: &Path) -> Result<Self, String> {
        Ok(Self {
            manifest: manifest::Transaction::load(trash_dir)?,
            put_back: put_back_index(trash_dir),
        })
    }

    fn original_path(&self, id: &str) -> Option<(String, Option<i64>)> {
        match self.manifest.lookup(id) {
            Some((original_path, deleted_at)) => Some((original_path, Some(deleted_at))),
            None => self.put_back.get(id).map(|put_back| {
                (
                    dsstore::resolve_original_path(
                        Path::new("/"),
                        &put_back.original_dir,
                        &put_back.original_name,
                    ),
                    None,
                )
            }),
        }
    }

    fn commit(&mut self) -> Result<(), String> {
        self.manifest.commit()
    }
}

pub fn home_trash_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".Trash"))
        .ok_or_else(|| "could not resolve home directory".to_string())
}

pub fn list_trash() -> Result<Vec<TrashEntry>, String> {
    let trash_dir = home_trash_dir()?;
    if !trash_dir.exists() {
        return Ok(Vec::new());
    }

    let entries =
        fs::read_dir(&trash_dir).map_err(|error| format!("{}: {error}", trash_dir.display()))?;
    let mut result = Vec::new();
    let indexes = CommandIndexes::load(&trash_dir)?;

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                log::warn!(
                    "Skipping unreadable trash entry in '{}': {error}",
                    trash_dir.display()
                );
                continue;
            }
        };
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if manifest::is_manifest_file(name) {
            continue;
        }

        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                log::warn!(
                    "Skipping trash entry '{}': failed to read file type: {error}",
                    path.display()
                );
                continue;
            }
        };
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => Some(metadata),
            Err(error) => {
                log::warn!(
                    "Trash entry '{}' is listed without metadata: {error}",
                    path.display()
                );
                None
            }
        };
        let is_dir = file_type.is_dir();
        let (original_path, deleted_at) = indexes.original_path(name).map_or_else(
            || (None, None),
            |(original_path, manifest_deleted_at)| {
                (
                    Some(original_path),
                    manifest_deleted_at.or_else(|| metadata.as_ref().map(|value| value.ctime())),
                )
            },
        );

        result.push(TrashEntry {
            id: name.to_string(),
            name: name.to_string(),
            original_path,
            size_bytes: metadata
                .as_ref()
                .and_then(|metadata| (!is_dir).then_some(metadata.len())),
            is_dir,
            deleted_at,
        });
    }

    Ok(result)
}

pub fn restore_from_trash(ids: &[String]) -> Result<(), String> {
    let trash_dir = home_trash_dir()?;
    let mut indexes = CommandIndexes::load(&trash_dir)?;

    for id in ids {
        let Some((original_path, _)) = indexes.original_path(id) else {
            return Err(format!(
                "'{id}' has no known original location and cannot be restored"
            ));
        };

        let source = trash_dir.join(id);
        let target = PathBuf::from(&original_path);
        if target.exists() {
            return Err(format!(
                "cannot restore '{id}': an item already exists at {original_path}"
            ));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        fs::rename(&source, &target).map_err(|error| error.to_string())?;
        indexes.manifest.remove(id);
    }
    indexes.commit()
}

pub fn empty_trash() -> Result<(), String> {
    let trash_dir = home_trash_dir()?;
    if !trash_dir.exists() {
        return Ok(());
    }

    let entries = fs::read_dir(&trash_dir).map_err(|error| error.to_string())?;
    let mut indexes = CommandIndexes::load(&trash_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if manifest::is_manifest_file(name) {
            continue;
        }

        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|error| error.to_string())?;
        } else {
            fs::remove_file(&path).map_err(|error| error.to_string())?;
        }
    }

    indexes.manifest.force_clear_commit()
}

pub fn delete_from_trash(ids: &[String]) -> Result<(), String> {
    let trash_dir = home_trash_dir()?;

    for id in ids {
        let path = trash_dir.join(id);
        if !path.exists() {
            return Err(format!("'{id}' is no longer in the trash"));
        }

        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|error| error.to_string())?;
        } else {
            fs::remove_file(&path).map_err(|error| error.to_string())?;
        }
    }
    let mut indexes = CommandIndexes::load(&trash_dir)?;
    for id in ids {
        indexes.manifest.remove(id);
    }
    indexes.manifest.force_commit()
}
