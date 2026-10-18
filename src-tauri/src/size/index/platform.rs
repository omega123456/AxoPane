//! The folder-size index's only contact with macOS services: the FSEvents
//! stream, event IDs and the volume UUID, the OS build, power and thermal
//! notifications, the Full Disk Access probe and the home directory.
//!
//! Under `feature = "test-utils"` only [`FakePlatform`] exists, so tests
//! never reach a real OS service.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::persist::ChangeFlags;

/// Where the Data volume is mounted; FSEvents may report paths under it.
pub const DATA_VOLUME_PREFIX: &str = "/System/Volumes/Data";

/// One directory-level FSEvents record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEvent {
    pub path: PathBuf,
    pub id: u64,
    pub flags: ChangeFlags,
    /// Marks the end of history replay; carries no change.
    pub history_done: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSignal {
    WillSleep,
    DidWake,
    /// Current Low Power Mode and serious-or-critical thermal state.
    Conditions {
        low_power: bool,
        thermal_pressure: bool,
    },
}

pub type EventSink = Arc<dyn Fn(Vec<RawEvent>) + Send + Sync>;
pub type PowerSink = Arc<dyn Fn(PowerSignal) + Send + Sync>;
/// A running event stream; dropping it stops the stream.
pub type StreamHandle = Box<dyn Send>;

pub trait IndexPlatform: Send + Sync {
    fn current_event_id(&self) -> u64;
    fn volume_uuid(&self, path: &Path) -> Option<String>;
    /// The macOS build (`kern.osversion`); a change means a rescan.
    fn os_build(&self) -> Option<String>;
    /// Starts a directory-level stream over `roots` (minus `excluded`),
    /// replaying from `since` when given. `None` if it could not start.
    fn start_stream(
        &self,
        roots: &[PathBuf],
        excluded: &[PathBuf],
        since: Option<u64>,
        sink: EventSink,
    ) -> Option<StreamHandle>;
    /// Non-prompting probe; any failure means no Full Disk Access.
    fn full_disk_access(&self) -> bool;
    /// Starts power observation and returns the current conditions.
    fn observe_power(&self, sink: PowerSink) -> PowerSignal;
}

/// Maps a `/System/Volumes/Data`-prefixed path onto its firmlinked,
/// pane-visible path.
pub fn normalize(path: &Path) -> PathBuf {
    match path.strip_prefix(DATA_VOLUME_PREFIX) {
        Ok(rest) if rest.components().next().is_some() => Path::new("/").join(rest),
        _ => path.to_path_buf(),
    }
}

/// Folders background work never reads until visited (FR5.1).
pub fn protected_roots(home: &Path) -> Vec<PathBuf> {
    [
        "Desktop",
        "Documents",
        "Downloads",
        "Library/Mobile Documents",
        "Library/CloudStorage",
        "Library/Containers",
        "Library/Group Containers",
    ]
    .iter()
    .map(|relative| home.join(relative))
    .collect()
}

/// Paths left out of the stream over `/`: other volumes and devfs, then the
/// high-churn locations (FR2.3). `/System/Volumes` stays in, because
/// Data-volume changes may be reported under [`DATA_VOLUME_PREFIX`]; the
/// index drops the rest of it after [`normalize`]. FSEvents takes at most
/// eight exclusions.
pub fn stream_exclusions(home: &Path) -> Vec<PathBuf> {
    [
        PathBuf::from("/Volumes"),
        PathBuf::from("/dev"),
        PathBuf::from("/private/var/folders"),
        PathBuf::from("/private/var/db"),
        PathBuf::from("/private/var/log"),
        home.join("Library/Caches"),
    ]
    .to_vec()
}

#[cfg(feature = "test-utils")]
pub use fake::FakePlatform;

#[cfg(feature = "test-utils")]
mod fake {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use super::{EventSink, IndexPlatform, PowerSignal, PowerSink, RawEvent, StreamHandle};

    /// Scriptable platform: tests set IDs, the UUID, the OS build, Full Disk
    /// Access and power, and push event batches into the running stream.
    pub struct FakePlatform {
        state: Mutex<State>,
    }

    struct State {
        event_id: u64,
        uuid: Option<String>,
        os_build: Option<String>,
        full_disk_access: bool,
        stream_fails: bool,
        conditions: PowerSignal,
        events: Option<EventSink>,
        power: Option<PowerSink>,
        stream_starts: Vec<Option<u64>>,
    }

    impl Default for FakePlatform {
        fn default() -> Self {
            Self {
                state: Mutex::new(State {
                    event_id: 100,
                    uuid: Some("FAKE-VOLUME".to_string()),
                    os_build: Some("FAKE-BUILD".to_string()),
                    full_disk_access: false,
                    stream_fails: false,
                    conditions: PowerSignal::Conditions {
                        low_power: false,
                        thermal_pressure: false,
                    },
                    events: None,
                    power: None,
                    stream_starts: Vec::new(),
                }),
            }
        }
    }

