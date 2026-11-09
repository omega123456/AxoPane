pub mod block;
pub mod file;
pub mod writer;

use std::collections::HashMap;
use std::fs;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use block::{BlockSpan, DecodedBlock, NamesCorrupt, BLOCK_TARGET_BYTES};
use file::{remove_if_present, spill_path, MappedNames, NamesHeader, SpillFile, SpillLocation};
use writer::{PairCapture, RewriteStats, WrittenNames};

pub const NAME_TERMINATOR: u8 = 0;
pub const OVERLAY_THRESHOLD_BYTES: usize = 4 * 1024 * 1024;
pub const OVERLAY_THRESHOLD_FOLDERS: usize = 5_000;

const NO_SLOT: u32 = u32::MAX;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackedNames(Vec<u8>);

impl PackedNames {
    pub fn from_names<N: AsRef<[u8]>>(names: impl IntoIterator<Item = N>) -> Self {
        let mut packed = Self::default();
        for name in names {
            packed.push(name.as_ref());
        }
        packed
    }

    pub fn push(&mut self, name: &[u8]) {
        if !is_storable_name(name) {
            return;
        }
        self.0.extend_from_slice(name);
        self.0.push(NAME_TERMINATOR);
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn names(&self) -> impl Iterator<Item = &[u8]> {
        names_in(&self.0)
    }
}

fn is_storable_name(name: &[u8]) -> bool {
    !name.is_empty() && !name.contains(&NAME_TERMINATOR)
}

pub fn names_in(packed: &[u8]) -> impl Iterator<Item = &[u8]> {
    packed
        .strip_suffix(&[NAME_TERMINATOR])
        .into_iter()
        .flat_map(|body| body.split(|&byte| byte == NAME_TERMINATOR))
}

pub(crate) fn is_packed_list_or_empty(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    let terminated = bytes.last() == Some(&NAME_TERMINATOR);
    let starts_with_name = bytes.first() != Some(&NAME_TERMINATOR);
    let has_empty_name = bytes
        .windows(2)
        .any(|pair| pair == [NAME_TERMINATOR, NAME_TERMINATOR]);
    terminated && starts_with_name && !has_empty_name
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderNames<'names> {
    pub slot: u32,
    pub packed: &'names [u8],
}

impl<'names> FolderNames<'names> {
    pub fn names(&self) -> impl Iterator<Item = &'names [u8]> {
        names_in(self.packed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamesUnreadable;

#[derive(Debug, Default)]
pub struct NamesScratch {
    pub(crate) compressed: Vec<u8>,
    pub(crate) raw: Vec<u8>,
}

pub struct SpillRead<'read> {
    pub slots: &'read [u32],
    pub scratch: &'read mut NamesScratch,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NamesCounters {
    pub blocks_decompressed: u64,
    pub blocks_reencoded: u64,
    pub blocks_copied: u64,
    pub peak_open_output_blocks: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OverlayStats {
    pub folders: usize,
    pub name_bytes: usize,
}

impl OverlayStats {
    pub fn exceeds_threshold(&self) -> bool {
        self.name_bytes > OVERLAY_THRESHOLD_BYTES || self.folders > OVERLAY_THRESHOLD_FOLDERS
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayEntry {
    Names(PackedNames),
    Tombstone,
}

impl OverlayEntry {
    fn names(&self) -> Option<&PackedNames> {
        match self {
            Self::Names(names) => Some(names),
            Self::Tombstone => None,
        }
    }

    fn packed(&self) -> &[u8] {
        self.names().map_or(&[], PackedNames::as_bytes)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Overlay {
    entries: HashMap<u32, OverlayEntry>,
    name_bytes: usize,
}

impl Overlay {
    fn insert(&mut self, slot: u32, entry: OverlayEntry) {
        self.name_bytes += entry.packed().len();
        if let Some(replaced) = self.entries.insert(slot, entry) {
            self.name_bytes -= replaced.packed().len();
        }
    }

    fn remove(&mut self, slot: u32) {
        if let Some(removed) = self.entries.remove(&slot) {
            self.name_bytes -= removed.packed().len();
        }
    }

    pub(crate) fn get(&self, slot: u32) -> Option<&OverlayEntry> {
        self.entries.get(&slot)
    }

    fn stats(&self) -> OverlayStats {
        OverlayStats {
            folders: self.entries.len(),
            name_bytes: self.name_bytes,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SpillIndex {
    locations: Vec<SpillLocation>,
}

impl SpillIndex {
    pub(crate) fn location(&self, slot: u32) -> Option<SpillLocation> {
        self.locations
            .get(slot as usize)
            .copied()
            .filter(SpillLocation::is_present)
    }

    fn set(&mut self, slot: u32, location: SpillLocation) {
        let index = slot as usize;
        if index >= self.locations.len() {
            self.locations.resize(index + 1, SpillLocation::ABSENT);
        }
        self.locations[index] = location;
    }

    fn clear(&mut self, slot: u32) {
        if let Some(location) = self.locations.get_mut(slot as usize) {
            *location = SpillLocation::ABSENT;
        }
    }

    fn present(&self) -> impl Iterator<Item = (u32, SpillLocation)> + '_ {
        self.locations
            .iter()
            .enumerate()
            .filter(|(_, location)| location.is_present())
            .map(|(slot, location)| (slot as u32, *location))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Spill {
    pub(crate) file: Arc<SpillFile>,
    pub(crate) index: SpillIndex,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Translation {
    to_slot: Vec<u32>,
    to_ordinal: Vec<u32>,
}

impl Translation {
    fn from_order(order: Vec<u32>) -> Self {
        let slot_count = order
            .iter()
            .map(|&slot| slot as usize + 1)
            .max()
            .unwrap_or(0);
        let mut to_ordinal = vec![NO_SLOT; slot_count];
        for (ordinal, &slot) in order.iter().enumerate() {
            to_ordinal[slot as usize] = ordinal as u32;
        }
        Self {
            to_slot: order,
            to_ordinal,
        }
    }

    fn identity(count: u32) -> Self {
        Self::from_order((0..count).collect())
    }

    pub(crate) fn slot_of(&self, ordinal: u32) -> Option<u32> {
        self.to_slot
            .get(ordinal as usize)
            .copied()
            .filter(|&slot| slot != NO_SLOT)
    }

    pub(crate) fn ordinal_of(&self, slot: u32) -> Option<u32> {
        self.to_ordinal
            .get(slot as usize)
            .copied()
            .filter(|&ordinal| ordinal != NO_SLOT)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MappedState {
    pub(crate) file: Arc<MappedNames>,
    pub(crate) translation: Translation,
}

#[derive(Debug, Default)]
struct ReadFailure {
    failed: AtomicBool,
    reported: AtomicBool,
}

impl ReadFailure {
    fn mark(&self) {
        self.failed.store(true, Ordering::Release);
    }

    fn is_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    fn take_unreported(&self) -> bool {
        self.is_failed() && !self.reported.swap(true, Ordering::AcqRel)
    }

    fn mark_handled(&self) {
        self.mark();
        self.reported.store(true, Ordering::Release);
    }

    fn clear(&self) {
        self.failed.store(false, Ordering::Release);
        self.reported.store(false, Ordering::Release);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairExpectation {
    pub pair_token: u64,
    pub folder_count: u32,
}

#[derive(Debug)]
pub enum InstallError {
    NamesNotReplaced(io::Error),
    SnapshotNotReplaced(io::Error),
}

#[derive(Debug)]
pub struct FileNameStore {
    path: PathBuf,
    block_target: usize,
    mapped: Option<MappedState>,
    overlay: Overlay,
    spill: Option<Spill>,
    failure: ReadFailure,
    blocks_decompressed: AtomicU64,
    rewrites: RewriteStats,
}

impl FileNameStore {
    pub fn new(path: PathBuf) -> Self {
        Self::with_block_target(path, BLOCK_TARGET_BYTES)
    }

    pub fn with_block_target(path: PathBuf, block_target: usize) -> Self {
        Self {
            path,
            block_target,
            mapped: None,
            overlay: Overlay::default(),
            spill: None,
            failure: ReadFailure::default(),
            blocks_decompressed: AtomicU64::new(0),
            rewrites: RewriteStats::default(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn spill_path(&self) -> PathBuf {
        spill_path(&self.path)
    }

    pub fn is_available(&self) -> bool {
        let has_source = self.mapped.is_some() || self.spill.is_some();
        has_source && !self.failure.is_failed()
    }

    pub fn has_names_file(&self) -> bool {
        self.mapped.is_some()
    }

    pub fn is_spilling(&self) -> bool {
        self.spill.is_some()
    }

    pub fn spill_len(&self) -> Option<u64> {
        self.spill.as_ref().map(|spill| spill.file.len())
    }

    pub fn names_header(&self) -> Option<NamesHeader> {
        self.mapped.as_ref().map(|mapped| mapped.file.header())
    }

    pub fn overlay_stats(&self) -> OverlayStats {
        self.overlay.stats()
    }

    pub fn has_unsaved_names(&self) -> bool {
        !self.overlay.entries.is_empty() || self.spill.is_some()
    }

    pub fn counters(&self) -> NamesCounters {
        NamesCounters {
            blocks_decompressed: self.blocks_decompressed.load(Ordering::Relaxed),
            blocks_reencoded: self.rewrites.blocks_reencoded,
            blocks_copied: self.rewrites.blocks_copied,
            peak_open_output_blocks: self.rewrites.peak_open_blocks,
        }
    }

    pub fn ordinal_of(&self, slot: u32) -> Option<u32> {
        self.mapped.as_ref()?.translation.ordinal_of(slot)
    }

    pub fn slot_of(&self, ordinal: u32) -> Option<u32> {
        self.mapped.as_ref()?.translation.slot_of(ordinal)
    }

    pub fn block_count(&self) -> usize {
        self.mapped
            .as_ref()
            .map_or(0, |mapped| mapped.file.block_count())
    }

    pub fn blocks_covering(&self, ordinals: Range<u32>) -> Range<usize> {
        self.mapped
            .as_ref()
            .map_or(0..0, |mapped| mapped.file.blocks_covering(ordinals))
    }

    pub fn blocks_for_slots(&self, slots: &[u32]) -> Vec<usize> {
        let mut blocks: Vec<usize> = slots
            .iter()
            .filter_map(|&slot| self.ordinal_of(slot))
            .map(|ordinal| self.blocks_covering(ordinal..ordinal + 1).start)
            .collect();
        blocks.sort_unstable();
        blocks.dedup();
        blocks
    }

    pub fn read_block<'read>(
        &'read self,
        index: usize,
        scratch: &'read mut NamesScratch,
    ) -> Result<BlockNames<'read>, NamesUnreadable> {
        let decoded = self.decode_block(index, &mut scratch.raw)?;
        Ok(BlockNames {
            store: self,
            decoded,
        })
    }

    fn decode_block<'raw>(
        &self,
        index: usize,
        raw: &'raw mut Vec<u8>,
    ) -> Result<DecodedBlock<'raw>, NamesUnreadable> {
        let stored = self
            .mapped
            .as_ref()
            .and_then(|mapped| mapped.file.block(index))
            .ok_or(NamesUnreadable)?;
        self.blocks_decompressed.fetch_add(1, Ordering::Relaxed);
        block::decode(stored, raw).map_err(|corrupt| self.unreadable(corrupt))
    }

    fn unreadable(&self, _corrupt: NamesCorrupt) -> NamesUnreadable {
        self.failure.mark();
        NamesUnreadable
    }

    pub fn visit_overlay(&self, visit: &mut dyn FnMut(FolderNames<'_>)) {
        let named = self.overlay.entries.iter().filter_map(|(&slot, entry)| {
            let names = entry.names()?;
            Some(FolderNames {
                slot,
                packed: names.as_bytes(),
            })
        });
        for folder in named {
            visit(folder);
        }
    }

    pub fn visit_spill(
        &self,
        scratch: &mut NamesScratch,
        visit: &mut dyn FnMut(FolderNames<'_>),
    ) -> Result<(), NamesUnreadable> {
        let Some(spill) = &self.spill else {
            return Ok(());
        };
        let spilled: Vec<u32> = spill.index.present().map(|(slot, _)| slot).collect();
        let request = SpillRead {
            slots: &spilled,
            scratch,
        };
        self.visit_spilled(request, visit)
    }

    pub fn visit_spilled(
        &self,
        request: SpillRead<'_>,
        visit: &mut dyn FnMut(FolderNames<'_>),
    ) -> Result<(), NamesUnreadable> {
        let Some(spill) = &self.spill else {
            return Ok(());
        };
        let unshadowed = request
            .slots
            .iter()
            .filter(|&&slot| self.overlay.get(slot).is_none())
            .filter_map(|&slot| Some((slot, spill.index.location(slot)?)));
        for (slot, location) in unshadowed {
            let packed = spill
                .file
                .read(location, request.scratch)
                .map_err(|corrupt| self.unreadable(corrupt))?;
            visit(FolderNames { slot, packed });
        }
        Ok(())
    }

    pub fn names_of(&self, slot: u32) -> Result<Option<PackedNames>, NamesUnreadable> {
        if let Some(entry) = self.overlay.get(slot) {
            return Ok(Some(PackedNames(entry.packed().to_vec())));
        }
        let mut scratch = NamesScratch::default();
        if let Some(location) = self.spill_location(slot) {
            return self.spilled_names(location, &mut scratch).map(Some);
        }
        self.block_names_of(slot, &mut scratch)
    }

    fn spill_location(&self, slot: u32) -> Option<SpillLocation> {
        self.spill.as_ref()?.index.location(slot)
    }

    fn spilled_names(
        &self,
        location: SpillLocation,
        scratch: &mut NamesScratch,
    ) -> Result<PackedNames, NamesUnreadable> {
        let spill = self.spill.as_ref().ok_or(NamesUnreadable)?;
        let packed = spill
            .file
            .read(location, scratch)
            .map_err(|corrupt| self.unreadable(corrupt))?;
        Ok(PackedNames(packed.to_vec()))
    }

    fn block_names_of(
        &self,
        slot: u32,
        scratch: &mut NamesScratch,
    ) -> Result<Option<PackedNames>, NamesUnreadable> {
        let Some(ordinal) = self.ordinal_of(slot) else {
            return Ok(None);
        };
        let index = self.blocks_covering(ordinal..ordinal + 1).start;
        let decoded = self.decode_block(index, &mut scratch.raw)?;
        let packed = decoded.names_of(ordinal).unwrap_or_default();
        Ok(Some(PackedNames(packed.to_vec())))
    }

    fn overrides(&self, slot: u32) -> bool {
        self.overlay.get(slot).is_some() || self.spill_location(slot).is_some()
    }

    pub fn take_unreported_failure(&self) -> bool {
        self.failure.take_unreported()
    }

    pub fn mark_unreadable(&self) {
        self.failure.mark();
    }

    pub fn set_names(&mut self, slot: u32, names: PackedNames) {
        self.overlay.insert(slot, OverlayEntry::Names(names));
    }

    pub fn tombstone(&mut self, slot: u32) {
        self.overlay.insert(slot, OverlayEntry::Tombstone);
        if let Some(spill) = &mut self.spill {
            spill.index.clear(slot);
        }
    }

    pub fn spill_names(&mut self, slot: u32, names: PackedNames) {
        let appended = self
            .spill
            .as_ref()
            .map(|spill| spill.file.append(names.as_bytes()));
        let Some(Ok(location)) = appended else {
            self.set_names(slot, names);
            return;
        };
        self.overlay.remove(slot);
        if let Some(spill) = &mut self.spill {
            spill.index.set(slot, location);
        }
    }

    pub fn begin_spill(&mut self) -> io::Result<()> {
        self.close_spill();
        let file = SpillFile::create(&self.spill_path())?;
        self.spill = Some(Spill {
            file: Arc::new(file),
            index: SpillIndex::default(),
        });
        Ok(())
    }

    fn close_spill(&mut self) {
        let Some(spill) = self.spill.take() else {
            return;
        };
        let removed = Arc::try_unwrap(spill.file).map(SpillFile::close_and_remove);
        if let Ok(Err(error)) = removed {
            log::warn!("size index: spill file removal failed: {error}");
        }
    }

    pub fn remove_stale_spill(&self) -> io::Result<()> {
        remove_if_present(&self.spill_path())
    }

    pub fn open_pair(&mut self, expected: PairExpectation) -> Option<NamesHeader> {
        let file = MappedNames::open(&self.path)?;
        let header = file.header();
        let token_matches = header.pair_token == expected.pair_token;
        let count_matches = header.folder_count == expected.folder_count;
        if !(token_matches && count_matches) {
            return None;
        }
        self.mapped = Some(MappedState {
            file: Arc::new(file),
            translation: Translation::identity(header.folder_count),
        });
        self.failure.clear();
        Some(header)
    }

    pub fn fail_and_discard(&mut self) {
        self.failure.mark_handled();
        self.mapped = None;
    }

    pub fn capture(&self, order: Vec<u32>) -> PairCapture {
        PairCapture {
            path: self.path.clone(),
            block_target: self.block_target,
            order,
            overlay: self.overlay.clone(),
            spill: self.spill.clone(),
            mapped: self.mapped.clone().filter(|_| !self.failure.is_failed()),
        }
    }

    pub fn install(
        &mut self,
        written: WrittenNames,
        commit_snapshot: impl FnOnce() -> io::Result<()>,
    ) -> Result<(), InstallError> {
        let previous_translation = self.mapped.take().map(|state| state.translation);
        if let Err(error) = fs::rename(&written.temp, &self.path) {
            let _ = remove_if_present(&written.temp);
            self.mapped = previous_translation.and_then(|translation| self.remapped(translation));
            return Err(InstallError::NamesNotReplaced(error));
        }
        let committed = commit_snapshot();
        self.adopt(written);
        committed.map_err(InstallError::SnapshotNotReplaced)
    }

    fn remapped(&self, translation: Translation) -> Option<MappedState> {
        let file = MappedNames::open(&self.path)?;
        Some(MappedState {
            file: Arc::new(file),
            translation,
        })
    }

    fn adopt(&mut self, written: WrittenNames) {
        let WrittenNames { order, stats, .. } = written;
        self.rewrites.absorb(stats);
        self.mapped = self.remapped(Translation::from_order(order));
        self.overlay = Overlay::default();
        self.close_spill();
        self.failure.clear();
    }
}

pub struct BlockNames<'read> {
    store: &'read FileNameStore,
    decoded: DecodedBlock<'read>,
}

impl<'read> BlockNames<'read> {
    pub fn span(&self) -> BlockSpan {
        self.decoded.span
    }

    pub fn folders(&self) -> impl Iterator<Item = FolderNames<'read>> + '_ {
        let first_ordinal = self.decoded.span.first_ordinal;
        self.decoded.entries().filter_map(move |entry| {
            let slot = self.store.slot_of(first_ordinal + entry.relative_id)?;
            let shadowed = self.store.overrides(slot);
            (!shadowed).then_some(FolderNames {
                slot,
                packed: entry.names,
            })
        })
    }
}
