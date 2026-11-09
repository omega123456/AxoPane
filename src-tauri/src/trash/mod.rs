use serde::{Deserialize, Serialize};

#[cfg(any(feature = "test-utils", target_os = "macos"))]
mod manifest;

#[cfg(any(feature = "test-utils", target_os = "macos"))]
mod directory_bin;

#[cfg(any(feature = "test-utils", target_os = "macos"))]
mod macos_adapter;

#[cfg(any(feature = "test-utils", target_os = "macos"))]
mod dsstore;

#[cfg(all(not(feature = "test-utils"), windows))]
mod windows_bin;

#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
mod macos_bin;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    pub id: String,
    pub name: String,
    pub original_path: Option<String>,
    pub size_bytes: Option<u64>,
    pub is_dir: bool,
    pub deleted_at: Option<i64>,
}

pub fn move_to_trash(paths: &[String]) -> Result<(), String> {
    move_to_trash_cancellable(paths, || false)
}

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

#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
fn move_to_trash_impl(paths: &[String]) -> Result<(), String> {
    let mut adapter = macos_adapter::MacosSystemTrashAdapter::new();
    let result = macos_adapter::delete_batch(&mut adapter, paths);
    if let Some((_, error)) = result.failures.into_iter().next() {
        return Err(error);
    }
    Ok(())
}

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
    BatchCorrelation, BatchDeletionResult, ManifestBatch, StableFileIdentity, SystemTrashAdapter,
    TrashFacts, TrashSnapshotEntry,
};

#[cfg(feature = "test-utils")]
pub fn delete_batch_with_adapter_for_tests<A: SystemTrashAdapter>(
    adapter: &mut A,
    paths: &[String],
) -> BatchDeletionResult {
    macos_adapter::delete_batch(adapter, paths)
}

#[cfg(feature = "test-utils")]
pub fn parse_put_back_for_tests(bytes: &[u8]) -> std::collections::HashMap<String, PutBack> {
    dsstore::parse_put_back(bytes)
}

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

        ensure_fake_trash_dir(trash_dir)?;
        move_to_fake_trash_path(source, &target)?;
        let deleted_at = crate::epoch::now_since_epoch().as_secs() as i64;
        manifest_entries.push((target_name, path.to_string(), deleted_at));
    }

    let _ = manifest::record_batch(trash_dir, manifest_entries);

    Ok(())
}

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
pub fn list_trash() -> Result<Vec<TrashEntry>, String> {
    directory_bin::list_trash_in::<manifest::Transaction>(&fake_trash_dir())
}

#[cfg(feature = "test-utils")]
pub fn restore_from_trash(ids: &[String]) -> Result<(), String> {
    directory_bin::restore_from_trash_in::<manifest::Transaction>(ids, &fake_trash_dir())
}

#[cfg(feature = "test-utils")]
pub fn empty_trash() -> Result<(), String> {
    directory_bin::empty_trash_in::<manifest::Transaction>(&fake_trash_dir())
}

#[cfg(feature = "test-utils")]
pub fn delete_from_trash(ids: &[String]) -> Result<(), String> {
    directory_bin::delete_from_trash_in::<manifest::Transaction>(ids, &fake_trash_dir())
}

#[cfg(feature = "test-utils")]
pub fn list_trash_for_tests(trash_dir: &std::path::Path) -> Result<Vec<TrashEntry>, String> {
    directory_bin::list_trash_in::<manifest::Transaction>(trash_dir)
}

#[cfg(feature = "test-utils")]
pub fn restore_from_trash_for_tests(
    ids: &[String],
    trash_dir: &std::path::Path,
) -> Result<(), String> {
    directory_bin::restore_from_trash_in::<manifest::Transaction>(ids, trash_dir)
}

#[cfg(feature = "test-utils")]
pub fn empty_trash_for_tests(trash_dir: &std::path::Path) -> Result<(), String> {
    directory_bin::empty_trash_in::<manifest::Transaction>(trash_dir)
}

#[cfg(feature = "test-utils")]
pub fn delete_from_trash_for_tests(
    ids: &[String],
    trash_dir: &std::path::Path,
) -> Result<(), String> {
    directory_bin::delete_from_trash_in::<manifest::Transaction>(ids, trash_dir)
}
