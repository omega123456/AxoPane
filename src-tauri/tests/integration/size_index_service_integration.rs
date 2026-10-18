//! Folder-size index service: fake platform, temp roots, injected clock and
//! walk hooks. macOS only (the index is a macOS backend).

use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use file_explorer_lib::resource_coordinator::ResourceCoordinator;
use file_explorer_lib::size::fast_walk::{subtree_total, TestHooks, WalkOptions};
use file_explorer_lib::size::index::persist::{self, ChangeFlags};
use file_explorer_lib::size::index::platform::{
    normalize, protected_roots, stream_exclusions, FakePlatform, PowerSignal, RawEvent,
    DATA_VOLUME_PREFIX,
};
use file_explorer_lib::size::index::{IndexOptions, LaneSnapshot, SNAPSHOT_FILE};
use file_explorer_lib::size::{
    FolderSizeIndexStatus, IndexBackend, IndexState, NaReason, PauseReason, RequestMode,
    SizeBackend, SizeService, SizeSource, SizeStateKind, SizeUpdate,
};
use tempfile::{tempdir, TempDir};

const WAIT: Duration = Duration::from_secs(1);

/// Bumped by every fake, hook and emitter callback, so waits re-check their
/// condition on each change instead of polling.
static CHANGES: Mutex<u64> = Mutex::new(0);
static CHANGED: Condvar = Condvar::new();

fn notify() {
    *CHANGES.lock().unwrap() += 1;
    CHANGED.notify_all();
}

fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    loop {
        let seen = *CHANGES.lock().unwrap();
        if condition() {
            return;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "timed out waiting for {what}");
        let changes = CHANGES.lock().unwrap();
        let _ = CHANGED
            .wait_timeout_while(changes, remaining, |now| *now == seen)
            .unwrap();
    }
}

/// Directories listed in it block in `before_dir_open` until released.
#[derive(Default)]
struct Blocker {
    blocked: Mutex<HashSet<PathBuf>>,
    released: Condvar,
}

impl Blocker {
    fn block(&self, path: &Path) {
        self.blocked.lock().unwrap().insert(path.to_path_buf());
    }

    fn release(&self, path: &Path) {
        self.blocked.lock().unwrap().remove(path);
        self.released.notify_all();
    }

    fn wait(&self, path: &Path) {
        let mut blocked = self.blocked.lock().unwrap();
        while blocked.contains(path) {
            blocked = self.released.wait(blocked).unwrap();
        }
    }
}

struct Harness {
    _dir: TempDir,
    root: PathBuf,
    roots: Vec<PathBuf>,
    excluded: Vec<PathBuf>,
    snapshot: PathBuf,
    platform: Arc<FakePlatform>,
    now: Arc<Mutex<SystemTime>>,
    live: Arc<Mutex<Vec<SizeUpdate>>>,
    statuses: Arc<Mutex<Vec<FolderSizeIndexStatus>>>,
    opened: Arc<Mutex<Vec<PathBuf>>>,
    blocker: Arc<Blocker>,
    /// Times a background walk parked for the power gate or a boost.
    parked: Arc<AtomicUsize>,
    protected: Vec<PathBuf>,
    service: Option<SizeService>,
}

