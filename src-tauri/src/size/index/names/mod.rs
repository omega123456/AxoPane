pub mod block;
pub mod file;
pub mod signature;
pub mod writer;

use std::collections::HashMap;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use block::{BlockFolder, BlockSpan, BlockView, NamesCorrupt, StoredBlock};
use file::{remove_if_present, spill_path, MappedNames};
use signature::{SignatureFilter, SignedName};
use writer::RewriteStats;

use super::persist::{put_bytes, put_count, put_u32, Reader};
use super::tree::IdRanges;

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
enum FolderSignatures<'names> {
    Stored(BlockFolder<'names>),
    Live,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderNames<'names> {
    pub slot: u32,
    pub packed: &'names [u8],
    signatures: FolderSignatures<'names>,
}

impl<'names> FolderNames<'names> {
    pub fn live(slot: u32, packed: &'names [u8]) -> Self {
        Self {
            slot,
            packed,
            signatures: FolderSignatures::Live,
        }
    }

    fn stored(slot: u32, folder: BlockFolder<'names>) -> Self {
        Self {
            slot,
            packed: folder.packed,
            signatures: FolderSignatures::Stored(folder),
        }
    }

    pub fn names(&self) -> impl Iterator<Item = &'names [u8]> {
        names_in(self.packed)
    }

    pub fn visit_admitted(
        &self,
        filter: &dyn SignatureFilter,
        visit: &mut dyn FnMut(SignedName<'names>),
    ) {
        match self.signatures {
            FolderSignatures::Stored(folder) => folder.admitted_names(filter).for_each(visit),
            FolderSignatures::Live => self.live_admitted_names(filter).for_each(visit),
        }
    }

    fn live_admitted_names<'filter>(
        &self,
        filter: &'filter dyn SignatureFilter,
    ) -> impl Iterator<Item = SignedName<'names>> + 'filter
    where
        'names: 'filter,
    {
        let signed = self.names().map(SignedName::of);
        signed.filter(move |name| filter.admits(name.signature))
    }

    pub fn name_count(&self) -> u64 {
        let terminators = self.packed.iter().filter(|&&byte| byte == NAME_TERMINATOR);
        terminators.count() as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamesUnreadable;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NamesCounters {
    pub blocks_read: u64,
    pub blocks_verified: u64,
    pub blocks_rebuilt: u64,
    pub blocks_copied: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReusableBlock {
    pub index: usize,
    pub first_slot: u32,
    pub folder_count: u32,
}

impl ReusableBlock {
    pub fn slots(&self) -> Range<u32> {
        self.first_slot..self.first_slot + self.folder_count
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OverlayStats {
    pub folders: usize,
    pub name_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayThreshold {
    pub folders: usize,
    pub name_bytes: usize,
}

impl Default for OverlayThreshold {
    fn default() -> Self {
        Self {
            folders: OVERLAY_THRESHOLD_FOLDERS,
            name_bytes: OVERLAY_THRESHOLD_BYTES,
        }
    }
}

impl OverlayStats {
    pub fn exceeds(&self, threshold: OverlayThreshold) -> bool {
        let holds_too_many_bytes = self.name_bytes > threshold.name_bytes;
        let holds_too_many_folders = self.folders > threshold.folders;
        holds_too_many_bytes || holds_too_many_folders
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

    fn write(&self, out: &mut Vec<u8>) {
        out.push(u8::from(self.names().is_some()));
        put_bytes(out, self.packed());
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        let has_names = reader.byte()? != 0;
        let packed = reader.bytes()?;
        if !has_names {
            return packed.is_empty().then_some(Self::Tombstone);
        }
        is_packed_list_or_empty(packed).then(|| Self::Names(PackedNames(packed.to_vec())))
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

    fn touches(&self, mut slots: Range<u32>) -> bool {
        slots.any(|slot| self.entries.contains_key(&slot))
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

#[derive(Debug, Clone)]
struct SegmentNames {
    file: Arc<MappedNames>,
    slots: Range<u32>,
}

impl SegmentNames {
    fn slot_of(&self, ordinal: u32) -> Option<u32> {
        let slot = self.slots.start.checked_add(ordinal)?;
        self.slots.contains(&slot).then_some(slot)
    }

    fn ordinal_of(&self, slot: u32) -> Option<u32> {
        self.slots.contains(&slot).then(|| slot - self.slots.start)
    }
}

struct PlacedSource<'store> {
    source: &'store SegmentNames,
    first_block: usize,
}

impl PlacedSource<'_> {
    fn blocks_covering(&self, slots: &Range<u32>) -> Range<usize> {
        let source_slots = &self.source.slots;
        let start = slots.start.max(source_slots.start);
        let end = slots.end.min(source_slots.end);
        if start >= end {
            return 0..0;
        }
        let ordinals = start - source_slots.start..end - source_slots.start;
        let blocks = self.source.file.blocks_covering(ordinals);
        self.first_block + blocks.start..self.first_block + blocks.end
    }

    fn blocks_within(&self, slot_ranges: &IdRanges) -> Vec<usize> {
        let indexes = 0..self.source.file.block_count();
        let inside = indexes.filter(|&index| self.block_lies_within(index, slot_ranges));
        inside.map(|index| self.first_block + index).collect()
    }

    fn block_lies_within(&self, index: usize, slot_ranges: &IdRanges) -> bool {
        let Some(span) = self.source.file.span(index) else {
            return false;
        };
        let first_slot = self.source.slots.start + span.first_ordinal;
        slot_ranges.covers(&(first_slot..first_slot + span.folder_count))
    }
}

struct LocatedFolder<'store> {
    source: &'store SegmentNames,
    ordinal: u32,
    first_block: usize,
}

impl<'store> LocatedFolder<'store> {
    fn block_in_source(&self) -> usize {
        let own_ordinal = self.ordinal..self.ordinal + 1;
        self.source.file.blocks_covering(own_ordinal).start
    }

    fn located_block(&self) -> LocatedBlock<'store> {
        LocatedBlock {
            source: self.source,
            first_block: self.first_block,
            index_in_source: self.block_in_source(),
        }
    }
}

struct LocatedBlock<'store> {
    source: &'store SegmentNames,
    first_block: usize,
    index_in_source: usize,
}

impl LocatedBlock<'_> {
    fn global_index(&self) -> usize {
        self.first_block + self.index_in_source
    }
}

#[derive(Debug, Default)]
pub(crate) struct ReadFailure {
    failed: AtomicBool,
    reported: AtomicBool,
}

impl ReadFailure {
    pub(crate) fn mark(&self) {
        self.failed.store(true, Ordering::Release);
    }

    pub(crate) fn is_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    pub(crate) fn take_unreported(&self) -> bool {
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

#[derive(Debug, Default)]
pub struct SavedNames {
    segments: Vec<SegmentNames>,
    overlay: Overlay,
    dead: IdRanges,
}

impl SavedNames {
    pub fn attach_segment(&mut self, slots: Range<u32>, file: Arc<MappedNames>) {
        self.segments.push(SegmentNames { file, slots });
    }

    pub(crate) fn read_overlay(&mut self, reader: &mut Reader<'_>) -> Option<()> {
        for _ in 0..reader.u32()? {
            let slot = reader.u32()?;
            let entry = OverlayEntry::read(reader)?;
            self.overlay.insert(slot, entry);
        }
        self.dead = IdRanges::read(reader)?;
        Some(())
    }
}

#[derive(Debug, Default)]
pub struct NamesCursor {
    last_block: Option<usize>,
}

#[derive(Debug)]
pub struct FileNameStore {
    path: PathBuf,
    segments: Vec<SegmentNames>,
    overlay: Overlay,
    dead: IdRanges,
    failure: ReadFailure,
    blocks_read: AtomicU64,
    blocks_verified: AtomicU64,
    rewrites: RewriteStats,
    unsaved: bool,
}

impl FileNameStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            segments: Vec::new(),
            overlay: Overlay::default(),
            dead: IdRanges::default(),
            failure: ReadFailure::default(),
            blocks_read: AtomicU64::new(0),
            blocks_verified: AtomicU64::new(0),
            rewrites: RewriteStats::default(),
            unsaved: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_available(&self) -> bool {
        let holds_names = self.has_saved_names() || self.overlay.stats().folders > 0;
        holds_names && !self.failure.is_failed()
    }

    pub fn has_saved_names(&self) -> bool {
        !self.segments.is_empty()
    }

    pub fn attach_segment(&mut self, slots: Range<u32>, file: Arc<MappedNames>) {
        self.segments.push(SegmentNames { file, slots });
        self.unsaved = true;
    }

    pub fn overlay_stats(&self) -> OverlayStats {
        self.overlay.stats()
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.unsaved
    }

    pub fn mark_saved(&mut self) {
        self.unsaved = false;
    }

    pub fn counters(&self) -> NamesCounters {
        NamesCounters {
            blocks_read: self.blocks_read.load(Ordering::Relaxed),
            blocks_verified: self.blocks_verified.load(Ordering::Relaxed),
            blocks_rebuilt: self.rewrites.blocks_rebuilt,
            blocks_copied: self.rewrites.blocks_copied,
        }
    }

    fn sources(&self) -> impl Iterator<Item = &SegmentNames> {
        self.segments.iter()
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
            let ordinal = placed.source.ordinal_of(slot)?;
            Some(LocatedFolder {
                source: placed.source,
                ordinal,
                first_block: placed.first_block,
            })
        })
    }

    pub fn reusable_block_at(&self, slot: u32) -> Option<ReusableBlock> {
        let folder = self.locate(slot)?;
        let index_in_source = folder.source.file.block_starting_at(folder.ordinal)?;
        let span = folder.source.file.span(index_in_source)?;
        let block = ReusableBlock {
            index: folder.first_block + index_in_source,
            first_slot: slot,
            folder_count: span.folder_count,
        };
        let is_unchanged =
            !self.dead.intersects(&block.slots()) && !self.overlay.touches(block.slots());
        is_unchanged.then_some(block)
    }

    pub fn copied_block(&self, index: usize) -> Result<StoredBlock<'_>, NamesUnreadable> {
        let located = self.locate_block(index).ok_or(NamesUnreadable)?;
        self.read_located(&located)?;
        located
            .source
            .file
            .block(located.index_in_source)
            .ok_or(NamesUnreadable)
    }

    fn locate_block(&self, index: usize) -> Option<LocatedBlock<'_>> {
        self.placed_sources().find_map(|placed| {
            let index_in_source = index.checked_sub(placed.first_block)?;
            let is_in_source = index_in_source < placed.source.file.block_count();
            is_in_source.then_some(LocatedBlock {
                source: placed.source,
                first_block: placed.first_block,
                index_in_source,
            })
        })
    }

    pub fn block_count(&self) -> usize {
        self.sources().map(|source| source.file.block_count()).sum()
    }

    pub fn blocks_for_slots(&self, slots: &[u32]) -> Vec<usize> {
        let single_slots: Vec<Range<u32>> = slots.iter().map(|&slot| slot..slot + 1).collect();
        self.blocks_covering(&single_slots)
    }

    pub fn blocks_covering(&self, slot_ranges: &[Range<u32>]) -> Vec<usize> {
        let mut blocks: Vec<usize> = self
            .placed_sources()
            .flat_map(|placed| {
                slot_ranges
                    .iter()
                    .flat_map(move |slots| placed.blocks_covering(slots))
            })
            .collect();
        blocks.sort_unstable();
        blocks.dedup();
        blocks
    }

    pub fn blocks_within(&self, slot_ranges: &IdRanges) -> Vec<usize> {
        let placed = self.placed_sources();
        placed
            .flat_map(|source| source.blocks_within(slot_ranges))
            .collect()
    }

    pub fn dead_slots(&self) -> &IdRanges {
        &self.dead
    }

    pub fn read_block(&self, index: usize) -> Result<BlockNames<'_>, NamesUnreadable> {
        let block = self.locate_block(index).ok_or(NamesUnreadable)?;
        let view = self.read_located(&block)?;
        Ok(BlockNames {
            store: self,
            source: block.source,
            view,
        })
    }

    fn read_located<'store>(
        &self,
        block: &LocatedBlock<'store>,
    ) -> Result<BlockView<'store>, NamesUnreadable> {
        self.blocks_read.fetch_add(1, Ordering::Relaxed);
        self.view_of(block)
    }

    fn view_of<'store>(
        &self,
        block: &LocatedBlock<'store>,
    ) -> Result<BlockView<'store>, NamesUnreadable> {
        let touched = block
            .source
            .file
            .touch_block(block.index_in_source)
            .map_err(|corrupt| self.unreadable(corrupt))?;
        if touched.newly_verified {
            self.blocks_verified.fetch_add(1, Ordering::Relaxed);
        }
        Ok(touched.view)
    }

    fn unreadable(&self, _corrupt: NamesCorrupt) -> NamesUnreadable {
        self.failure.mark();
        NamesUnreadable
    }

    pub fn visit_overlay(&self, visit: &mut dyn FnMut(FolderNames<'_>)) {
        let named = self.overlay.entries.iter().filter_map(|(&slot, entry)| {
            let names = entry.names()?;
            Some(FolderNames::live(slot, names.as_bytes()))
        });
        for folder in named {
            visit(folder);
        }
    }

    pub fn count_saved_and_overlay_names(&self) -> Result<u64, NamesUnreadable> {
        let mut count = 0;
        self.visit_overlay(&mut |folder| count += folder.name_count());
        for index in 0..self.block_count() {
            let block = self.read_block(index)?;
            count += block
                .folders()
                .map(|folder| folder.name_count())
                .sum::<u64>();
        }
        Ok(count)
    }

    pub fn names_of(&self, slot: u32) -> Result<Option<PackedNames>, NamesUnreadable> {
        if let Some(entry) = self.overlay.get(slot) {
            return Ok(Some(PackedNames(entry.packed().to_vec())));
        }
        if self.dead.contains(slot) {
            return Ok(Some(PackedNames::default()));
        }
        self.block_names_of(slot)
    }

    fn block_names_of(&self, slot: u32) -> Result<Option<PackedNames>, NamesUnreadable> {
        let Some(folder) = self.locate(slot) else {
            return Ok(None);
        };
        let view = self.read_located(&folder.located_block())?;
        let packed = view.names_of(folder.ordinal).unwrap_or_default();
        Ok(Some(PackedNames(packed.to_vec())))
    }

    fn overrides(&self, slot: u32) -> bool {
        self.overlay.get(slot).is_some() || self.dead.contains(slot)
    }

    pub fn take_unreported_failure(&self) -> bool {
        self.failure.take_unreported()
    }

    pub fn mark_unreadable(&self) {
        self.failure.mark();
    }

    pub fn set_names(&mut self, slot: u32, names: PackedNames) {
        self.overlay.insert(slot, OverlayEntry::Names(names));
        self.unsaved = true;
    }

    pub fn tombstone(&mut self, slot: u32) {
        self.unsaved = true;
        let shadows_saved_names = self.locate(slot).is_some();
        if !shadows_saved_names {
            self.overlay.remove(slot);
            return;
        }
        self.overlay.insert(slot, OverlayEntry::Tombstone);
    }

    pub fn tombstone_range(&mut self, slots: Range<u32>) {
        self.overlay.remove_range(&slots);
        self.dead.add(slots);
        self.unsaved = true;
    }

    pub fn remove_legacy_files(&self) -> io::Result<()> {
        remove_if_present(&spill_path(&self.path))?;
        remove_if_present(&self.path)
    }

    pub fn fail_and_discard(&mut self) {
        self.failure.mark_handled();
        self.segments.clear();
    }

    pub fn restore(&mut self, saved: SavedNames) {
        let SavedNames {
            segments,
            overlay,
            dead,
        } = saved;
        self.segments = segments;
        self.overlay = overlay;
        self.dead = dead;
        self.unsaved = false;
        self.failure.clear();
    }

    pub fn adopt_base(&mut self, base: SavedNames, stats: RewriteStats) {
        self.rewrites.absorb(stats);
        self.restore(base);
    }

    pub(crate) fn write_overlay(&self, out: &mut Vec<u8>) {
        let mut slots: Vec<u32> = self.overlay.entries.keys().copied().collect();
        slots.sort_unstable();
        put_count(out, slots.len());
        for slot in slots {
            put_u32(out, slot);
            self.overlay.entries[&slot].write(out);
        }
        self.dead.write(out);
    }

    pub fn names_for_rewrite(
        &self,
        slot: u32,
        cursor: &mut NamesCursor,
    ) -> Result<&[u8], NamesUnreadable> {
        if let Some(entry) = self.overlay.get(slot) {
            return Ok(entry.packed());
        }
        if self.dead.contains(slot) {
            return Ok(&[]);
        }
        let Some(folder) = self.locate(slot) else {
            return Ok(&[]);
        };
        self.cached_block_names(&folder, cursor)
    }

    fn cached_block_names<'store>(
        &'store self,
        folder: &LocatedFolder<'store>,
        cursor: &mut NamesCursor,
    ) -> Result<&'store [u8], NamesUnreadable> {
        let block = folder.located_block();
        let view = self.cursor_view(&block, cursor)?;
        Ok(view.names_of(folder.ordinal).unwrap_or_default())
    }

    fn cursor_view<'store>(
        &self,
        block: &LocatedBlock<'store>,
        cursor: &mut NamesCursor,
    ) -> Result<BlockView<'store>, NamesUnreadable> {
        let index = block.global_index();
        if cursor.last_block == Some(index) {
            return self.view_of(block);
        }
        cursor.last_block = None;
        let view = self.read_located(block)?;
        cursor.last_block = Some(index);
        Ok(view)
    }
}

pub struct BlockNames<'read> {
    store: &'read FileNameStore,
    source: &'read SegmentNames,
    view: BlockView<'read>,
}

impl<'read> BlockNames<'read> {
    pub fn span(&self) -> BlockSpan {
        self.view.span
    }

    pub fn folders(&self) -> impl Iterator<Item = FolderNames<'read>> + '_ {
        let first_ordinal = self.view.span.first_ordinal;
        self.view.folders().filter_map(move |folder| {
            let slot = self.source.slot_of(first_ordinal + folder.relative_id)?;
            let shadowed = self.store.overrides(slot);
            (!shadowed).then_some(FolderNames::stored(slot, folder))
        })
    }
}
