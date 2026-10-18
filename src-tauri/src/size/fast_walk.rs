//! macOS folder-size walker built on `getattrlistbulk(2)`.
//!
//! One bulk call returns the type and logical length of many directory entries
//! at once, replacing the shared serial walker's `lstat` per entry. A job walks
//! on its own thread plus up to `MAX_JOB_THREADS - 1` helper threads, and every
//! job draws its helpers from one process-wide budget of
//! [`MAX_HELPER_THREADS`], so concurrent jobs can never multiply the thread
//! count. A job that gets no helpers simply walks on its own thread.
//!
//! Sizes match the previous implementation exactly: logical file length,
//! symlinks are neither followed nor counted, and directories contribute 0
//! bytes themselves. Entries that vanish mid-walk are skipped; directories the
//! OS refuses to read are reported as denied.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs::OpenOptions;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Instant;

use super::index::platform::DATA_VOLUME_PREFIX;
use super::manual::ManualSizeError;

/// Helper threads shared by every concurrent fast-walk job in the process.
pub const MAX_HELPER_THREADS: usize = 8;
/// Threads one job walks with: its own thread plus up to three helpers.
pub const MAX_JOB_THREADS: usize = 4;
/// Paths an index walk never descends into (see [`WalkOptions::with_index_boundaries`]):
/// other volumes, the second view of the Data volume (and Preboot, VM, …)
/// under `/System/Volumes`, and devfs.
pub const INDEX_EXCLUDED_ROOTS: [&str; 3] = ["/System/Volumes", "/Volumes", "/dev"];

const BUFFER_SIZE: usize = 128 * 1024;

/// `ATTR_CMN_ERROR` from `<sys/attr.h>`; `libc` does not declare it.
const ATTR_CMN_ERROR: libc::attrgroup_t = 0x2000_0000;
/// `enum vtype` values from `<sys/vnode.h>`, as reported by `ATTR_CMN_OBJTYPE`.
const VDIR: u32 = 2;
const VLNK: u32 = 5;

// Entry layout. With `FSOPT_PACK_INVAL_ATTRS` every requested attribute is
// packed (zero-filled when invalid), so offsets are fixed: length,
// returned `attribute_set_t`, `ATTR_CMN_ERROR` (always right after the
// returned set), name `attrreference_t`, object type, then file data length.
const ENTRY_LENGTH: usize = 0;
const RETURNED_COMMON: usize = 4;
const RETURNED_FILE: usize = 16;
const ENTRY_ERROR: usize = 24;
const ENTRY_NAME: usize = 28;
const ENTRY_TYPE: usize = 36;
const ENTRY_DATA_LENGTH: usize = 40;

static HELPERS_IN_USE: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "test-utils")]
static PEAK_HELPERS: AtomicUsize = AtomicUsize::new(0);

/// Thread QoS a walk runs at: user-initiated for size jobs and boosts,
/// background for gentle index scans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Qos {
    UserInitiated,
    Background,
}

/// One walked directory: the bytes and count of its direct files (symlinks
/// excluded), its subdirectory names, and whether the OS refused to read it
/// (then all of these cover only what was read).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirRecord {
    pub path: PathBuf,
    pub direct_bytes: u64,
    pub file_count: u64,
    pub subdirs: Vec<OsString>,
    pub denied: bool,
}

/// A single directory's contents, as read by [`list_dir`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirListing {
    pub direct_bytes: u64,
    pub file_count: u64,
    pub subdirs: Vec<OsString>,
    pub denied: bool,
}

#[cfg(feature = "test-utils")]
pub type DirHook = Arc<dyn Fn(&Path) + Send + Sync>;
#[cfg(feature = "test-utils")]
pub type DeviceHook = Arc<dyn Fn(&Path, u64) -> u64 + Send + Sync>;

/// Deterministic test seams; never compiled into release builds.
#[cfg(feature = "test-utils")]
#[derive(Clone)]
pub struct TestHooks {
    /// `getattrlistbulk` buffer size; small values force multi-fill reads.
    pub buffer_size: usize,
    /// Runs just before a directory is opened, e.g. to make it vanish.
    pub before_dir_open: Option<DirHook>,
    /// Maps a directory's real `st_dev`, to fake a device boundary.
    pub device_of: Option<DeviceHook>,
}