impl Harness {
    /// A temp index root (`root`) with `home/Documents` as the only
    /// protected folder; nothing starts until [`Harness::start`].
    fn new() -> Self {
        let dir = tempdir().expect("temp dir");
        let base = dir.path().canonicalize().expect("canonical temp dir");
        let root = base.join("idx");
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::create_dir_all(root.join("s")).unwrap();
        fs::create_dir_all(root.join("home/Documents/inner")).unwrap();
        fs::write(root.join("top.bin"), vec![0; 1]).unwrap();
        fs::write(root.join("a/a.bin"), vec![0; 10]).unwrap();
        fs::write(root.join("a/b/b.bin"), vec![0; 100]).unwrap();
        fs::write(root.join("s/s.bin"), vec![0; 5]).unwrap();
        fs::write(root.join("home/h.bin"), vec![0; 7]).unwrap();
        fs::write(root.join("home/Documents/secret.bin"), vec![0; 1000]).unwrap();
        Self {
            snapshot: base.join("cache").join(SNAPSHOT_FILE),
            protected: vec![root.join("home/Documents")],
            _dir: dir,
            roots: vec![root.clone()],
            excluded: Vec::new(),
            root,
            platform: Arc::new(FakePlatform::default()),
            now: Arc::new(Mutex::new(
                SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000),
            )),
            live: Arc::default(),
            statuses: Arc::default(),
            opened: Arc::default(),
            blocker: Arc::default(),
            parked: Arc::default(),
            service: None,
        }
    }

    /// Starts the index and waits until its stream is running.
    fn start(&mut self) -> &SizeService {
        let starts = self.platform.stream_starts().len();
        let (opened, blocker) = (Arc::clone(&self.opened), Arc::clone(&self.blocker));
        let (now, parked) = (Arc::clone(&self.now), Arc::clone(&self.parked));
        let (live, statuses) = (Arc::clone(&self.live), Arc::clone(&self.statuses));
        let options = IndexOptions {
            roots: self.roots.clone(),
            excluded: self.excluded.clone(),
            protected: self.protected.clone(),
            stream_excluded: Vec::new(),
            snapshot_path: self.snapshot.clone(),
            platform: self.platform.clone(),
            clock: Arc::new(move || *now.lock().unwrap()),
            coordinator: Arc::new(ResourceCoordinator::new()),
            emit_sizes: Arc::new(move |updates| {
                live.lock().unwrap().extend(updates);
                notify();
            }),
            emit_status: Arc::new(move |status| {
                statuses.lock().unwrap().push(status);
                notify();
            }),
            walk_hooks: TestHooks {
                buffer_size: 4096,
                before_dir_open: Some(Arc::new(move |dir: &Path| {
                    opened.lock().unwrap().push(dir.to_path_buf());
                    notify();
                    blocker.wait(dir);
                })),
                device_of: None,
            },
            on_background_parked: Arc::new(move || {
                parked.fetch_add(1, Ordering::SeqCst);
                notify();
            }),
        };
        self.service = Some(SizeService::with_folder_size_index_for_tests(
            Duration::from_secs(1),
            options,
        ));
        wait_until("stream start", || {
            self.platform.stream_starts().len() > starts
        });
        self.service()
    }

    fn service(&self) -> &SizeService {
        self.service.as_ref().expect("started")
    }

    fn stop(&mut self) {
        self.service = None;
    }

    fn path(&self, relative: &str) -> PathBuf {
        if relative.is_empty() {
            self.root.clone()
        } else {
            self.root.join(relative)
        }
    }

    fn key(&self, relative: &str) -> String {
        self.path(relative).to_string_lossy().into_owned()
    }

    fn state(&self) -> IndexState {
        self.service().folder_size_index_status().state
    }

    fn wait_state(&self, state: IndexState) {
        wait_until(&format!("{state:?}"), || self.state() == state);
    }

    fn lanes(&self) -> LaneSnapshot {
        self.service()
            .index_lanes_for_tests()
            .expect("index attached")
    }

    fn opened(&self, relative: &str) -> bool {
        self.opened.lock().unwrap().contains(&self.path(relative))
    }

    fn request(&self, relatives: &[&str], mode: RequestMode) -> Arc<Mutex<Vec<SizeUpdate>>> {
        let paths = relatives
            .iter()
            .map(|relative| self.key(relative))
            .collect();
        self.request_paths(paths, mode)
    }

    fn request_paths(&self, paths: Vec<String>, mode: RequestMode) -> Arc<Mutex<Vec<SizeUpdate>>> {
        let updates = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&updates);
        self.service().request_paths_with_mode(
            paths,
            mode,
            Vec::new(),
            Arc::new(move |batch| {
                sink.lock().unwrap().extend(batch);
                notify();
            }),
        );
        updates
    }

    fn event(&self, relative: &str, flags: ChangeFlags) {
        let event = RawEvent {
            path: self.path(relative),
            id: 200,
            flags,
            history_done: false,
        };
        assert!(self.platform.emit(vec![event]), "stream running");
    }

    fn live_for(&self, relative: &str) -> Vec<SizeUpdate> {
        let key = self.key(relative);
        let live = self.live.lock().unwrap();
        live.iter()
            .filter(|update| update.path == key)
            .cloned()
            .collect()
    }
}

/// The last terminal update for `path` (waits for it).
fn terminal(updates: &Arc<Mutex<Vec<SizeUpdate>>>, path: &str) -> SizeUpdate {
    let mut found = None;
    wait_until(&format!("terminal update for {path}"), || {
        found = updates
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|update| update.path == path && update.state != SizeStateKind::Calculating)
            .cloned();
        found.is_some()
    });
    found.expect("terminal update")
}

fn fresh_total(path: &Path) -> u64 {
    subtree_total(
        path,
        &WalkOptions::new(Arc::new(AtomicBool::new(false)), None),
    )
    .expect("walk")
}

fn ready(index: &Harness, relative: &str) -> u64 {
    let key = index.key(relative);
    let update = terminal(&index.request(&[relative], RequestMode::Auto), &key);
    assert_eq!(update.state, SizeStateKind::Ready, "{relative}: {update:?}");
    assert_eq!(update.source, SizeSource::Index);
    update.size_bytes.expect("bytes")
}