    impl FakePlatform {
        fn state(&self) -> std::sync::MutexGuard<'_, State> {
            self.state.lock().expect("fake platform lock")
        }

        pub fn set_event_id(&self, id: u64) {
            self.state().event_id = id;
        }

        pub fn set_volume_uuid(&self, uuid: Option<&str>) {
            self.state().uuid = uuid.map(str::to_string);
        }

        pub fn set_os_build(&self, build: Option<&str>) {
            self.state().os_build = build.map(str::to_string);
        }

        pub fn set_full_disk_access(&self, granted: bool) {
            self.state().full_disk_access = granted;
        }

        pub fn fail_stream(&self) {
            self.state().stream_fails = true;
        }

        /// Conditions reported when observation starts.
        pub fn set_initial_conditions(&self, low_power: bool, thermal_pressure: bool) {
            self.state().conditions = PowerSignal::Conditions {
                low_power,
                thermal_pressure,
            };
        }

        /// Delivers a batch to the running stream; false if none runs.
        pub fn emit(&self, events: Vec<RawEvent>) -> bool {
            let sink = self.state().events.clone();
            sink.map(|sink| sink(events)).is_some()
        }

        /// Delivers a power signal; false before observation starts.
        pub fn signal(&self, signal: PowerSignal) -> bool {
            let sink = self.state().power.clone();
            sink.map(|sink| sink(signal)).is_some()
        }

        /// The `since` argument of every stream start, in order.
        pub fn stream_starts(&self) -> Vec<Option<u64>> {
            self.state().stream_starts.clone()
        }
    }

    impl IndexPlatform for FakePlatform {
        fn current_event_id(&self) -> u64 {
            self.state().event_id
        }

        fn volume_uuid(&self, _path: &Path) -> Option<String> {
            self.state().uuid.clone()
        }

        fn os_build(&self) -> Option<String> {
            self.state().os_build.clone()
        }

        fn start_stream(
            &self,
            _roots: &[PathBuf],
            _excluded: &[PathBuf],
            since: Option<u64>,
            sink: EventSink,
        ) -> Option<StreamHandle> {
            let mut state = self.state();
            state.stream_starts.push(since);
            if state.stream_fails {
                return None;
            }
            state.events = Some(sink);
            Some(Box::new(()))
        }

        fn full_disk_access(&self) -> bool {
            self.state().full_disk_access
        }

        fn observe_power(&self, sink: PowerSink) -> PowerSignal {
            let mut state = self.state();
            state.power = Some(sink);
            state.conditions
        }
    }
}

#[cfg(not(feature = "test-utils"))]
pub use mac::{home_dir, MacPlatform};

#[cfg(not(feature = "test-utils"))]
mod mac {
    use std::ffi::{c_char, c_void, CStr, CString, OsStr};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    use std::path::{Path, PathBuf};
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::thread::{self, JoinHandle};

    use block2::RcBlock;
    use fsevent_sys as fse;
    use fsevent_sys::core_foundation as cf;
    use objc2_app_kit::{
        NSWorkspace, NSWorkspaceDidWakeNotification, NSWorkspaceWillSleepNotification,
    };
    use objc2_foundation::{
        NSNotification, NSNotificationCenter, NSNotificationName, NSProcessInfo,
        NSProcessInfoPowerStateDidChangeNotification, NSProcessInfoThermalState,
        NSProcessInfoThermalStateDidChangeNotification,
    };

    use super::super::persist::ChangeFlags;
    use super::{EventSink, IndexPlatform, PowerSignal, PowerSink, RawEvent, StreamHandle};

    const TCC_DATABASE: &str = "/Library/Application Support/com.apple.TCC/TCC.db";
    /// Directory-level events, coalesced for about a second (D3).
    const LATENCY_SECS: f64 = 1.0;
    /// `FSEventStreamSetExclusionPaths` accepts at most eight paths.
    const MAX_EXCLUSIONS: usize = 8;

    extern "C" {
        /// Declared in `<CoreServices/FSEvents.h>`; `fsevent-sys` omits it.
        fn FSEventsCopyUUIDForDevice(dev: libc::dev_t) -> *const c_void;
    }

