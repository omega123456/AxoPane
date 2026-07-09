use std::cmp::Ordering;
use std::ffi::OsStr;
use std::fs::{self, DirEntry, Metadata};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::SystemTime;

use lexical_sort::natural_lexical_cmp;
use rayon::prelude::*;
use rayon::{ThreadPool, ThreadPoolBuilder};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SortKey {
    Name,
    Size,
    Items,
    Type,
    Modified,
    Created,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDirOptions {
    pub path: String,
    pub sort_key: SortKey,
    pub sort_direction: SortDirection,
    pub filter: String,
    pub show_hidden: bool,
    pub include_item_counts: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub icon_data_url: Option<String>,
    pub size_bytes: Option<u64>,
    pub item_count: Option<u64>,
    pub type_label: String,
    pub modified_at: Option<String>,
    pub created_at: Option<String>,
    pub attributes: Vec<String>,
    pub is_hidden: bool,
    pub is_system: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDirResponse {
    pub path: String,
    pub entries: Vec<DirectoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TreeChildEntry {
    pub name: String,
    pub path: String,
    pub has_children: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTreeChildrenOptions {
    pub path: String,
    pub show_hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTreeChildrenResponse {
    pub path: String,
    pub children: Vec<TreeChildEntry>,
}

#[derive(Debug)]
pub enum FsError {
    Io(std::io::Error),
    InvalidFileName(PathBuf),
    InvalidName(String),
    AlreadyExists(PathBuf),
}

impl std::fmt::Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::InvalidFileName(path) => write!(f, "invalid file name: {}", path.display()),
            Self::InvalidName(name) => write!(f, "invalid name: \"{name}\""),
            Self::AlreadyExists(path) => {
                write!(f, "an item named \"{}\" already exists", path.display())
            }
        }
    }
}

impl std::error::Error for FsError {}

impl From<std::io::Error> for FsError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug)]
pub enum ListDirOutcome {
    Complete(ListDirResponse),
    Cancelled,
}

/// Canonicalizes a user-supplied path into an absolute, real path.
///
/// Uses `dunce::canonicalize` so Windows results stay in the familiar
/// `C:\Users\...` form instead of the extended-length `\\?\C:\Users\...`
/// prefix that `std::fs::canonicalize` returns. The result is the absolute
/// location the OS actually resolves, which is what every later navigation,
/// breadcrumb, and watcher operation depends on.
pub fn canonicalize_dir(path: &Path) -> Result<PathBuf, FsError> {
    Ok(dunce::canonicalize(expand_home_path(path))?)
}

pub fn display_path_from_path(path: &Path) -> String {
    display_path_from_text(&path.to_string_lossy())
}

pub fn display_path_from_text(path: &str) -> String {
    #[cfg(windows)]
    {
        if let Some(rest) = path.strip_prefix("\\\\?\\UNC\\") {
            return format!("\\\\{rest}");
        }

        if let Some(rest) = path.strip_prefix("\\\\?\\") {
            return rest.to_string();
        }
    }

    path.to_string()
}

/// The directory the app should open into on a fresh session: the user's home
/// directory when known, otherwise the first available volume root, falling
/// back to the platform filesystem root (`C:\` on Windows, `/` elsewhere).
pub fn default_start_dir() -> PathBuf {
    if let Some(home) = home_dir() {
        if home.is_dir() {
            return home;
        }
    }

    if let Some(volume) = crate::volumes::list_volumes().into_iter().next() {
        let root = PathBuf::from(volume.mount_root);
        if root.is_dir() {
            return root;
        }
    }

    platform_root()
}

pub fn platform_root() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("C:\\")
    } else {
        PathBuf::from("/")
    }
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let keys = ["USERPROFILE", "HOME"];
    #[cfg(not(windows))]
    let keys = ["HOME"];

    for key in keys {
        if let Some(value) = std::env::var_os(key) {
            if !value.is_empty() {
                return Some(PathBuf::from(value));
            }
        }
    }

    None
}

pub fn expand_home_path(path: &Path) -> PathBuf {
    expand_home_path_with(&path.to_string_lossy(), home_dir().as_deref())
}

