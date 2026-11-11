use std::ops::Range;

use super::{NodeId, OwnCounters, HEAP_BIT, NONE, PERSISTED_FLAGS};
use crate::size::index::names::signature::signature_of;
use crate::size::index::persist::{
    put_count, put_ids, put_ranges, put_str, put_u32, put_u64, Reader,
};

pub const OVERLAY_CAP_FOLDERS: usize = 100_000;
pub const OVERLAY_CAP_DEAD_RANGES: usize = 10_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TreeOverlayStats {
    pub heap_folders: usize,
    pub patched_folders: usize,
    pub dead_ranges: usize,
}

impl TreeOverlayStats {
    pub fn exceeds_cap(&self) -> bool {
        let folders = self.heap_folders + self.patched_folders;
        folders > OVERLAY_CAP_FOLDERS || self.dead_ranges > OVERLAY_CAP_DEAD_RANGES
    }
}

pub(super) fn is_heap(id: NodeId) -> bool {
    id & HEAP_BIT != 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OwnSize {
    pub(super) direct: u64,
    pub(super) flags: u8,
}

impl OwnSize {
    fn write(&self, out: &mut Vec<u8>) {
        put_u64(out, self.direct);
        out.push(self.flags);
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        let direct = reader.u64()?;
        let flags = reader.byte()?;
        let has_unknown_flags = flags & !PERSISTED_FLAGS != 0;
        (!has_unknown_flags).then_some(Self { direct, flags })
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct Delta {
    pub(super) bytes: i64,
    pub(super) pending: i32,
    pub(super) skipped: i32,
    pub(super) denied: i32,
}

impl Delta {
    pub(super) fn own(own: OwnSize) -> Self {
        let counters = OwnCounters::of(own.flags);
        Self {
            bytes: own.direct as i64,
            pending: counters.pending as i32,
            skipped: counters.skipped as i32,
            denied: counters.denied as i32,
        }
    }

    pub(super) fn of_subtree(facts: &Facts) -> Self {
        Self {
            bytes: facts.total as i64,
            pending: facts.pending as i32,
            skipped: facts.skipped as i32,
            denied: facts.denied as i32,
        }
    }

    pub(super) fn minus(self, other: Self) -> Self {
        Self {
            bytes: self.bytes.wrapping_sub(other.bytes),
            pending: self.pending - other.pending,
            skipped: self.skipped - other.skipped,
            denied: self.denied - other.denied,
        }
    }

    pub(super) fn plus(self, other: Self) -> Self {
        Self {
            bytes: self.bytes.wrapping_add(other.bytes),
            pending: self.pending + other.pending,
            skipped: self.skipped + other.skipped,
            denied: self.denied + other.denied,
        }
    }

    fn write(&self, out: &mut Vec<u8>) {
        put_u64(out, self.bytes as u64);
        put_u32(out, self.pending as u32);
        put_u32(out, self.skipped as u32);
        put_u32(out, self.denied as u32);
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        Some(Self {
            bytes: reader.u64()? as i64,
            pending: reader.u32()? as i32,
            skipped: reader.u32()? as i32,
            denied: reader.u32()? as i32,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Facts {
    pub(super) parent: NodeId,
    pub(super) direct: u64,
    pub(super) total: u64,
    pub(super) pending: u32,
    pub(super) skipped: u32,
    pub(super) denied: u32,
    pub(super) flags: u8,
}

impl Facts {
    pub(super) fn absent() -> Self {
        Self {
            parent: NONE,
            direct: 0,
            total: 0,
            pending: 0,
            skipped: 0,
            denied: 0,
            flags: 0,
        }
    }

    pub(super) fn own(&self) -> OwnSize {
        OwnSize {
            direct: self.direct,
            flags: self.flags,
        }
    }

    pub(super) fn with_delta(mut self, delta: Delta) -> Self {
        self.total = self.total.wrapping_add_signed(delta.bytes);
        self.pending = self.pending.wrapping_add_signed(delta.pending);
        self.skipped = self.skipped.wrapping_add_signed(delta.skipped);
        self.denied = self.denied.wrapping_add_signed(delta.denied);
        self
    }
}

#[derive(Debug, Default)]
pub(super) struct HeapNode {
    pub(super) parent: NodeId,
    pub(super) name: Box<str>,
    pub(super) children: Vec<NodeId>,
    pub(super) direct: u64,
    pub(super) total: u64,
    pub(super) pending: u32,
    pub(super) skipped: u32,
    pub(super) denied: u32,
    pub(super) flags: u8,
    pub(super) dirty: bool,
}

impl HeapNode {
    pub(super) fn named(parent: NodeId, name: Box<str>) -> Self {
        Self {
            parent,
            name,
            ..Self::default()
        }
    }

    pub(super) fn facts(&self) -> Facts {
        Facts {
            parent: self.parent,
            direct: self.direct,
            total: self.total,
            pending: self.pending,
            skipped: self.skipped,
            denied: self.denied,
            flags: self.flags,
        }
    }

    pub(super) fn signature(&self) -> u32 {
        signature_of(self.name.as_bytes())
    }

    pub(super) fn add(&mut self, delta: Delta) {
        let changed = self.facts().with_delta(delta);
        self.total = changed.total;
        self.pending = changed.pending;
        self.skipped = changed.skipped;
        self.denied = changed.denied;
    }

    fn write(&self, out: &mut Vec<u8>) {
        put_u32(out, self.parent);
        put_str(out, &self.name);
        self.facts().own().write(out);
        Delta::of_subtree(&self.facts()).write(out);
        put_ids(out, &self.children);
    }

    fn read(reader: &mut Reader<'_>) -> Option<Self> {
        let parent = reader.u32()?;
        let name = reader.string()?;
        let own = OwnSize::read(reader)?;
        let subtree = Delta::read(reader)?;
        let children = reader.ids()?;
        if name.is_empty() {
            return None;
        }
        let mut node = Self {
            direct: own.direct,
            flags: own.flags,
            children,
            ..Self::named(parent, name.into())
        };
        node.add(subtree);
        Some(node)
    }
}

fn index_of(id: NodeId) -> usize {
    (id & !HEAP_BIT) as usize
}

fn id_at(index: usize) -> NodeId {
    index as NodeId | HEAP_BIT
}

#[derive(Debug, Default)]
pub(super) struct HeapNodes {
    nodes: Vec<HeapNode>,
    free: Vec<NodeId>,
}

impl HeapNodes {
    pub(super) fn ids(&self) -> Range<NodeId> {
        HEAP_BIT..id_at(self.nodes.len())
    }

    pub(super) fn live_count(&self) -> usize {
        self.nodes.len() - self.free.len()
    }

    pub(super) fn find(&self, id: NodeId) -> Option<&HeapNode> {
        if !is_heap(id) {
            return None;
        }
        self.nodes.get(index_of(id))
    }

    pub(super) fn find_mut(&mut self, id: NodeId) -> Option<&mut HeapNode> {
        if !is_heap(id) {
            return None;
        }
        self.nodes.get_mut(index_of(id))
    }

    pub(super) fn get_mut(&mut self, id: NodeId) -> &mut HeapNode {
        &mut self.nodes[index_of(id)]
    }

    pub(super) fn alloc(&mut self, node: HeapNode) -> NodeId {
        let Some(reused) = self.free.pop() else {
            self.nodes.push(node);
            return id_at(self.nodes.len() - 1);
        };
        *self.get_mut(reused) = node;
        reused
    }

    pub(super) fn release(&mut self, id: NodeId) -> HeapNode {
        self.free.push(id);
        std::mem::take(self.get_mut(id))
    }

    pub(super) fn live(&self) -> impl Iterator<Item = (NodeId, &HeapNode)> {
        let numbered = self.nodes.iter().enumerate();
        numbered
            .filter(|(_, node)| !node.name.is_empty())
            .map(|(index, node)| (id_at(index), node))
    }

    pub(super) fn write(&self, out: &mut Vec<u8>) {
        put_count(out, self.live_count());
        for (id, node) in self.live() {
            put_u32(out, id);
            node.write(out);
        }
        put_ids(out, &self.free);
    }

    pub(super) fn read(reader: &mut Reader<'_>) -> Option<Self> {
        let live = (0..reader.u32()?)
            .map(|_| Some((reader.u32()?, HeapNode::read(reader)?)))
            .collect::<Option<Vec<_>>>()?;
        let free = reader.ids()?;
        let slot_count = live.len() + free.len();
        let mut heap = Self {
            nodes: (0..slot_count).map(|_| HeapNode::default()).collect(),
            free,
        };
        for (id, node) in live {
            *heap.find_mut(id)? = node;
        }
        let every_free_slot_is_empty = heap
            .free
            .iter()
            .all(|&id| heap.find(id).is_some_and(|node| node.name.is_empty()));
        every_free_slot_is_empty.then_some(heap)
    }
}

#[derive(Debug, Default, Clone)]
pub(super) struct Patch {
    pub(super) own: Option<OwnSize>,
    pub(super) delta: Delta,
    pub(super) children: Option<Vec<NodeId>>,
    pub(super) parent: Option<NodeId>,
    pub(super) dirty: bool,
}

impl Patch {
    pub(super) fn changes_nothing(&self) -> bool {
        let keeps_base_size = self.own.is_none() && self.delta == Delta::default();
        let keeps_base_links = self.children.is_none() && self.parent.is_none();
        keeps_base_size && keeps_base_links && !self.dirty
    }

    pub(super) fn applied_to(&self, base: Facts) -> Facts {
        let own = self.own.unwrap_or(base.own());
        let patched = Facts {
            parent: self.parent.unwrap_or(base.parent),
            direct: own.direct,
            flags: own.flags,
            ..base
        };
        patched.with_delta(self.delta)
    }

    pub(super) fn write(&self, out: &mut Vec<u8>) {
        self.delta.write(out);
        out.push(u8::from(self.own.is_some()));
        if let Some(own) = self.own {
            own.write(out);
        }
        out.push(u8::from(self.children.is_some()));
        if let Some(children) = &self.children {
            put_ids(out, children);
        }
        out.push(u8::from(self.parent.is_some()));
        if let Some(parent) = self.parent {
            put_u32(out, parent);
        }
    }

    pub(super) fn read(reader: &mut Reader<'_>) -> Option<Self> {
        let delta = Delta::read(reader)?;
        let own = read_if_present(reader, OwnSize::read)?;
        let children = read_if_present(reader, |reader| reader.ids())?;
        let parent = read_if_present(reader, |reader| reader.u32())?;
        Some(Self {
            own,
            delta,
            children,
            parent,
            dirty: false,
        })
    }
}

fn read_if_present<'bytes, T>(
    reader: &mut Reader<'bytes>,
    read: impl FnOnce(&mut Reader<'bytes>) -> Option<T>,
) -> Option<Option<T>> {
    let is_present = reader.byte()? != 0;
    if !is_present {
        return Some(None);
    }
    read(reader).map(Some)
}

#[derive(Debug, Default, Clone)]
pub struct IdRanges {
    ranges: Vec<Range<NodeId>>,
    covered_ids: usize,
}

impl IdRanges {
    pub(crate) fn len(&self) -> usize {
        self.ranges.len()
    }

    pub(crate) fn covered_ids(&self) -> usize {
        self.covered_ids
    }

    pub fn ranges(&self) -> &[Range<NodeId>] {
        &self.ranges
    }

    pub fn intersects(&self, ids: &Range<NodeId>) -> bool {
        let first_reaching = self.ranges.partition_point(|range| range.end <= ids.start);
        self.ranges
            .get(first_reaching)
            .is_some_and(|range| range.start < ids.end)
    }

    pub fn contains(&self, id: NodeId) -> bool {
        self.range_holding(id)
            .is_some_and(|range| range.contains(&id))
    }

    pub fn covers(&self, ids: &Range<NodeId>) -> bool {
        self.range_holding(ids.start)
            .is_some_and(|range| reaches_across(range, ids))
    }

    fn range_holding(&self, id: NodeId) -> Option<&Range<NodeId>> {
        let after = self.ranges.partition_point(|range| range.start <= id);
        after.checked_sub(1).map(|position| &self.ranges[position])
    }

    pub(crate) fn add(&mut self, added: Range<NodeId>) {
        if added.is_empty() {
            return;
        }
        let first = self.ranges.partition_point(|range| range.end < added.start);
        let after = self
            .ranges
            .partition_point(|range| range.start <= added.end);
        let mut joined = added;
        for touching in self.ranges.drain(first..after) {
            self.covered_ids -= touching.len();
            joined = joined.start.min(touching.start)..joined.end.max(touching.end);
        }
        self.covered_ids += joined.len();
        self.ranges.insert(first, joined);
    }

    pub(crate) fn add_all(&mut self, other: Self) {
        for range in other.ranges {
            self.add(range);
        }
    }

    pub(crate) fn write(&self, out: &mut Vec<u8>) {
        put_ranges(out, &self.ranges);
    }

    pub(crate) fn read(reader: &mut Reader<'_>) -> Option<Self> {
        let mut read = Self::default();
        for range in reader.ranges()? {
            read.add(range);
        }
        Some(read)
    }

    pub(crate) fn last_end(&self) -> NodeId {
        self.ranges.last().map_or(0, |range| range.end)
    }
}

fn reaches_across(outer: &Range<NodeId>, inner: &Range<NodeId>) -> bool {
    let starts_inside = outer.contains(&inner.start);
    let ends_inside = outer.end >= inner.end;
    starts_inside && ends_inside
}

#[derive(Debug, Default)]
pub(super) struct KeptRanges {
    sorted: Vec<Range<NodeId>>,
}

impl KeptRanges {
    pub(super) fn of(mut ranges: Vec<Range<NodeId>>) -> Self {
        ranges.sort_by_key(|range| range.start);
        Self { sorted: ranges }
    }

    pub(super) fn uncovered_parts_of(&self, whole: Range<NodeId>) -> Vec<Range<NodeId>> {
        let mut uncovered = Vec::new();
        let mut next_start = whole.start;
        let inside = self
            .sorted
            .iter()
            .filter(|kept| kept.start >= whole.start && kept.end <= whole.end);
        for kept in inside {
            uncovered.push(next_start..kept.start);
            next_start = kept.end;
        }
        uncovered.push(next_start..whole.end);
        uncovered.retain(|part| !part.is_empty());
        uncovered
    }
}