#[cfg(feature = "test-utils")]
impl Default for TestHooks {
    fn default() -> Self {
        Self {
            buffer_size: BUFFER_SIZE,
            before_dir_open: None,
            device_of: None,
        }
    }
}

pub struct WalkOptions {
    pub cancel: Arc<AtomicBool>,
    /// `None` walks without a time limit (index background scans).
    pub deadline: Option<Instant>,
    /// Threads for this job, clamped to `1..=MAX_JOB_THREADS`.
    pub threads: usize,
    pub qos: Qos,
    /// Directories never opened or descended into.
    pub excluded: HashSet<PathBuf>,
    /// Stop at directories on a device other than the root's or the Data
    /// volume's (on macOS 10.15–13 `/` and the Data volume differ).
    pub stay_on_device: bool,
    #[cfg(feature = "test-utils")]
    pub hooks: TestHooks,
}

impl WalkOptions {
    pub fn new(cancel: Arc<AtomicBool>, deadline: Option<Instant>) -> Self {
        Self {
            cancel,
            deadline,
            threads: MAX_JOB_THREADS,
            qos: Qos::UserInitiated,
            excluded: HashSet::new(),
            stay_on_device: false,
            #[cfg(feature = "test-utils")]
            hooks: TestHooks::default(),
        }
    }

    /// Index mode: never enter [`INDEX_EXCLUDED_ROOTS`] and never cross
    /// onto a device other than the root's or the Data volume's.
    pub fn with_index_boundaries(mut self) -> Self {
        self.excluded
            .extend(INDEX_EXCLUDED_ROOTS.iter().map(PathBuf::from));
        self.stay_on_device = true;
        self
    }
}

/// Total logical bytes under `root`. A denied directory fails the walk.
pub fn subtree_total(root: &Path, options: &WalkOptions) -> Result<u64, ManualSizeError> {
    let total = AtomicU64::new(0);
    walk(root, options, true, &|record| {
        total.fetch_add(record.direct_bytes, Ordering::Relaxed);
    })?;
    Ok(total.into_inner())
}

/// Calls `on_record` once per directory under `root` (from any walk thread).
/// Denied directories are reported, not fatal. Excluded, off-device and
/// vanished directories produce no record.
pub fn subtree_records(
    root: &Path,
    options: &WalkOptions,
    on_record: impl Fn(DirRecord) + Sync,
) -> Result<(), ManualSizeError> {
    walk(root, options, false, &on_record)
}

/// Reads one directory. Ignores exclusions and device boundaries; a missing
/// directory is a `NotFound` I/O error.
pub fn list_dir(dir: &Path, options: &WalkOptions) -> Result<DirListing, ManualSizeError> {
    let mut buffer = vec![0_u8; buffer_size(options)];
    read_dir(dir, options, None, &mut buffer)?
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound).into())
}

/// Current and peak helper threads in use across the process.
#[cfg(feature = "test-utils")]
pub fn helper_threads_for_tests() -> (usize, usize) {
    (
        HELPERS_IN_USE.load(Ordering::Acquire),
        PEAK_HELPERS.load(Ordering::Acquire),
    )
}

struct Queue {
    pending: Vec<PathBuf>,
    busy: usize,
    failure: Option<ManualSizeError>,
}

struct Walk<'a> {
    options: &'a WalkOptions,
    allowed_devs: Option<Vec<u64>>,
    fail_on_denied: bool,
    on_record: &'a (dyn Fn(DirRecord) + Sync),
    queue: Mutex<Queue>,
    wake: Condvar,
}