pub fn expand_home_path_with(path: &str, home: Option<&Path>) -> PathBuf {
    let Some(rest) = path.strip_prefix('~') else {
        return PathBuf::from(path);
    };

    if !rest.is_empty() && !rest.starts_with(['/', '\\']) {
        return PathBuf::from(path);
    }

    let Some(home) = home else {
        return PathBuf::from(path);
    };

    let rest = rest.trim_start_matches(['/', '\\']);
    if rest.is_empty() {
        home.to_path_buf()
    } else {
        home.join(rest)
    }
}

pub fn list_dir(options: &ListDirOptions) -> Result<ListDirResponse, FsError> {
    match list_dir_with_cancellation(options, || false)? {
        ListDirOutcome::Complete(response) => Ok(response),
        ListDirOutcome::Cancelled => unreachable!("default list_dir cannot be cancelled"),
    }
}

pub fn list_dir_with_cancellation<F>(
    options: &ListDirOptions,
    is_cancelled: F,
) -> Result<ListDirOutcome, FsError>
where
    F: Fn() -> bool + Sync,
{
    let requested = Path::new(&options.path);
    let directory = canonicalize_dir(requested).map_err(|error| {
        log::warn!(
            "list_dir: failed to canonicalize {:?}: {error}",
            options.path
        );
        error
    })?;
    log::debug!(
        "list_dir: requested {:?} -> canonical {}",
        options.path,
        directory.display()
    );
    let normalized_filter = options.filter.to_lowercase();
    let mut entries = Vec::new();

    for entry in fs::read_dir(&directory)? {
        if is_cancelled() {
            return Ok(ListDirOutcome::Cancelled);
        }
        let entry = entry?;
        let listed = build_entry(&entry)?;

        if !options.show_hidden && (listed.is_hidden || listed.is_system) {
            continue;
        }

        if !normalized_filter.is_empty() && !listed.name.to_lowercase().contains(&normalized_filter)
        {
            continue;
        }

        entries.push(listed);
    }

    if options.include_item_counts {
        populate_item_counts_with_cancellation(&mut entries, &is_cancelled);
        if is_cancelled() {
            return Ok(ListDirOutcome::Cancelled);
        }
    }

    entries.sort_by(|left, right| {
        compare_entries(left, right, options.sort_key, options.sort_direction)
    });

    if is_cancelled() {
        return Ok(ListDirOutcome::Cancelled);
    }

    log::info!(
        "list_dir: {} -> {} entries",
        directory.display(),
        entries.len()
    );

    Ok(ListDirOutcome::Complete(ListDirResponse {
        path: display_path_from_path(&directory),
        entries,
    }))
}

pub fn list_tree_children(
    options: &ListTreeChildrenOptions,
) -> Result<ListTreeChildrenResponse, FsError> {
    let requested = Path::new(&options.path);
    let directory = canonicalize_tree_dir(requested, &options.path)?;

    let mut children = Vec::new();
    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if !resolve_is_dir(&path, &metadata) {
            continue;
        }

        let attributes = collect_attributes(&path, &metadata);
        if !options.show_hidden
            && attributes
                .iter()
                .any(|attribute| attribute == "hidden" || attribute == "system")
        {
            continue;
        }

        let name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(_) => return Err(FsError::InvalidFileName(path)),
        };
        children.push(TreeChildEntry {
            name,
            path: display_path_from_path(&path),
            has_children: directory_has_visible_child_dirs(&path, options.show_hidden),
        });
    }

    children.sort_by(|left, right| natural_name_compare(&left.name, &right.name));

    Ok(ListTreeChildrenResponse {
        path: display_path_from_path(&directory),
        children,
    })
}

/// Validates a single user-supplied file/folder name. Names must be non-empty,
/// must not be the special `.`/`..` entries, and must not contain a path
/// separator or NUL (which would let a "name" escape its parent directory).
pub fn validate_name(name: &str) -> Result<(), FsError> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains('\0')
    {
        return Err(FsError::InvalidName(name.to_string()));
    }

    Ok(())
}

