use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::names::{FileNameStore, SavedNames};
use super::segment::{MappedSegment, SegmentPaths};
use super::tree::FolderSizeTree;

pub const FORMAT_VERSION: u32 = 4;
const MAGIC: &[u8; 8] = b"AXOSIZE\0";
const MAX_STRING_BYTES: usize = 64 * 1024;
const MAX_VISITED_ROOTS: u32 = 1024;
const MAX_EXCLUSIONS: u32 = 1024;
const MAX_SEGMENTS: u32 = 1024;
const TEMP_EXTENSION: &str = "tmp";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SnapshotMeta {
    pub volume_uuid: String,
    pub os_build: String,
    pub event_id: u64,
    pub last_full_scan_secs: u64,
    pub visited_roots: Vec<PathBuf>,
    pub has_file_names: bool,
    pub names_purge_due: bool,
    pub full_scan_running: bool,
    pub exclusions: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentRef {
    pub sequence: u64,
    pub token: u64,
}

pub struct Manifest<'index> {
    pub meta: &'index SnapshotMeta,
    pub segments: &'index [SegmentRef],
    pub tree: &'index FolderSizeTree,
    pub names: &'index FileNameStore,
}

#[derive(Debug)]
pub struct Snapshot {
    pub meta: SnapshotMeta,
    pub segments: Vec<SegmentRef>,
    pub tree: FolderSizeTree,
    pub names: SavedNames,
}

pub fn encode(manifest: &Manifest<'_>) -> Vec<u8> {
    let Manifest {
        meta,
        segments,
        tree,
        names,
    } = manifest;
    let mut out = MAGIC.to_vec();
    put_u32(&mut out, FORMAT_VERSION);
    put_str(&mut out, &meta.volume_uuid);
    put_str(&mut out, &meta.os_build);
    put_u64(&mut out, meta.event_id);
    put_u64(&mut out, meta.last_full_scan_secs);
    put_paths(&mut out, &meta.visited_roots);
    out.push(u8::from(meta.has_file_names));
    out.push(u8::from(meta.names_purge_due));
    out.push(u8::from(meta.full_scan_running));
    put_paths(&mut out, &meta.exclusions);
    put_count(&mut out, segments.len());
    for segment in *segments {
        put_u64(&mut out, segment.sequence);
        put_u64(&mut out, segment.token);
    }
    tree.write_overlay(&mut out);
    names.write_overlay(&mut out);
    let checksum = fnv1a(&out);
    put_u64(&mut out, checksum);
    out
}

pub fn decode(bytes: &[u8], directory: &Path) -> Option<Snapshot> {
    let (body, checksum) = bytes.split_last_chunk::<{ size_of::<u64>() }>()?;
    let checksum_matches = fnv1a(body) == u64::from_le_bytes(*checksum);
    if !checksum_matches {
        return None;
    }
    let mut reader = Reader(body);
    let magic_matches = reader.take(MAGIC.len())? == MAGIC;
    if !magic_matches {
        return None;
    }
    let version_matches = reader.u32()? == FORMAT_VERSION;
    if !version_matches {
        return None;
    }
    let meta = SnapshotMeta {
        volume_uuid: reader.string()?,
        os_build: reader.string()?,
        event_id: reader.u64()?,
        last_full_scan_secs: reader.u64()?,
        visited_roots: reader.paths(MAX_VISITED_ROOTS)?,
        has_file_names: reader.byte()? != 0,
        names_purge_due: reader.byte()? != 0,
        full_scan_running: reader.byte()? != 0,
        exclusions: reader.paths(MAX_EXCLUSIONS)?,
    };
    let segments = reader.segment_refs()?;
    let mut tree = FolderSizeTree::default();
    let mut names = SavedNames::default();
    for segment in &segments {
        let mapped = open_segment(directory, *segment)?;
        let file_names = mapped.shared_file_names();
        let ids = tree.attach_segment(Arc::new(mapped))?;
        names.attach_segment(ids, file_names);
    }
    tree.read_overlay(&mut reader)?;
    names.read_overlay(&mut reader)?;
    if !reader.is_exhausted() {
        return None;
    }
    Some(Snapshot {
        meta,
        segments,
        tree,
        names,
    })
}

fn open_segment(directory: &Path, segment: SegmentRef) -> Option<MappedSegment> {
    let mapped = MappedSegment::open(&SegmentPaths::numbered(directory, segment.sequence))?;
    (mapped.token() == segment.token).then_some(mapped)
}

pub fn save_encoded(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = write_temp(path, bytes)?;
    let renamed = fs::rename(&temp, path);
    if renamed.is_err() {
        let _ = fs::remove_file(&temp);
    }
    renamed
}

pub fn write_temp(path: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
    let mut temp = TempFile::create(path)?;
    temp.file.write_all(bytes)?;
    sync(&temp.file)?;
    Ok(temp.path)
}

pub fn temp_path(path: &Path) -> PathBuf {
    path.with_extension(TEMP_EXTENSION)
}

