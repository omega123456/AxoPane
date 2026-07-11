//! The shared volume registry: the single long-lived owner of platform
//! volume discovery, native mount/unmount registrations, and cached
//! mounted-volume identity/capability facts.
//!
//! Prior to this module, every `list_volumes` IPC call re-enumerated the
//! platform and `VolumeMonitorService` separately owned the native
//! Disk Arbitration session (macOS) / shell change-notification window
//! (Windows) purely to re-emit `VOLUMES_CHANGED`. `VolumeRegistry` folds
//! both responsibilities into one revisioned snapshot: native events and the
//! window-focus reconcile safety net both call [`VolumeRegistry::refresh`],
//! which re-enumerates once, builds a new [`RegistrySnapshot`] with stable
//! resource identities, and publishes it only if the snapshot actually
//! changed. Readers (IPC commands, and future resource-coordinator/transfer
//! consumers) take an immutable clone of the current snapshot via
//! [`VolumeRegistry::snapshot`] — no platform work happens on that path.

use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::VolumeInfo;

#[cfg(not(feature = "test-utils"))]
use tauri::{AppHandle, Emitter};

/// Bounds how long [`VolumeRegistry::refresh`] waits for a fresh snapshot to
/// publish before returning the (possibly stale) previous snapshot to the
/// caller. This does **not** cancel the underlying native discovery call —
/// platform volume enumeration (`GetLogicalDrives`/`WNetEnumResourceW` on
/// Windows, `getfsstat`/`Disks::new_with_refreshed_list` on macOS) is not
/// cancellable. A refresh that misses the deadline keeps running on its
/// worker thread and, if it completes, publishes its snapshot as long as no
/// newer refresh generation has started; a generation that loses the race is
/// simply discarded. The previous snapshot is never redescribed as freshly
/// validated just because a caller stopped waiting for it.
pub const REFRESH_DEADLINE: Duration = Duration::from_millis(1_500);

/// Stable identity kind for a volume resource key. This tells consumers how
/// strong the identity guarantee is: `Fallback` roots are an explicit,
/// weaker guarantee (a normalized path) rather than a platform-durable
/// identifier, and must never be presented as equivalent to a GUID/UUID
/// identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VolumeResourceKind {
    /// Windows local/fixed/removable volume identified by its normalized
    /// volume GUID path (`\\?\Volume{guid}\`).
    WindowsVolumeGuid,
    /// Windows UNC network share identified by normalized `server\share`
    /// identity (after collapsing `\\?\UNC\...` and ordinary UNC aliases).
    UncShare,
    /// macOS volume identified by filesystem/APFS volume UUID.
    MacosVolumeUuid,
    /// macOS volume identified by filesystem kind + mount alias when no
    /// UUID is available (e.g. some network filesystems).
    MacosFilesystemIdentity,
    /// Explicit fallback: a normalized mount root used as the resource key
    /// because no stronger platform identity could be resolved. Never
    /// presented as a GUID/UUID-equivalent identity.
    FallbackRoot,
}

/// Whether a previously discovered resource is currently reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VolumeAvailability {
    Available,
    Disconnected,
    Unknown,
}

/// Transfer capability facts a volume record carries so later phases
/// (native transfer adapters, resource admission) do not need to
/// re-probe the platform to answer "can this destination accept a native
/// copy/rename". Phase 1 only populates the facts derivable from ordinary
/// volume enumeration; deeper capability probing is a later-phase concern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferCapabilityFacts {
    /// Same-volume rename is generally available on local, non-read-only
    /// filesystems. Network shares can usually rename too, but cross-share
    /// renames are not same-volume; this flag describes the *volume*, not a
    /// specific transfer.
    pub supports_rename: bool,
    /// Whether the filesystem kind is known to support sparse-file-aware
    /// copy semantics. Conservative default `false` until a real capability
    /// probe (later phases) confirms otherwise.
    pub sparse_aware: bool,
}

impl Default for TransferCapabilityFacts {
    fn default() -> Self {
        Self {
            supports_rename: true,
            sparse_aware: false,
        }
    }
}

