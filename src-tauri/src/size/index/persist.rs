//! Index snapshot encoding plus the catch-up decision. Pure: the snapshot
//! path is injected, and FSEvents flags arrive pre-mapped to [`ChangeFlags`].
//!
//! Layout (little-endian): magic, `u32` version, volume UUID, OS build, `u64` saved
//! event ID, `u64` last full-scan Unix seconds, visited roots, node table,
//! then an FNV-1a 64 checksum of everything before it. Strings are a `u32`
//! byte length plus UTF-8. Nodes are written parents-first as `u32` parent
//! position (`u32::MAX` for roots), name, `u64` direct bytes, `u8` flags.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::tree::{FolderSizeTree, ImportedNode};

pub const FORMAT_VERSION: u32 = 2;
const MAGIC: &[u8; 8] = b"AXOSIZE\0";
const MAX_STRING_BYTES: usize = 64 * 1024;
const MAX_VISITED_ROOTS: u32 = 1024;

/// Everything in a snapshot except the tree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SnapshotMeta {
    pub volume_uuid: String,
    /// The macOS build the tree was scanned on; an update means a rescan.
    pub os_build: String,
    /// The event ID captured before the scan that produced the tree.
    pub event_id: u64,
    /// Unix seconds when the last full scan completed.
    pub last_full_scan_secs: u64,
    pub visited_roots: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct Snapshot {
    pub meta: SnapshotMeta,
    pub tree: FolderSizeTree,
}

pub fn encode(meta: &SnapshotMeta, tree: &FolderSizeTree) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    put_str(&mut out, &meta.volume_uuid);
    put_str(&mut out, &meta.os_build);
    out.extend_from_slice(&meta.event_id.to_le_bytes());
    out.extend_from_slice(&meta.last_full_scan_secs.to_le_bytes());
    out.extend_from_slice(&(meta.visited_roots.len() as u32).to_le_bytes());
    for root in &meta.visited_roots {
        put_str(&mut out, &root.to_string_lossy());
    }
    let count_at = out.len();
    out.extend_from_slice(&0_u32.to_le_bytes());
    let mut count = 0_u32;
    tree.export(|parent, name, direct, flags| {
        out.extend_from_slice(&parent.to_le_bytes());
        put_str(&mut out, name);
        out.extend_from_slice(&direct.to_le_bytes());
        out.push(flags);
        count += 1;
    });
    out[count_at..count_at + size_of::<u32>()].copy_from_slice(&count.to_le_bytes());
    let checksum = fnv1a(&out);
    out.extend_from_slice(&checksum.to_le_bytes());
    out
}

/// `None` for anything but an intact snapshot of the current version.
pub fn decode(bytes: &[u8]) -> Option<Snapshot> {
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
    let volume_uuid = reader.string()?;
    let os_build = reader.string()?;
    let event_id = reader.u64()?;
    let last_full_scan_secs = reader.u64()?;
    let visited = reader.u32()?;
    if visited > MAX_VISITED_ROOTS {
        return None;
    }
    let visited_roots = (0..visited)
        .map(|_| reader.string().map(PathBuf::from))
        .collect::<Option<_>>()?;
    let mut tree = FolderSizeTree::default();
    for _ in 0..reader.u32()? {
        let parent = reader.u32()?;
        let name = reader.string()?;
        let direct = reader.u64()?;
        let flags = reader.take(1)?[0];
        tree.import_node(ImportedNode {
            parent,
            name: name.into(),
            direct,
            flags,
        })?;
    }
    if !reader.is_exhausted() {
        return None;
    }
    tree.finish_import();
    Some(Snapshot {
        meta: SnapshotMeta {
            volume_uuid,
            os_build,
            event_id,
            last_full_scan_secs,
            visited_roots,
        },
        tree,
    })
}

/// Writes atomically: a sibling temp file, fsync, then rename over `path`.
pub fn save(path: &Path, meta: &SnapshotMeta, tree: &FolderSizeTree) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("tmp");
    let mut file = fs::File::create(&temp)?;
    file.write_all(&encode(meta, tree))?;
    sync(&file)?;
    drop(file);
    fs::rename(&temp, path)
}