#[test]
fn cold_scan_reports_progress_states_and_saves_the_pre_scan_event_id() {
    let mut index = Harness::new();
    index.platform.set_event_id(500);
    index.blocker.block(&index.path(""));
    index.start();
    wait_until("cold scan", || index.opened(""));
    // Events after the capture must not move the saved ID.
    index.platform.set_event_id(900);
    index.blocker.release(&index.path(""));
    index.wait_state(IndexState::UpToDate);

    let states: Vec<IndexState> = index
        .statuses
        .lock()
        .unwrap()
        .iter()
        .map(|s| s.state)
        .collect();
    assert_eq!(states.first(), Some(&IndexState::NotReady));
    let indexing = states
        .iter()
        .position(|s| *s == IndexState::Indexing)
        .expect("indexing");
    assert_eq!(states.last(), Some(&IndexState::UpToDate));
    assert!(indexing > 0);
    let status = index.service().folder_size_index_status();
    assert_eq!(status.backend, IndexBackend::MacIndex);
    assert!(status.is_available);
    assert_eq!(index.platform.stream_starts(), vec![None]);

    wait_until("snapshot", || persist::load(&index.snapshot).is_some());
    let saved = persist::load(&index.snapshot).expect("snapshot");
    assert_eq!(saved.meta.event_id, 500);
    assert_eq!(saved.meta.volume_uuid, "FAKE-VOLUME");
    assert_eq!(saved.meta.os_build, "FAKE-BUILD");
    assert!(saved.meta.last_full_scan_secs > 0);

    // The protected folder was never read; totals match a fresh walk without it.
    assert!(!index.opened("home/Documents"));
    let without = fresh_total(&index.root) - 1000;
    assert_eq!(ready(&index, ""), without);
    assert_eq!(ready(&index, "a"), fresh_total(&index.path("a")));
}

#[test]
fn indexed_requests_answer_without_walking_and_auto_skips_outside_paths() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    index.opened.lock().unwrap().clear();

    assert_eq!(ready(&index, "a/b"), 100);
    assert!(
        index.opened.lock().unwrap().is_empty(),
        "served from the index"
    );
    assert_eq!(
        index
            .service()
            .choose_backend_for_path(&index.path("a"), &[]),
        SizeBackend::Index
    );

    let outside = index.root.parent().unwrap().join("cache");
    fs::create_dir_all(&outside).unwrap();
    let key = outside.to_string_lossy().into_owned();
    let auto = terminal(
        &index.request_paths(vec![key.clone()], RequestMode::Auto),
        &key,
    );
    assert_eq!(auto.state, SizeStateKind::Unknown);
    assert!(!index.opened.lock().unwrap().contains(&outside));
    let manual = terminal(
        &index.request_paths(vec![key.clone()], RequestMode::Manual),
        &key,
    );
    assert_eq!(manual.state, SizeStateKind::Ready);
    assert_eq!(manual.source, SizeSource::Manual);
}

#[test]
fn a_boost_is_served_while_background_work_is_stuck() {
    let mut index = Harness::new();
    index.blocker.block(&index.path(""));
    index.start();
    wait_until("cold scan", || index.opened(""));

    let key = index.key("a");
    let updates = index.request(&["a"], RequestMode::Auto);
    let first = updates.lock().unwrap().first().cloned().expect("immediate");
    assert_eq!(first.state, SizeStateKind::Calculating);
    let answer = terminal(&updates, &key);
    assert_eq!(answer.size_bytes, Some(110));
    assert_eq!(
        index.state(),
        IndexState::Indexing,
        "background still blocked"
    );

    index.blocker.release(&index.path(""));
    index.wait_state(IndexState::UpToDate);
    assert_eq!(ready(&index, ""), fresh_total(&index.root) - 1000);
}

#[test]
fn one_boost_runs_at_a_time_newest_first() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    index.opened.lock().unwrap().clear();

    index.blocker.block(&index.path("a"));
    let first = index.request(&["a"], RequestMode::Refresh);
    wait_until("first boost", || index.opened("a"));
    let second = index.request(&["s"], RequestMode::Refresh);
    let third = index.request(&["home"], RequestMode::Refresh);
    let lanes = index.lanes();
    assert_eq!(lanes.boost_running, Some(index.key("a")));
    assert_eq!(
        lanes.boosts_queued,
        vec![index.key("home"), index.key("s")],
        "queued behind, newest first"
    );
    assert!(!index.opened("s") && !index.opened("home"));
    assert_eq!(
        index.service().cancel_many(&[index.key("s")]),
        1,
        "a queued boost can be cancelled"
    );
    let fourth = index.request(&["s"], RequestMode::Refresh);

    index.blocker.release(&index.path("a"));
    assert_eq!(terminal(&first, &index.key("a")).size_bytes, Some(110));
    assert_eq!(terminal(&fourth, &index.key("s")).size_bytes, Some(5));
    assert_eq!(terminal(&third, &index.key("home")).size_bytes, Some(7));
    assert!(second
        .lock()
        .unwrap()
        .iter()
        .all(|u| u.state == SizeStateKind::Calculating));
    let opened = index.opened.lock().unwrap().clone();
    let at = |path: PathBuf| opened.iter().position(|p| *p == path).expect("opened");
    assert!(
        at(index.path("s")) < at(index.path("home")),
        "newest first: {opened:?}"
    );
}

