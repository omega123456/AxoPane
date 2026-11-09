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

use super::index::names::PackedNames;
use super::index::platform::DATA_VOLUME_PREFIX;
use super::manual::ManualSizeError;

pub const MAX_HELPER_THREADS: usize = 8;
pub const MAX_JOB_THREADS: usize = 4;
pub const INDEX_EXCLUDED_ROOTS: [&str; 3] = ["/System/Volumes", "/Volumes", "/dev"];

const BUFFER_SIZE: usize = 128 * 1024;

const ATTR_CMN_ERROR: libc::attrgroup_t = 0x2000_0000;
const VDIR: u32 = 2;
const VLNK: u32 = 5;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Qos {
    UserInitiated,
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirRecord {
    pub path: PathBuf,
    pub direct_bytes: u64,
    pub file_count: u64,
    pub subdirs: Vec<OsString>,
    pub denied: bool,
    pub file_names: Option<PackedNames>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirListing {
    pub direct_bytes: u64,
    pub file_count: u64,
    pub subdirs: Vec<OsString>,
    pub denied: bool,
    pub file_names: Option<PackedNames>,
}

impl DirListing {
    fn for_options(options: &WalkOptions) -> Self {
        Self {
            file_names: options.collect_file_names.then(PackedNames::default),
            ..Self::default()
        }
    }

    fn with_sorted_names(mut self) -> Self {
        if let Some(names) = &mut self.file_names {
            names.sort();
        }
        self
    }

    fn add_file(&mut self, file: ListedFile<'_>) {
        self.direct_bytes = self.direct_bytes.saturating_add(file.length);
        self.file_count += 1;
        let Some(names) = &mut self.file_names else {
            return;
        };
        if let Some(name) = file.name {
            names.push(name);
        }
    }
}

#[cfg(feature = "test-utils")]
pub type DirHook = Arc<dyn Fn(&Path) + Send + Sync>;
#[cfg(feature = "test-utils")]
pub type DeviceHook = Arc<dyn Fn(&Path, u64) -> u64 + Send + Sync>;

#[cfg(feature = "test-utils")]
#[derive(Clone)]
pub struct TestHooks {
    pub buffer_size: usize,
    pub before_dir_open: Option<DirHook>,
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
    pub deadline: Option<Instant>,
    pub threads: usize,
    pub qos: Qos,
    pub excluded: HashSet<PathBuf>,
    pub stay_on_device: bool,
    pub collect_file_names: bool,
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
            collect_file_names: false,
            #[cfg(feature = "test-utils")]
            hooks: TestHooks::default(),
        }
    }

    pub fn with_index_boundaries(mut self) -> Self {
        self.excluded
            .extend(INDEX_EXCLUDED_ROOTS.iter().map(PathBuf::from));
        self.stay_on_device = true;
        self
    }
}

pub fn subtree_total(root: &Path, options: &WalkOptions) -> Result<u64, ManualSizeError> {
    let total = AtomicU64::new(0);
    let sink = RecordSink {
        fail_on_denied: true,
        on_record: &|record| {
            total.fetch_add(record.direct_bytes, Ordering::Relaxed);
        },
    };
    walk(root, options, sink)?;
    Ok(total.into_inner())
}

pub fn subtree_records(
    root: &Path,
    options: &WalkOptions,
    on_record: impl Fn(DirRecord) + Sync,
) -> Result<(), ManualSizeError> {
    let sink = RecordSink {
        fail_on_denied: false,
        on_record: &on_record,
    };
    walk(root, options, sink)
}

pub fn list_dir(dir: &Path, options: &WalkOptions) -> Result<DirListing, ManualSizeError> {
    let mut buffer = vec![0_u8; buffer_size(options)];
    let scope = ReadScope {
        options,
        allowed_devs: None,
    };
    read_dir(dir, scope, &mut buffer)?
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound).into())
}

#[cfg(feature = "test-utils")]
pub fn helper_threads_for_tests() -> HelperThreadCounts {
    HelperThreadCounts {
        in_use: HELPERS_IN_USE.load(Ordering::Acquire),
        peak: PEAK_HELPERS.load(Ordering::Acquire),
    }
}

