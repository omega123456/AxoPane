//! OS trash / Recycle Bin integration: moving items in, browsing what's
//! there, restoring items, and emptying it.
//!
//! Moving items to the OS trash mutates machine-global state, so under
//! `feature = "test-utils"` (and coverage) the real [`trash`] crate is never
//! invoked. Instead each item is relocated into a fake-trash directory under
//! the system temp dir, so tests observe the same effect (the source path is
//! gone) without ever touching the real Recycle Bin / Trash.
//!
//! Browsing/restoring/emptying is platform-specific:
//! - Windows: the `trash` crate's `os_limited` module, backed by the Shell
//!   `IFileOperation` COM API, already tracks original locations.
//! - macOS: `~/.Trash` carries no original-location metadata we can read
//!   back, so [`manifest`] records it ourselves whenever this app deletes
//!   something.

use serde::{Deserialize, Serialize};

#[cfg(any(feature = "test-utils", target_os = "macos"))]
mod manifest;

#[cfg(any(feature = "test-utils", target_os = "macos"))]
mod macos_adapter;

// Available under `test-utils` (any OS) so its parsing logic is unit-testable
// and counted toward coverage, even though it's only ever wired up on macOS.
#[cfg(any(feature = "test-utils", target_os = "macos"))]
mod dsstore;

#[cfg(all(not(feature = "test-utils"), windows))]
mod windows_bin;

#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
mod macos_bin;

/// A single item currently sitting in the OS trash / Recycle Bin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    /// Opaque identifier used to restore/purge this item. On Windows this is
    /// the Shell's item id; elsewhere it's the item's file name inside the
    /// trash directory.
    pub id: String,
    pub name: String,
    /// `None` when the original location is unknown (e.g. something the user
    /// trashed via Finder directly, outside of AxoPane, on macOS).
    pub original_path: Option<String>,
    pub size_bytes: Option<u64>,
    pub is_dir: bool,
    /// Unix timestamp (seconds) the item was deleted at, when known.
    pub deleted_at: Option<i64>,
}

/// Move the given paths to the OS trash. Best-effort: the first failure aborts
/// and is reported. An empty list is a no-op.
pub fn move_to_trash(paths: &[String]) -> Result<(), String> {
    move_to_trash_cancellable(paths, || false)
}

/// Cooperative batch boundary for the latency IPC owner. Platform trash APIs
/// can only cancel between calls, while the deterministic fake also checks
/// between individual relocations.
pub fn move_to_trash_cancellable<F>(paths: &[String], is_cancelled: F) -> Result<(), String>
where
    F: Fn() -> bool,
{
    if is_cancelled() {
        return Err("trash request cancelled".to_string());
    }
    if paths.is_empty() {
        return Ok(());
    }
    #[cfg(feature = "test-utils")]
    {
        move_to_trash_impl_cancellable(paths, &fake_trash_dir(), &is_cancelled)
    }
    #[cfg(not(feature = "test-utils"))]
    {
        move_to_trash_impl(paths)
    }
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn move_to_trash_impl(paths: &[String]) -> Result<(), String> {
    trash::delete_all(paths).map_err(|error| error.to_string())
}

// `NsFileManager` is the macOS deletion authority. The adapter observes the
// home Trash once before and once after the complete batch, preserving Finder
// and third-party entries while avoiding per-item rescans.
#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
fn move_to_trash_impl(paths: &[String]) -> Result<(), String> {
    let mut adapter = macos_adapter::MacosSystemTrashAdapter::new();
    let result = macos_adapter::delete_batch(&mut adapter, paths);
    if let Some((_, error)) = result.failures.into_iter().next() {
        return Err(error);
    }
    Ok(())
}

/// Directory the fake trash relocates items into during tests.
///
/// Tests that need to exercise `list_trash`/`restore_from_trash`/`empty_trash`
/// through the IPC command layer (which, matching the real command
/// signatures, take no directory parameter) can point this at a private
/// directory via `AXOPANE_TEST_FAKE_TRASH_DIR` so a call to `empty_trash`
/// never wipes another concurrently-running test's fixtures out of the
/// otherwise-shared fake trash dir.
#[cfg(feature = "test-utils")]
pub fn fake_trash_dir() -> std::path::PathBuf {
    if let Some(override_dir) = std::env::var_os("AXOPANE_TEST_FAKE_TRASH_DIR") {
        return std::path::PathBuf::from(override_dir);
    }
    std::env::temp_dir().join("axopane-fake-trash")
}

#[cfg(feature = "test-utils")]
fn ensure_fake_trash_dir(path: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|error| error.to_string())
}

#[cfg(feature = "test-utils")]
pub fn ensure_fake_trash_dir_for_tests(path: &std::path::Path) -> Result<(), String> {
    ensure_fake_trash_dir(path)
}