#[test]
fn a_boost_skips_indexed_subtrees_and_ignores_the_power_gate() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);

    // Low Power Mode holds background work; a new folder stays pending.
    assert!(index.platform.signal(PowerSignal::Conditions {
        low_power: true,
        thermal_pressure: false,
    }));
    fs::create_dir(index.path("a/new")).unwrap();
    fs::write(index.path("a/new/n.bin"), vec![0; 3]).unwrap();
    index.opened.lock().unwrap().clear();
    index.event("a", ChangeFlags::default());
    wait_until("re-read", || index.opened("a"));
    wait_until("paused", || index.state() == IndexState::Paused);

    let key = index.key("a");
    let answer = terminal(&index.request(&["a"], RequestMode::Auto), &key);
    assert_eq!(answer.size_bytes, Some(113));
    assert!(index.opened("a/new"));
    assert!(!index.opened("a/b"), "indexed subtree skipped");
}

#[test]
fn background_work_pauses_for_power_and_resumes() {
    let mut index = Harness::new();
    index.platform.set_initial_conditions(true, false);
    index.start();
    wait_until("paused", || index.state() == IndexState::Paused);
    let status = index.service().folder_size_index_status();
    assert_eq!(status.pause_reason, Some(PauseReason::LowPowerMode));
    let held = LaneSnapshot {
        background_queued: 1,
        background_running: false,
        boosts_queued: Vec::new(),
        boost_running: None,
        power_paused: true,
    };
    assert_eq!(index.lanes(), held, "the cold scan waits at the gate");
    assert!(index.opened.lock().unwrap().is_empty(), "nothing walked");

    index.platform.signal(PowerSignal::Conditions {
        low_power: false,
        thermal_pressure: true,
    });
    wait_until("thermal", || {
        index.service().folder_size_index_status().pause_reason == Some(PauseReason::Thermal)
    });
    index.platform.signal(PowerSignal::WillSleep);
    index.platform.signal(PowerSignal::Conditions {
        low_power: false,
        thermal_pressure: false,
    });
    wait_until("asleep", || index.state() == IndexState::Indexing);
    assert_eq!(index.lanes(), held, "sleep still holds the gate");
    assert!(index.opened.lock().unwrap().is_empty(), "asleep");

    index.platform.signal(PowerSignal::DidWake);
    index.wait_state(IndexState::UpToDate);
}

#[test]
fn a_running_background_walk_stops_at_the_next_folder_while_paused() {
    let mut index = Harness::new();
    index.blocker.block(&index.path("a"));
    index.start();
    wait_until("walk reaches a", || index.opened("a"));
    index.platform.signal(PowerSignal::Conditions {
        low_power: true,
        thermal_pressure: false,
    });
    wait_until("paused", || index.state() == IndexState::Paused);
    let before = index.opened.lock().unwrap().len();
    index.blocker.release(&index.path("a"));
    wait_until("walk parked", || index.parked.load(Ordering::SeqCst) > 0);
    assert!(index.lanes().background_running);
    assert_eq!(
        index.opened.lock().unwrap().len(),
        before,
        "no further folders"
    );

    index.platform.signal(PowerSignal::Conditions {
        low_power: false,
        thermal_pressure: false,
    });
    index.wait_state(IndexState::UpToDate);
    assert!(index.opened("a/b"));
}

