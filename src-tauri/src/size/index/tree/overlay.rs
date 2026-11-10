use std::ops::Range;

use super::{NodeId, OwnCounters, HEAP_BIT, NONE};

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

    pub(super) fn add(&mut self, delta: Delta) {
        let changed = self.facts().with_delta(delta);
        self.total = changed.total;
        self.pending = changed.pending;
        self.skipped = changed.skipped;
        self.denied = changed.denied;
    }

    fn reset_to_own_totals(&mut self) {
        let own = OwnCounters::of(self.flags);
        self.total = self.direct;
        self.pending = own.pending;
        self.skipped = own.skipped;
        self.denied = own.denied;
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
    pub(super) fn id_of_order(order: u32) -> Option<NodeId> {
        (order < HEAP_BIT).then_some(order | HEAP_BIT)
    }

    pub(super) fn ids(&self) -> Range<NodeId> {
        HEAP_BIT..self.next_id()
    }

    pub(super) fn next_id(&self) -> NodeId {
        id_at(self.nodes.len())
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
            return self.push(node);
        };
        *self.get_mut(reused) = node;
        reused
    }

    pub(super) fn push(&mut self, node: HeapNode) -> NodeId {
        let id = self.next_id();
        self.nodes.push(node);
        id
    }

    pub(super) fn release(&mut self, id: NodeId) -> HeapNode {
        self.free.push(id);
        std::mem::take(self.get_mut(id))
    }

    pub(super) fn roll_up_imported(&mut self) {
        for node in &mut self.nodes {
            node.reset_to_own_totals();
        }
        for index in (0..self.nodes.len()).rev() {
            self.roll_up_into_parent(index);
        }
    }

    fn roll_up_into_parent(&mut self, index: usize) {
        let child = &self.nodes[index];
        if child.parent == NONE {
            return;
        }
        let subtree = Delta::of_subtree(&child.facts());
        let parent = child.parent;
        self.get_mut(parent).add(subtree);
    }
}

#[derive(Debug, Default, Clone)]
pub(super) struct Patch {
    pub(super) own: Option<OwnSize>,
    pub(super) delta: Delta,
    pub(super) children: Option<Vec<NodeId>>,
    pub(super) dirty: bool,
}

impl Patch {
    pub(super) fn changes_nothing(&self) -> bool {
        let keeps_base_size = self.own.is_none() && self.delta == Delta::default();
        keeps_base_size && self.children.is_none() && !self.dirty
    }

    pub(super) fn applied_to(&self, base: Facts) -> Facts {
        let own = self.own.unwrap_or(base.own());
        let patched = Facts {
            direct: own.direct,
            flags: own.flags,
            ..base
        };
        patched.with_delta(self.delta)
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct DeadRanges {
    ranges: Vec<Range<NodeId>>,
    dead_folders: usize,
}

impl DeadRanges {
    pub(crate) fn len(&self) -> usize {
        self.ranges.len()
    }

    pub(crate) fn dead_folders(&self) -> usize {
        self.dead_folders
    }

    pub(crate) fn contains(&self, id: NodeId) -> bool {
        let after = self.ranges.partition_point(|range| range.start <= id);
        let candidate = after.checked_sub(1).map(|position| &self.ranges[position]);
        candidate.is_some_and(|range| range.contains(&id))
    }

    pub(crate) fn add(&mut self, dead: Range<NodeId>) {
        let first = self.ranges.partition_point(|range| range.end < dead.start);
        let after = self.ranges.partition_point(|range| range.start <= dead.end);
        let mut joined = dead;
        for touching in self.ranges.drain(first..after) {
            self.dead_folders -= touching.len();
            joined = joined.start.min(touching.start)..joined.end.max(touching.end);
        }
        self.dead_folders += joined.len();
        self.ranges.insert(first, joined);
    }
}
