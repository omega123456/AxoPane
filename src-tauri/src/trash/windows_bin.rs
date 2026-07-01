//! Windows Recycle Bin browsing, backed by the `trash` crate's
//! `os_limited` module, which itself drives the Shell `IFileOperation` COM
//! API. No bookkeeping of our own is required: Windows already tracks each
//! item's original location.

use std::collections::HashSet;

use super::TrashEntry;

fn to_entry(item: trash::TrashItem) -> TrashEntry {
    let metadata = trash::os_limited::metadata(&item).ok();
    let (size_bytes, is_dir) = match metadata.map(|value| value.size) {
        Some(trash::TrashItemSize::Bytes(bytes)) => (Some(bytes), false),
        Some(trash::TrashItemSize::Entries(_)) => (None, true),
        None => (None, false),
    };

    TrashEntry {
        id: item.id.to_string_lossy().into_owned(),
        name: item.name.to_string_lossy().into_owned(),
        original_path: Some(item.original_path().to_string_lossy().into_owned()),
        size_bytes,
        is_dir,
        deleted_at: Some(item.time_deleted),
    }
}

pub fn list_trash() -> Result<Vec<TrashEntry>, String> {
    let items = trash::os_limited::list().map_err(|error| error.to_string())?;
    Ok(items.into_iter().map(to_entry).collect())
}

pub fn restore_from_trash(ids: &[String]) -> Result<(), String> {
    let id_set: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let items = trash::os_limited::list().map_err(|error| error.to_string())?;
    let matched: Vec<_> = items
        .into_iter()
        .filter(|item| id_set.contains(item.id.to_string_lossy().as_ref()))
        .collect();

    if matched.len() != id_set.len() {
        return Err("one or more trash items could not be found".to_string());
    }

    trash::os_limited::restore_all(matched).map_err(|error| error.to_string())
}

pub fn empty_trash() -> Result<(), String> {
    let items = trash::os_limited::list().map_err(|error| error.to_string())?;
    trash::os_limited::purge_all(items).map_err(|error| error.to_string())
}

pub fn delete_from_trash(ids: &[String]) -> Result<(), String> {
    let id_set: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let items = trash::os_limited::list().map_err(|error| error.to_string())?;
    let matched: Vec<_> = items
        .into_iter()
        .filter(|item| id_set.contains(item.id.to_string_lossy().as_ref()))
        .collect();

    if matched.len() != id_set.len() {
        return Err("one or more trash items could not be found".to_string());
    }

    trash::os_limited::purge_all(matched).map_err(|error| error.to_string())
}
