use crate::ipc::types::{CompressArchiveRequest, ExtractArchiveRequest, MenuActionStatus};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const INVALID_REQUEST_MESSAGE: &str = "invalid-request";
const INVALID_DESTINATION_MESSAGE: &str = "invalid-destination";
const SOURCE_NOT_FOUND_MESSAGE: &str = "source-not-found";
const UNSUPPORTED_ARCHIVE_FORMAT_MESSAGE: &str = "unsupported-archive-format";
const ARCHIVE_CREATE_FAILED_MESSAGE: &str = "archive-create-failed";
const ARCHIVE_EXTRACT_FAILED_MESSAGE: &str = "archive-extract-failed";

pub fn compress_archive(payload: CompressArchiveRequest) -> MenuActionStatus {
    match compress_archive_impl(&payload) {
        Ok(archive_path) => {
            MenuActionStatus::handled_with_message(archive_path.to_string_lossy().into_owned())
        }
        Err(error) => {
            log::warn!("compress archive failed: {}", error);
            MenuActionStatus::unsupported(error.status_message())
        }
    }
}

pub fn extract_archive(payload: ExtractArchiveRequest) -> MenuActionStatus {
    match extract_archive_impl(&payload) {
        Ok(extracted_root) => {
            MenuActionStatus::handled_with_message(extracted_root.to_string_lossy().into_owned())
        }
        Err(error) => {
            log::warn!("extract archive failed: {}", error);
            MenuActionStatus::unsupported(error.status_message())
        }
    }
}

fn compress_archive_impl(payload: &CompressArchiveRequest) -> Result<PathBuf, ArchiveError> {
    if payload.paths.is_empty() {
        return Err(ArchiveError::status(INVALID_REQUEST_MESSAGE));
    }

    let destination_dir = Path::new(&payload.destination_dir);
    if !destination_dir.is_dir() {
        return Err(ArchiveError::status(INVALID_DESTINATION_MESSAGE));
    }

    let source_paths = collect_source_paths(&payload.paths)?;
    let archive_name = archive_name_for_sources(&source_paths);
    let archive_path = unique_output_path(destination_dir, &archive_name, "zip");

    let file = File::create(&archive_path).map_err(|source| {
        ArchiveError::with_source(ARCHIVE_CREATE_FAILED_MESSAGE, archive_path.clone(), source)
    })?;
    let mut writer = ZipWriter::new(file);

    for source_path in &source_paths {
        let root_name = archive_root_name(source_path);
        append_path(&mut writer, source_path, &root_name)?;
    }

    writer.finish().map_err(|source| {
        ArchiveError::with_source(ARCHIVE_CREATE_FAILED_MESSAGE, archive_path.clone(), source)
    })?;

    Ok(archive_path)
}

fn extract_archive_impl(payload: &ExtractArchiveRequest) -> Result<PathBuf, ArchiveError> {
    if payload.paths.is_empty() {
        return Err(ArchiveError::status(INVALID_REQUEST_MESSAGE));
    }

    let destination_dir = Path::new(&payload.destination_dir);
    if destination_dir.exists() && !destination_dir.is_dir() {
        return Err(ArchiveError::status(INVALID_DESTINATION_MESSAGE));
    }
    fs::create_dir_all(destination_dir).map_err(|source| {
        ArchiveError::with_source(
            ARCHIVE_EXTRACT_FAILED_MESSAGE,
            destination_dir.to_path_buf(),
            source,
        )
    })?;

    let archive_paths = collect_source_paths(&payload.paths)?;
    let mut first_output_root: Option<PathBuf> = None;

    for archive_path in archive_paths {
        if !is_zip_archive_path(&archive_path) {
            return Err(ArchiveError::status(UNSUPPORTED_ARCHIVE_FORMAT_MESSAGE));
        }

        let output_root = unique_directory_path(destination_dir, &archive_stem(&archive_path));
        extract_zip_archive(&archive_path, &output_root, &archive_stem(&archive_path))?;
        if first_output_root.is_none() {
            first_output_root = Some(output_root);
        }
    }

    first_output_root.ok_or_else(|| ArchiveError::status(INVALID_REQUEST_MESSAGE))
}

fn collect_source_paths(paths: &[String]) -> Result<Vec<PathBuf>, ArchiveError> {
    let mut source_paths = Vec::with_capacity(paths.len());
    for raw_path in paths {
        let trimmed = raw_path.trim();
        if trimmed.is_empty() {
            return Err(ArchiveError::status(INVALID_REQUEST_MESSAGE));
        }

        let path = PathBuf::from(trimmed);
        if !path.exists() {
            return Err(ArchiveError::status(SOURCE_NOT_FOUND_MESSAGE));
        }
        source_paths.push(path);
    }
    Ok(source_paths)
}

