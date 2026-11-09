use std::fs;
use std::io;
use std::path::Path;

use super::conflict::wait_while_paused;
use super::execution::{is_operation_cancelled, WorkerContext};
use super::paths::is_real_directory_type;
use super::progress::{maybe_emit_delete, refresh_rate_locked_with_now, TransferThrottle};
use super::LockOperation;
use crate::error_text::StringifyError;

pub fn delete_path_with_progress(
    source: &Path,
    add_discovered_totals: bool,
    context: &WorkerContext<'_>,
) -> Result<(), String> {
    let mut throttle = TransferThrottle::starting_now(context);
    let mut walk = DeleteWalk {
        add_discovered_totals,
        context,
        throttle: &mut throttle,
    };
    walk.delete_path(source)
}

struct DeleteWalk<'run, 'context> {
    add_discovered_totals: bool,
    context: &'run WorkerContext<'context>,
    throttle: &'run mut TransferThrottle,
}

impl DeleteWalk<'_, '_> {
    fn delete_path(&mut self, source: &Path) -> Result<(), String> {
        let metadata = fs::symlink_metadata(source).stringify_err()?;
        if is_real_directory_type(metadata.file_type()) {
            return self.delete_dir_recursive(source);
        }
        self.delete_file(source)
    }

    fn delete_dir_recursive(&mut self, source: &Path) -> Result<(), String> {
        let context = self.context;
        let entries = fs::read_dir(source).stringify_err()?;
        for entry in entries.take_while(|_| !is_operation_cancelled(context)) {
            self.delete_child(&entry.stringify_err()?)?;
        }

        if is_operation_cancelled(self.context) {
            return Ok(());
        }

        remove_clearing_readonly(source, |path| fs::remove_dir(path))
            .map_err(|error| format!("{}: {error}", source.display()))
    }

    fn delete_child(&mut self, entry: &fs::DirEntry) -> Result<(), String> {
        let file_type = entry.file_type().stringify_err()?;
        if is_real_directory_type(file_type) {
            return self.delete_dir_recursive(&entry.path());
        }
        self.delete_file(&entry.path())
    }

    fn delete_file(&mut self, source: &Path) -> Result<(), String> {
        let context = self.context;
        wait_while_paused(context.shared_operation, context.resolver);
        if is_operation_cancelled(context) {
            return Ok(());
        }

        let link_metadata = fs::symlink_metadata(source).stringify_err()?;
        let file_size = link_metadata.len();
        remove_file_or_symlink(source, &link_metadata)
            .map_err(|error| format!("{}: {error}", source.display()))?;

        self.record_deleted_bytes(file_size);
        maybe_emit_delete(context, self.throttle);
        Ok(())
    }

    fn record_deleted_bytes(&self, file_size: u64) {
        let mut operation = self.context.shared_operation.lock_operation();
        if self.add_discovered_totals {
            operation.total_bytes = operation.total_bytes.saturating_add(file_size);
        }
        operation.runtime.copied_bytes = operation.runtime.copied_bytes.saturating_add(file_size);
        operation.clear_current_file();
        refresh_rate_locked_with_now(&mut operation, self.context.rate_clock());
    }
}

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

pub fn remove_target(target: &Path) -> Result<(), String> {
    remove_path(target).stringify_err()
}

pub fn remove_source(source: &Path) -> Result<(), String> {
    remove_path(source).stringify_err()
}

fn remove_path(path: &Path) -> io::Result<()> {
    let link_metadata = fs::symlink_metadata(path)?;
    if is_real_directory_type(link_metadata.file_type()) {
        return fs::remove_dir_all(path);
    }
    remove_file_or_symlink(path, &link_metadata)
}
