use std::collections::HashSet;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
#[cfg(feature = "test-utils")]
use std::sync::atomic::{AtomicU64, Ordering};

use super::conflict::wait_while_paused;
use super::delete::{remove_clearing_readonly, remove_source, remove_target};
use super::execution::{is_operation_cancelled, WorkerContext};
use super::manifest::ProgressiveManifest;
use super::paths::{is_real_directory, is_real_directory_type};
use super::progress::{
    add_discovered_total, begin_file_progress, finish_file_progress, report_chunk_progress,
    TransferThrottle,
};
#[cfg(target_os = "macos")]
use super::transfer::macos::MacosCapabilities;
#[cfg(not(any(target_os = "macos", windows)))]
use super::transfer::portable::PortableAdapter;
#[cfg(windows)]
use super::transfer::windows::WindowsCapabilities;
use super::transfer::{self, TransferRequirements};
use super::{worker, COPY_BUFFER_BYTES};
use crate::error_text::StringifyError;

/// Test-only hook proving the capability-adapter selection genuinely
/// executes along the real copy path. `select_adapter` is `test-utils`-gated
/// to always resolve to `Portable(UnsupportedPlatform)`, so this counter is
/// the only way an integration test can distinguish "the decision ran" from
/// "the decision was skipped".
#[cfg(feature = "test-utils")]
static ADAPTER_SELECTION_CALLS: AtomicU64 = AtomicU64::new(0);

/// Number of times a real file transfer has invoked [`worker::select_adapter`].
#[cfg(feature = "test-utils")]
pub fn adapter_selection_calls_for_tests() -> u64 {
    ADAPTER_SELECTION_CALLS.load(Ordering::Relaxed)
}

pub fn copy_path(source: &Path, target: &Path, context: &WorkerContext<'_>) -> Result<(), String> {
    copy_path_with_total_discovery(source, target, false, context, None)
}

pub(super) fn copy_path_with_total_discovery(
    source: &Path,
    target: &Path,
    add_discovered_totals: bool,
    context: &WorkerContext<'_>,
    manifest: Option<&mut ProgressiveManifest>,
) -> Result<(), String> {
    let source_metadata = fs::symlink_metadata(source).stringify_err()?;
    let mut whole_transfer_throttle = TransferThrottle::starting_now(context);
    if source_metadata.file_type().is_symlink() {
        return copy_symlink_with_total_discovery(
            source,
            target,
            add_discovered_totals,
            context,
            Some(&mut whole_transfer_throttle),
        );
    }
    if source_metadata.is_dir() {
        let root = source.canonicalize().stringify_err()?;
        let mut visited = HashSet::from([root.clone()]);
        let tree = TreeCopy {
            root: &root,
            visited: &mut visited,
            add_discovered_totals,
            context,
            throttle: Some(&mut whole_transfer_throttle),
            manifest,
        };
        return copy_dir_recursive(tree, source, target);
    }
    copy_file_with_total_discovery(
        source,
        target,
        add_discovered_totals,
        context,
        Some(&mut whole_transfer_throttle),
    )
}

pub fn copy_dir_recursive(
    mut tree: TreeCopy<'_, '_>,
    source: &Path,
    target: &Path,
) -> Result<(), String> {
    tree.copy_directory(source, target)
}

pub struct TreeCopy<'walk, 'context> {
    pub root: &'walk Path,
    pub visited: &'walk mut HashSet<PathBuf>,
    pub add_discovered_totals: bool,
    pub context: &'walk WorkerContext<'context>,
    pub throttle: Option<&'walk mut TransferThrottle>,
    pub manifest: Option<&'walk mut ProgressiveManifest>,
}

impl TreeCopy<'_, '_> {
    fn copy_directory(&mut self, source: &Path, target: &Path) -> Result<(), String> {
        clear_merge_target(target, true)?;
        fs::create_dir_all(target).stringify_err()?;
        let context = self.context;
        let entries = fs::read_dir(source)
            .stringify_err()?
            .take_while(|_| !is_operation_cancelled(context));
        for entry in entries {
            self.copy_entry(&entry.stringify_err()?, target)?;
        }
        Ok(())
    }

    fn copy_entry(&mut self, entry: &fs::DirEntry, target: &Path) -> Result<(), String> {
        let child_source = entry.path();
        let child_target = target.join(entry.file_name());
        let file_type = entry.file_type().stringify_err()?;
        if file_type.is_symlink() {
            self.record_in_manifest(&child_source, true)?;
            return copy_symlink_with_total_discovery(
                &child_source,
                &child_target,
                self.add_discovered_totals,
                self.context,
                self.throttle.as_deref_mut(),
            );
        }
        if file_type.is_dir() {
            return self.copy_child_directory(&child_source, &child_target);
        }
        self.record_in_manifest(&child_source, false)?;
        copy_file_with_total_discovery(
            &child_source,
            &child_target,
            self.add_discovered_totals,
            self.context,
            self.throttle.as_deref_mut(),
        )
    }