/// Creates a new directory named `name` inside `parent` and returns its entry.
pub fn create_directory(parent: &str, name: &str) -> Result<DirectoryEntry, FsError> {
    validate_name(name)?;
    let directory = canonicalize_dir(Path::new(parent))?;
    let target = directory.join(name.trim());
    if target.exists() {
        return Err(FsError::AlreadyExists(target));
    }

    fs::create_dir(&target)?;
    log::info!("create_directory: {}", target.display());
    build_entry_from_path(&target)
}

/// Creates a new empty file named `name` inside `parent` and returns its entry.
pub fn create_file(parent: &str, name: &str) -> Result<DirectoryEntry, FsError> {
    validate_name(name)?;
    let directory = canonicalize_dir(Path::new(parent))?;
    let target = directory.join(name.trim());
    if target.exists() {
        return Err(FsError::AlreadyExists(target));
    }

    // create_new fails if the file already exists, closing the check-then-create
    // race left open by the `exists()` guard above.
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)?;
    log::info!("create_file: {}", target.display());
    build_entry_from_path(&target)
}

/// Renames the item at `path` to `new_name` within its current directory and
/// returns the resulting entry.
pub fn rename_entry(path: &str, new_name: &str) -> Result<DirectoryEntry, FsError> {
    validate_name(new_name)?;
    let source = Path::new(path);
    let parent = match source.parent() {
        Some(parent) => parent,
        None => return Err(FsError::InvalidFileName(source.to_path_buf())),
    };
    let target = parent.join(new_name.trim());

    // A no-op rename to the same name is allowed; otherwise refuse to clobber an
    // existing item.
    if target != source && target.exists() {
        return Err(FsError::AlreadyExists(target));
    }

    fs::rename(source, &target)?;
    log::info!("rename_entry: {} -> {}", source.display(), target.display());
    build_entry_from_path(&target)
}

/// Builds a [`DirectoryEntry`] from an absolute path (used after a mutating
/// operation produces a new/renamed item). Mirrors [`build_entry`] but reads
/// metadata directly from the path rather than a [`DirEntry`].
fn build_entry_from_path(path: &Path) -> Result<DirectoryEntry, FsError> {
    build_entry_from_path_with_options(path, true)
}

fn build_entry_from_path_with_options(
    path: &Path,
    include_item_count: bool,
) -> Result<DirectoryEntry, FsError> {
    let metadata = path.metadata()?;
    let Some(file_name) = path.file_name() else {
        return Err(FsError::InvalidFileName(path.to_path_buf()));
    };
    let Some(name) = OsStr::to_str(file_name) else {
        return Err(FsError::InvalidFileName(path.to_path_buf()));
    };
    let name = name.to_string();
    let is_dir = metadata.is_dir();
    let attributes = collect_attributes(path, &metadata);
    let mut is_hidden = false;
    let mut is_system = false;
    for attribute in &attributes {
        if attribute == "hidden" {
            is_hidden = true;
        } else if attribute == "system" {
            is_system = true;
        }
    }

    Ok(DirectoryEntry {
        id: display_path_from_path(path),
        name: name.clone(),
        path: display_path_from_path(path),
        is_dir,
        icon_data_url: None,
        size_bytes: (!is_dir).then_some(metadata.len()),
        item_count: if is_dir && include_item_count {
            read_item_count(path)
        } else {
            None
        },
        type_label: infer_type_label(&name, is_dir),
        modified_at: system_time_to_rfc3339(metadata.modified().ok()),
        created_at: system_time_to_rfc3339(metadata.created().ok()),
        attributes,
        is_hidden,
        is_system,
    })
}

pub fn directory_entry_from_path(path: &Path) -> Result<DirectoryEntry, FsError> {
    build_entry_from_path(path)
}

pub fn directory_entry_from_path_without_item_count(
    path: &Path,
) -> Result<DirectoryEntry, FsError> {
    build_entry_from_path_with_options(path, false)
}