fn walk(
    root: &Path,
    options: &WalkOptions,
    fail_on_denied: bool,
    on_record: &(dyn Fn(DirRecord) + Sync),
) -> Result<(), ManualSizeError> {
    let allowed_devs = if options.stay_on_device {
        let mut devs = vec![device_of(
            options,
            root,
            std::fs::symlink_metadata(root)?.dev(),
        )];
        let data = Path::new(DATA_VOLUME_PREFIX);
        if let Ok(meta) = std::fs::metadata(data) {
            devs.push(device_of(options, data, meta.dev()));
        }
        Some(devs)
    } else {
        None
    };
    let walk = Walk {
        options,
        allowed_devs,
        fail_on_denied,
        on_record,
        queue: Mutex::new(Queue {
            pending: vec![root.to_path_buf()],
            busy: 0,
            failure: None,
        }),
        wake: Condvar::new(),
    };

    let helpers = acquire_helpers(options.threads.clamp(1, MAX_JOB_THREADS) - 1);
    thread::scope(|scope| {
        for _ in 0..helpers {
            let permit = HelperPermit;
            let walk = &walk;
            // A helper that fails to spawn drops its permit with the closure;
            // the remaining threads still finish the walk.
            let _ = thread::Builder::new()
                .name("fast-walk".to_string())
                .spawn_scoped(scope, move || {
                    let _permit = permit;
                    apply_qos(walk.options.qos);
                    walk.work();
                });
        }
        apply_qos(options.qos);
        walk.work();
    });

    if let Some(failure) = walk.queue.into_inner().expect("fast walk queue").failure {
        return Err(failure);
    }
    if options.cancel.load(Ordering::Relaxed) {
        return Err(ManualSizeError::Cancelled);
    }
    Ok(())
}

impl Walk<'_> {
    fn work(&self) {
        let mut buffer = vec![0_u8; buffer_size(self.options)];
        while let Some(dir) = self.next() {
            let mut claim = Claim {
                walk: self,
                subdirs: Vec::new(),
                failure: None,
            };
            match self.visit(dir, &mut buffer) {
                Ok(subdirs) => claim.subdirs = subdirs,
                Err(failure) => claim.failure = Some(failure),
            }
        }
    }

    /// Claims the next pending directory, or `None` once the walk is done
    /// (nothing pending and nobody busy) or has failed.
    fn next(&self) -> Option<PathBuf> {
        let mut queue = self.queue.lock().expect("fast walk queue");
        loop {
            if queue.failure.is_some() {
                return None;
            }
            if let Some(dir) = queue.pending.pop() {
                queue.busy += 1;
                return Some(dir);
            }
            if queue.busy == 0 {
                return None;
            }
            queue = self.wake.wait(queue).expect("fast walk queue");
        }
    }

    fn visit(&self, dir: PathBuf, buffer: &mut [u8]) -> Result<Vec<PathBuf>, ManualSizeError> {
        check_stop(self.options)?;
        if self.options.excluded.contains(&dir) {
            return Ok(Vec::new());
        }
        let Some(listing) = read_dir(&dir, self.options, self.allowed_devs.as_deref(), buffer)?
        else {
            return Ok(Vec::new());
        };
        if listing.denied && self.fail_on_denied {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("permission denied: {}", dir.display()),
            )
            .into());
        }
        let subdirs = listing.subdirs.iter().map(|name| dir.join(name)).collect();
        (self.on_record)(DirRecord {
            path: dir,
            direct_bytes: listing.direct_bytes,
            file_count: listing.file_count,
            subdirs: listing.subdirs,
            denied: listing.denied,
        });
        Ok(subdirs)
    }
}

/// A directory being visited. Dropping it (normally or while unwinding)
/// queues its subdirectories, records any failure and wakes idle workers, so
/// a panicking `on_record` stops the walk instead of leaving it hung.
struct Claim<'a> {
    walk: &'a Walk<'a>,
    subdirs: Vec<PathBuf>,
    failure: Option<ManualSizeError>,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        let mut queue = self.walk.queue.lock().expect("fast walk queue");
        queue.busy -= 1;
        queue.pending.append(&mut self.subdirs);
        if let Some(failure) = self.failure.take() {
            queue.failure.get_or_insert(failure);
        } else if thread::panicking() {
            queue.failure.get_or_insert(ManualSizeError::Cancelled);
        }
        self.walk.wake.notify_all();
    }
}