    fn record_in_manifest(&mut self, child_source: &Path, is_link: bool) -> Result<(), String> {
        let Some(manifest) = self.manifest.as_deref_mut() else {
            return Ok(());
        };
        let link_metadata = fs::symlink_metadata(child_source).stringify_err()?;
        manifest.record(
            self.root,
            child_source.to_path_buf(),
            link_metadata.len(),
            is_link,
        );
        Ok(())
    }

    fn copy_child_directory(
        &mut self,
        child_source: &Path,
        child_target: &Path,
    ) -> Result<(), String> {
        let canonical_child = child_source.canonicalize().stringify_err()?;
        if !canonical_child.starts_with(self.root) {
            return Err(format!(
                "Refusing to copy linked directory \"{}\" outside the selected source tree.",
                child_source.display()
            ));
        }
        if !self.visited.insert(canonical_child.clone()) {
            return Err(format!(
                "Refusing to recurse into cyclic directory link \"{}\".",
                child_source.display()
            ));
        }
        self.copy_directory(child_source, child_target)?;
        self.visited.remove(&canonical_child);
        Ok(())
    }
}

/// Recreate a symlink without dereferencing it. This preserves relative,
/// dangling, and directory links, so a copied tree has the same topology as
/// its source and does not accidentally walk outside the selected folder.
fn copy_symlink_with_total_discovery(
    source: &Path,
    target: &Path,
    add_discovered_total_bytes: bool,
    context: &WorkerContext<'_>,
    mut throttle: Option<&mut TransferThrottle>,
) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).stringify_err()?;
    }

    clear_merge_target(target, false)?;

    let link_metadata = fs::symlink_metadata(source).stringify_err()?;
    let link_size = link_metadata.len();
    if add_discovered_total_bytes {
        add_discovered_total(context, link_size, throttle.as_deref_mut());
    }
    begin_file_progress(context, source, link_size, throttle.as_deref_mut());

    let link_target = fs::read_link(source).stringify_err()?;
    create_symlink(source, &link_target, target).stringify_err()?;

    report_chunk_progress(context, link_size, throttle.as_deref_mut());
    finish_file_progress(context, throttle);
    Ok(())
}

#[cfg(unix)]
fn create_symlink(_source: &Path, link_target: &Path, target: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(link_target, target)
}

#[cfg(windows)]
fn create_symlink(source: &Path, link_target: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::fs::FileTypeExt;

    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink_dir() {
        return std::os::windows::fs::symlink_dir(link_target, target);
    }
    std::os::windows::fs::symlink_file(link_target, target)
}

#[cfg(not(any(unix, windows)))]
fn create_symlink(_source: &Path, _link_target: &Path, _target: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "symlink copying is unsupported on this platform",
    ))
}
pub(super) fn copy_reader_with_progress<R, W>(
    reader: &mut R,
    writer: &mut W,
    context: &WorkerContext<'_>,
    mut throttle: Option<&mut TransferThrottle>,
) -> Result<(), String>
where
    R: Read,
    W: Write,
{
    let mut buffer = [0_u8; COPY_BUFFER_BYTES];
    loop {
        wait_while_paused(context.shared_operation, context.resolver);
        if is_operation_cancelled(context) {
            return Ok(());
        }

        let read = reader.read(&mut buffer).stringify_err()?;
        if read == 0 {
            break;
        }
        writer.write_all(&buffer[..read]).stringify_err()?;
        report_chunk_progress(context, read as u64, throttle.as_deref_mut());
    }
    Ok(())
}

pub fn move_path(source: &Path, target: &Path, context: &WorkerContext<'_>) -> Result<(), String> {
    move_path_with_total_discovery(source, target, false, context)
}

pub(super) fn move_path_with_total_discovery(
    source: &Path,
    target: &Path,
    add_discovered_totals: bool,
    context: &WorkerContext<'_>,
) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).stringify_err()?;
    }
    // `same_resource: true` preserves current behavior: rename is always
    // attempted first regardless of resource identity, and only an
    // OS-confirmed cross-device error falls back to copy+delete.
    let renamed =
        worker::rename_or_cross_device(source, target, true, |src, dst| fs::rename(src, dst))?;
    if renamed {
        return Ok(());
    }
    // The progressive copy path grows totals while it discovers files, and
    // source removal happens only after destination completion. A fresh
    // manifest records every file discovered along this fallback walk so the
    // cross-device copy leaves a real, inspectable transfer record behind.
    let mut manifest = ProgressiveManifest::default();
    copy_path_with_total_discovery(
        source,
        target,
        add_discovered_totals,
        context,
        Some(&mut manifest),
    )?;
    if is_operation_cancelled(context) {
        return Ok(());
    }
    remove_source(source)
}