fn build_entry(entry: &DirEntry) -> Result<DirectoryEntry, FsError> {
    let path = entry.path();
    let metadata = entry.metadata()?;
    let name = match entry.file_name().into_string() {
        Ok(name) => name,
        Err(_) => return Err(FsError::InvalidFileName(path.clone())),
    };
    let is_dir = resolve_is_dir(&path, &metadata);
    let attributes = collect_attributes(&path, &metadata);
    let mut is_hidden = false;
    let mut is_system = false;
    for attribute in &attributes {
        if attribute == "hidden" {
            is_hidden = true;
        } else if attribute == "system" {
            is_system = true;
        }
    }

    Ok(DirectoryEntry {
        id: display_path_from_path(&path),
        name: name.clone(),
        path: display_path_from_path(&path),
        is_dir,
        icon_data_url: None,
        size_bytes: (!is_dir).then_some(metadata.len()),
        item_count: None,
        type_label: infer_type_label(&name, is_dir),
        modified_at: system_time_to_rfc3339(metadata.modified().ok()),
        created_at: system_time_to_rfc3339(metadata.created().ok()),
        attributes,
        is_hidden,
        is_system,
    })
}

fn item_count_pool() -> Option<&'static ThreadPool> {
    static POOL: OnceLock<Option<ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        ThreadPoolBuilder::new()
            .num_threads(4)
            .thread_name(|index| format!("fe-item-count-{index}"))
            .build()
            .ok()
    })
    .as_ref()
}

pub fn populate_item_counts_with_cancellation<F>(entries: &mut [DirectoryEntry], is_cancelled: &F)
where
    F: Fn() -> bool + Sync,
{
    if let Some(pool) = item_count_pool() {
        pool.install(|| {
            entries.par_iter_mut().for_each(|entry| {
                if entry.is_dir && !is_cancelled() {
                    entry.item_count = read_item_count(Path::new(&entry.path));
                }
            });
        });
        return;
    }

    for entry in entries {
        if is_cancelled() {
            return;
        }
        if entry.is_dir {
            entry.item_count = read_item_count(Path::new(&entry.path));
        }
    }
}

/// Counts the immediate children of a directory for the "Items" column.
///
/// This deliberately never propagates an error: drive roots and home folders
/// routinely contain entries we cannot open (`System Volume Information`,
/// `$RECYCLE.BIN`, legacy access-denied junctions like `Application Data`).
/// A failure to count one child must not abort the entire parent listing, so an
/// unreadable directory simply reports an unknown count (`None`).
pub fn read_item_count(path: &Path) -> Option<u64> {
    match fs::read_dir(path) {
        Ok(entries) => Some(entries.count() as u64),
        Err(error) => {
            log::debug!("read_item_count: cannot count {}: {error}", path.display());
            None
        }
    }
}

fn directory_has_visible_child_dirs(path: &Path, show_hidden: bool) -> bool {
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };

    for entry in entries.flatten() {
        let child_path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !resolve_is_dir(&child_path, &metadata) {
            continue;
        }
        if show_hidden {
            return true;
        }
        let attributes = collect_attributes(&child_path, &metadata);
        if !attributes
            .iter()
            .any(|attribute| is_hidden_or_system_attribute(attribute))
        {
            return true;
        }
    }

    false
}

pub fn infer_type_label(name: &str, is_dir: bool) -> String {
    if is_dir {
        return "Folder".to_string();
    }

    Path::new(name)
        .extension()
        .and_then(OsStr::to_str)
        .filter(|extension| !extension.is_empty())
        .map(|extension| format!("{} file", extension.to_ascii_uppercase()))
        .unwrap_or_else(|| "File".to_string())
}