/// One volume's identity and capability facts as owned by the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeRecord {
    /// Stable resource key. Scheduling/admission and transfer adapters use
    /// this instead of a raw path so that mount-path churn (e.g. a UNC path
    /// resolved two different ways) does not fragment resource identity.
    pub resource_key: String,
    pub kind: VolumeResourceKind,
    /// Primary mount/display path, in the platform's normal display form
    /// (e.g. `C:\`, `\\server\share`, `/Volumes/Data`).
    pub primary_path: String,
    /// Additional normalized path aliases that resolve to this same
    /// resource (e.g. a `\\?\UNC\server\share` form alongside `\\server\share`).
    pub aliases: Vec<String>,
    /// Filesystem kind as reported by the platform (e.g. `NTFS`, `apfs`,
    /// `smbfs`); empty string when unknown.
    pub filesystem_kind: String,
    pub is_network: bool,
    pub is_removable: bool,
    pub is_read_only: bool,
    pub availability: VolumeAvailability,
    /// Milliseconds since `UNIX_EPOCH` of the last successful validation
    /// (i.e. the last refresh in which this resource was confirmed
    /// present). Serialized as a plain integer so the frontend need not
    /// special-case a Rust duration type.
    pub last_validated_at_ms: u64,
    pub transfer_capabilities: TransferCapabilityFacts,
    /// Legacy/display facts preserved for the existing `VolumeInfo` IPC
    /// contract and UI (label, capacity). Kept alongside identity facts
    /// rather than folded into `VolumeInfo` directly so callers that only
    /// need identity are not forced to carry space/label churn.
    pub label: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

impl VolumeRecord {
    fn to_volume_info(&self) -> VolumeInfo {
        VolumeInfo {
            mount_root: self.primary_path.clone(),
            label: self.label.clone(),
            total_bytes: self.total_bytes,
            free_bytes: self.free_bytes,
            is_network: self.is_network,
            is_removable: self.is_removable,
        }
    }
}

/// An immutable, revisioned view of the registry's current volume
/// inventory. Cloning is cheap relative to re-enumerating the platform: it
/// is the unit consumers (IPC commands, later phases' resource coordinator)
/// read without triggering any native work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySnapshot {
    /// Monotonically increasing revision. Bumped only when the published
    /// volume list actually changes (add/remove/reidentify), mirroring the
    /// existing `volume_inventory_changed` semantics so `VOLUMES_CHANGED`
    /// emission behavior is preserved.
    pub revision: u64,
    /// Milliseconds since `UNIX_EPOCH` when this snapshot was captured.
    pub captured_at_ms: u64,
    /// Set when the snapshot is known to be stale (e.g. a refresh could not
    /// complete before the deadline, or discovery failed) but is still the
    /// best available data. `None` means the snapshot is believed current.
    pub stale_since_ms: Option<u64>,
    /// Whether a refresh is currently in flight on a worker thread. A
    /// caller waiting on [`VolumeRegistry::refresh`] that hits the deadline
    /// while this is `true` knows a fresher snapshot may still land later.
    pub refresh_in_flight: bool,
    pub volumes: Vec<VolumeRecord>,
}

impl RegistrySnapshot {
    fn empty(now_ms: u64) -> Self {
        Self {
            revision: 0,
            captured_at_ms: now_ms,
            stale_since_ms: None,
            refresh_in_flight: false,
            volumes: Vec::new(),
        }
    }

    /// Read-only projection to the legacy `VolumeInfo` list the frontend and
    /// existing IPC contract expect. Order matches `self.volumes` (already
    /// platform-sorted by the enumeration layer).
    pub fn to_volume_infos(&self) -> Vec<VolumeInfo> {
        self.volumes
            .iter()
            .map(VolumeRecord::to_volume_info)
            .collect()
    }

    /// Resolves `path` to the volume record that owns it, using exact
    /// normalized-alias matching first, then longest mount-root
    /// containment, then (only as a compatibility fallback matching the
    /// repository's existing Windows/macOS behavior) case-insensitive
    /// matching. Returns `None` when no record's mount root is a prefix of
    /// `path` under any of those rules — callers should treat that as an
    /// unknown/fallback resource, not necessarily an error.
    pub fn resolve(&self, path: &str) -> Option<&VolumeRecord> {
        // Exact alias/primary match first: the strongest guarantee, no
        // normalization ambiguity possible.
        if let Some(exact) = self.volumes.iter().find(|volume| {
            volume.primary_path == path || volume.aliases.iter().any(|alias| alias == path)
        }) {
            return Some(exact);
        }

        // Longest mount-root containment: prefer the most specific mount
        // (e.g. a UNC share mounted under a shorter root) over a shorter
        // ancestor.
        if let Some(contained) = self
            .volumes
            .iter()
            .filter(|volume| {
                path_is_within_root(path, &volume.primary_path)
                    || volume
                        .aliases
                        .iter()
                        .any(|alias| path_is_within_root(path, alias))
            })
            .max_by_key(|volume| volume.primary_path.len())
        {
            return Some(contained);
        }

        // Case-insensitive fallback only, matching the existing
        // `path_is_network`/mount-matching compatibility behavior for
        // Windows drive letters and macOS mount aliasing.
        let path_lower = path.to_ascii_lowercase();
        self.volumes
            .iter()
            .filter(|volume| {
                path_is_within_root(&path_lower, &volume.primary_path.to_ascii_lowercase())
                    || volume
                        .aliases
                        .iter()
                        .any(|alias| path_is_within_root(&path_lower, &alias.to_ascii_lowercase()))
            })
            .max_by_key(|volume| volume.primary_path.len())
    }
}