    pub fn home_dir() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .filter(|home| !home.is_empty())
            .map(PathBuf::from)
    }

    pub struct MacPlatform;

    impl IndexPlatform for MacPlatform {
        fn current_event_id(&self) -> u64 {
            // SAFETY: a plain query with no arguments.
            unsafe { fse::FSEventsGetCurrentEventId() }
        }

        fn volume_uuid(&self, path: &Path) -> Option<String> {
            let dev = std::fs::metadata(path).ok()?.dev();
            // SAFETY: the returned CFUUID (if any) is owned by us and released
            // below; the string is copied before its release.
            unsafe {
                let uuid = FSEventsCopyUUIDForDevice(dev as libc::dev_t);
                if uuid.is_null() {
                    return None;
                }
                let string = core_foundation_sys::uuid::CFUUIDCreateString(
                    std::ptr::null(),
                    uuid as core_foundation_sys::uuid::CFUUIDRef,
                );
                cf::CFRelease(uuid as cf::CFRef);
                if string.is_null() {
                    return None;
                }
                let mut buffer = [0 as c_char; 64];
                let copied = core_foundation_sys::string::CFStringGetCString(
                    string,
                    buffer.as_mut_ptr(),
                    buffer.len() as isize,
                    core_foundation_sys::string::kCFStringEncodingUTF8,
                );
                cf::CFRelease(string as cf::CFRef);
                (copied != 0).then(|| {
                    CStr::from_ptr(buffer.as_ptr())
                        .to_string_lossy()
                        .into_owned()
                })
            }
        }

        fn os_build(&self) -> Option<String> {
            let mut buffer = [0 as c_char; 64];
            let mut length = buffer.len();
            // SAFETY: a read-only query into a live buffer of `length` bytes;
            // the kernel NUL-terminates the string it writes.
            let status = unsafe {
                libc::sysctlbyname(
                    c"kern.osversion".as_ptr(),
                    buffer.as_mut_ptr().cast(),
                    &mut length,
                    std::ptr::null_mut(),
                    0,
                )
            };
            // SAFETY: on success `buffer` holds a NUL-terminated string.
            (status == 0).then(|| {
                unsafe { CStr::from_ptr(buffer.as_ptr()) }
                    .to_string_lossy()
                    .into_owned()
            })
        }

        fn start_stream(
            &self,
            roots: &[PathBuf],
            excluded: &[PathBuf],
            since: Option<u64>,
            sink: EventSink,
        ) -> Option<StreamHandle> {
            let roots = roots.to_vec();
            let excluded = excluded
                .iter()
                .take(MAX_EXCLUSIONS)
                .cloned()
                .collect::<Vec<_>>();
            let (started_tx, started_rx) = mpsc::channel();
            let stopped = Arc::new(AtomicBool::new(false));
            let thread_stopped = Arc::clone(&stopped);
            let thread = thread::Builder::new()
                .name("size-index-fsevents".to_string())
                .spawn(move || {
                    run_stream(&roots, &excluded, since, sink, &thread_stopped, started_tx)
                })
                .ok()?;
            match started_rx.recv() {
                Ok(Some(run_loop)) => Some(Box::new(Stream {
                    run_loop,
                    stopped,
                    thread: Some(thread),
                })),
                _ => {
                    let _ = thread.join();
                    None
                }
            }
        }

        fn full_disk_access(&self) -> bool {
            std::fs::File::open(TCC_DATABASE).is_ok()
        }

        fn observe_power(&self, sink: PowerSink) -> PowerSignal {
            let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
            let process = NSNotificationCenter::defaultCenter();
            // SAFETY: these are immutable framework-provided string constants.
            unsafe {
                observe(
                    &workspace,
                    NSWorkspaceWillSleepNotification,
                    &sink,
                    Some(PowerSignal::WillSleep),
                );
                observe(
                    &workspace,
                    NSWorkspaceDidWakeNotification,
                    &sink,
                    Some(PowerSignal::DidWake),
                );
                observe(
                    &process,
                    NSProcessInfoPowerStateDidChangeNotification,
                    &sink,
                    None,
                );
                observe(
                    &process,
                    NSProcessInfoThermalStateDidChangeNotification,
                    &sink,
                    None,
                );
            }
            conditions()
        }
    }

    fn conditions() -> PowerSignal {
        let info = NSProcessInfo::processInfo();
        PowerSignal::Conditions {
            low_power: info.isLowPowerModeEnabled(),
            thermal_pressure: info.thermalState() >= NSProcessInfoThermalState::Serious,
        }
    }

    /// Forwards `name` to `sink` as `signal`, or as fresh conditions.
    fn observe(
        center: &NSNotificationCenter,
        name: &NSNotificationName,
        sink: &PowerSink,
        signal: Option<PowerSignal>,
    ) {
        let sink = Arc::clone(sink);
        let block = RcBlock::new(move |_note: NonNull<NSNotification>| {
            sink(signal.unwrap_or_else(conditions));
        });
        // SAFETY: no object filter or queue; the block only forwards a value
        // and is safe to run on any thread.
        let token = unsafe {
            center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block)
        };
        // One index per app, observing for the process lifetime, so the
        // observer is deliberately never removed.
        std::mem::forget(token);
    }

    /// The stream's run-loop thread; dropping stops the loop and joins.
    struct Stream {
        run_loop: usize,
        stopped: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl Drop for Stream {
        fn drop(&mut self) {
            self.stopped.store(true, Ordering::SeqCst);
            // SAFETY: the run loop belongs to the stream thread, which stays
            // alive until joined below; stopping it from another thread is
            // documented as safe.
            unsafe { cf::CFRunLoopStop(self.run_loop as cf::CFRunLoopRef) };
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn cf_paths(paths: &[PathBuf]) -> cf::CFMutableArrayRef {
        // SAFETY: CF ownership: the array retains each appended string, which
        // we release right after appending.
        unsafe {
            let array =
                cf::CFArrayCreateMutable(cf::kCFAllocatorDefault, 0, &cf::kCFTypeArrayCallBacks);
            for path in paths {
                let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
                    continue;
                };
                let string = cf::CFStringCreateWithCString(
                    cf::kCFAllocatorDefault,
                    c_path.as_ptr(),
                    cf::kCFStringEncodingUTF8,
                );
                if !string.is_null() {
                    cf::CFArrayAppendValue(array, string);
                    cf::CFRelease(string);
                }
            }
            array
        }
    }

    fn run_stream(
        roots: &[PathBuf],
        excluded: &[PathBuf],
        since: Option<u64>,
        sink: EventSink,
        stopped: &AtomicBool,
        started: mpsc::Sender<Option<usize>>,
    ) {
        let sink = Box::into_raw(Box::new(sink));
        let context = fse::FSEventStreamContext {
            version: 0,
            info: sink.cast(),
            retain: None,
            release: None,
            copy_description: None,
        };
        // SAFETY: every CF object created here is released before returning;
        // `sink` outlives the stream (freed after it is invalidated).
        unsafe {
            let paths = cf_paths(roots);
            let stream = fse::FSEventStreamCreate(
                cf::kCFAllocatorDefault,
                on_events,
                &context,
                paths,
                since.unwrap_or(fse::kFSEventStreamEventIdSinceNow),
                LATENCY_SECS,
                fse::kFSEventStreamCreateFlagWatchRoot,
            );
            cf::CFRelease(paths);
            if stream.is_null() {
                drop(Box::from_raw(sink));
                let _ = started.send(None);
                return;
            }
            if !excluded.is_empty() {
                let exclusions = cf_paths(excluded);
                fse::FSEventStreamSetExclusionPaths(stream, exclusions);
                cf::CFRelease(exclusions);
            }
            let run_loop = cf::CFRunLoopGetCurrent();
            fse::FSEventStreamScheduleWithRunLoop(stream, run_loop, cf::kCFRunLoopDefaultMode);
            if fse::FSEventStreamStart(stream) == 0 {
                let _ = started.send(None);
            } else {
                let _ = started.send(Some(run_loop as usize));
                // A stop that lands before the loop runs leaves the loop's
                // stop flag set, so `CFRunLoopRun` returns at once.
                while !stopped.load(Ordering::SeqCst) {
                    cf::CFRunLoopRun();
                }
                fse::FSEventStreamStop(stream);
            }
            fse::FSEventStreamInvalidate(stream);
            fse::FSEventStreamRelease(stream);
            drop(Box::from_raw(sink));
        }
    }

    extern "C" fn on_events(
        _stream: fse::FSEventStreamRef,
        info: *mut c_void,
        count: usize,
        paths: *mut c_void,
        flags: *const fse::FSEventStreamEventFlags,
        ids: *const fse::FSEventStreamEventId,
    ) {
        // SAFETY: FSEvents passes `count` C-string paths, flags and IDs, and
        // `info` is the `EventSink` installed by `run_stream`.
        let batch = unsafe {
            let sink = &*(info as *const EventSink);
            let paths = paths as *const *const c_char;
            let batch = (0..count)
                .map(|i| {
                    let path = CStr::from_ptr(*paths.add(i));
                    let flags = *flags.add(i);
                    RawEvent {
                        path: PathBuf::from(OsStr::from_bytes(path.to_bytes())),
                        id: *ids.add(i),
                        flags: ChangeFlags {
                            must_scan_sub_dirs: flags & fse::kFSEventStreamEventFlagMustScanSubDirs
                                != 0,
                            user_dropped: flags & fse::kFSEventStreamEventFlagUserDropped != 0,
                            kernel_dropped: flags & fse::kFSEventStreamEventFlagKernelDropped != 0,
                            event_ids_wrapped: flags & fse::kFSEventStreamEventFlagEventIdsWrapped
                                != 0,
                            root_changed: flags & fse::kFSEventStreamEventFlagRootChanged != 0,
                        },
                        history_done: flags & fse::kFSEventStreamEventFlagHistoryDone != 0,
                    }
                })
                .collect();
            (sink, batch)
        };
        (batch.0)(batch.1);
    }
}