/// On macOS `sync_all` is `F_FULLFSYNC`, which flushes the whole drive's
/// write cache.
#[cfg(not(feature = "test-utils"))]
fn sync(file: &fs::File) -> io::Result<()> {
    file.sync_all()
}

/// Tests skip the device-wide flush; a torn snapshot is caught by `load`'s
/// checksum anyway.
#[cfg(feature = "test-utils")]
fn sync(_file: &fs::File) -> io::Result<()> {
    Ok(())
}

/// `None` when the file is missing, unreadable, corrupt or outdated.
pub fn load(path: &Path) -> Option<Snapshot> {
    decode(&fs::read(path).ok()?)
}

/// The FSEvents flags the index reacts to, independent of FSEvents itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChangeFlags {
    pub must_scan_sub_dirs: bool,
    pub user_dropped: bool,
    pub kernel_dropped: bool,
    pub event_ids_wrapped: bool,
    pub root_changed: bool,
}

/// One reported directory change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeEvent {
    pub path: PathBuf,
    pub flags: ChangeFlags,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayOutcome {
    UpToDate,
    /// Re-read these single directories and rescan these subtrees. Both are
    /// sorted and deduplicated; nothing is listed inside a rescanned subtree.
    Apply {
        reread: Vec<PathBuf>,
        rescan: Vec<PathBuf>,
    },
    /// Rescan everything in the background, keeping old totals visible.
    FullRescan,
}

#[derive(Clone, Copy)]
pub struct CurrentVolume<'volume> {
    pub uuid: &'volume str,
    pub os_build: &'volume str,
    pub event_id: u64,
}

/// Launch catch-up: `events` are those replayed since
/// `saved.event_id`. Any `MustScanSubDirs` during replay means history was
/// coalesced, so it forces a full rescan here, as does a different volume or
/// OS build (an update rewrites the sealed system volume without events).
pub fn plan_catch_up(
    saved: &SnapshotMeta,
    current: CurrentVolume<'_>,
    events: &[ChangeEvent],
) -> ReplayOutcome {
    let CurrentVolume {
        uuid: current_uuid,
        os_build: current_os_build,
        event_id: current_event_id,
    } = current;
    let volume_changed = saved.volume_uuid != current_uuid;
    let os_changed = saved.os_build != current_os_build;
    let history_rewound = saved.event_id > current_event_id;
    let subtree_dropped = events.iter().any(|event| event.flags.must_scan_sub_dirs);
    let needs_full_rescan = volume_changed || os_changed || history_rewound || subtree_dropped;
    if needs_full_rescan {
        return ReplayOutcome::FullRescan;
    }
    if saved.event_id == current_event_id {
        return ReplayOutcome::UpToDate;
    }
    plan_batch(events)
}

/// A live batch, or the per-directory part of a catch-up: `MustScanSubDirs`
/// rescans that subtree only; dropped, wrapped or root-changed events force a
/// full rescan.
pub fn plan_batch(events: &[ChangeEvent]) -> ReplayOutcome {
    if events.iter().any(|event| forces_full_rescan(event.flags)) {
        return ReplayOutcome::FullRescan;
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

fn forces_full_rescan(flags: ChangeFlags) -> bool {
    flags.user_dropped || flags.kernel_dropped || flags.event_ids_wrapped || flags.root_changed
}

/// Sorted paths with every path inside another one dropped.
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

fn put_str(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

const FNV1A_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A_PRIME: u64 = 0x0100_0000_01b3;

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(FNV1A_OFFSET_BASIS, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(FNV1A_PRIME)
    })
}

/// A bounds-checked cursor; every read is `None` past the end.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn is_exhausted(&self) -> bool {
        self.0.is_empty()
    }

    fn take(&mut self, byte_count: usize) -> Option<&'a [u8]> {
        let (head, rest) = self.0.split_at_checked(byte_count)?;
        self.0 = rest;
        Some(head)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(
            self.take(size_of::<u32>())?.try_into().ok()?,
        ))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(
            self.take(size_of::<u64>())?.try_into().ok()?,
        ))
    }

    fn string(&mut self) -> Option<String> {
        let byte_count = self.u32()? as usize;
        if byte_count > MAX_STRING_BYTES {
            return None;
        }
        String::from_utf8(self.take(byte_count)?.to_vec()).ok()
    }
}