fn path_is_within_root(path: &str, root: &str) -> bool {
    if path == root {
        return true;
    }
    let Some(remainder) = path.strip_prefix(root) else {
        return false;
    };
    remainder.starts_with('\\') || remainder.starts_with('/')
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Platform-specific native registration session owned by the registry. In
/// production this holds the macOS Disk Arbitration session or the Windows
/// shell-notification watcher thread; both are constructed once and live
/// for the process lifetime instead of being recreated per `list_volumes`
/// call. Under `test-utils` no native registration exists.
#[cfg(not(feature = "test-utils"))]
#[allow(dead_code)] // Held only for its `Drop` impl (unregisters native callbacks); never read.
enum NativeSession {
    #[cfg(target_os = "macos")]
    Macos(super::disk_arbitration::Session),
    #[cfg(windows)]
    Windows(super::windows_watch::Watcher),
    None,
}

struct RegistryInner {
    snapshot: RegistrySnapshot,
    #[cfg(not(feature = "test-utils"))]
    app: Option<AppHandle>,
    #[cfg(not(feature = "test-utils"))]
    native: Option<NativeSession>,
}

impl Default for RegistryInner {
    fn default() -> Self {
        Self {
            snapshot: RegistrySnapshot::empty(now_ms()),
            #[cfg(not(feature = "test-utils"))]
            app: None,
            #[cfg(not(feature = "test-utils"))]
            native: None,
        }
    }
}

/// The application-owned, long-lived volume registry. Managed as Tauri
/// state (like `WatchService`/`ItemCountService`), constructed once at
/// startup, and never recreated for the life of the process.
pub struct VolumeRegistry {
    inner: Mutex<RegistryInner>,
    /// Refresh generation counter. Each call to `refresh` claims the next
    /// generation before doing any platform work; a refresh only publishes
    /// its result if its generation is still the latest when the platform
    /// call returns, so a superseded (but still running) native discovery
    /// cannot clobber a newer snapshot.
    generation: AtomicU64,
    #[cfg(feature = "test-utils")]
    fake: Mutex<Vec<VolumeInfo>>,
}

impl Default for VolumeRegistry {
    fn default() -> Self {
        Self {
            inner: Mutex::new(RegistryInner::default()),
            generation: AtomicU64::new(0),
            #[cfg(feature = "test-utils")]
            fake: Mutex::new(super::list_volumes()),
        }
    }
}

impl VolumeRegistry {
    /// Starts the registry: performs the first enumeration, builds the
    /// initial snapshot, and — outside `test-utils` — registers the
    /// long-lived native mount/unmount session that will call
    /// [`VolumeRegistry::refresh`] on future OS events. Idempotent: calling
    /// this more than once only re-registers native events if the first
    /// registration failed.
    #[cfg(not(feature = "test-utils"))]
    pub fn start(self: &Arc<Self>, app: AppHandle) {
        {
            let mut inner = self.inner.lock().expect("volume registry inner lock");
            if inner.app.is_some() {
                return;
            }
            inner.app = Some(app.clone());
        }

        // Build the first snapshot synchronously so `list_volumes`/`snapshot`
        // callers immediately have real data without waiting on a
        // background refresh.
        self.publish_if_changed(build_snapshot(0));

        let registration = start_native_session(self);
        let mut inner = self.inner.lock().expect("volume registry inner lock");
        match registration {
            Ok(session) => inner.native = Some(session),
            Err(error) => log::error!("failed to start volume registry native session: {error}"),
        }
    }

    #[cfg(feature = "test-utils")]
    pub fn start(self: &Arc<Self>, _app: ()) {
        // No native registration under test-utils: tests must never touch
        // real Disk Arbitration or Windows shell notifications. Seed the
        // first snapshot from the fake inventory so `snapshot()` returns
        // real data without requiring every test to call `refresh` first.
        self.publish_if_changed(self.build_fake_snapshot(0));
    }

    /// Returns the current immutable snapshot without doing any platform
    /// work. This is the read path every IPC command and future consumer
    /// should use; it never re-enumerates or re-registers anything.
    pub fn snapshot(&self) -> RegistrySnapshot {
        self.inner
            .lock()
            .expect("volume registry inner lock")
            .snapshot
            .clone()
    }

    /// Re-enumerates the platform (or, under `test-utils`, reads the fake
    /// inventory) and publishes a new snapshot only if the inventory
    /// changed, bumping the revision. Bounded by [`REFRESH_DEADLINE`]: if
    /// the platform call does not return before the deadline, the previous
    /// snapshot is returned to the caller (marked stale) while the
    /// in-flight discovery keeps running in the background and will publish
    /// on completion if it is still the latest generation.
    #[cfg(not(feature = "test-utils"))]
    pub fn refresh(self: &Arc<Self>) -> RegistrySnapshot {
        let generation = self.generation.fetch_add(1, AtomicOrdering::SeqCst) + 1;
        self.mark_refreshing(true);

        // The spawned thread — not this call — owns publishing and clearing
        // `refresh_in_flight` for this generation. Platform enumeration is
        // not cancellable, so if this call times out below, the thread keeps
        // running well after `rx`/`tx` are gone; it must still be able to
        // publish its result and settle `refresh_in_flight` on its own,
        // rather than depending on a receiver that may already be dropped.
        let this = Arc::clone(self);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let snapshot = build_snapshot(generation);
            let is_latest = generation == this.generation.load(AtomicOrdering::SeqCst);
            if is_latest {
                this.publish_if_changed(snapshot);
                this.mark_refreshing(false);
            }
            // Best-effort: only observed if the caller is still waiting
            // within the deadline. A closed receiver (caller already timed
            // out) is expected and not an error — the publish above already
            // happened regardless.
            let _ = tx.send(());
        });

        match rx.recv_timeout(REFRESH_DEADLINE) {
            Ok(()) => {
                // The spawned thread already published (if it was still the
                // latest generation) and cleared `refresh_in_flight` itself.
                self.snapshot()
            }
            Err(_) => {
                // Deadline missed. Do not cancel and do not touch
                // `refresh_in_flight` here — the still-running thread above
                // owns clearing it once its own result lands. Mark the
                // existing snapshot stale so callers know it was not
                // refreshed within the deadline.
                self.mark_stale();
                self.snapshot()
            }
        }
    }

    #[cfg(feature = "test-utils")]
    pub fn refresh(self: &Arc<Self>) -> RegistrySnapshot {
        let generation = self.generation.fetch_add(1, AtomicOrdering::SeqCst) + 1;
        self.mark_refreshing(true);
        self.publish_if_changed(self.build_fake_snapshot(generation));
        self.mark_refreshing(false);
        self.snapshot()
    }

    /// Resolves `path` against the current snapshot. Convenience wrapper
    /// around `snapshot().resolve(path)` for callers that only need one
    /// lookup and do not want to hold onto the snapshot.
    pub fn resolve_resource_key(&self, path: &str) -> Option<String> {
        self.snapshot()
            .resolve(path)
            .map(|volume| volume.resource_key.clone())
    }

    fn mark_refreshing(&self, refreshing: bool) {
        let mut inner = self.inner.lock().expect("volume registry inner lock");
        inner.snapshot.refresh_in_flight = refreshing;
    }

    #[cfg(not(feature = "test-utils"))]
    fn mark_stale(&self) {
        let mut inner = self.inner.lock().expect("volume registry inner lock");
        if inner.snapshot.stale_since_ms.is_none() {
            inner.snapshot.stale_since_ms = Some(now_ms());
        }
    }

    /// Publishes `candidate` as the current snapshot only if its volume
    /// inventory differs from what is currently published (using the same
    /// identity-and-facts equality `volume_inventory_changed` used, now
    /// generalized to `VolumeRecord`). Preserves the revision counter
    /// semantics: unchanged inventories (e.g. free-space-only churn is
    /// already excluded by `VolumeRecord` not carrying free-space in its
    /// equality-relevant identity fields beyond what `VolumeInfo` tracked)
    /// do not bump the revision or emit an event. On a real change, emits
    /// `VOLUMES_CHANGED` exactly as `VolumeMonitorState::emit_if_changed`
    /// used to.
    fn publish_if_changed(&self, mut candidate: RegistrySnapshot) {
        let mut inner = self.inner.lock().expect("volume registry inner lock");

        if !volumes_changed(&inner.snapshot.volumes, &candidate.volumes) {
            // Inventory identical: still clear staleness/in-flight markers
            // since we did just successfully observe the platform, but keep
            // the existing revision so consumers relying on revision
            // stability for cache validity are not disturbed.
            inner.snapshot.stale_since_ms = None;
            inner.snapshot.captured_at_ms = candidate.captured_at_ms;
            return;
        }

        candidate.revision = inner.snapshot.revision + 1;
        candidate.stale_since_ms = None;
        candidate.refresh_in_flight = inner.snapshot.refresh_in_flight;
        let previous = std::mem::replace(&mut inner.snapshot, candidate.clone());
        drop(previous);

        #[cfg(not(feature = "test-utils"))]
        if let Some(app) = inner.app.clone() {
            drop(inner);
            let _ = app.emit(
                crate::ipc::events::VOLUMES_CHANGED,
                crate::ipc::types::VolumesChangedEvent {
                    volumes: candidate.to_volume_infos(),
                },
            );
        }
    }

    #[cfg(feature = "test-utils")]
    fn build_fake_snapshot(&self, generation: u64) -> RegistrySnapshot {
        let volumes = self.fake.lock().expect("volume registry fake lock").clone();
        build_snapshot_from_infos(generation, &volumes)
    }

    /// Test-only seam: replaces the fake platform inventory `refresh` will
    /// observe next, simulating a native add/remove/change event without
    /// touching any real OS API. Exists only under `test-utils`.
    #[cfg(feature = "test-utils")]
    pub fn set_fake_inventory(&self, volumes: Vec<VolumeInfo>) {
        *self.fake.lock().expect("volume registry fake lock") = volumes;
    }
}