fn check_stop(options: &WalkOptions) -> Result<(), ManualSizeError> {
    if options.cancel.load(Ordering::Relaxed) {
        return Err(ManualSizeError::Cancelled);
    }
    if options
        .deadline
        .is_some_and(|deadline| Instant::now() >= deadline)
    {
        return Err(ManualSizeError::Timeout);
    }
    Ok(())
}

/// Reads `dir` with `getattrlistbulk`. `Ok(None)` means it vanished or lies
/// on a device outside `allowed_devs`.
fn read_dir(
    dir: &Path,
    options: &WalkOptions,
    allowed_devs: Option<&[u64]>,
    buffer: &mut [u8],
) -> Result<Option<DirListing>, ManualSizeError> {
    before_dir_open(options, dir);
    let mut listing = DirListing::default();
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(dir)
    {
        Ok(file) => file,
        Err(error) => return Ok(absorb(error, &mut listing)?.then_some(listing)),
    };
    if let Some(allowed) = allowed_devs {
        if !allowed.contains(&device_of(options, dir, file.metadata()?.dev())) {
            return Ok(None);
        }
    }

    let mut attributes = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_RETURNED_ATTRS
            | libc::ATTR_CMN_NAME
            | ATTR_CMN_ERROR
            | libc::ATTR_CMN_OBJTYPE,
        volattr: 0,
        dirattr: 0,
        fileattr: libc::ATTR_FILE_DATALENGTH,
        forkattr: 0,
    };
    loop {
        check_stop(options)?;
        // SAFETY: `attributes` and `buffer` are live, exclusively borrowed and
        // sized as passed; the kernel writes at most `buffer.len()` bytes.
        let count = unsafe {
            libc::getattrlistbulk(
                file.as_raw_fd(),
                (&mut attributes as *mut libc::attrlist).cast(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                u64::from(libc::FSOPT_PACK_INVAL_ATTRS),
            )
        };
        if count < 0 {
            let error = io::Error::last_os_error();
            return Ok(absorb(error, &mut listing)?.then_some(listing));
        }
        if count == 0 {
            return Ok(Some(listing));
        }

        let mut offset = 0;
        for _ in 0..count {
            let entry = buffer
                .get(offset..)
                .and_then(|rest| rest.get(..read_u32(rest, ENTRY_LENGTH)? as usize))
                .filter(|entry| entry.len() >= ENTRY_DATA_LENGTH)
                .ok_or_else(malformed)?;
            offset += entry.len();
            match parse_entry(entry).ok_or_else(malformed)? {
                Entry::File(length) => {
                    listing.direct_bytes = listing.direct_bytes.saturating_add(length);
                    listing.file_count += 1;
                }
                Entry::Dir(name) => listing.subdirs.push(OsStr::from_bytes(name).to_os_string()),
                Entry::Skip => {}
                Entry::Error(errno) => {
                    absorb(io::Error::from_raw_os_error(errno), &mut listing)?;
                }
            }
        }
    }
}

enum Entry<'a> {
    File(u64),
    Dir(&'a [u8]),
    Skip,
    Error(i32),
}

/// Decodes one bounds-checked entry; `None` means it is malformed.
fn parse_entry(entry: &[u8]) -> Option<Entry<'_>> {
    let error = read_u32(entry, ENTRY_ERROR)?;
    if error != 0 {
        return Some(Entry::Error(error as i32));
    }
    let required = libc::ATTR_CMN_NAME | libc::ATTR_CMN_OBJTYPE;
    if read_u32(entry, RETURNED_COMMON)? & required != required {
        return Some(Entry::Skip);
    }
    match read_u32(entry, ENTRY_TYPE)? {
        VDIR => {
            // The name offset is relative to the `attrreference_t` itself and
            // its length includes the trailing NUL.
            let start = ENTRY_NAME.checked_add_signed(read_i32(entry, ENTRY_NAME)? as isize)?;
            let length = read_u32(entry, ENTRY_NAME + 4)? as usize;
            let name = entry.get(start..start.checked_add(length)?)?;
            let name = name.strip_suffix(&[0]).unwrap_or(name);
            let unsafe_name =
                name.is_empty() || name == b"." || name == b".." || name.contains(&b'/');
            (!unsafe_name).then_some(Entry::Dir(name))
        }
        VLNK => Some(Entry::Skip),
        _ if read_u32(entry, RETURNED_FILE)? & libc::ATTR_FILE_DATALENGTH != 0 => {
            Some(Entry::File(read_u64(entry, ENTRY_DATA_LENGTH)?))
        }
        _ => Some(Entry::Skip),
    }
}