#[cfg(feature = "test-utils")]
pub use dsstore::PutBack;

#[cfg(feature = "test-utils")]
pub use macos_adapter::{
    BatchCorrelation, BatchDeletionResult, StableFileIdentity, SystemTrashAdapter, TrashFacts,
    TrashSnapshotEntry,
};

#[cfg(feature = "test-utils")]
pub fn delete_batch_with_adapter_for_tests<A: SystemTrashAdapter>(
    adapter: &mut A,
    paths: &[String],
) -> BatchDeletionResult {
    macos_adapter::delete_batch(adapter, paths)
}

/// Exposes [`dsstore::parse_put_back`] to integration tests, which only see
/// the crate's public API.
#[cfg(feature = "test-utils")]
pub fn parse_put_back_for_tests(bytes: &[u8]) -> std::collections::HashMap<String, PutBack> {
    dsstore::parse_put_back(bytes)
}

/// Exposes [`dsstore::resolve_original_path`] to integration tests.
#[cfg(feature = "test-utils")]
pub fn resolve_original_path_for_tests(
    volume_root: &std::path::Path,
    dir: &str,
    name: &str,
) -> String {
    dsstore::resolve_original_path(volume_root, dir, name)
}

#[cfg(feature = "test-utils")]
#[inline(never)]
fn move_to_fake_trash_path(
    source: &std::path::Path,
    target: &std::path::Path,
) -> Result<(), String> {
    std::fs::rename(source, target).map_err(|error| error.to_string())
}

#[cfg(feature = "test-utils")]
pub fn move_to_fake_trash_path_for_tests(
    source: &std::path::Path,
    target: &std::path::Path,
) -> Result<(), String> {
    move_to_fake_trash_path(source, target)
}

#[cfg(feature = "test-utils")]
fn move_to_trash_impl_cancellable<F>(
    paths: &[String],
    trash_dir: &std::path::Path,
    is_cancelled: &F,
) -> Result<(), String>
where
    F: Fn() -> bool,
{
    use std::path::Path;

    ensure_fake_trash_dir(trash_dir)?;

    let mut manifest_entries = Vec::with_capacity(paths.len());
    for path in paths {
        if is_cancelled() {
            return Err("trash request cancelled".to_string());
        }
        let source = Path::new(path);
        let file_name = source
            .file_name()
            .ok_or_else(|| format!("invalid path: {path}"))?;

        // Resolve name collisions so repeated/parallel calls never fail.
        let mut target = trash_dir.join(file_name);
        let mut counter = 1;
        while target.exists() {
            target = trash_dir.join(format!("{}.{counter}", file_name.to_string_lossy()));
            counter += 1;
        }
        let target_name = target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| file_name.to_string_lossy().into_owned());

        // The fake-trash directory is shared across parallel tests, so another
        // test may remove it between the initial setup and this rename.
        ensure_fake_trash_dir(trash_dir)?;
        move_to_fake_trash_path(source, &target)?;
        let deleted_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0);
        manifest_entries.push((target_name, path.to_string(), deleted_at));
    }

    // Best-effort, matching the real macOS path: all successfully relocated
    // items share one manifest read/modify/write transaction.
    let _ = manifest::record_batch(trash_dir, manifest_entries);

    Ok(())
}

/// Lets tests move items into an isolated trash directory (instead of the
/// shared [`fake_trash_dir`]) so list/restore/empty assertions never race
/// with other tests running concurrently in the same fake trash dir.
#[cfg(feature = "test-utils")]
pub fn move_to_trash_into_for_tests(
    paths: &[String],
    trash_dir: &std::path::Path,
) -> Result<(), String> {
    move_to_trash_impl_cancellable(paths, trash_dir, &|| false)
}

#[cfg(all(not(feature = "test-utils"), windows))]
pub fn list_trash() -> Result<Vec<TrashEntry>, String> {
    windows_bin::list_trash()
}

#[cfg(all(not(feature = "test-utils"), windows))]
pub fn restore_from_trash(ids: &[String]) -> Result<(), String> {
    windows_bin::restore_from_trash(ids)
}

#[cfg(all(not(feature = "test-utils"), windows))]
pub fn empty_trash() -> Result<(), String> {
    windows_bin::empty_trash()
}

#[cfg(all(not(feature = "test-utils"), windows))]
pub fn delete_from_trash(ids: &[String]) -> Result<(), String> {
    windows_bin::delete_from_trash(ids)
}

#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
pub fn list_trash() -> Result<Vec<TrashEntry>, String> {
    macos_bin::list_trash()
}

#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
pub fn restore_from_trash(ids: &[String]) -> Result<(), String> {
    macos_bin::restore_from_trash(ids)
}