fn volumes_changed(previous: &[VolumeRecord], next: &[VolumeRecord]) -> bool {
    fn identity(
        volume: &VolumeRecord,
    ) -> (
        String,
        VolumeResourceKind,
        String,
        bool,
        bool,
        bool,
        VolumeAvailability,
    ) {
        (
            volume.resource_key.clone(),
            volume.kind,
            volume.label.clone(),
            volume.is_network,
            volume.is_removable,
            volume.is_read_only,
            volume.availability,
        )
    }

    let mut previous_identities: Vec<_> = previous.iter().map(identity).collect();
    let mut next_identities: Vec<_> = next.iter().map(identity).collect();
    previous_identities.sort();
    next_identities.sort();
    previous_identities != next_identities
}

#[cfg(not(feature = "test-utils"))]
fn build_snapshot(generation: u64) -> RegistrySnapshot {
    let volumes = super::list_volumes();
    build_snapshot_from_infos(generation, &volumes)
}

fn build_snapshot_from_infos(_generation: u64, volumes: &[VolumeInfo]) -> RegistrySnapshot {
    let now = now_ms();
    RegistrySnapshot {
        revision: 0, // overwritten by `publish_if_changed` on real change
        captured_at_ms: now,
        stale_since_ms: None,
        refresh_in_flight: false,
        volumes: volumes.iter().map(|info| to_record(info, now)).collect(),
    }
}