#[test]
fn subscribed_folders_get_live_updates_and_others_do_not() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);

    let service = index.service();
    service.set_subscriptions("left".into(), Some(index.key("")), vec![index.key("a")]);
    wait_until("current totals on subscribe", || {
        !index.live_for("a").is_empty()
    });
    assert_eq!(index.live_for("a")[0].size_bytes, Some(110));

    fs::write(index.path("a/more.bin"), vec![0; 40]).unwrap();
    fs::write(index.path("s/more.bin"), vec![0; 40]).unwrap();
    index.event("a", ChangeFlags::default());
    index.event("s", ChangeFlags::default());
    wait_until("live update", || index.live_for("a").len() == 2);
    assert_eq!(index.live_for("a")[1].size_bytes, Some(150));
    assert!(index.live_for("s").is_empty(), "not subscribed");

    // Replacing the set drops the old paths.
    index
        .service()
        .set_subscriptions("left".into(), Some(index.key("")), vec![index.key("s")]);
    wait_until("s subscribed", || index.live_for("s").len() == 1);
    fs::write(index.path("a/again.bin"), vec![0; 1]).unwrap();
    index.event("a", ChangeFlags::default());
    fs::write(index.path("s/again.bin"), vec![0; 1]).unwrap();
    index.event("s", ChangeFlags::default());
    wait_until("s update", || index.live_for("s").len() == 2);
    assert_eq!(index.live_for("a").len(), 2, "a no longer subscribed");

    // Clearing the set (Trash, tab close) stops updates.
    index
        .service()
        .set_subscriptions("left".into(), None, Vec::new());
    fs::write(index.path("s/third.bin"), vec![0; 1]).unwrap();
    index.event("s", ChangeFlags::default());
    index
        .service()
        .set_subscriptions("right".into(), None, vec![index.key("a")]);
    wait_until("right pane", || index.live_for("a").len() == 3);
    assert_eq!(index.live_for("s").len(), 2);
}

#[test]
fn live_events_add_prune_and_rescan_and_follow_the_data_volume_prefix() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    index.service().set_subscriptions(
        "left".into(),
        None,
        vec![index.key("a/new"), index.key("s")],
    );

    // A new subscribed folder is boosted; the event path is Data-prefixed.
    fs::create_dir(index.path("a/new")).unwrap();
    fs::write(index.path("a/new/n.bin"), vec![0; 3]).unwrap();
    let prefixed = PathBuf::from(format!("{DATA_VOLUME_PREFIX}{}", index.key("a")));
    assert_eq!(normalize(&prefixed), index.path("a"));
    index.platform.emit(vec![RawEvent {
        path: prefixed,
        id: 300,
        flags: ChangeFlags::default(),
        history_done: false,
    }]);
    wait_until("new folder sized", || !index.live_for("a/new").is_empty());
    assert_eq!(ready(&index, "a"), 113);

    // A removed folder is pruned and subtracted.
    fs::remove_dir_all(index.path("a/b")).unwrap();
    index.event("a/b", ChangeFlags::default());
    index.event("a", ChangeFlags::default());
    // Re-reads run on the index thread in order; a flush is a barrier.
    index.service().flush_index();
    assert_eq!(ready(&index, "a"), 13, "pruned");

    // MustScanSubDirs rescans that subtree only.
    fs::write(index.path("s/deep.bin"), vec![0; 20]).unwrap();
    index.event(
        "s",
        ChangeFlags {
            must_scan_sub_dirs: true,
            ..ChangeFlags::default()
        },
    );
    wait_until("rescan", || {
        index.live_for("s").iter().any(|u| u.size_bytes == Some(25))
    });
    index.wait_state(IndexState::UpToDate);

    // Dropped events force a full rescan.
    index.event(
        "",
        ChangeFlags {
            kernel_dropped: true,
            ..ChangeFlags::default()
        },
    );
    wait_until("full rescan", || {
        index
            .statuses
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.state == IndexState::Indexing)
            .count()
            >= 2
    });
    index.wait_state(IndexState::UpToDate);
}

#[test]
fn unvisited_protected_folders_are_unknown_until_visited() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);

    let home = terminal(
        &index.request(&["home"], RequestMode::Auto),
        &index.key("home"),
    );
    assert_eq!(home.size_bytes, Some(7));
    assert!(home.is_lower_bound);
    assert_eq!(home.skipped_roots, Some(vec!["Documents".to_string()]));
    assert_eq!(home.skipped_more, None);

    for mode in [RequestMode::Auto, RequestMode::Manual] {
        let docs = index.request(&["home/Documents", "home/Documents/inner"], mode);
        assert_eq!(
            terminal(&docs, &index.key("home/Documents")).state,
            SizeStateKind::Unknown
        );
        assert_eq!(
            terminal(&docs, &index.key("home/Documents/inner")).state,
            SizeStateKind::Unknown
        );
    }
    index.event("home/Documents", ChangeFlags::default());
    index.service().flush_index();
    assert!(!index.opened("home/Documents"), "never read");

    // Visiting it (a pane's current path) marks it visited and scans it.
    index.service().set_subscriptions(
        "left".into(),
        Some(index.key("home/Documents/inner")),
        vec![index.key("home")],
    );
    let docs = terminal(
        &index.request(&["home/Documents"], RequestMode::Auto),
        &index.key("home/Documents"),
    );
    assert_eq!(docs.size_bytes, Some(1000));
    wait_until("home resolves", || {
        index
            .live_for("home")
            .iter()
            .any(|u| u.size_bytes == Some(1007) && !u.is_lower_bound)
    });
    index.service().flush_index();
    let saved = persist::load(&index.snapshot).expect("snapshot");
    assert_eq!(saved.meta.visited_roots, vec![index.path("home/Documents")]);

    // The visit survives a relaunch.
    index.stop();
    index.opened.lock().unwrap().clear();
    index.start();
    let docs = terminal(
        &index.request(&["home/Documents"], RequestMode::Auto),
        &index.key("home/Documents"),
    );
    assert_eq!(docs.size_bytes, Some(1000), "{docs:?}");
}

