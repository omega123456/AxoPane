use std::cmp::Ordering as NameOrder;
use std::fs::File;
use std::ops::Range;
use std::sync::Arc;

use memmap2::Mmap;

use super::format::{
    decode_range, le_u32, le_u64, Cell, CellRun, Column, Layout, NodeFlags, OpenedTree,
    SegmentPaths, TreeHeader, NO_PARENT,
};
use crate::size::index::names::block;
use crate::size::index::names::file::MappedNames;
use crate::size::index::verified::VerifiedSet;

const CHILD_ID_BYTES: usize = size_of::<u32>();
const FIRST_ORDINAL: u32 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentCorrupt;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SegmentRow {
    pub parent: Option<u32>,
    pub direct: u64,
    pub total: u64,
    pub pending: u32,
    pub skipped: u32,
    pub denied: u32,
    pub flags: NodeFlags,
    pub subtree_end: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChildIds<'segment> {
    bytes: &'segment [u8],
}

impl<'segment> ChildIds<'segment> {
    pub fn len(&self) -> usize {
        self.bytes.len() / CHILD_ID_BYTES
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn get(&self, position: usize) -> Option<u32> {
        let start = position.checked_mul(CHILD_ID_BYTES)?;
        le_u32(self.bytes.get(start..start.checked_add(CHILD_ID_BYTES)?)?)
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = u32> + 'segment {
        self.bytes.chunks_exact(CHILD_ID_BYTES).filter_map(le_u32)
    }
}

#[derive(Debug)]
pub struct MappedSegment {
    map: Mmap,
    header: TreeHeader,
    layout: Layout,
    verified: VerifiedSet,
    names: Arc<MappedNames>,
}

impl MappedSegment {
    pub fn open(paths: &SegmentPaths) -> Option<Self> {
        let file = File::open(&paths.tree).ok()?;
        // SAFETY: a segment file is written once under a temp name and never changed after its rename.
        let map = unsafe { Mmap::map(&file) }.ok()?;
        let OpenedTree { header, layout } = TreeHeader::open(&map)?;
        let names = MappedNames::open(&paths.names)?;
        let names_header = names.header();
        let same_token = names_header.pair_token == header.token;
        let same_folders = names_header.folder_count == header.counts.nodes;
        if !(same_token && same_folders) {
            return None;
        }
        Some(Self {
            verified: VerifiedSet::none_of(layout.chunk_count()),
            map,
            header,
            layout,
            names: Arc::new(names),
        })
    }

    pub fn node_count(&self) -> u32 {
        self.header.counts.nodes
    }

    pub fn token(&self) -> u64 {
        self.header.token
    }

    pub fn file_names(&self) -> &MappedNames {
        &self.names
    }

    pub fn shared_file_names(&self) -> Arc<MappedNames> {
        Arc::clone(&self.names)
    }

    pub fn top_level(&self) -> Result<Vec<u32>, SegmentCorrupt> {
        let mut ids = Vec::with_capacity(self.header.counts.top_level as usize);
        let mut next = 0;
        while next < self.node_count() {
            ids.push(next);
            next = self.subtree(next)?.end;
        }
        Ok(ids)
    }

    pub fn row(&self, id: u32) -> Result<SegmentRow, SegmentCorrupt> {
        let parent = self.u32_at(Column::Parent.at(id))?;
        let parent_precedes = parent == NO_PARENT || parent < id;
        if !parent_precedes {
            return Err(SegmentCorrupt);
        }
        let flag_bits = self.verified(self.cell(Column::Flags.at(id))?)?[0];
        Ok(SegmentRow {
            parent: (parent != NO_PARENT).then_some(parent),
            direct: self.u64_at(Column::Direct.at(id))?,
            total: self.u64_at(Column::Total.at(id))?,
            pending: self.u32_at(Column::Pending.at(id))?,
            skipped: self.u32_at(Column::Skipped.at(id))?,
            denied: self.u32_at(Column::Denied.at(id))?,
            flags: NodeFlags::from_bits(flag_bits).ok_or(SegmentCorrupt)?,
            subtree_end: self.subtree(id)?.end,
        })
    }

    pub fn subtree(&self, id: u32) -> Result<Range<u32>, SegmentCorrupt> {
        let end = self.u32_at(Column::SubtreeEnd.at(id))?;
        let in_bounds = end > id && end <= self.node_count();
        if !in_bounds {
            return Err(SegmentCorrupt);
        }
        Ok(id..end)
    }

