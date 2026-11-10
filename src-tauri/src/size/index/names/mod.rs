pub mod block;
pub mod codec;
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
use codec::DecodeScratch;
use file::{remove_if_present, spill_path, MappedNames, NamesHeader, SpillFile, SpillLocation};
use writer::{PairCapture, RewriteStats, WrittenNames};

use super::tree::DeadRanges;

pub const NAME_TERMINATOR: u8 = 0;
pub const OVERLAY_THRESHOLD_BYTES: usize = 4 * 1024 * 1024;
pub const OVERLAY_THRESHOLD_FOLDERS: usize = 5_000;

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

    pub fn sort(&mut self) {
        let mut names: Vec<&[u8]> = self.names().collect();
        names.sort_unstable();
        let sorted = Self::from_names(names);
        *self = sorted;
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

    pub fn name_count(&self) -> u64 {
        let terminators = self.packed.iter().filter(|&&byte| byte == NAME_TERMINATOR);
        terminators.count() as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamesUnreadable;

#[derive(Debug, Default)]
pub struct NamesScratch {
    pub(crate) compressed: Vec<u8>,
    pub(crate) decode: DecodeScratch,
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

    fn remove_range(&mut self, slots: &Range<u32>) {
        let inside: Vec<u32> = self
            .entries
            .keys()
            .copied()
            .filter(|slot| slots.contains(slot))
            .collect();
        for slot in inside {
            self.remove(slot);
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
    locations: HashMap<u32, SpillLocation>,
}

impl SpillIndex {
    pub(crate) fn location(&self, slot: u32) -> Option<SpillLocation> {
        self.locations.get(&slot).copied()
    }

    fn set(&mut self, slot: u32, location: SpillLocation) {
        self.locations.insert(slot, location);
    }

    fn clear(&mut self, slot: u32) {
        self.locations.remove(&slot);
    }

    fn clear_range(&mut self, slots: &Range<u32>) {
        self.locations.retain(|slot, _| !slots.contains(slot));
    }

    fn slots(&self) -> Vec<u32> {
        let mut slots: Vec<u32> = self.locations.keys().copied().collect();
        slots.sort_unstable();
        slots
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
    to_ordinal: HashMap<u32, u32>,
}

impl Translation {
    fn from_order(order: Vec<u32>) -> Self {
        let to_ordinal = order
            .iter()
            .enumerate()
            .map(|(ordinal, &slot)| (slot, ordinal as u32))
            .collect();
        Self {
            to_slot: order,
            to_ordinal,
        }
    }

    fn slot_of(&self, ordinal: u32) -> Option<u32> {
        self.to_slot.get(ordinal as usize).copied()
    }

    fn ordinal_of(&self, slot: u32) -> Option<u32> {
        self.to_ordinal.get(&slot).copied()
    }
}

#[derive(Debug, Clone)]
pub(crate) enum SlotMap {
    Listed(Translation),
    Contiguous(Range<u32>),
}

impl SlotMap {
    pub(crate) fn slot_of(&self, ordinal: u32) -> Option<u32> {
        match self {
            Self::Listed(translation) => translation.slot_of(ordinal),
            Self::Contiguous(slots) => slots
                .start
                .checked_add(ordinal)
                .filter(|slot| slots.contains(slot)),
        }
    }

    pub(crate) fn ordinal_of(&self, slot: u32) -> Option<u32> {
        match self {
            Self::Listed(translation) => translation.ordinal_of(slot),
            Self::Contiguous(slots) => slots.contains(&slot).then(|| slot - slots.start),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MappedState {
    pub(crate) file: Arc<MappedNames>,
    pub(crate) translation: SlotMap,
}

struct PlacedSource<'store> {
    source: &'store MappedState,
    first_block: usize,
}

struct LocatedFolder<'store> {
    source: &'store MappedState,
    ordinal: u32,
    first_block: usize,
}

impl LocatedFolder<'_> {
    fn block_in_source(&self) -> usize {
        let own_ordinal = self.ordinal..self.ordinal + 1;
        self.source.file.blocks_covering(own_ordinal).start
    }

    fn block(&self) -> usize {
        self.first_block + self.block_in_source()
    }
}

struct LocatedBlock<'store> {
    source: &'store MappedState,
    index_in_source: usize,
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
    pub first_slot: u32,
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
    segments: Vec<MappedState>,
    overlay: Overlay,
    dead: DeadRanges,
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
            segments: Vec::new(),
            overlay: Overlay::default(),
            dead: DeadRanges::default(),
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
        let has_mapped_source = self.sources().next().is_some();
        let has_source = has_mapped_source || self.spill.is_some();
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

    fn sources(&self) -> impl Iterator<Item = &MappedState> {
        self.mapped.iter().chain(&self.segments)
    }

    fn placed_sources(&self) -> impl Iterator<Item = PlacedSource<'_>> {
        self.sources().scan(0, |next_block, source| {
            let first_block = *next_block;
            *next_block += source.file.block_count();
            Some(PlacedSource {
                source,
                first_block,
            })
        })
    }

    fn locate(&self, slot: u32) -> Option<LocatedFolder<'_>> {
        self.placed_sources().find_map(|placed| {
            let ordinal = placed.source.translation.ordinal_of(slot)?;
            Some(LocatedFolder {
                source: placed.source,
                ordinal,
                first_block: placed.first_block,
            })
        })
    }

    fn locate_block(&self, index: usize) -> Option<LocatedBlock<'_>> {
        self.placed_sources().find_map(|placed| {
            let index_in_source = index.checked_sub(placed.first_block)?;
            let is_in_source = index_in_source < placed.source.file.block_count();
            is_in_source.then_some(LocatedBlock {
                source: placed.source,
                index_in_source,
            })
        })
    }

    pub fn attach_segment(&mut self, slots: Range<u32>, file: Arc<MappedNames>) {
        self.segments.push(MappedState {
            file,
            translation: SlotMap::Contiguous(slots),
        });
    }

    pub fn block_count(&self) -> usize {
        self.sources().map(|source| source.file.block_count()).sum()
    }

    pub fn blocks_covering(&self, ordinals: Range<u32>) -> Range<usize> {
        self.mapped
            .as_ref()
            .map_or(0..0, |mapped| mapped.file.blocks_covering(ordinals))
    }

    pub fn blocks_for_slots(&self, slots: &[u32]) -> Vec<usize> {
        let mut blocks: Vec<usize> = slots
            .iter()
            .filter_map(|&slot| self.locate(slot))
            .map(|folder| folder.block())
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
        let block = self.locate_block(index).ok_or(NamesUnreadable)?;
        let decoded = self.decode_block(&block, &mut scratch.decode)?;
        Ok(BlockNames {
            store: self,
            slots: &block.source.translation,
            decoded,
        })
    }

    fn decode_block<'raw>(
        &self,
        block: &LocatedBlock<'_>,
        scratch: &'raw mut DecodeScratch,
    ) -> Result<DecodedBlock<'raw>, NamesUnreadable> {
        let stored = block
            .source
            .file
            .block(block.index_in_source)
            .ok_or(NamesUnreadable)?;
        self.blocks_decompressed.fetch_add(1, Ordering::Relaxed);
        block::decode(stored, scratch).map_err(|corrupt| self.unreadable(corrupt))
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

    pub fn count_saved_and_overlay_names(&self) -> Result<u64, NamesUnreadable> {
        let mut count = 0;
        self.visit_overlay(&mut |folder| count += folder.name_count());
        let mut scratch = NamesScratch::default();
        for index in 0..self.block_count() {
            let block = self.read_block(index, &mut scratch)?;
            count += block
                .folders()
                .map(|folder| folder.name_count())
                .sum::<u64>();
        }
        Ok(count)
    }

    pub fn visit_spill(
        &self,
        scratch: &mut NamesScratch,
        visit: &mut dyn FnMut(FolderNames<'_>),
    ) -> Result<(), NamesUnreadable> {
        let Some(spill) = &self.spill else {
            return Ok(());
        };
        let spilled = spill.index.slots();
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
        if self.dead.contains(slot) {
            return Ok(Some(PackedNames::default()));
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
        let Some(folder) = self.locate(slot) else {
            return Ok(None);
        };
        let block = LocatedBlock {
            source: folder.source,
            index_in_source: folder.block_in_source(),
        };
        let decoded = self.decode_block(&block, &mut scratch.decode)?;
        let packed = decoded.names_of(folder.ordinal).unwrap_or_default();
        Ok(Some(PackedNames(packed.to_vec())))
    }

    fn overrides(&self, slot: u32) -> bool {
        let has_newer_names =
            self.overlay.get(slot).is_some() || self.spill_location(slot).is_some();
        has_newer_names || self.dead.contains(slot)
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

    pub fn tombstone_range(&mut self, slots: Range<u32>) {
        self.overlay.remove_range(&slots);
        if let Some(spill) = &mut self.spill {
            spill.index.clear_range(&slots);
        }
        self.dead.add(slots);
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
        let first_slot = expected.first_slot;
        self.mapped = Some(MappedState {
            file: Arc::new(file),
            translation: SlotMap::Contiguous(first_slot..first_slot + header.folder_count),
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
        let previous_slots = self.mapped.take().map(|state| state.translation);
        if let Err(error) = fs::rename(&written.temp, &self.path) {
            let _ = remove_if_present(&written.temp);
            self.mapped = previous_slots.and_then(|slots| self.remapped(slots));
            return Err(InstallError::NamesNotReplaced(error));
        }
        let committed = commit_snapshot();
        self.adopt(written);
        committed.map_err(InstallError::SnapshotNotReplaced)
    }

    fn remapped(&self, translation: SlotMap) -> Option<MappedState> {
        let file = MappedNames::open(&self.path)?;
        Some(MappedState {
            file: Arc::new(file),
            translation,
        })
    }

    fn adopt(&mut self, written: WrittenNames) {
        let WrittenNames { order, stats, .. } = written;
        self.rewrites.absorb(stats);
        self.mapped = self.remapped(SlotMap::Listed(Translation::from_order(order)));
        self.overlay = Overlay::default();
        self.close_spill();
        self.failure.clear();
    }
}

pub struct BlockNames<'read> {
    store: &'read FileNameStore,
    slots: &'read SlotMap,
    decoded: DecodedBlock<'read>,
}

impl<'read> BlockNames<'read> {
    pub fn span(&self) -> BlockSpan {
        self.decoded.span
    }

    pub fn folders(&self) -> impl Iterator<Item = FolderNames<'read>> + '_ {
        let first_ordinal = self.decoded.span.first_ordinal;
        self.decoded.entries().filter_map(move |entry| {
            let slot = self.slots.slot_of(first_ordinal + entry.relative_id)?;
            let shadowed = self.store.overrides(slot);
            (!shadowed).then_some(FolderNames {
                slot,
                packed: entry.names,
            })
        })
    }
}
