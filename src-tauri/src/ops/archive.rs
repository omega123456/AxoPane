//! Archive operation helpers live under the queue owner.
//!
//! ZIP execution is intentionally dispatched by `OpsService`; this module is
//! the stable home for archive-specific code.

use std::fs;
#[cfg(unix)]
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use zip::read::ZipFile;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::conflict::wait_while_paused;
use super::copy::{copy_reader_with_progress, ChunkReporter};
use super::execution::{is_operation_cancelled, NativeItemPaths, WorkerContext};
use super::progress::{
    add_discovered_total, begin_file_progress, finish_file_progress, report_chunk_progress,
    FileProgressStart, TransferThrottle,
};
use super::{LockOperation, OpItem, OpKind};
use crate::error_text::StringifyError;
use crate::traversal::{TraversalEntry, TraversalError, TraversalOptions};

#[derive(Clone, Copy)]
struct MemberPaths<'path> {
    source: &'path Path,
    archive_path: &'path Path,
}

pub fn needs_cpu_admission(kind: OpKind) -> bool {
    matches!(kind, OpKind::Compress | OpKind::Extract)
}

pub(super) fn compress_item_with_progress(
    paths: NativeItemPaths<'_>,
    item: &OpItem,
    context: &WorkerContext<'_>,
) -> Result<(), String> {
    let NativeItemPaths {
        source,
        target: archive_path,
    } = paths;
    if !is_zip_archive_path(archive_path) {
        return Err("Archive output must end in .zip.".to_string());
    }
    if !source.exists() {
        return Err(format!(
            "Archive source does not exist: {}",
            source.display()
        ));
    }
    let mut writer = open_archive_writer(archive_path, is_later_archive_item(item, context))?;
    // One throttle shared across every member appended below (mirrors how
    // `copy_dir_recursive` shares a single throttle through its recursion).
    let mut throttle = TransferThrottle::starting_now(context);
    let mut append = ArchiveAppend {
        writer: &mut writer,
        add_discovered_totals: item.size_bytes == 0,
        context,
        throttle: Some(&mut throttle),
    };
    append.append_path(source, &archive_root_name(source))?;
    writer.finish().stringify_err()?;
    Ok(())
}

/// A multi-item compress builds one archive: the first item creates it and
/// every later item appends to it.
fn is_later_archive_item(item: &OpItem, context: &WorkerContext<'_>) -> bool {
    context
        .shared_operation
        .lock_operation()
        .items
        .iter()
        .position(|queued| queued.source_path == item.source_path)
        .is_some_and(|index| index > 0)
}

fn open_archive_writer(
    archive_path: &Path,
    is_later_item: bool,
) -> Result<ZipWriter<fs::File>, String> {
    if is_later_item && archive_path.exists() {
        let archive_file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(archive_path)
            .stringify_err()?;
        return ZipWriter::new_append(archive_file).stringify_err();
    }
    if archive_path.exists() {
        return Err(format!(
            "Archive already exists: {}",
            archive_path.display()
        ));
    }
    let parent = archive_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).stringify_err()?;
    let archive_file = fs::File::create_new(archive_path).stringify_err()?;
    Ok(ZipWriter::new(archive_file))
}

pub(super) fn extract_item_with_progress(
    paths: NativeItemPaths<'_>,
    item: &OpItem,
    context: &WorkerContext<'_>,
) -> Result<(), String> {
    let NativeItemPaths {
        source,
        target: destination_dir,
    } = paths;
    ensure_extract_destination(destination_dir)?;
    if !is_zip_archive_path(source) {
        return Err("Only .zip archives can be extracted.".to_string());
    }

    let archive_stem = archive_stem_for_item(source);
    let output_root = unique_archive_directory_path(destination_dir, &archive_stem);
    let discovered_total = zip_uncompressed_size(source)?;
    if item.size_bytes == 0 {
        add_discovered_total(context, discovered_total, None);
    }

    fs::create_dir_all(&output_root).stringify_err()?;
    let file = fs::File::open(source).stringify_err()?;
    let mut archive = ZipArchive::new(file).stringify_err()?;
    let wrapper_root = detect_redundant_wrapper_root(&mut archive, &archive_stem)?;
    let mut extraction = Extraction {
        output_root,
        wrapper_root,
        context,
        throttle: TransferThrottle::starting_now(context),
        directory_modes: Vec::new(),
    };

    for index in 0..archive.len() {
        wait_while_paused(context.shared_operation, context.resolver);
        if is_operation_cancelled(context) {
            return Ok(());
        }
        let mut entry = archive.by_index(index).stringify_err()?;
        extraction.extract_entry(&mut entry)?;
    }

    extraction.apply_directory_modes()
}