#[test]
fn full_disk_access_lifts_the_skip_list() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    assert!(!index.opened("home/Documents"));

    index.platform.set_full_disk_access(true);
    index.service().window_focus_changed(true);
    wait_until("documents scanned", || index.opened("home/Documents"));
    index.wait_state(IndexState::UpToDate);
    let home = terminal(
        &index.request(&["home"], RequestMode::Auto),
        &index.key("home"),
    );
    assert_eq!(home.size_bytes, Some(1007));
    assert!(!home.is_lower_bound);

    // A later launch without access drops the stale data again.
    index.stop();
    index.platform.set_full_disk_access(false);
    index.start();
    let home = terminal(
        &index.request(&["home"], RequestMode::Auto),
        &index.key("home"),
    );
    assert_eq!(home.size_bytes, Some(7));
    assert!(home.is_lower_bound);
}

#[test]
fn denied_folders_are_na_and_make_ancestors_lower_bounds() {
    let mut index = Harness::new();
    let locked = index.path("s/locked");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("x.bin"), vec![0; 9]).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    index.start();
    index.wait_state(IndexState::UpToDate);

    let denied = terminal(
        &index.request(&["s/locked"], RequestMode::Auto),
        &index.key("s/locked"),
    );
    assert_eq!(denied.state, SizeStateKind::Na);
    assert_eq!(denied.na_reason, Some(NaReason::Denied));
    let parent = terminal(&index.request(&["s"], RequestMode::Auto), &index.key("s"));
    assert_eq!(parent.size_bytes, Some(5));
    assert!(parent.is_lower_bound);
    assert_eq!(parent.skipped_roots, None);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn progress_is_throttled_by_the_injected_clock() {
    let mut index = Harness::new();
    let second = index.root.parent().unwrap().join("second");
    fs::create_dir_all(second.join("d")).unwrap();
    fs::write(second.join("d/f.bin"), vec![0; 4]).unwrap();
    index.roots.push(second.clone());
    index.blocker.block(&second);
    index.start();
    let scanned = |index: &Harness| -> Vec<u64> {
        index
            .statuses
            .lock()
            .unwrap()
            .iter()
            .filter_map(|status| status.items_scanned)
            .collect()
    };

    // The first root's records land while the clock stands still: no tick.
    wait_until("second root", || {
        index.opened.lock().unwrap().contains(&second)
    });
    index.service().flush_index();
    assert_eq!(scanned(&index), vec![0], "only the transition");

    // A second later, the next records report progress: folders plus files
    // (idx: 5 folders, 5 files; second: 2 folders, 1 file).
    *index.now.lock().unwrap() += Duration::from_secs(1);
    index.blocker.release(&second);
    index.wait_state(IndexState::UpToDate);
    assert_eq!(scanned(&index), vec![0, 13]);
}

#[test]
fn relaunch_replays_history_and_weekly_verification_rescans() {
    let mut index = Harness::new();
    index.platform.set_event_id(500);
    index.start();
    index.wait_state(IndexState::UpToDate);
    index.stop();

    // While closed: a file appears; the device moved on to ID 700.
    fs::write(index.path("a/b/offline.bin"), vec![0; 1000]).unwrap();
    index.platform.set_event_id(700);
    index.opened.lock().unwrap().clear();
    index.statuses.lock().unwrap().clear();
    index.start();
    wait_until("catch-up", || index.state() == IndexState::Updating);
    assert_eq!(ready(&index, "a/b"), 100, "old totals served at once");
    assert_eq!(index.platform.stream_starts().last(), Some(&Some(500)));
    index.platform.emit(vec![
        RawEvent {
            path: index.path("a/b"),
            id: 650,
            flags: ChangeFlags::default(),
            history_done: false,
        },
        RawEvent {
            path: index.path(""),
            id: 700,
            flags: ChangeFlags::default(),
            history_done: true,
        },
    ]);
    index.wait_state(IndexState::UpToDate);
    assert_eq!(ready(&index, "a/b"), 1100);
    assert_eq!(
        index.opened.lock().unwrap().clone(),
        vec![index.path("a/b")],
        "no rescan"
    );
    let saved = persist::load(&index.snapshot).expect("snapshot");
    assert_eq!(saved.meta.event_id, 700);

    // Eight days later, focus triggers the weekly full rescan.
    *index.now.lock().unwrap() += Duration::from_secs(8 * 24 * 60 * 60);
    index.service().window_focus_changed(true);
    wait_until("weekly rescan", || index.opened(""));
    index.wait_state(IndexState::UpToDate);
}