fn to_record(info: &VolumeInfo, now_ms: u64) -> VolumeRecord {
    let (kind, resource_key, aliases) = resolve_identity(info);
    VolumeRecord {
        resource_key,
        kind,
        primary_path: info.mount_root.clone(),
        aliases,
        filesystem_kind: String::new(),
        is_network: info.is_network,
        is_removable: info.is_removable,
        is_read_only: false,
        availability: VolumeAvailability::Available,
        last_validated_at_ms: now_ms,
        transfer_capabilities: TransferCapabilityFacts {
            supports_rename: true,
            sparse_aware: false,
        },
        label: info.label.clone(),
        total_bytes: info.total_bytes,
        free_bytes: info.free_bytes,
    }
}

/// Resolves the strongest available identity for `info`.
///
/// - Windows UNC paths (`\\server\share`, including the `\\?\UNC\server\share`
///   long-path form) normalize to a `UncShare` identity keyed on
///   lowercased `server\share`, with both the plain and `\\?\UNC\` forms
///   recorded as aliases so either resolves to the same record.
/// - Windows drive-letter roots use a `FallbackRoot` identity in Phase 1:
///   resolving the true volume GUID requires `GetVolumeNameForVolumeMountPointW`,
///   which is deferred to a later phase's deeper capability probing: it is
///   deliberately not invented here so the identity strength described in
///   `kind` stays honest. The Windows platform bullet in the plan expects a
///   GUID identity when it is confirmed; until that probe exists, the
///   fallback root is explicit rather than presented as GUID-backed.
/// - macOS mount roots use a `FallbackRoot` identity for the same reason:
///   resolving a true APFS volume UUID requires a `DiskArbitration`/`statfs`
///   probe not yet wired into ordinary enumeration.
///
/// This keeps the identity contract honest today (no fabricated GUID/UUID)
/// while establishing the exact shape (`kind` + `resource_key` + `aliases`)
/// later phases extend once the platform probes are added.
fn resolve_identity(info: &VolumeInfo) -> (VolumeResourceKind, String, Vec<String>) {
    let mount_root = info.mount_root.as_str();

    if let Some((server, share)) = parse_unc_share(mount_root) {
        let key = format!(
            "unc:{}\\{}",
            server.to_ascii_lowercase(),
            share.to_ascii_lowercase()
        );
        let plain_alias = format!("\\\\{server}\\{share}");
        let long_alias = format!("\\\\?\\UNC\\{server}\\{share}");
        let mut aliases = vec![plain_alias, long_alias];
        aliases.retain(|alias| alias != mount_root);
        return (VolumeResourceKind::UncShare, key, aliases);
    }

    let normalized_root = normalize_root_for_key(mount_root);
    (
        VolumeResourceKind::FallbackRoot,
        format!("fallback:{}", normalized_root.to_ascii_lowercase()),
        Vec::new(),
    )
}