fn append_path(
    writer: &mut ZipWriter<File>,
    source_path: &Path,
    archive_path: &Path,
) -> Result<(), ArchiveError> {
    if source_path.is_dir() {
        let mut directory_name = to_zip_path(archive_path)?;
        if !directory_name.ends_with('/') {
            directory_name.push('/');
        }

        writer
            .add_directory(directory_name, dir_options())
            .map_err(|source| {
                ArchiveError::with_source(
                    ARCHIVE_CREATE_FAILED_MESSAGE,
                    source_path.to_path_buf(),
                    source,
                )
            })?;

        let mut children = fs::read_dir(source_path)
            .map_err(|source| {
                ArchiveError::with_source(
                    ARCHIVE_CREATE_FAILED_MESSAGE,
                    source_path.to_path_buf(),
                    source,
                )
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| {
                ArchiveError::with_source(
                    ARCHIVE_CREATE_FAILED_MESSAGE,
                    source_path.to_path_buf(),
                    source,
                )
            })?;

        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            let child_source = child.path();
            let child_archive_path = archive_path.join(child.file_name());
            append_path(writer, &child_source, &child_archive_path)?;
        }
        return Ok(());
    }

    writer
        .start_file(to_zip_path(archive_path)?, file_options())
        .map_err(|source| {
            ArchiveError::with_source(
                ARCHIVE_CREATE_FAILED_MESSAGE,
                source_path.to_path_buf(),
                source,
            )
        })?;

    let mut file = File::open(source_path).map_err(|source| {
        ArchiveError::with_source(
            ARCHIVE_CREATE_FAILED_MESSAGE,
            source_path.to_path_buf(),
            source,
        )
    })?;
    io::copy(&mut file, writer).map_err(|source| {
        ArchiveError::with_source(
            ARCHIVE_CREATE_FAILED_MESSAGE,
            source_path.to_path_buf(),
            source,
        )
    })?;

    Ok(())
}

fn extract_zip_archive(
    archive_path: &Path,
    output_root: &Path,
    archive_stem: &str,
) -> Result<(), ArchiveError> {
    fs::create_dir_all(output_root).map_err(|source| {
        ArchiveError::with_source(
            ARCHIVE_EXTRACT_FAILED_MESSAGE,
            output_root.to_path_buf(),
            source,
        )
    })?;

    let file = File::open(archive_path).map_err(|source| {
        ArchiveError::with_source(
            ARCHIVE_EXTRACT_FAILED_MESSAGE,
            archive_path.to_path_buf(),
            source,
        )
    })?;
    let mut archive = ZipArchive::new(file).map_err(|source| {
        ArchiveError::with_source(
            ARCHIVE_EXTRACT_FAILED_MESSAGE,
            archive_path.to_path_buf(),
            source,
        )
    })?;
    let wrapper_root = detect_redundant_wrapper_root(&mut archive, archive_stem)?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|source| {
            ArchiveError::with_source(
                ARCHIVE_EXTRACT_FAILED_MESSAGE,
                archive_path.to_path_buf(),
                source,
            )
        })?;
        let enclosed_name = entry.enclosed_name().map(PathBuf::from).ok_or_else(|| {
            ArchiveError::path_status(
                ARCHIVE_EXTRACT_FAILED_MESSAGE,
                archive_path.to_path_buf(),
                "invalid-archive-entry",
            )
        })?;
        let relative_name = strip_wrapper_root(&enclosed_name, wrapper_root.as_deref());
        let output_path = output_root.join(relative_name);

        if entry.is_dir() {
            fs::create_dir_all(&output_path).map_err(|source| {
                ArchiveError::with_source(
                    ARCHIVE_EXTRACT_FAILED_MESSAGE,
                    output_path.clone(),
                    source,
                )
            })?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|source| {
                ArchiveError::with_source(
                    ARCHIVE_EXTRACT_FAILED_MESSAGE,
                    parent.to_path_buf(),
                    source,
                )
            })?;
        }

        let mut output_file = File::create(&output_path).map_err(|source| {
            ArchiveError::with_source(ARCHIVE_EXTRACT_FAILED_MESSAGE, output_path.clone(), source)
        })?;
        io::copy(&mut entry, &mut output_file).map_err(|source| {
            ArchiveError::with_source(ARCHIVE_EXTRACT_FAILED_MESSAGE, output_path.clone(), source)
        })?;
    }

    Ok(())
}