pub fn compare_entries(
    left: &DirectoryEntry,
    right: &DirectoryEntry,
    sort_key: SortKey,
    sort_direction: SortDirection,
) -> Ordering {
    match left.is_dir.cmp(&right.is_dir).reverse() {
        Ordering::Equal => {}
        non_equal => return non_equal,
    }

    let base_order = match sort_key {
        SortKey::Name => natural_name_compare(&left.name, &right.name),
        SortKey::Size => compare_optional_u64(left.size_bytes, right.size_bytes)
            .then_with(|| natural_name_compare(&left.name, &right.name)),
        SortKey::Items => compare_optional_u64(left.item_count, right.item_count)
            .then_with(|| natural_name_compare(&left.name, &right.name)),
        SortKey::Type => natural_name_compare(&left.type_label, &right.type_label)
            .then_with(|| natural_name_compare(&left.name, &right.name)),
        SortKey::Modified => compare_with_name_tiebreak(
            compare_optional_string(left.modified_at.as_deref(), right.modified_at.as_deref()),
            &left.name,
            &right.name,
        ),
        SortKey::Created => compare_with_name_tiebreak(
            compare_optional_string(left.created_at.as_deref(), right.created_at.as_deref()),
            &left.name,
            &right.name,
        ),
    };

    match sort_direction {
        SortDirection::Asc => base_order,
        SortDirection::Desc => base_order.reverse(),
    }
}

fn canonicalize_tree_dir(requested: &Path, display_path: &str) -> Result<PathBuf, FsError> {
    canonicalize_dir(requested).map_err(|error| {
        log::warn!(
            "list_tree_children: failed to canonicalize {:?}: {error}",
            display_path
        );
        error
    })
}

fn is_hidden_or_system_attribute(attribute: &str) -> bool {
    attribute == "hidden" || attribute == "system"
}

fn compare_with_name_tiebreak(base: Ordering, left_name: &str, right_name: &str) -> Ordering {
    match base {
        Ordering::Equal => natural_name_compare(left_name, right_name),
        other => other,
    }
}

pub fn natural_name_compare(left: &str, right: &str) -> Ordering {
    natural_lexical_cmp(left, right)
}

pub fn compare_optional_u64(left: Option<u64>, right: Option<u64>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => Ordering::Equal,
    }
}

pub fn compare_optional_string(left: Option<&str>, right: Option<&str>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(right),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => Ordering::Equal,
    }
}

pub fn system_time_to_rfc3339(value: Option<SystemTime>) -> Option<String> {
    value.and_then(|time| OffsetDateTime::from(time).format(&Rfc3339).ok())
}

/// Determines whether `path` should be treated as a directory, following
/// symlinks/junctions when the raw entry metadata is a reparse point.
///
/// On Windows, `std::fs::Metadata::is_dir()` is `false` for *any* reparse
/// point (symlink or junction) even when it targets a directory, because
/// `is_dir()`/`is_symlink()` are mutually exclusive there. Without this, a
/// symlinked folder (e.g. a `mklink /d` or junction) is reported as a file
/// and the UI can't navigate into it. Following the link to check the
/// target's real type keeps folder-symlinks navigable on both platforms.
pub fn resolve_is_dir(path: &Path, metadata: &Metadata) -> bool {
    if metadata.is_dir() {
        return true;
    }
    if metadata.file_type().is_symlink() {
        return fs::metadata(path)
            .map(|target| target.is_dir())
            .unwrap_or(false);
    }
    false
}

pub fn collect_attributes(path: &Path, metadata: &Metadata) -> Vec<String> {
    let mut attributes = Vec::new();

    if metadata.permissions().readonly() {
        attributes.push("readonly".to_string());
    }

    if metadata.file_type().is_symlink() {
        attributes.push("symlink".to_string());
    }

    if is_hidden(path, metadata) {
        attributes.push("hidden".to_string());
    }

    if is_system(metadata) {
        attributes.push("system".to_string());
    }

    attributes
}

#[cfg(windows)]
fn is_hidden(path: &Path, metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;

    let file_attributes = metadata.file_attributes();
    file_attributes & FILE_ATTRIBUTE_HIDDEN != 0
        || (path
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.starts_with('.'))
            && file_attributes & FILE_ATTRIBUTE_SYSTEM == 0)
}

#[cfg(not(windows))]
fn is_hidden(path: &Path, _metadata: &Metadata) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name.starts_with('.'))
}

#[cfg(windows)]
fn is_system(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    metadata.file_attributes() & FILE_ATTRIBUTE_SYSTEM != 0
}

#[cfg(not(windows))]
fn is_system(_metadata: &Metadata) -> bool {
    false
}