#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
pub fn empty_trash() -> Result<(), String> {
    macos_bin::empty_trash()
}

#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
pub fn delete_from_trash(ids: &[String]) -> Result<(), String> {
    macos_bin::delete_from_trash(ids)
}

#[cfg(feature = "test-utils")]
fn list_trash_in(trash_dir: &std::path::Path) -> Result<Vec<TrashEntry>, String> {
    if !trash_dir.exists() {
        return Ok(Vec::new());
    }

    let entries = std::fs::read_dir(trash_dir)
        .map_err(|error| format!("{}: {error}", trash_dir.display()))?;
    let manifest = manifest::Transaction::load(trash_dir)?;
    let mut result = Vec::new();

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
        let metadata = match std::fs::metadata(&path) {
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
        let (original_path, deleted_at) = match manifest.lookup(name) {
            Some((original_path, deleted_at)) => (Some(original_path), Some(deleted_at)),
            None => (None, None),
        };

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

#[cfg(feature = "test-utils")]
fn restore_from_trash_in(ids: &[String], trash_dir: &std::path::Path) -> Result<(), String> {
    let mut manifest = manifest::Transaction::load(trash_dir)?;
    for id in ids {
        let Some((original_path, _)) = manifest.lookup(id) else {
            return Err(format!(
                "'{id}' has no known original location and cannot be restored"
            ));
        };

        let source = trash_dir.join(id);
        let target = std::path::PathBuf::from(&original_path);
        if target.exists() {
            return Err(format!(
                "cannot restore '{id}': an item already exists at {original_path}"
            ));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        std::fs::rename(&source, &target).map_err(|error| error.to_string())?;
        manifest.remove(id);
    }
    manifest.commit()
}

#[cfg(feature = "test-utils")]
fn delete_from_trash_in(ids: &[String], trash_dir: &std::path::Path) -> Result<(), String> {
    for id in ids {
        let path = trash_dir.join(id);
        if !path.exists() {
            return Err(format!("'{id}' is no longer in the trash"));
        }

        if path.is_dir() {
            std::fs::remove_dir_all(&path).map_err(|error| error.to_string())?;
        } else {
            std::fs::remove_file(&path).map_err(|error| error.to_string())?;
        }
    }
    // Purging does not need provenance to remove an item.  Preserve that
    // behavior if a corrupt manifest is discovered only after the filesystem
    // mutations, then perform one sidecar transaction for the whole command.
    let mut manifest = manifest::Transaction::load(trash_dir)?;
    for id in ids {
        manifest.remove(id);
    }
    manifest.force_commit()
}

#[cfg(feature = "test-utils")]
fn empty_trash_in(trash_dir: &std::path::Path) -> Result<(), String> {
    if !trash_dir.exists() {
        return Ok(());
    }

    let entries = std::fs::read_dir(trash_dir).map_err(|error| error.to_string())?;
    let mut manifest = manifest::Transaction::load(trash_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if manifest::is_manifest_file(name) {
            continue;
        }

        if path.is_dir() {
            std::fs::remove_dir_all(&path).map_err(|error| error.to_string())?;
        } else {
            std::fs::remove_file(&path).map_err(|error| error.to_string())?;
        }
    }

    manifest.force_clear_commit()
}

#[cfg(feature = "test-utils")]
pub fn list_trash() -> Result<Vec<TrashEntry>, String> {
    list_trash_in(&fake_trash_dir())
}

#[cfg(feature = "test-utils")]
pub fn restore_from_trash(ids: &[String]) -> Result<(), String> {
    restore_from_trash_in(ids, &fake_trash_dir())
}

#[cfg(feature = "test-utils")]
pub fn empty_trash() -> Result<(), String> {
    empty_trash_in(&fake_trash_dir())
}

#[cfg(feature = "test-utils")]
pub fn delete_from_trash(ids: &[String]) -> Result<(), String> {
    delete_from_trash_in(ids, &fake_trash_dir())
}

#[cfg(feature = "test-utils")]
pub fn list_trash_for_tests(trash_dir: &std::path::Path) -> Result<Vec<TrashEntry>, String> {
    list_trash_in(trash_dir)
}

#[cfg(feature = "test-utils")]
pub fn restore_from_trash_for_tests(
    ids: &[String],
    trash_dir: &std::path::Path,
) -> Result<(), String> {
    restore_from_trash_in(ids, trash_dir)
}

#[cfg(feature = "test-utils")]
pub fn empty_trash_for_tests(trash_dir: &std::path::Path) -> Result<(), String> {
    empty_trash_in(trash_dir)
}

#[cfg(feature = "test-utils")]
pub fn delete_from_trash_for_tests(
    ids: &[String],
    trash_dir: &std::path::Path,
) -> Result<(), String> {
    delete_from_trash_in(ids, trash_dir)
}