fn detect_redundant_wrapper_root(
    archive: &mut ZipArchive<File>,
    archive_stem: &str,
) -> Result<Option<String>, ArchiveError> {
    let normalized_stem = archive_stem.trim();
    if normalized_stem.is_empty() {
        return Ok(None);
    }

    let mut candidate_root: Option<String> = None;
    let mut saw_nested_entry = false;

    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|source| {
            ArchiveError::with_source(
                ARCHIVE_EXTRACT_FAILED_MESSAGE,
                PathBuf::from(archive_stem),
                source,
            )
        })?;
        let Some(enclosed_name) = entry.enclosed_name() else {
            return Err(ArchiveError::path_status(
                ARCHIVE_EXTRACT_FAILED_MESSAGE,
                PathBuf::from(archive_stem),
                "invalid-archive-entry",
            ));
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
        if components.next().is_some() {
            saw_nested_entry = true;
        } else if !entry.is_dir() {
            return Ok(None);
        }
    }

    Ok(
        candidate_root
            .filter(|root| saw_nested_entry && root.eq_ignore_ascii_case(normalized_stem)),
    )
}

fn strip_wrapper_root<'a>(path: &'a Path, wrapper_root: Option<&str>) -> &'a Path {
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

fn archive_name_for_sources(source_paths: &[PathBuf]) -> String {
    if source_paths.len() == 1 {
        let source_path = &source_paths[0];
        if source_path.is_file() {
            return source_path
                .file_stem()
                .and_then(|value| value.to_str())
                .filter(|value| !value.is_empty())
                .unwrap_or("Archive")
                .to_string();
        }

        return archive_root_name(source_path)
            .to_string_lossy()
            .into_owned();
    }

    "Archive".to_string()
}

fn archive_root_name(source_path: &Path) -> PathBuf {
    source_path
        .file_name()
        .map(PathBuf::from)
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("Archive"))
}

fn archive_stem(archive_path: &Path) -> String {
    archive_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("Archive")
        .to_string()
}

fn is_zip_archive_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("zip"))
}

fn unique_output_path(destination_dir: &Path, stem: &str, extension: &str) -> PathBuf {
    let mut attempt = 0usize;

    loop {
        let file_name = if attempt == 0 {
            format!("{stem}.{extension}")
        } else {
            format!("{stem} ({attempt}).{extension}")
        };
        let candidate = destination_dir.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
        attempt += 1;
    }
}

fn unique_directory_path(destination_dir: &Path, stem: &str) -> PathBuf {
    let mut attempt = 0usize;

    loop {
        let file_name = if attempt == 0 {
            stem.to_string()
        } else {
            format!("{stem} ({attempt})")
        };
        let candidate = destination_dir.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
        attempt += 1;
    }
}

fn to_zip_path(path: &Path) -> Result<String, ArchiveError> {
    let value = path.to_string_lossy().replace('\\', "/");
    if value.is_empty() {
        return Err(ArchiveError::status(INVALID_REQUEST_MESSAGE));
    }
    Ok(value)
}

fn file_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644)
}

fn dir_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o755)
}

#[derive(Debug)]
struct ArchiveError {
    status_message: &'static str,
    path: Option<PathBuf>,
    source: Option<String>,
    detail: Option<&'static str>,
}

impl ArchiveError {
    fn status(status_message: &'static str) -> Self {
        Self {
            status_message,
            path: None,
            source: None,
            detail: None,
        }
    }

    fn path_status(status_message: &'static str, path: PathBuf, detail: &'static str) -> Self {
        Self {
            status_message,
            path: Some(path),
            source: None,
            detail: Some(detail),
        }
    }

    fn with_source(
        status_message: &'static str,
        path: PathBuf,
        source: impl std::fmt::Display,
    ) -> Self {
        Self {
            status_message,
            path: Some(path),
            source: Some(source.to_string()),
            detail: None,
        }
    }

    fn status_message(&self) -> &'static str {
        self.status_message
    }
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.status_message)?;
        if let Some(path) = &self.path {
            write!(formatter, " [{}]", path.display())?;
        }
        if let Some(detail) = self.detail {
            write!(formatter, " ({detail})")?;
        }
        if let Some(source) = &self.source {
            write!(formatter, ": {source}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ArchiveError {}