/// Move-merge `source` into the existing real directory `target`: children
/// that collide as directories recurse, anything else replaces whatever is in
/// the way and moves by rename (with the per-child cross-device fallback).
pub(super) fn move_merge_dir(
    source: &Path,
    target: &Path,
    context: &WorkerContext<'_>,
) -> Result<(), String> {
    for entry in fs::read_dir(source).stringify_err()? {
        if is_operation_cancelled(context) {
            return Ok(());
        }
        let entry = entry.stringify_err()?;
        let child_source = entry.path();
        let child_target = target.join(entry.file_name());
        let source_is_real_directory = is_real_directory_type(entry.file_type().stringify_err()?);
        let merges_into_existing_directory =
            source_is_real_directory && is_real_directory(&child_target);
        if merges_into_existing_directory {
            move_merge_dir(&child_source, &child_target, context)?;
            continue;
        }
        clear_merge_target(&child_target, false)?;
        move_path_with_total_discovery(&child_source, &child_target, false, context)?;
    }
    if is_operation_cancelled(context) {
        return Ok(());
    }
    remove_clearing_readonly(source, |path| fs::remove_dir(path)).stringify_err()
}

/// Clear whatever sits at `target` so a merge can write there. A real
/// directory is kept when `keep_real_dir` (merge into it); anything else is
/// removed so the walk never writes through a link or trips on a type clash.
fn clear_merge_target(target: &Path, keep_real_dir: bool) -> Result<(), String> {
    match fs::symlink_metadata(target) {
        Err(_) => Ok(()),
        Ok(metadata) if keep_real_dir && is_real_directory_type(metadata.file_type()) => Ok(()),
        Ok(_) => remove_target(target),
    }
}

pub fn copy_file_with_progress(
    source: &Path,
    target: &Path,
    context: &WorkerContext<'_>,
) -> Result<(), String> {
    copy_file_with_total_discovery(source, target, false, context, None)
}

/// Runs the capability-adapter selection decision for a single file transfer
/// and logs the outcome at debug level. The actual byte transfer always runs
/// through the portable streaming loop below regardless of the selection
/// result — no native `CopyFile2`/`copyfile` implementation exists yet, so
/// every real platform resolves to `Portable(..)` today. This makes the
/// selection scaffolding in `worker::select_adapter` and the platform
/// capability structs genuinely execute on the real copy path instead of
/// sitting unreachable.
fn log_transfer_adapter_selection(source: &Path) {
    #[cfg(target_os = "macos")]
    let adapter: MacosCapabilities = MacosCapabilities::default();
    #[cfg(windows)]
    let adapter: WindowsCapabilities = WindowsCapabilities::default();
    #[cfg(not(any(target_os = "macos", windows)))]
    let adapter: PortableAdapter = PortableAdapter;

    let requirements = TransferRequirements {
        progress: true,
        cancellation: true,
        ..TransferRequirements::default()
    };
    let selection = worker::select_adapter(&adapter, requirements);

    #[cfg(feature = "test-utils")]
    ADAPTER_SELECTION_CALLS.fetch_add(1, Ordering::Relaxed);

    match &selection {
        transfer::AdapterSelection::Portable(reason) => {
            log::debug!(
                "transfer adapter selection for \"{}\": portable fallback ({reason:?})",
                source.display()
            );
        }
        transfer::AdapterSelection::WindowsCopyFile2 => {
            log::debug!(
                "transfer adapter selection for \"{}\": WindowsCopyFile2",
                source.display()
            );
        }
        transfer::AdapterSelection::MacosCopyfile => {
            log::debug!(
                "transfer adapter selection for \"{}\": MacosCopyfile",
                source.display()
            );
        }
    }
}

fn copy_file_with_total_discovery(
    source: &Path,
    target: &Path,
    add_discovered_total_bytes: bool,
    context: &WorkerContext<'_>,
    mut throttle: Option<&mut TransferThrottle>,
) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).stringify_err()?;
    }

    clear_merge_target(target, false)?;
    log_transfer_adapter_selection(source);

    let file_size = fs::metadata(source)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if add_discovered_total_bytes {
        add_discovered_total(context, file_size, throttle.as_deref_mut());
    }
    begin_file_progress(context, source, file_size, throttle.as_deref_mut());

    let mut reader = fs::File::open(source).stringify_err()?;
    let mut writer = fs::File::create(target).stringify_err()?;
    copy_reader_with_progress(&mut reader, &mut writer, context, throttle.as_deref_mut())?;
    if !is_operation_cancelled(context) {
        writer.flush().stringify_err()?;
    }
    finish_file_progress(context, throttle);
    Ok(())
}