    pub fn name(&self, id: u32) -> Result<&str, SegmentCorrupt> {
        let next = id.checked_add(1).ok_or(SegmentCorrupt)?;
        let start = self.u32_at(Column::NameEnds.at(id))?;
        let end = self.u32_at(Column::NameEnds.at(next))?;
        let bytes = self.verified_run(CellRun {
            column: Column::NameHeap,
            first: start,
            count: end.checked_sub(start).ok_or(SegmentCorrupt)?,
        })?;
        std::str::from_utf8(bytes).map_err(|_| SegmentCorrupt)
    }

    pub fn signature(&self, id: u32) -> Result<u32, SegmentCorrupt> {
        self.u32_at(Column::Signature.at(id))
    }

    pub fn hidden_ranges(&self) -> Result<Vec<Range<u32>>, SegmentCorrupt> {
        let mut ranges: Vec<Range<u32>> = Vec::new();
        for index in 0..self.header.counts.hidden_ranges {
            let previous_end = ranges.last().map_or(FIRST_ORDINAL, |previous| previous.end);
            ranges.push(self.hidden_range_after(index, previous_end)?);
        }
        Ok(ranges)
    }

    fn hidden_range_after(
        &self,
        index: u32,
        previous_end: u32,
    ) -> Result<Range<u32>, SegmentCorrupt> {
        let range = self.hidden_range(index)?;
        let follows_previous = range.start >= previous_end;
        let spans_one_subtree = self.subtree(range.start)? == range;
        let is_well_placed = follows_previous && spans_one_subtree;
        if !is_well_placed {
            return Err(SegmentCorrupt);
        }
        Ok(range)
    }

    fn hidden_range(&self, index: u32) -> Result<Range<u32>, SegmentCorrupt> {
        let bytes = self.verified(self.cell(Column::HiddenRanges.at(index))?)?;
        decode_range(bytes).ok_or(SegmentCorrupt)
    }

    pub fn children(&self, id: u32) -> Result<ChildIds<'_>, SegmentCorrupt> {
        let bytes = self.verified_run(CellRun {
            column: Column::Children,
            first: self.u32_at(Column::ChildOffset.at(id))?,
            count: self.u32_at(Column::ChildCount.at(id))?,
        })?;
        Ok(ChildIds { bytes })
    }

    pub fn child_named(&self, id: u32, name: &str) -> Result<Option<u32>, SegmentCorrupt> {
        let children = self.children(id)?;
        let mut candidates = 0..children.len();
        while !candidates.is_empty() {
            let middle = candidates.start + candidates.len() / 2;
            let child = children.get(middle).ok_or(SegmentCorrupt)?;
            match self.name(child)?.cmp(name) {
                NameOrder::Equal => return Ok(Some(child)),
                NameOrder::Less => candidates.start = middle + 1,
                NameOrder::Greater => candidates.end = middle,
            }
        }
        Ok(None)
    }

    fn u32_at(&self, cell: Cell) -> Result<u32, SegmentCorrupt> {
        le_u32(self.verified(self.cell(cell)?)?).ok_or(SegmentCorrupt)
    }

    fn u64_at(&self, cell: Cell) -> Result<u64, SegmentCorrupt> {
        le_u64(self.verified(self.cell(cell)?)?).ok_or(SegmentCorrupt)
    }

    fn cell(&self, cell: Cell) -> Result<Range<usize>, SegmentCorrupt> {
        self.layout.cell(cell).ok_or(SegmentCorrupt)
    }

    fn verified_run(&self, run: CellRun) -> Result<&[u8], SegmentCorrupt> {
        self.verified(self.layout.run(run).ok_or(SegmentCorrupt)?)
    }

    fn verified(&self, bytes: Range<usize>) -> Result<&[u8], SegmentCorrupt> {
        for chunk in self.layout.chunks_covering(&bytes) {
            self.verify_chunk(chunk)?;
        }
        Ok(&self.map[bytes])
    }

    fn verify_chunk(&self, chunk: usize) -> Result<(), SegmentCorrupt> {
        if self.verified.contains(chunk) {
            return Ok(());
        }
        let stored = le_u64(&self.map[self.layout.chunk_checksum(chunk)]).ok_or(SegmentCorrupt)?;
        if block::checksum(&self.map[self.layout.chunk(chunk)]) != stored {
            return Err(SegmentCorrupt);
        }
        self.verified.insert(chunk);
        Ok(())
    }
}