/// Folds an OS failure into `listing`: a vanished object yields `Ok(false)`,
/// a refused one marks the listing denied and yields `Ok(true)`, anything else
/// is a real error.
fn absorb(error: io::Error, listing: &mut DirListing) -> Result<bool, ManualSizeError> {
    match error.raw_os_error() {
        Some(libc::ENOENT | libc::ENOTDIR | libc::ELOOP) => Ok(false),
        Some(libc::EACCES | libc::EPERM) => {
            listing.denied = true;
            Ok(true)
        }
        _ => Err(error.into()),
    }
}

fn malformed() -> ManualSizeError {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "malformed getattrlistbulk entry",
    )
    .into()
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn read_i32(bytes: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_ne_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_ne_bytes(
        bytes.get(offset..offset.checked_add(8)?)?.try_into().ok()?,
    ))
}

/// Reserves up to `wanted` helpers from the process-wide budget.
fn acquire_helpers(wanted: usize) -> usize {
    let Ok(previous) = HELPERS_IN_USE.fetch_update(Ordering::AcqRel, Ordering::Acquire, |in_use| {
        let granted = wanted.min(MAX_HELPER_THREADS.saturating_sub(in_use));
        (granted > 0).then_some(in_use + granted)
    }) else {
        return 0;
    };
    let granted = wanted.min(MAX_HELPER_THREADS - previous);
    #[cfg(feature = "test-utils")]
    PEAK_HELPERS.fetch_max(previous + granted, Ordering::AcqRel);
    granted
}

/// One reserved helper slot, returned when the helper thread ends.
struct HelperPermit;

impl Drop for HelperPermit {
    fn drop(&mut self) {
        HELPERS_IN_USE.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(not(feature = "test-utils"))]
fn apply_qos(qos: Qos) {
    let class = match qos {
        Qos::UserInitiated => libc::qos_class_t::QOS_CLASS_USER_INITIATED,
        Qos::Background => libc::qos_class_t::QOS_CLASS_BACKGROUND,
    };
    // SAFETY: only changes the calling thread's scheduling class. The calling
    // thread keeps it after the walk; every walk sets its own class first.
    unsafe {
        libc::pthread_set_qos_class_self_np(class, 0);
    }
}

/// QoS is process-scheduling state; tests never change it.
#[cfg(feature = "test-utils")]
fn apply_qos(_qos: Qos) {}

#[cfg(not(feature = "test-utils"))]
fn buffer_size(_options: &WalkOptions) -> usize {
    BUFFER_SIZE
}

#[cfg(feature = "test-utils")]
fn buffer_size(options: &WalkOptions) -> usize {
    options.hooks.buffer_size
}

#[cfg(not(feature = "test-utils"))]
fn before_dir_open(_options: &WalkOptions, _dir: &Path) {}

#[cfg(feature = "test-utils")]
fn before_dir_open(options: &WalkOptions, dir: &Path) {
    if let Some(hook) = &options.hooks.before_dir_open {
        hook(dir);
    }
}

#[cfg(not(feature = "test-utils"))]
fn device_of(_options: &WalkOptions, _dir: &Path, dev: u64) -> u64 {
    dev
}

#[cfg(feature = "test-utils")]
fn device_of(options: &WalkOptions, dir: &Path, dev: u64) -> u64 {
    options
        .hooks
        .device_of
        .as_ref()
        .map_or(dev, |hook| hook(dir, dev))
}