#[test]
fn replay_fallbacks_force_a_full_rescan() {
    let mut index = Harness::new();
    index.platform.set_event_id(500);
    index.start();
    index.wait_state(IndexState::UpToDate);
    index.stop();

    // Coalesced history during replay.
    index.platform.set_event_id(600);
    index.opened.lock().unwrap().clear();
    index.start();
    index.platform.emit(vec![
        RawEvent {
            path: index.path("a"),
            id: 550,
            flags: ChangeFlags {
                must_scan_sub_dirs: true,
                ..ChangeFlags::default()
            },
            history_done: false,
        },
        RawEvent {
            path: index.path(""),
            id: 600,
            flags: ChangeFlags::default(),
            history_done: true,
        },
    ]);
    wait_until("rescan", || index.opened(""));
    index.wait_state(IndexState::UpToDate);
    index.stop();

    // A macOS update (another build) rescans and stores the new build.
    index.platform.set_os_build(Some("NEXT-BUILD"));
    index.start();
    index.wait_state(IndexState::UpToDate);
    assert_eq!(index.platform.stream_starts().last(), Some(&None));
    index.service().flush_index();
    let saved = persist::load(&index.snapshot).expect("snapshot");
    assert_eq!(saved.meta.os_build, "NEXT-BUILD");
    index.stop();

    // Another volume, then an event ID behind the saved one.
    index.platform.set_volume_uuid(Some("OTHER"));
    index.start();
    index.wait_state(IndexState::UpToDate);
    assert_eq!(index.platform.stream_starts().last(), Some(&None));
    index.stop();
    index.platform.set_event_id(10);
    index.start();
    index.wait_state(IndexState::UpToDate);
    assert_eq!(index.platform.stream_starts().last(), Some(&None));
    index.stop();

    // A corrupt snapshot is a fresh start, never a crash.
    fs::write(&index.snapshot, b"garbage").unwrap();
    index.start();
    index.wait_state(IndexState::UpToDate);
    assert_eq!(ready(&index, "a"), 110);
}

#[test]
fn blur_writes_the_snapshot_only_when_changed() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    let written = fs::metadata(&index.snapshot).unwrap().modified().unwrap();

    index.service().window_focus_changed(false);
    index.service().flush_index();
    assert_eq!(
        fs::metadata(&index.snapshot).unwrap().modified().unwrap(),
        written
    );

    fs::write(index.path("s/new.bin"), vec![0; 2]).unwrap();
    index.platform.emit(vec![RawEvent {
        path: index.path("s"),
        id: 4242,
        flags: ChangeFlags::default(),
        history_done: false,
    }]);
    index.service().window_focus_changed(false);
    index.service().flush_index();
    let saved = persist::load(&index.snapshot).expect("snapshot");
    assert_eq!(saved.meta.event_id, 4242);
    let s = saved.tree.total(&index.path("s")).expect("s");
    assert_eq!(s.bytes, 7);
}

#[test]
fn a_failed_stream_makes_the_index_unavailable_and_routes_manually() {
    let mut index = Harness::new();
    index.platform.fail_stream();
    index.start();
    wait_until("unavailable", || index.state() == IndexState::Unavailable);
    let status = index.service().folder_size_index_status();
    assert!(!status.is_available);
    assert_eq!(status.backend, IndexBackend::MacIndex);
    assert_eq!(
        index
            .service()
            .choose_backend_for_path(&index.path("a"), &[]),
        SizeBackend::Manual
    );
    let key = index.key("a");
    let answer = terminal(&index.request(&["a"], RequestMode::Manual), &key);
    assert_eq!(answer.source, SizeSource::Manual);
    index.service().flush_index();
    assert!(!index.snapshot.exists());
}

#[test]
fn missing_and_failed_walks_report_errors() {
    let mut index = Harness::new();
    index.start();
    index.wait_state(IndexState::UpToDate);
    let key = index.key("never/there");
    let missing = terminal(&index.request(&["never/there"], RequestMode::Auto), &key);
    assert_eq!(missing.state, SizeStateKind::Error);

    // A cancelled boost answers nothing.
    index.blocker.block(&index.path("a"));
    let cancelled = index.request(&["a"], RequestMode::Refresh);
    wait_until("boost running", || index.opened("a"));
    assert_eq!(index.service().cancel_many(&[index.key("a")]), 1);
    assert!(index.service().cancel(&index.key("unrelated")).eq(&false));
    index.blocker.release(&index.path("a"));
    let after = index.request(&["s"], RequestMode::Refresh);
    terminal(&after, &index.key("s"));
    assert!(cancelled
        .lock()
        .unwrap()
        .iter()
        .all(|u| u.state == SizeStateKind::Calculating));
}