#[cfg(feature = "test-utils")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelperThreadCounts {
    pub in_use: usize,
    pub peak: usize,
}

struct Queue {
    pending: Vec<PathBuf>,
    busy: usize,
    failure: Option<ManualSizeError>,
}

impl Queue {
    fn is_waiting_for_work(&self) -> bool {
        self.failure.is_none() && self.pending.is_empty() && self.busy > 0
    }

    fn claim_pending(&mut self) -> Option<PathBuf> {
        if self.failure.is_some() {
            return None;
        }
        let dir = self.pending.pop()?;
        self.busy += 1;
        Some(dir)
    }
}

struct Walk<'a> {
    options: &'a WalkOptions,
    allowed_devs: Option<Vec<u64>>,
    fail_on_denied: bool,
    on_record: &'a (dyn Fn(DirRecord) + Sync),
    queue: Mutex<Queue>,
    wake: Condvar,
}

fn allowed_devices(
    options: &WalkOptions,
    root: &Path,
) -> Result<Option<Vec<u64>>, ManualSizeError> {
    if !options.stay_on_device {
        return Ok(None);
    }
    let root_device = std::fs::symlink_metadata(root)?.dev();
    let mut devices = vec![device_of(options, root, root_device)];
    let data_volume = Path::new(DATA_VOLUME_PREFIX);
    if let Ok(data_metadata) = std::fs::metadata(data_volume) {
        devices.push(device_of(options, data_volume, data_metadata.dev()));
    }
    Ok(Some(devices))
}

#[derive(Clone, Copy)]
struct RecordSink<'sink> {
    fail_on_denied: bool,
    on_record: &'sink (dyn Fn(DirRecord) + Sync),
}

#[derive(Clone, Copy)]
struct ReadScope<'scope> {
    options: &'scope WalkOptions,
    allowed_devs: Option<&'scope [u64]>,
}

fn walk(root: &Path, options: &WalkOptions, sink: RecordSink<'_>) -> Result<(), ManualSizeError> {
    let allowed_devs = allowed_devices(options, root)?;
    let walk = Walk {
        options,
        allowed_devs,
        fail_on_denied: sink.fail_on_denied,
        on_record: sink.on_record,
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
            let _ = thread::Builder::new()
                .name("fast-walk".to_string())
                .spawn_scoped(scope, move || run_helper(permit, walk));
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

fn run_helper(permit: HelperPermit, walk: &Walk<'_>) {
    let _permit = permit;
    apply_qos(walk.options.qos);
    walk.work();
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

    fn next(&self) -> Option<PathBuf> {
        let mut queue = self.queue.lock().expect("fast walk queue");
        while queue.is_waiting_for_work() {
            queue = self.wake.wait(queue).expect("fast walk queue");
        }
        queue.claim_pending()
    }

    fn visit(&self, dir: PathBuf, buffer: &mut [u8]) -> Result<Vec<PathBuf>, ManualSizeError> {
        check_stop(self.options)?;
        if self.options.excluded.contains(&dir) {
            return Ok(Vec::new());
        }
        let scope = ReadScope {
            options: self.options,
            allowed_devs: self.allowed_devs.as_deref(),
        };
        let Some(listing) = read_dir(&dir, scope, buffer)? else {
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
            file_names: listing.file_names,
        });
        Ok(subdirs)
    }
}

struct Claim<'a> {
    walk: &'a Walk<'a>,
    subdirs: Vec<PathBuf>,
    failure: Option<ManualSizeError>,
}