fn ensure_extract_destination(destination_dir: &Path) -> Result<(), String> {
    if !destination_dir.exists() {
        return fs::create_dir_all(destination_dir).stringify_err();
    }
    if !destination_dir.is_dir() {
        return Err(format!(
            "Extract destination is not a folder: {}",
            destination_dir.display()
        ));
    }
    Ok(())
}

/// Directory modes are applied after every member is written so a read-only
/// folder cannot block extraction of its own contents.
struct Extraction<'walk, 'context> {
    output_root: PathBuf,
    wrapper_root: Option<String>,
    context: &'walk WorkerContext<'context>,
    throttle: TransferThrottle,
    directory_modes: Vec<(PathBuf, Option<u32>)>,
}

impl Extraction<'_, '_> {
    fn extract_entry(&mut self, entry: &mut ZipFile<'_, fs::File>) -> Result<(), String> {
        let enclosed_name = entry
            .enclosed_name()
            .ok_or_else(|| "Archive contains an unsafe entry path.".to_string())?;
        let relative_name = strip_wrapper_root(&enclosed_name, self.wrapper_root.as_deref());
        let output_path =
            crate::traversal::safe_destination(&self.output_root, relative_name).stringify_err()?;

        if entry.is_dir() {
            fs::create_dir_all(&output_path).stringify_err()?;
            self.directory_modes.push((output_path, entry.unix_mode()));
            return Ok(());
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).stringify_err()?;
        }
        begin_file_progress(
            self.context,
            FileProgressStart {
                source: &output_path,
                total_bytes: entry.size(),
            },
            Some(&mut self.throttle),
        );
        #[cfg(unix)]
        if entry.is_symlink() {
            return self.extract_symlink(entry, &output_path);
        }
        self.extract_file(entry, &output_path)
    }

    /// ZIP stores a link's target as its payload. `safe_destination` refuses
    /// to write later members through any link created here.
    #[cfg(unix)]
    fn extract_symlink(
        &mut self,
        entry: &mut ZipFile<'_, fs::File>,
        output_path: &Path,
    ) -> Result<(), String> {
        let mut target = String::new();
        entry.read_to_string(&mut target).stringify_err()?;
        std::os::unix::fs::symlink(&target, output_path).stringify_err()?;
        report_chunk_progress(self.context, entry.size(), Some(&mut self.throttle));
        finish_file_progress(self.context, Some(&mut self.throttle));
        Ok(())
    }

    fn extract_file(
        &mut self,
        entry: &mut ZipFile<'_, fs::File>,
        output_path: &Path,
    ) -> Result<(), String> {
        let mut output_file = fs::File::create(output_path).stringify_err()?;
        let reporter = ChunkReporter {
            context: self.context,
            throttle: Some(&mut self.throttle),
        };
        copy_reader_with_progress(entry, &mut output_file, reporter)?;
        output_file.flush().stringify_err()?;
        drop(output_file);
        apply_extracted_mode(output_path, entry.unix_mode())?;
        finish_file_progress(self.context, Some(&mut self.throttle));
        Ok(())
    }

    /// Deepest first: chmod on a child needs search permission on its parent.
    fn apply_directory_modes(&self) -> Result<(), String> {
        for (path, mode) in self.directory_modes.iter().rev() {
            apply_extracted_mode(path, *mode)?;
        }
        Ok(())
    }
}

/// Restores an archived Unix mode on an extracted file or folder. Setuid,
/// setgid and sticky bits are dropped (as `unzip` does by default). Windows
/// has no mode bits, so only the read-only flag is honoured there.
fn apply_extracted_mode(path: &Path, mode: Option<u32>) -> Result<(), String> {
    let Some(mode) = mode else {
        return Ok(());
    };
    #[cfg(unix)]
    let permissions = {
        use std::os::unix::fs::PermissionsExt;
        fs::Permissions::from_mode(mode & 0o777)
    };
    #[cfg(not(unix))]
    let permissions = {
        let mut permissions = fs::metadata(path).stringify_err()?.permissions();
        permissions.set_readonly(mode & 0o222 == 0);
        permissions
    };
    fs::set_permissions(path, permissions).stringify_err()
}

/// Appends a file, a symlink, or a directory tree to `writer`. `throttle` is
/// shared across the *whole* member loop of one compress operation so a
/// many-small-member source tree does not re-flood by minting a fresh
/// throttle per member.
struct ArchiveAppend<'walk, 'context> {
    writer: &'walk mut ZipWriter<fs::File>,
    add_discovered_totals: bool,
    context: &'walk WorkerContext<'context>,
    throttle: Option<&'walk mut TransferThrottle>,
}