pub(crate) struct TempFile {
    pub(crate) file: fs::File,
    pub(crate) path: PathBuf,
}

impl TempFile {
    pub(crate) fn create(final_path: &Path) -> io::Result<Self> {
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let path = temp_path(final_path);
        let file = fs::File::create(&path)?;
        Ok(Self { file, path })
    }
}

#[cfg(not(feature = "test-utils"))]
pub(crate) fn sync(file: &fs::File) -> io::Result<()> {
    file.sync_all()
}

#[cfg(feature = "test-utils")]
pub(crate) fn sync(_file: &fs::File) -> io::Result<()> {
    Ok(())
}

pub fn load(path: &Path) -> Option<Snapshot> {
    decode(&fs::read(path).ok()?, path.parent()?)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChangeFlags {
    pub must_scan_sub_dirs: bool,
    pub user_dropped: bool,
    pub kernel_dropped: bool,
    pub event_ids_wrapped: bool,
    pub root_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeEvent {
    pub path: PathBuf,
    pub flags: ChangeFlags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RescanReason {
    UserDropped,
    KernelDropped,
    EventIdsWrapped,
    RootChanged,
    VolumeChanged,
    OsBuildChanged,
    HistoryRewound,
    SubtreeDropped,
    NoSnapshot,
    NoFileNames,
}

impl RescanReason {
    pub fn text(self) -> &'static str {
        match self {
            Self::UserDropped => "user-dropped events",
            Self::KernelDropped => "kernel-dropped events",
            Self::EventIdsWrapped => "event ids wrapped",
            Self::RootChanged => "watched root changed",
            Self::VolumeChanged => "volume changed",
            Self::OsBuildChanged => "OS build changed",
            Self::HistoryRewound => "event history rewound",
            Self::SubtreeDropped => "subtree dropped during catch-up",
            Self::NoSnapshot => "no usable snapshot",
            Self::NoFileNames => "saved index holds no file names",
        }
    }
}

impl fmt::Display for RescanReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.text())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RescanCause {
    pub reason: RescanReason,
    pub path: Option<PathBuf>,
}

impl RescanCause {
    fn of_volume(reason: RescanReason) -> Self {
        Self { reason, path: None }
    }

    fn of_event(reason: RescanReason, event: &ChangeEvent) -> Self {
        Self {
            reason,
            path: Some(event.path.clone()),
        }
    }
}

impl fmt::Display for RescanCause {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "event history needs a full rescan ({})",
            self.reason
        )?;
        let Some(path) = &self.path else {
            return Ok(());
        };
        write!(formatter, " at {}", path.display())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayOutcome {
    UpToDate,
    Apply {
        reread: Vec<PathBuf>,
        rescan: Vec<PathBuf>,
    },
    FullRescan(RescanCause),
}

#[derive(Clone, Copy)]
pub struct CurrentVolume<'volume> {
    pub uuid: &'volume str,
    pub os_build: &'volume str,
    pub event_id: u64,
}

#[derive(Clone, Copy)]
pub struct LaunchState<'launch> {
    pub saved: Option<&'launch SnapshotMeta>,
    pub current: CurrentVolume<'launch>,
    pub collects_file_names: bool,
}

pub fn flags_rescan_reason(flags: ChangeFlags) -> Option<RescanReason> {
    if flags.user_dropped {
        return Some(RescanReason::UserDropped);
    }
    if flags.kernel_dropped {
        return Some(RescanReason::KernelDropped);
    }
    if flags.event_ids_wrapped {
        return Some(RescanReason::EventIdsWrapped);
    }
    flags.root_changed.then_some(RescanReason::RootChanged)
}

pub fn volume_rescan_reason(
    saved: &SnapshotMeta,
    current: CurrentVolume<'_>,
) -> Option<RescanReason> {
    if saved.volume_uuid != current.uuid {
        return Some(RescanReason::VolumeChanged);
    }
    if saved.os_build != current.os_build {
        return Some(RescanReason::OsBuildChanged);
    }
    let history_rewound = saved.event_id > current.event_id;
    history_rewound.then_some(RescanReason::HistoryRewound)
}

pub fn launch_rescan_reason(launch: LaunchState<'_>) -> Option<RescanReason> {
    let Some(saved) = launch.saved else {
        return Some(RescanReason::NoSnapshot);
    };
    let names_missing = launch.collects_file_names && !saved.has_file_names;
    if names_missing {
        return Some(RescanReason::NoFileNames);
    }
    volume_rescan_reason(saved, launch.current)
}

pub fn plan_catch_up(
    saved: &SnapshotMeta,
    current: CurrentVolume<'_>,
    events: &[ChangeEvent],
) -> ReplayOutcome {
    if let Some(reason) = volume_rescan_reason(saved, current) {
        return ReplayOutcome::FullRescan(RescanCause::of_volume(reason));
    }
    let subtree_dropped = events.iter().find(|event| event.flags.must_scan_sub_dirs);
    if let Some(event) = subtree_dropped {
        return ReplayOutcome::FullRescan(RescanCause::of_event(
            RescanReason::SubtreeDropped,
            event,
        ));
    }
    if saved.event_id == current.event_id {
        return ReplayOutcome::UpToDate;
    }
    plan_batch(events)
}