impl Claim<'_> {
    fn take_failure(&mut self) -> Option<ManualSizeError> {
        if let Some(failure) = self.failure.take() {
            return Some(failure);
        }
        if thread::panicking() {
            return Some(ManualSizeError::Cancelled);
        }
        None
    }
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        let failure = self.take_failure();
        let mut queue = self.walk.queue.lock().expect("fast walk queue");
        queue.busy -= 1;
        queue.pending.append(&mut self.subdirs);
        if let Some(failure) = failure {
            queue.failure.get_or_insert(failure);
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

fn read_dir(
    dir: &Path,
    scope: ReadScope<'_>,
    buffer: &mut [u8],
) -> Result<Option<DirListing>, ManualSizeError> {
    let listing = read_unsorted_dir(dir, scope, buffer)?;
    Ok(listing.map(DirListing::with_sorted_names))
}

fn read_unsorted_dir(
    dir: &Path,
    scope: ReadScope<'_>,
    buffer: &mut [u8],
) -> Result<Option<DirListing>, ManualSizeError> {
    let ReadScope {
        options,
        allowed_devs,
    } = scope;
    before_dir_open(options, dir);
    let mut listing = DirListing::for_options(options);
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

    let mut attributes = bulk_attributes();
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

        absorb_entries(buffer, count, &mut listing)?;
    }
}

fn bulk_attributes() -> libc::attrlist {
    libc::attrlist {
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
    }
}

fn absorb_entries(
    buffer: &[u8],
    count: libc::c_int,
    listing: &mut DirListing,
) -> Result<(), ManualSizeError> {
    let mut offset = 0;
    for _ in 0..count {
        let entry = buffer
            .get(offset..)
            .and_then(|rest| rest.get(..read_u32(rest, ENTRY_LENGTH)? as usize))
            .filter(|entry| entry.len() >= ENTRY_DATA_LENGTH)
            .ok_or_else(malformed)?;
        offset += entry.len();
        absorb_entry(parse_entry(entry).ok_or_else(malformed)?, listing)?;
    }
    Ok(())
}

fn absorb_entry(entry: Entry<'_>, listing: &mut DirListing) -> Result<(), ManualSizeError> {
    match entry {
        Entry::File(file) => listing.add_file(file),
        Entry::Dir(name) => listing.subdirs.push(OsStr::from_bytes(name).to_os_string()),
        Entry::Skip => {}
        Entry::Error(errno) => {
            absorb(io::Error::from_raw_os_error(errno), listing)?;
        }
    }
    Ok(())
}

struct ListedFile<'a> {
    length: u64,
    name: Option<&'a [u8]>,
}

enum Entry<'a> {
    File(ListedFile<'a>),
    Dir(&'a [u8]),
    Skip,
    Error(i32),
}

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
            let name = entry_name(entry)?;
            is_listable_name(name).then_some(Entry::Dir(name))
        }
        VLNK => Some(Entry::Skip),
        _ if read_u32(entry, RETURNED_FILE)? & libc::ATTR_FILE_DATALENGTH != 0 => {
            Some(Entry::File(ListedFile {
                length: read_u64(entry, ENTRY_DATA_LENGTH)?,
                name: entry_name(entry).filter(|name| is_listable_name(name)),
            }))
        }
        _ => Some(Entry::Skip),
    }
}

fn entry_name(entry: &[u8]) -> Option<&[u8]> {
    let start = ENTRY_NAME.checked_add_signed(read_i32(entry, ENTRY_NAME)? as isize)?;
    let length = read_u32(entry, ENTRY_NAME + size_of::<i32>())? as usize;
    let name = entry.get(start..start.checked_add(length)?)?;
    Some(name.strip_suffix(&[0]).unwrap_or(name))
}

fn is_listable_name(name: &[u8]) -> bool {
    let unsafe_name = name.is_empty() || name == b"." || name == b".." || name.contains(&b'/');
    !unsafe_name
}

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
        bytes
            .get(offset..offset.checked_add(size_of::<u32>())?)?
            .try_into()
            .ok()?,
    ))
}

fn read_i32(bytes: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_ne_bytes(
        bytes
            .get(offset..offset.checked_add(size_of::<i32>())?)?
            .try_into()
            .ok()?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_ne_bytes(
        bytes
            .get(offset..offset.checked_add(size_of::<u64>())?)?
            .try_into()
            .ok()?,
    ))
}

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