impl ArchiveAppend<'_, '_> {
    /// Never follow a directory link while archiving. ZIP stores the link as a
    /// link entry (its payload is the link target), so cycles and targets
    /// outside the selected root cannot become archived content.
    fn append_path(&mut self, source: &Path, archive_path: &Path) -> Result<(), String> {
        wait_while_paused(self.context.shared_operation, self.context.resolver);
        if is_operation_cancelled(self.context) {
            return Ok(());
        }

        let metadata = fs::symlink_metadata(source).stringify_err()?;
        if metadata.file_type().is_symlink() {
            return self.append_symlink(source, archive_path);
        }
        if metadata.is_dir() {
            let member = MemberPaths {
                source,
                archive_path,
            };
            return self.append_tree(member, &metadata);
        }
        self.append_file(source, archive_path)
    }

    /// Walks the subtree with the shared `crate::traversal::walk` service
    /// (flat, bounded iteration). Archive member order is intentionally not a
    /// contract, so the walk never materializes or sorts a wide directory.
    fn append_tree(
        &mut self,
        member: MemberPaths<'_>,
        metadata: &fs::Metadata,
    ) -> Result<(), String> {
        let MemberPaths {
            source,
            archive_path,
        } = member;
        self.add_directory_entry(archive_path, metadata)?;
        let cancel = Arc::clone(&self.context.shared_operation.lock_operation().cancel);
        let mut entries =
            crate::traversal::walk(source, TraversalOptions::default(), cancel).stringify_err()?;
        while let Some(entry) = self.next_entry_to_append(&mut entries)? {
            self.append_walked_entry(member, &entry)?;
        }
        Ok(())
    }

    fn next_entry_to_append(
        &self,
        entries: &mut impl Iterator<Item = Result<TraversalEntry, TraversalError>>,
    ) -> Result<Option<TraversalEntry>, String> {
        let Some(entry) = entries.next() else {
            return Ok(None);
        };
        wait_while_paused(self.context.shared_operation, self.context.resolver);
        if is_operation_cancelled(self.context) {
            return Ok(None);
        }
        match entry {
            Ok(entry) => Ok(Some(entry)),
            Err(TraversalError::Cancelled) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn append_walked_entry(
        &mut self,
        root: MemberPaths<'_>,
        entry: &TraversalEntry,
    ) -> Result<(), String> {
        let MemberPaths {
            source: source_root,
            archive_path: archive_root,
        } = root;
        let relative = entry.path.strip_prefix(source_root).stringify_err()?;
        let member_archive_path = archive_root.join(relative);
        if entry.file_type.is_symlink() {
            return self.append_symlink(&entry.path, &member_archive_path);
        }
        if entry.file_type.is_dir() {
            let member_metadata = fs::symlink_metadata(&entry.path).stringify_err()?;
            return self.add_directory_entry(&member_archive_path, &member_metadata);
        }
        self.append_file(&entry.path, &member_archive_path)
    }

    fn add_directory_entry(
        &mut self,
        archive_path: &Path,
        metadata: &fs::Metadata,
    ) -> Result<(), String> {
        let mut directory_name = to_zip_path(archive_path)?;
        if !directory_name.ends_with('/') {
            directory_name.push('/');
        }
        self.writer
            .add_directory(directory_name, zip_dir_options(metadata))
            .stringify_err()
    }

    /// Stores the link target as the entry's content (ZIP symlink convention).
    fn append_symlink(&mut self, source: &Path, archive_path: &Path) -> Result<(), String> {
        let target = fs::read_link(source).stringify_err()?;
        let target = target.to_string_lossy();
        let target_length = target.len() as u64;
        if self.add_discovered_totals {
            add_discovered_total(self.context, target_length, self.throttle.as_deref_mut());
        }
        begin_file_progress(
            self.context,
            FileProgressStart {
                source,
                total_bytes: target_length,
            },
            self.throttle.as_deref_mut(),
        );
        // `unix_permissions` masks to 0o777, so only `add_symlink` sets S_IFLNK.
        self.writer
            .add_symlink(
                to_zip_path(archive_path)?,
                &*target,
                SimpleFileOptions::default(),
            )
            .stringify_err()?;
        report_chunk_progress(self.context, target_length, self.throttle.as_deref_mut());
        finish_file_progress(self.context, self.throttle.as_deref_mut());
        Ok(())
    }

    fn append_file(&mut self, source: &Path, archive_path: &Path) -> Result<(), String> {
        let metadata = fs::metadata(source).stringify_err()?;
        let file_size = metadata.len();
        if self.add_discovered_totals {
            add_discovered_total(self.context, file_size, self.throttle.as_deref_mut());
        }
        begin_file_progress(
            self.context,
            FileProgressStart {
                source,
                total_bytes: file_size,
            },
            self.throttle.as_deref_mut(),
        );
        self.writer
            .start_file(to_zip_path(archive_path)?, zip_file_options(&metadata))
            .stringify_err()?;
        let mut input_file = fs::File::open(source).stringify_err()?;
        let reporter = ChunkReporter {
            context: self.context,
            throttle: self.throttle.as_deref_mut(),
        };
        copy_reader_with_progress(&mut input_file, self.writer, reporter)?;
        finish_file_progress(self.context, self.throttle.as_deref_mut());
        Ok(())
    }
}

pub(super) fn zip_uncompressed_size(path: &Path) -> Result<u64, String> {
    let file = fs::File::open(path).stringify_err()?;
    let mut archive = ZipArchive::new(file).stringify_err()?;
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).stringify_err()?;
        if !entry.is_dir() {
            total = total.saturating_add(entry.size());
        }
    }
    Ok(total)
}

