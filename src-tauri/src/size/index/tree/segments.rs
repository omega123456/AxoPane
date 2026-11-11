use std::ops::Range;
use std::sync::Arc;

use super::overlay::Facts;
use super::{NodeId, HEAP_BIT, NONE};
use crate::size::index::names::ReadFailure;
use crate::size::index::segment::{ChildIds, MappedSegment, SegmentCorrupt};

const UNKNOWN_SIGNATURE: u32 = 0;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Children<'tree> {
    listed: &'tree [NodeId],
    mapped: ChildIds<'tree>,
    first_mapped_id: NodeId,
}

impl<'tree> Children<'tree> {
    pub(super) fn listed(listed: &'tree [NodeId]) -> Self {
        Self {
            listed,
            ..Self::default()
        }
    }

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = NodeId> + 'tree {
        let first_mapped_id = self.first_mapped_id;
        let mapped = self
            .mapped
            .iter()
            .map(move |ordinal| first_mapped_id + ordinal);
        self.listed.iter().copied().chain(mapped)
    }
}

#[derive(Debug)]
struct Attached {
    ids: Range<NodeId>,
    segment: Arc<MappedSegment>,
}

struct Located<'table> {
    segment: &'table MappedSegment,
    first_id: NodeId,
    ordinal: u32,
}

impl Located<'_> {
    fn id_of(&self, ordinal: u32) -> NodeId {
        self.first_id + ordinal
    }
}

#[derive(Debug, Default)]
pub(super) struct SegmentTable {
    attached: Vec<Attached>,
    corruption: ReadFailure,
}

impl SegmentTable {
    pub(super) fn attach(&mut self, segment: Arc<MappedSegment>) -> Option<Range<NodeId>> {
        let start = self.end();
        let end = start
            .checked_add(segment.node_count())
            .filter(|&end| end <= HEAP_BIT)?;
        self.attached.push(Attached {
            ids: start..end,
            segment,
        });
        Some(start..end)
    }

    pub(super) fn node_count(&self) -> usize {
        self.id_ranges().map(|ids| ids.len()).sum()
    }

    pub(super) fn id_ranges(&self) -> impl Iterator<Item = Range<NodeId>> + '_ {
        self.attached.iter().map(|attached| attached.ids.clone())
    }

    pub(super) fn contains(&self, id: NodeId) -> bool {
        self.locate(id).is_some()
    }

    pub(super) fn end(&self) -> NodeId {
        self.attached.last().map_or(0, |last| last.ids.end)
    }

    pub(super) fn is_corrupt(&self) -> bool {
        self.corruption.is_failed()
    }

    pub(super) fn take_unreported_corruption(&self) -> bool {
        self.corruption.take_unreported()
    }

    pub(super) fn facts(&self, id: NodeId) -> Facts {
        let Some(located) = self.locate(id) else {
            return Facts::absent();
        };
        let row = self.checked(located.segment.row(located.ordinal));
        Facts {
            parent: row.parent.map_or(NONE, |parent| located.id_of(parent)),
            direct: row.direct,
            total: row.total,
            pending: row.pending,
            skipped: row.skipped,
            denied: row.denied,
            flags: row.flags.bits(),
        }
    }

    pub(super) fn name(&self, id: NodeId) -> &str {
        let Some(located) = self.locate(id) else {
            return "";
        };
        self.checked(located.segment.name(located.ordinal))
    }

    pub(super) fn signature(&self, id: NodeId) -> u32 {
        let Some(located) = self.locate(id) else {
            return UNKNOWN_SIGNATURE;
        };
        self.checked(located.segment.signature(located.ordinal))
    }

    pub(super) fn hidden_ranges(&self) -> impl Iterator<Item = Range<NodeId>> + '_ {
        self.attached.iter().flat_map(|attached| {
            let first_id = attached.ids.start;
            let ordinals = self.checked(attached.segment.hidden_ranges());
            ordinals
                .into_iter()
                .map(move |range| first_id + range.start..first_id + range.end)
        })
    }

    pub(super) fn children(&self, id: NodeId) -> Children<'_> {
        let Some(located) = self.locate(id) else {
            return Children::default();
        };
        Children {
            listed: &[],
            mapped: self.checked(located.segment.children(located.ordinal)),
            first_mapped_id: located.first_id,
        }
    }

    pub(super) fn child_named(&self, id: NodeId, name: &str) -> Option<NodeId> {
        let located = self.locate(id)?;
        let child = self.checked(located.segment.child_named(located.ordinal, name))?;
        Some(located.id_of(child))
    }

    pub(super) fn subtree(&self, id: NodeId) -> Range<NodeId> {
        let own_end = id + 1;
        let subtree_end = self.locate(id).map_or(own_end, |located| {
            let ordinals = self.checked(located.segment.subtree(located.ordinal));
            located.id_of(ordinals.end)
        });
        id..subtree_end.max(own_end)
    }

    fn locate(&self, id: NodeId) -> Option<Located<'_>> {
        let after = self
            .attached
            .partition_point(|attached| attached.ids.start <= id);
        let attached = self.attached.get(after.checked_sub(1)?)?;
        attached.ids.contains(&id).then(|| Located {
            segment: &attached.segment,
            first_id: attached.ids.start,
            ordinal: id - attached.ids.start,
        })
    }

    fn checked<T: Default>(&self, read: Result<T, SegmentCorrupt>) -> T {
        if read.is_err() {
            self.corruption.mark();
        }
        read.unwrap_or_default()
    }
}
