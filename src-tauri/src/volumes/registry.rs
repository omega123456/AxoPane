use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::VolumeInfo;
use crate::path_text::{strip_unc_prefix, PATH_SEPARATORS, POSIX_ROOT, VERBATIM_UNC_PREFIX};

#[cfg(not(feature = "test-utils"))]
use tauri::{AppHandle, Emitter};

pub const REFRESH_DEADLINE: Duration = Duration::from_millis(1_500);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VolumeResourceKind {
    WindowsVolumeGuid,
    UncShare,
    MacosVolumeUuid,
    MacosFilesystemIdentity,
    FallbackRoot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VolumeAvailability {
    Available,
    Disconnected,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferCapabilityFacts {
    pub supports_rename: bool,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeRecord {
    pub resource_key: String,
    pub kind: VolumeResourceKind,
    pub primary_path: String,
    pub aliases: Vec<String>,
    pub filesystem_kind: String,
    pub is_network: bool,
    pub is_removable: bool,
    pub is_read_only: bool,
    pub availability: VolumeAvailability,
    pub last_validated_at_ms: u64,
    pub transfer_capabilities: TransferCapabilityFacts,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySnapshot {
    pub revision: u64,
    pub captured_at_ms: u64,
    pub stale_since_ms: Option<u64>,
    pub refresh_in_flight: bool,
    pub volumes: Vec<VolumeRecord>,
}

impl RegistrySnapshot {
    pub fn empty(now_ms: u64) -> Self {
        Self {
            revision: 0,
            captured_at_ms: now_ms,
            stale_since_ms: None,
            refresh_in_flight: false,
            volumes: Vec::new(),
        }
    }

    pub fn with_refresh_in_flight(&self, refresh_in_flight: bool) -> Self {
        Self {
            refresh_in_flight,
            ..self.clone()
        }
    }

    pub fn marked_stale(&self, now_ms: u64) -> Self {
        Self {
            stale_since_ms: Some(self.stale_since_ms.unwrap_or(now_ms)),
            ..self.clone()
        }
    }

    pub fn reobserved(&self, captured_at_ms: u64) -> Self {
        Self {
            stale_since_ms: None,
            captured_at_ms,
            ..self.clone()
        }
    }

    pub fn superseded_by(&self, candidate: RegistrySnapshot) -> Self {
        Self {
            revision: self.revision + 1,
            stale_since_ms: None,
            refresh_in_flight: self.refresh_in_flight,
            ..candidate
        }
    }

    pub fn to_volume_infos(&self) -> Vec<VolumeInfo> {
        self.volumes
            .iter()
            .map(VolumeRecord::to_volume_info)
            .collect()
    }

    pub fn resolve(&self, path: &str) -> Option<&VolumeRecord> {
        if let Some(exact) = self.volumes.iter().find(|volume| {
            volume.primary_path == path || volume.aliases.iter().any(|alias| alias == path)
        }) {
            return Some(exact);
        }

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
    crate::epoch::now_since_epoch().as_millis() as u64
}

#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
type NativeSession = super::disk_arbitration::Session;

#[cfg(all(not(feature = "test-utils"), windows))]
type NativeSession = super::windows_watch::Watcher;

#[cfg(all(not(feature = "test-utils"), not(any(target_os = "macos", windows))))]
type NativeSession = ();

struct RegistryInner {
    snapshot: RegistrySnapshot,
    #[cfg(not(feature = "test-utils"))]
    app: Option<AppHandle>,
    #[cfg(not(feature = "test-utils"))]
    _native_session: Option<NativeSession>,
}

impl Default for RegistryInner {
    fn default() -> Self {
        Self {
            snapshot: RegistrySnapshot::empty(now_ms()),
            #[cfg(not(feature = "test-utils"))]
            app: None,
            #[cfg(not(feature = "test-utils"))]
            _native_session: None,
        }
    }
}

pub struct VolumeRegistry {
    inner: Mutex<RegistryInner>,
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
    #[cfg(not(feature = "test-utils"))]
    fn attach_app(&self, app: &AppHandle) -> bool {
        let mut inner = self.inner.lock().expect("volume registry inner lock");
        if inner.app.is_some() {
            return false;
        }
        inner.app = Some(app.clone());
        true
    }

    #[cfg(not(feature = "test-utils"))]
    fn publish_refresh(&self, generation: u64) {
        let snapshot = build_snapshot(generation);
        let is_latest = generation == self.generation.load(AtomicOrdering::SeqCst);
        if !is_latest {
            return;
        }
        self.publish_if_changed(snapshot);
        self.mark_refreshing(false);
    }

    #[cfg(not(feature = "test-utils"))]
    pub fn start(self: &Arc<Self>, app: AppHandle) {
        let newly_started = self.attach_app(&app);
        if !newly_started {
            return;
        }

        self.publish_if_changed(build_snapshot(0));

        let registration = start_native_session(self);
        let mut inner = self.inner.lock().expect("volume registry inner lock");
        match registration {
            Ok(session) => inner._native_session = Some(session),
            Err(error) => log::error!("failed to start volume registry native session: {error}"),
        }
    }

    #[cfg(feature = "test-utils")]
    pub fn start(self: &Arc<Self>, _app: ()) {
        self.publish_if_changed(self.build_fake_snapshot(0));
    }

    pub fn snapshot(&self) -> RegistrySnapshot {
        self.inner
            .lock()
            .expect("volume registry inner lock")
            .snapshot
            .clone()
    }

    #[cfg(not(feature = "test-utils"))]
    pub fn refresh(self: &Arc<Self>) -> RegistrySnapshot {
        let generation = self.generation.fetch_add(1, AtomicOrdering::SeqCst) + 1;
        self.mark_refreshing(true);

        let this = Arc::clone(self);
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            this.publish_refresh(generation);
            let _ = sender.send(());
        });

        match receiver.recv_timeout(REFRESH_DEADLINE) {
            Ok(()) => self.snapshot(),
            Err(_) => {
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

    pub fn resolve_resource_key(&self, path: &str) -> Option<String> {
        self.snapshot()
            .resolve(path)
            .map(|volume| volume.resource_key.clone())
    }

    fn mark_refreshing(&self, refreshing: bool) {
        let mut inner = self.inner.lock().expect("volume registry inner lock");
        inner.snapshot = inner.snapshot.with_refresh_in_flight(refreshing);
    }

    #[cfg(not(feature = "test-utils"))]
    fn mark_stale(&self) {
        let mut inner = self.inner.lock().expect("volume registry inner lock");
        inner.snapshot = inner.snapshot.marked_stale(now_ms());
    }

    fn publish_if_changed(&self, candidate: RegistrySnapshot) {
        let mut inner = self.inner.lock().expect("volume registry inner lock");

        if !volumes_changed(&inner.snapshot.volumes, &candidate.volumes) {
            inner.snapshot = inner.snapshot.reobserved(candidate.captured_at_ms);
            return;
        }

        inner.snapshot = inner.snapshot.superseded_by(candidate);
        #[cfg(not(feature = "test-utils"))]
        let published = inner.snapshot.clone();

        #[cfg(not(feature = "test-utils"))]
        if let Some(app) = inner.app.clone() {
            drop(inner);
            let _ = app.emit(
                crate::ipc::events::VOLUMES_CHANGED,
                crate::ipc::types::VolumesChangedEvent {
                    volumes: published.to_volume_infos(),
                },
            );
        }
    }

    #[cfg(feature = "test-utils")]
    fn build_fake_snapshot(&self, generation: u64) -> RegistrySnapshot {
        let volumes = self.fake.lock().expect("volume registry fake lock").clone();
        build_snapshot_from_infos(generation, &volumes)
    }

    #[cfg(feature = "test-utils")]
    pub fn set_fake_inventory(&self, volumes: Vec<VolumeInfo>) {
        *self.fake.lock().expect("volume registry fake lock") = volumes;
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct VolumeChangeIdentity {
    resource_key: String,
    kind: VolumeResourceKind,
    label: String,
    is_network: bool,
    is_removable: bool,
    is_read_only: bool,
    availability: VolumeAvailability,
}

fn volumes_changed(previous: &[VolumeRecord], next: &[VolumeRecord]) -> bool {
    fn identity(volume: &VolumeRecord) -> VolumeChangeIdentity {
        VolumeChangeIdentity {
            resource_key: volume.resource_key.clone(),
            kind: volume.kind,
            label: volume.label.clone(),
            is_network: volume.is_network,
            is_removable: volume.is_removable,
            is_read_only: volume.is_read_only,
            availability: volume.availability,
        }
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
        revision: 0,
        captured_at_ms: now,
        stale_since_ms: None,
        refresh_in_flight: false,
        volumes: volumes.iter().map(|info| to_record(info, now)).collect(),
    }
}

fn to_record(info: &VolumeInfo, now_ms: u64) -> VolumeRecord {
    let identity = resolve_identity(info);
    VolumeRecord {
        resource_key: identity.resource_key,
        kind: identity.kind,
        primary_path: info.mount_root.clone(),
        aliases: identity.aliases,
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

fn resolve_identity(info: &VolumeInfo) -> VolumeIdentity {
    let mount_root = info.mount_root.as_str();

    if let Some(unc) = parse_unc_share(mount_root) {
        let UncShare { server, share } = unc;
        let resource_key = format!(
            "unc:{}\\{}",
            server.to_ascii_lowercase(),
            share.to_ascii_lowercase()
        );
        let plain_alias = format!("\\\\{server}\\{share}");
        let long_alias = format!("{VERBATIM_UNC_PREFIX}{server}\\{share}");
        let mut aliases = vec![plain_alias, long_alias];
        aliases.retain(|alias| alias != mount_root);
        return VolumeIdentity {
            kind: VolumeResourceKind::UncShare,
            resource_key,
            aliases,
        };
    }

    let normalized_root = normalize_root_for_key(mount_root);
    VolumeIdentity {
        kind: VolumeResourceKind::FallbackRoot,
        resource_key: format!("fallback:{}", normalized_root.to_ascii_lowercase()),
        aliases: Vec::new(),
    }
}

struct VolumeIdentity {
    kind: VolumeResourceKind,
    resource_key: String,
    aliases: Vec<String>,
}

struct UncShare {
    server: String,
    share: String,
}

fn parse_unc_share(path: &str) -> Option<UncShare> {
    let trimmed = path.trim();

    let after_prefix = strip_unc_prefix(trimmed)?;

    let mut parts = after_prefix
        .split(PATH_SEPARATORS)
        .filter(|part| !part.is_empty());
    let server = parts.next()?;
    let share = parts.next()?;
    Some(UncShare {
        server: server.to_string(),
        share: share.to_string(),
    })
}

fn normalize_root_for_key(mount_root: &str) -> String {
    if mount_root == POSIX_ROOT {
        return mount_root.to_string();
    }
    mount_root.trim_end_matches(PATH_SEPARATORS).to_string()
}

#[cfg(all(not(feature = "test-utils"), target_os = "macos"))]
fn start_native_session(registry: &Arc<VolumeRegistry>) -> Result<NativeSession, String> {
    let bridge = Arc::new(RegistryReconcileBridge {
        registry: Arc::clone(registry),
    });
    super::disk_arbitration::Session::start_for_registry(bridge)
}

#[cfg(all(not(feature = "test-utils"), windows))]
fn start_native_session(registry: &Arc<VolumeRegistry>) -> Result<NativeSession, String> {
    let bridge = Arc::new(RegistryReconcileBridge {
        registry: Arc::clone(registry),
    });
    super::windows_watch::Watcher::start_for_registry(bridge)
}

#[cfg(all(not(feature = "test-utils"), not(any(target_os = "macos", windows))))]
fn start_native_session(_registry: &Arc<VolumeRegistry>) -> Result<NativeSession, String> {
    Ok(())
}

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