pub(super) fn detect_redundant_wrapper_root(
    archive: &mut ZipArchive<fs::File>,
    archive_stem: &str,
) -> Result<Option<String>, String> {
    let normalized_stem = archive_stem.trim();
    if normalized_stem.is_empty() {
        return Ok(None);
    }

    let mut candidate_root: Option<String> = None;
    let mut saw_nested_entry = false;

    for index in 0..archive.len() {
        let entry = archive.by_index(index).stringify_err()?;
        let Some(enclosed_name) = entry.enclosed_name() else {
            return Err("Archive contains an unsafe entry path.".to_string());
        };

        let mut components = enclosed_name.components();
        let Some(first) = components.next() else {
            continue;
        };
        let Some(first_name) = first.as_os_str().to_str() else {
            return Ok(None);
        };
        if candidate_root
            .as_deref()
            .is_some_and(|current| current != first_name)
        {
            return Ok(None);
        }
        candidate_root.get_or_insert_with(|| first_name.to_string());
        let is_nested_entry = components.next().is_some();
        if is_nested_entry {
            saw_nested_entry = true;
            continue;
        }
        if !entry.is_dir() {
            return Ok(None);
        }
    }

    Ok(
        candidate_root
            .filter(|root| saw_nested_entry && root.eq_ignore_ascii_case(normalized_stem)),
    )
}

pub(super) fn strip_wrapper_root<'a>(path: &'a Path, wrapper_root: Option<&str>) -> &'a Path {
    let Some(wrapper_root) = wrapper_root else {
        return path;
    };

    let mut components = path.components();
    let Some(first) = components.next() else {
        return path;
    };
    if !first
        .as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(wrapper_root)
    {
        return path;
    }

    components.as_path()
}

pub(super) fn archive_stem_for_item(source: &Path) -> String {
    source
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            source
                .file_name()
                .and_then(|value| value.to_str())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or("Archive")
        .to_string()
}

pub(super) fn archive_root_name(source: &Path) -> PathBuf {
    source
        .file_name()
        .map(PathBuf::from)
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("Archive"))
}

pub(super) fn is_zip_archive_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("zip"))
}

fn numbered_directory_name(stem: &str, attempt: usize) -> String {
    if attempt == 0 {
        return stem.to_string();
    }
    format!("{stem} ({attempt})")
}

pub(super) fn unique_archive_directory_path(destination_dir: &Path, stem: &str) -> PathBuf {
    let mut attempt = 0usize;
    loop {
        let candidate = destination_dir.join(numbered_directory_name(stem, attempt));
        if !candidate.exists() {
            return candidate;
        }
        attempt += 1;
    }
}

pub(super) fn to_zip_path(path: &Path) -> Result<String, String> {
    let value = path.to_string_lossy().replace('\\', "/");
    if value.is_empty() {
        return Err("Archive entry path is empty.".to_string());
    }
    Ok(value)
}

fn zip_file_options(metadata: &fs::Metadata) -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(source_unix_mode(metadata, 0o644))
}

fn zip_dir_options(metadata: &fs::Metadata) -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(source_unix_mode(metadata, 0o755))
}

/// The permission bits to archive for a source entry. Windows has no mode
/// bits, so `default` is used there with write bits cleared for read-only
/// sources.
#[cfg(unix)]
fn source_unix_mode(metadata: &fs::Metadata, _default: u32) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o777
}

#[cfg(not(unix))]
fn source_unix_mode(metadata: &fs::Metadata, default: u32) -> u32 {
    const WRITE_PERMISSION_BITS: u32 = 0o222;
    if metadata.permissions().readonly() {
        return default & !WRITE_PERMISSION_BITS;
    }
    default
}