/// Parses a UNC path (`\\server\share`, `\\server\share\sub\dir`, or the
/// `\\?\UNC\server\share` long-path form) into `(server, share)`. Returns
/// `None` for drive-letter roots and POSIX paths.
fn parse_unc_share(path: &str) -> Option<(String, String)> {
    let trimmed = path.trim();

    let after_prefix = trimmed
        .strip_prefix("\\\\?\\UNC\\")
        .or_else(|| trimmed.strip_prefix("\\\\"))?;

    let mut parts = after_prefix
        .split(['\\', '/'])
        .filter(|part| !part.is_empty());
    let server = parts.next()?;
    let share = parts.next()?;
    Some((server.to_string(), share.to_string()))
}

fn normalize_root_for_key(mount_root: &str) -> String {
    if mount_root == "/" {
        return mount_root.to_string();
    }
    mount_root.trim_end_matches(['\\', '/']).to_string()
}

#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
fn start_native_session(registry: &Arc<VolumeRegistry>) -> Result<NativeSession, String> {
    let bridge = Arc::new(RegistryReconcileBridge {
        registry: Arc::clone(registry),
    });
    super::disk_arbitration::Session::start_for_registry(bridge).map(NativeSession::Macos)
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn start_native_session(registry: &Arc<VolumeRegistry>) -> Result<NativeSession, String> {
    let bridge = Arc::new(RegistryReconcileBridge {
        registry: Arc::clone(registry),
    });
    super::windows_watch::Watcher::start_for_registry(bridge).map(NativeSession::Windows)
}

#[cfg(all(not(feature = "test-utils"), not(any(target_os = "macos", windows))))]
fn start_native_session(_registry: &Arc<VolumeRegistry>) -> Result<NativeSession, String> {
    Ok(NativeSession::None)
}

/// Adapter implementing the native-callback-facing trait the platform
/// session modules invoke on mount/unmount/change events. Kept as a
/// separate small type (rather than handing the native modules an
/// `Arc<VolumeRegistry>` directly) so the platform modules depend only on
/// this narrow interface, not the whole registry's public API surface.
#[cfg(not(feature = "test-utils"))]
pub(super) struct RegistryReconcileBridge {
    registry: Arc<VolumeRegistry>,
}

#[cfg(not(feature = "test-utils"))]
impl RegistryReconcileBridge {
    pub(super) fn reconcile(&self) {
        let _ = self.registry.refresh();
    }
}

/// Records the instant a refresh was requested, for future debugging/log
/// context. Not currently surfaced to consumers; kept private and small.
#[allow(dead_code)]
fn elapsed_since(start: Instant) -> Duration {
    start.elapsed()
}
