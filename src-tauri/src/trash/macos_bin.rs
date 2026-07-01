//! macOS Trash browsing.
//!
//! `~/.Trash` is a plain directory, but macOS records no original-location
//! metadata we can read back generically, so we maintain our own
//! [`manifest`](super::manifest) alongside it, written whenever this app
//! moves something to the trash.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{manifest, TrashEntry};

pub fn home_trash_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".Trash"))
        .ok_or_else(|| "could not resolve home directory".to_string())
}

pub fn snapshot_names(trash_dir: &std::path::Path) -> HashSet<String> {
    let Ok(entries) = fs::read_dir(trash_dir) else {
        return HashSet::new();
    };

    entries
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !manifest::is_manifest_file(name))
        .collect()
}

/// Diffs the trash directory against a pre-deletion snapshot to find the
/// name the just-moved item landed under (Finder may have de-duplicated a
/// collision, e.g. `report.txt` -> `report.txt 2`), then records it.
pub fn record_deletion(
    before: &HashSet<String>,
    trash_dir: &std::path::Path,
    original_path: &str,
) -> Result<(), String> {
    let after = snapshot_names(trash_dir);
    let Some(new_name) = after.difference(before).next() else {
        return Err(format!(
            "could not detect the trashed name for '{original_path}' in {trash_dir:?}; it will show up in the trash without a known original location"
        ));
    };

    let deleted_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);

    manifest::record(trash_dir, new_name, original_path, deleted_at)
}

pub fn list_trash() -> Result<Vec<TrashEntry>, String> {
    let trash_dir = home_trash_dir()?;
    if !trash_dir.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(&trash_dir).map_err(|error| error.to_string())?;
    let mut result = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if manifest::is_manifest_file(name) {
            continue;
        }

        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        let is_dir = metadata.is_dir();
        let (original_path, deleted_at) = match manifest::lookup(&trash_dir, name) {
            Some((original_path, deleted_at)) => (Some(original_path), Some(deleted_at)),
            None => (None, None),
        };

        result.push(TrashEntry {
            id: name.to_string(),
            name: name.to_string(),
            original_path,
            size_bytes: (!is_dir).then_some(metadata.len()),
            is_dir,
            deleted_at,
        });
    }

    Ok(result)
}

pub fn restore_from_trash(ids: &[String]) -> Result<(), String> {
    let trash_dir = home_trash_dir()?;

    for id in ids {
        let Some((original_path, _)) = manifest::lookup(&trash_dir, id) else {
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
        manifest::remove(&trash_dir, id)?;
    }

    Ok(())
}

pub fn empty_trash() -> Result<(), String> {
    let trash_dir = home_trash_dir()?;
    if !trash_dir.exists() {
        return Ok(());
    }

    let entries = fs::read_dir(&trash_dir).map_err(|error| error.to_string())?;
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

    manifest::clear(&trash_dir)
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
        manifest::remove(&trash_dir, id)?;
    }

    Ok(())
}