pub fn plan_batch(events: &[ChangeEvent]) -> ReplayOutcome {
    if let Some(cause) = first_flag_cause(events) {
        return ReplayOutcome::FullRescan(cause);
    }
    let (rescans, rereads): (Vec<_>, Vec<_>) = events
        .iter()
        .partition(|event| event.flags.must_scan_sub_dirs);
    let rescan = outermost(
        rescans
            .into_iter()
            .map(|event| event.path.clone())
            .collect(),
    );
    let mut reread: Vec<PathBuf> = rereads
        .into_iter()
        .map(|event| event.path.clone())
        .collect();
    reread.sort();
    reread.dedup();
    reread.retain(|path| !rescan.iter().any(|rescanned| path.starts_with(rescanned)));
    let nothing_to_apply = reread.is_empty() && rescan.is_empty();
    if nothing_to_apply {
        return ReplayOutcome::UpToDate;
    }
    ReplayOutcome::Apply { reread, rescan }
}

fn first_flag_cause(events: &[ChangeEvent]) -> Option<RescanCause> {
    events.iter().find_map(|event| {
        let reason = flags_rescan_reason(event.flags)?;
        Some(RescanCause::of_event(reason, event))
    })
}

fn outermost(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort();
    let mut kept: Vec<PathBuf> = Vec::new();
    for path in paths {
        let nested = kept.iter().any(|kept_path| path.starts_with(kept_path));
        if !nested {
            kept.push(path);
        }
    }
    kept
}

fn put_paths(out: &mut Vec<u8>, paths: &[PathBuf]) {
    put_count(out, paths.len());
    for path in paths {
        put_str(out, &path.to_string_lossy());
    }
}

pub(crate) fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub(crate) fn put_count(out: &mut Vec<u8>, count: usize) {
    put_u32(out, count as u32);
}

pub(crate) fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    put_count(out, bytes.len());
    out.extend_from_slice(bytes);
}

pub(crate) fn put_str(out: &mut Vec<u8>, value: &str) {
    put_bytes(out, value.as_bytes());
}

pub(crate) fn put_ids(out: &mut Vec<u8>, ids: &[u32]) {
    put_count(out, ids.len());
    for &id in ids {
        put_u32(out, id);
    }
}

pub(crate) fn put_ranges(out: &mut Vec<u8>, ranges: &[Range<u32>]) {
    put_count(out, ranges.len());
    for range in ranges {
        put_u32(out, range.start);
        put_u32(out, range.end);
    }
}

const FNV1A_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A_PRIME: u64 = 0x0100_0000_01b3;

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(FNV1A_OFFSET_BASIS, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(FNV1A_PRIME)
    })
}

pub(crate) struct Reader<'a>(pub(crate) &'a [u8]);

impl<'a> Reader<'a> {
    pub(crate) fn is_exhausted(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn take(&mut self, byte_count: usize) -> Option<&'a [u8]> {
        let (head, rest) = self.0.split_at_checked(byte_count)?;
        self.0 = rest;
        Some(head)
    }

    pub(crate) fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(
            self.take(size_of::<u32>())?.try_into().ok()?,
        ))
    }

    pub(crate) fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(
            self.take(size_of::<u64>())?.try_into().ok()?,
        ))
    }

    pub(crate) fn byte(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    pub(crate) fn bytes(&mut self) -> Option<&'a [u8]> {
        let byte_count = self.u32()? as usize;
        self.take(byte_count)
    }

    pub(crate) fn string(&mut self) -> Option<String> {
        let bytes = self.bytes()?;
        if bytes.len() > MAX_STRING_BYTES {
            return None;
        }
        String::from_utf8(bytes.to_vec()).ok()
    }

    pub(crate) fn ids(&mut self) -> Option<Vec<u32>> {
        (0..self.u32()?).map(|_| self.u32()).collect()
    }

    pub(crate) fn ranges(&mut self) -> Option<Vec<Range<u32>>> {
        (0..self.u32()?).map(|_| self.range()).collect()
    }

    fn range(&mut self) -> Option<Range<u32>> {
        let start = self.u32()?;
        let end = self.u32()?;
        (start < end).then_some(start..end)
    }

    fn segment_refs(&mut self) -> Option<Vec<SegmentRef>> {
        let count = self.u32()?;
        if count > MAX_SEGMENTS {
            return None;
        }
        (0..count).map(|_| self.segment_ref()).collect()
    }

    fn segment_ref(&mut self) -> Option<SegmentRef> {
        Some(SegmentRef {
            sequence: self.u64()?,
            token: self.u64()?,
        })
    }

    fn paths(&mut self, max_count: u32) -> Option<Vec<PathBuf>> {
        let count = self.u32()?;
        if count > max_count {
            return None;
        }
        (0..count)
            .map(|_| self.string().map(PathBuf::from))
            .collect()
    }
}