#[test]
fn excluded_paths_are_never_indexed_walked_or_watched() {
    let mut index = Harness::new();
    index.excluded = vec![index.path("s")];
    index.start();
    index.wait_state(IndexState::UpToDate);
    assert!(!index.opened("s"));
    assert_eq!(ready(&index, ""), fresh_total(&index.root) - 1000 - 5);
    assert_eq!(
        index
            .service()
            .choose_backend_for_path(&index.path("s"), &[]),
        SizeBackend::Manual
    );
    let auto = terminal(&index.request(&["s"], RequestMode::Auto), &index.key("s"));
    assert_eq!(auto.state, SizeStateKind::Unknown);

    index.event("s", ChangeFlags::default());
    index.service().flush_index();
    assert!(!index.opened("s"), "events inside an exclusion are dropped");
}

#[test]
fn symlinked_folders_are_sized_as_their_resolved_target() {
    let mut index = Harness::new();
    let outside = index.root.parent().unwrap().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("o.bin"), vec![0; 9]).unwrap();
    for (link, target) in [
        ("link-a", index.path("a")),
        ("link-docs", index.path("home/Documents")),
        ("link-out", outside.clone()),
        ("link-file", index.path("top.bin")),
        ("link-gone", index.path("nowhere")),
        ("loop-1", index.path("loop-2")),
        ("loop-2", index.path("loop-1")),
    ] {
        symlink(target, index.path(link)).unwrap();
    }
    index.start();
    index.wait_state(IndexState::UpToDate);
    // Walks never follow links, so the root counts `a` once.
    assert_eq!(ready(&index, ""), fresh_total(&index.root) - 1000);
    index.opened.lock().unwrap().clear();

    let check = |relative: &str, mode, state, source, bytes| {
        let update = terminal(&index.request(&[relative], mode), &index.key(relative));
        assert_eq!(
            (update.state, update.source, update.size_bytes),
            (state, source, bytes),
            "{relative} {mode:?}"
        );
    };
    use RequestMode::{Auto, Manual};
    use SizeSource::{Index, Manual as Walked};
    use SizeStateKind::{Error, Ready, Unknown};
    check("link-a", Auto, Ready, Index, Some(110));
    check("link-a/b", Auto, Ready, Index, Some(100));
    assert!(
        index.opened.lock().unwrap().is_empty(),
        "served from the index"
    );
    // A link into an unvisited protected folder is never read.
    check("link-docs", Auto, Unknown, Index, None);
    check("link-docs", Manual, Unknown, Index, None);
    assert!(!index.opened("home/Documents"));
    check("link-out", Auto, Unknown, Walked, None);
    check("link-out", Manual, Ready, Walked, Some(9));
    check("link-file", Auto, Unknown, Walked, None);
    check("link-file", Manual, Ready, Walked, Some(0));
    check("link-gone", Auto, Unknown, Walked, None);
    check("link-gone", Manual, Error, Walked, None);
    check("loop-1", Manual, Error, Walked, None);

    // A subscribed link row follows its target's live total; paths outside
    // the index are never resolved.
    index.service().set_subscriptions(
        "left".into(),
        Some(index.key("")),
        vec![index.key("link-a"), outside.to_string_lossy().into_owned()],
    );
    wait_until("link subscribed", || !index.live_for("link-a").is_empty());
    fs::write(index.path("a/more.bin"), vec![0; 40]).unwrap();
    index.event("a", ChangeFlags::default());
    wait_until("link live update", || {
        index
            .live_for("link-a")
            .iter()
            .any(|u| u.size_bytes == Some(150))
    });
}

#[test]
fn platform_helpers_list_stream_exclusions_and_protected_folders() {
    let home = Path::new("/Users/me");
    assert_eq!(protected_roots(home).len(), 7);
    assert!(protected_roots(home).contains(&home.join("Library/Mobile Documents")));
    let excluded = stream_exclusions(home);
    assert!(excluded.len() <= 8, "FSEvents takes at most eight");
    for path in ["/Volumes", "/dev"] {
        assert!(excluded.contains(&PathBuf::from(path)), "{path}");
    }
    // Data-volume events may arrive under `/System/Volumes/Data`.
    assert!(!excluded.iter().any(|path| path.starts_with("/System")));
    assert!(excluded.contains(&home.join("Library/Caches")));
    assert_eq!(
        normalize(Path::new(DATA_VOLUME_PREFIX)),
        PathBuf::from(DATA_VOLUME_PREFIX)
    );
    assert_eq!(
        normalize(Path::new("/Users/me")),
        PathBuf::from("/Users/me")
    );
}
