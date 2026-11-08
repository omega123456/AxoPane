use std::fs;
use std::io;
use std::path::Path;

use super::conflict::wait_while_paused;
use super::execution::{is_operation_cancelled, WorkerContext};
use super::paths::is_real_directory_type;
use super::progress::{maybe_emit_delete, refresh_rate_locked_with_now, TransferThrottle};
use super::LockOperation;
use crate::error_text::StringifyError;

/// Remove `source` (file or directory tree) reporting byte progress through the
/// shared worker context, honoring pause/cancel. When `add_discovered_totals`
/// is set (a directory whose size was not pre-measured), each removed file's
/// bytes are folded into the operation total as they are discovered.
pub fn delete_path_with_progress(
    source: &Path,
    add_discovered_totals: bool,
    context: &WorkerContext<'_>,
) -> Result<(), String> {
    let mut throttle = TransferThrottle::starting_now(context);
    delete_path_with_progress_throttled(source, add_discovered_totals, context, &mut throttle)
}

fn delete_path_with_progress_throttled(
    source: &Path,
    add_discovered_totals: bool,
    context: &WorkerContext<'_>,
    throttle: &mut TransferThrottle,
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source).stringify_err()?;
    if is_real_directory_type(metadata.file_type()) {
        return delete_dir_recursive(source, add_discovered_totals, context, throttle);
    }
    delete_file_with_progress(source, add_discovered_totals, context, throttle)
}

fn delete_dir_recursive(
    source: &Path,
    add_discovered_totals: bool,
    context: &WorkerContext<'_>,
    throttle: &mut TransferThrottle,
) -> Result<(), String> {
    for entry in fs::read_dir(source).stringify_err()? {
        if is_operation_cancelled(context) {
            return Ok(());
        }

        let entry = entry.stringify_err()?;
        let file_type = entry.file_type().stringify_err()?;
        if is_real_directory_type(file_type) {
            delete_dir_recursive(&entry.path(), add_discovered_totals, context, throttle)?;
            continue;
        }
        delete_file_with_progress(&entry.path(), add_discovered_totals, context, throttle)?;
    }

    if is_operation_cancelled(context) {
        return Ok(());
    }

    remove_clearing_readonly(source, |path| fs::remove_dir(path))
        .map_err(|error| format!("{}: {error}", source.display()))
}

/// Windows refuses to delete a read-only file or directory with "Access is
/// denied" — even for an administrator — so clear the attribute and retry
/// once. Unix deletion depends only on the parent directory's permissions.
#[cfg(windows)]
pub(super) fn remove_clearing_readonly(
    path: &Path,
    remove: impl Fn(&Path) -> io::Result<()>,
) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::{
        SetFileAttributesW, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_READONLY,
    };

    let error = match remove(path) {
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => error,
        result => return result,
    };
    let attributes = fs::symlink_metadata(path)?.file_attributes();
    if attributes & FILE_ATTRIBUTE_READONLY == 0 {
        return Err(error);
    }
    log::info!("clearing read-only attribute to delete {}", path.display());
    let cleared = match attributes & !FILE_ATTRIBUTE_READONLY {
        0 => FILE_ATTRIBUTE_NORMAL,
        rest => rest,
    };
    let wide = crate::win32::wide_nul(path);
    if unsafe { SetFileAttributesW(wide.as_ptr(), cleared) } == 0 {
        return Err(io::Error::last_os_error());
    }
    remove(path)
}

#[cfg(not(windows))]
pub(super) fn remove_clearing_readonly(
    path: &Path,
    remove: impl Fn(&Path) -> io::Result<()>,
) -> io::Result<()> {
    remove(path)
}

/// Removes a file or symlink at `path`.
///
/// On Windows, `DeleteFileW` (which `fs::remove_file` uses) fails with
/// "Access is denied" for a symlink/junction that points at a directory —
/// those reparse points must go through `RemoveDirectoryW`
/// (`fs::remove_dir`) instead, even though the link itself isn't recursed
/// into. `FileTypeExt::is_symlink_dir` reads that distinction straight off
/// the reparse point, so it works even for a broken/dangling symlink whose
/// target no longer exists. Unix `unlink` has no such distinction, so
/// `remove_file` always works there regardless of what the symlink targets.
#[cfg(windows)]
fn remove_file_or_symlink(path: &Path, link_metadata: &fs::Metadata) -> io::Result<()> {
    use std::os::windows::fs::FileTypeExt;

    if link_metadata.file_type().is_symlink_dir() {
        return remove_clearing_readonly(path, |path| fs::remove_dir(path));
    }
    remove_clearing_readonly(path, |path| fs::remove_file(path))
}

#[cfg(not(windows))]
fn remove_file_or_symlink(path: &Path, _link_metadata: &fs::Metadata) -> io::Result<()> {
    fs::remove_file(path)
}

fn delete_file_with_progress(
    source: &Path,
    add_discovered_totals: bool,
    context: &WorkerContext<'_>,
    throttle: &mut TransferThrottle,
) -> Result<(), String> {
    wait_while_paused(context.shared_operation, context.resolver);
    if is_operation_cancelled(context) {
        return Ok(());
    }

    let link_metadata = fs::symlink_metadata(source).stringify_err()?;
    let file_size = link_metadata.len();
    remove_file_or_symlink(source, &link_metadata)
        .map_err(|error| format!("{}: {error}", source.display()))?;

    {
        let mut operation = context.shared_operation.lock_operation();
        if add_discovered_totals {
            operation.total_bytes = operation.total_bytes.saturating_add(file_size);
        }
        operation.copied_bytes = operation.copied_bytes.saturating_add(file_size);
        operation.clear_current_file();
        refresh_rate_locked_with_now(
            &mut operation,
            context.start,
            context.rate_window,
            (context.instant_now)(),
        );
    }
    maybe_emit_delete(context, throttle);
    Ok(())
}

pub fn remove_target(target: &Path) -> Result<(), String> {
    remove_path(target).stringify_err()
}

pub fn remove_source(source: &Path) -> Result<(), String> {
    remove_path(source).stringify_err()
}

/// Removes a file, symlink, or directory at `path`.
///
/// A dangling directory symlink/junction still reports as a reparse point
/// rather than a real directory, so this checks `symlink_metadata` (which
/// doesn't follow the link) instead of `Path::is_dir()` before deciding
/// between `remove_dir_all` and [`remove_file_or_symlink`] — otherwise a
/// broken directory symlink on Windows would hit `remove_file` and fail with
/// access denied, same as the bug this guards against for live symlinks.
fn remove_path(path: &Path) -> io::Result<()> {
    let link_metadata = fs::symlink_metadata(path)?;
    if is_real_directory_type(link_metadata.file_type()) {
        return fs::remove_dir_all(path);
    }
    remove_file_or_symlink(path, &link_metadata)
}
