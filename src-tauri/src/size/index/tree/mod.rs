mod overlay;
mod segments;

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use overlay::{is_heap, Delta, Facts, HeapNode, HeapNodes, KeptRanges, OwnSize, Patch};
use segments::SegmentTable;

use crate::size::index::persist::{put_count, put_ids, put_u32, Reader};
use crate::size::index::segment::{MappedSegment, NodeFlags};

pub use overlay::{IdRanges, TreeOverlayStats, OVERLAY_CAP_DEAD_RANGES, OVERLAY_CAP_FOLDERS};
pub(crate) use segments::Children;

type NodeId = u32;
const NONE: NodeId = NodeId::MAX;
const HEAP_BIT: NodeId = 1 << 31;

pub(crate) const SCANNED: u8 = 1;
pub(crate) const DENIED: u8 = 2;
pub(crate) const SKIPPED: u8 = 4;
pub(crate) const PERSISTED_FLAGS: u8 = SCANNED | DENIED | SKIPPED;

const NO_FLAGS: u8 = 0;

const HIDDEN_NAME_PREFIX: &[u8] = b".";

const MAX_SKIPPED_NAMES: usize = 3;

const UNSCANNED: OwnSize = OwnSize {
    direct: 0,
    flags: NO_FLAGS,
};

const EXCLUDED: OwnSize = OwnSize {
    direct: 0,
    flags: SKIPPED,
};

pub(crate) fn is_hidden_name(name: &[u8]) -> bool {
    name.starts_with(HIDDEN_NAME_PREFIX)
}

fn denied_flag(denied: bool) -> u8 {
    if denied {
        return DENIED;
    }
    NO_FLAGS
}

fn with_skipped_flag(flags: u8, skipped: bool) -> u8 {
    if skipped {
        return flags | SKIPPED;
    }
    flags & !SKIPPED
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OwnCounters {
    pub(crate) pending: u32,
    pub(crate) skipped: u32,
    pub(crate) denied: u32,
}

impl OwnCounters {
    pub(crate) fn of(flags: u8) -> Self {
        Self {
            pending: u32::from(awaits_scan(flags)),
            skipped: u32::from(flags & SKIPPED != 0),
            denied: u32::from(flags & DENIED != 0),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LowerBound {
    pub skipped_names: Vec<String>,
    pub skipped_paths: Vec<PathBuf>,
    pub skipped_more: u32,
    pub denied_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderTotal {
    pub bytes: u64,
    pub fully_indexed: bool,
    pub scanned: bool,
    pub denied: bool,
    pub skipped: bool,
    pub lower_bound: Option<LowerBound>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExcludedFolder {
    pub slot: u32,
    pub changed: bool,
}

pub struct DirectoryListing<'listing, S> {
    pub direct_bytes: u64,
    pub subdirs: &'listing [S],
    pub denied: bool,
}

impl<'listing, S> DirectoryListing<'listing, S> {
    pub fn scanned(direct_bytes: u64, subdirs: &'listing [S]) -> Self {
        Self {
            direct_bytes,
            subdirs,
            denied: false,
        }
    }
}

#[derive(Clone, Copy)]
struct PathLookup<'path> {
    path: &'path Path,
    fold_case: bool,
}

#[derive(Clone, Copy)]
struct NameLookup<'name> {
    name: &'name str,
    fold_case: bool,
}

#[derive(Clone, Copy)]
struct GraftSite<'graft> {
    path: &'graft Path,
    kept: &'graft [PathBuf],
    graft_root: NodeId,
}

#[derive(Clone, Copy)]
struct ForestTop<'forest> {
    root: &'forest Path,
    top: &'forest Path,
    ids: &'forest Range<NodeId>,
}

#[derive(Clone, Copy)]
struct GraftedOver<'graft> {
    replaced: NodeId,
    graft_root: NodeId,
    path: &'graft Path,
}

struct RootedRest<'path> {
    root: NodeId,
    rest: std::path::Components<'path>,
}

struct UnscannedTopsWalk {
    tops: Vec<PathBuf>,
    stack: Vec<NodeId>,
}

#[derive(Debug, Clone, Copy)]
struct PendingVisit {
    id: NodeId,
    depth: usize,
}

#[derive(Debug, Default)]
pub struct DepthFirstCursor {
    pending: Vec<PendingVisit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisitedFolder<'tree> {
    pub slot: u32,
    pub depth: usize,
    pub name: &'tree str,
    pub direct: u64,
    pub flags: NodeFlags,
}

pub(crate) struct LiveNode<'tree> {
    pub(crate) name: &'tree str,
    pub(crate) parent: Option<u32>,
}

pub struct Graft<'graft> {
    pub path: &'graft Path,
    pub segment: Arc<MappedSegment>,
    pub kept: &'graft [PathBuf],
}

pub struct Forest<'forest> {
    pub root: &'forest Path,
    pub tops: &'forest [PathBuf],
    pub segment: Arc<MappedSegment>,
}

#[derive(Debug, Clone, Copy)]
struct KeptSubtree {
    root: NodeId,
    placeholder: NodeId,
    subtree: Delta,
}

#[derive(Debug, Clone, Copy)]
struct GraftPlace {
    replaced: Option<NodeId>,
    parent: NodeId,
}

#[derive(Debug, Default)]
struct Removed {
    slots: Vec<NodeId>,
    ranges: Vec<Range<NodeId>>,
}

#[derive(Debug, Default)]
pub struct FolderSizeTree {
    segments: SegmentTable,
    patches: BTreeMap<NodeId, Patch>,
    dead: IdRanges,
    heap: HeapNodes,
    roots: Vec<NodeId>,
    changed: Vec<NodeId>,
    removed: Removed,
    kept_while_replacing: KeptRanges,
    dirty: bool,
    structure_changed: bool,
}

impl FolderSizeTree {
    fn has_root(&self, path: &Path) -> bool {
        self.roots.iter().any(|&root| self.root_path(root) == path)
    }

    pub fn add_root(&mut self, path: &Path) -> bool {
        if path.as_os_str().is_empty() {
            return false;
        }
        if self.has_root(path) {
            return false;
        }
        let id = self.alloc(NONE, path.to_string_lossy().into());
        self.roots.push(id);
        self.bump(id, Delta::own(UNSCANNED));
        true
    }

    pub(crate) fn attach_segment(&mut self, segment: Arc<MappedSegment>) -> Option<Range<u32>> {
        self.segments.attach(segment)
    }

    pub fn attach_base(&mut self, segment: Arc<MappedSegment>) -> Option<Range<u32>> {
        let holds_folders = !self.roots.is_empty();
        if holds_folders {
            return None;
        }
        let top_level = segment.top_level().ok()?;
        let ids = self.attach_segment(segment)?;
        self.roots = top_level
            .into_iter()
            .map(|ordinal| ids.start + ordinal)
            .collect();
        Some(ids)
    }

    pub fn folder_name(&self, path: &Path) -> Option<&str> {
        self.find(path).map(|id| self.name(id))
    }

    pub fn install_graft(&mut self, graft: Graft<'_>) -> Option<Range<u32>> {
        let Graft {
            path,
            segment,
            kept,
        } = graft;
        let holds_one_subtree = segment.top_level().ok()?.len() == 1;
        if !holds_one_subtree {
            return None;
        }
        let place = self.graft_place(path)?;
        let ids = self.attach_segment(segment)?;
        let graft_root = ids.start;
        let site = GraftSite {
            path,
            kept,
            graft_root,
        };
        let kept_subtrees = self.kept_subtrees(place, site);
        for kept_subtree in &kept_subtrees {
            self.detach(kept_subtree.root, kept_subtree.subtree);
        }
        self.remove_replaced(place, &kept_subtrees);
        self.link_graft_root(place.parent, graft_root);
        for kept_subtree in kept_subtrees {
            self.move_under_graft(kept_subtree);
        }
        self.dirty = true;
        self.structure_changed = true;
        Some(ids)
    }

    pub fn install_forest(&mut self, forest: Forest<'_>) -> Option<Range<u32>> {
        let Forest {
            root,
            tops,
            segment,
        } = forest;
        let holds_one_root = segment.top_level().ok()?.len() == 1;
        let lies_under_a_root = self.find(root).is_some();
        if !(holds_one_root && lies_under_a_root) {
            return None;
        }
        let ids = self.attach_segment(segment)?;
        let grafted: Vec<Range<NodeId>> = tops
            .iter()
            .filter_map(|top| {
                self.graft_forest_top(ForestTop {
                    root,
                    top,
                    ids: &ids,
                })
            })
            .collect();
        let unused = KeptRanges::of(grafted).uncovered_parts_of(ids.clone());
        for dead in unused {
            self.dead.add(dead.clone());
            self.removed.ranges.push(dead);
        }
        self.dirty = true;
        self.structure_changed = true;
        Some(ids)
    }

    fn graft_forest_top(&mut self, forest_top: ForestTop<'_>) -> Option<Range<NodeId>> {
        let ForestTop { root, top, ids } = forest_top;
        let below_root = top.strip_prefix(root).ok()?;
        let top_id = self.descend(ids.start, below_root)?;
        let is_inside_the_segment = ids.contains(&top_id) && top_id != ids.start;
        if !is_inside_the_segment {
            return None;
        }
        let place = self.graft_place(top)?;
        self.remove_replaced(place, &[]);
        self.link_graft_root(place.parent, top_id);
        Some(self.segments.subtree(top_id))
    }

    fn graft_place(&mut self, path: &Path) -> Option<GraftPlace> {
        if let Some(replaced) = self.find(path) {
            return Some(GraftPlace {
                replaced: Some(replaced),
                parent: self.facts(replaced).parent,
            });
        }
        let parent = self.ensure(path.parent()?)?;
        Some(GraftPlace {
            replaced: None,
            parent,
        })
    }

    fn kept_subtrees(&self, place: GraftPlace, site: GraftSite<'_>) -> Vec<KeptSubtree> {
        let Some(replaced) = place.replaced else {
            return Vec::new();
        };
        let grafted = GraftedOver {
            replaced,
            graft_root: site.graft_root,
            path: site.path,
        };
        site.kept
            .iter()
            .filter_map(|kept_path| self.kept_subtree(grafted, kept_path))
            .collect()
    }

    fn kept_subtree(&self, grafted: GraftedOver<'_>, kept_path: &Path) -> Option<KeptSubtree> {
        let root = self.find(kept_path)?;
        let is_below_replaced = root != grafted.replaced && self.is_within(root, grafted.replaced);
        if !is_below_replaced {
            return None;
        }
        let below_graft = kept_path.strip_prefix(grafted.path).ok()?;
        let placeholder = self.descend(grafted.graft_root, below_graft)?;
        let was_walked_again = self.facts(placeholder).flags & SCANNED != 0;
        if was_walked_again || placeholder == grafted.graft_root {
            return None;
        }
        Some(KeptSubtree {
            root,
            placeholder,
            subtree: Delta::of_subtree(&self.facts(root)),
        })
    }

    fn descend(&self, from: NodeId, relative: &Path) -> Option<NodeId> {
        relative.components().try_fold(from, |id, component| {
            self.child(id, &normal_name(component)?)
        })
    }

    fn remove_replaced(&mut self, place: GraftPlace, kept_subtrees: &[KeptSubtree]) {
        let Some(replaced) = place.replaced else {
            return;
        };
        let kept_mapped_ranges = kept_subtrees
            .iter()
            .filter(|kept| !is_heap(kept.root))
            .map(|kept| self.segments.subtree(kept.root))
            .collect();
        self.kept_while_replacing = KeptRanges::of(kept_mapped_ranges);
        self.remove(replaced);
        self.kept_while_replacing = KeptRanges::default();
    }

    fn link_graft_root(&mut self, parent: NodeId, graft_root: NodeId) {
        if parent == NONE {
            self.roots.push(graft_root);
            self.mark_dirty(graft_root);
            return;
        }
        self.attach(parent, graft_root);
        self.bump(parent, Delta::of_subtree(&self.facts(graft_root)));
    }

    fn move_under_graft(&mut self, kept: KeptSubtree) {
        let KeptSubtree {
            root,
            placeholder,
            subtree,
        } = kept;
        let placeholder_facts = self.facts(placeholder);
        let parent = placeholder_facts.parent;
        let siblings = self.children_mut(parent);
        if let Some(sibling) = siblings.iter_mut().find(|sibling| **sibling == placeholder) {
            *sibling = root;
        }
        let placeholder_ids = placeholder..placeholder + 1;
        self.patches.remove(&placeholder);
        self.dead.add(placeholder_ids.clone());
        self.removed.ranges.push(placeholder_ids);
        self.set_parent(root, parent);
        let placeholder_subtree = Delta::of_subtree(&placeholder_facts);
        self.bump(parent, subtree.minus(placeholder_subtree));
    }

    fn set_parent(&mut self, id: NodeId, parent: NodeId) {
        if let Some(node) = self.heap.find_mut(id) {
            node.parent = parent;
            return;
        }
        self.patches.entry(id).or_default().parent = Some(parent);
    }

    pub fn node_count(&self) -> usize {
        self.heap.live_count() + self.segments.node_count() - self.dead.covered_ids()
    }

    pub fn overlay_stats(&self) -> TreeOverlayStats {
        TreeOverlayStats {
            heap_folders: self.heap.live_count(),
            patched_folders: self.patches.len(),
            dead_ranges: self.dead.len(),
        }
    }

    pub fn is_corrupt(&self) -> bool {
        self.segments.is_corrupt()
    }

    pub fn take_unreported_corruption(&self) -> bool {
        self.segments.take_unreported_corruption()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    pub fn structure_changed(&self) -> bool {
        self.structure_changed
    }

    pub fn mark_structure_saved(&mut self) {
        self.structure_changed = false;
    }

    pub fn take_removed_slots(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.removed.slots)
    }

    pub fn take_removed_ranges(&mut self) -> Vec<Range<u32>> {
        std::mem::take(&mut self.removed.ranges)
    }

    pub fn merge_record<S: AsRef<OsStr>>(
        &mut self,
        path: &Path,
        listing: DirectoryListing<'_, S>,
    ) -> bool {
        self.merge_record_slot(path, listing).is_some()
    }

    pub fn merge_record_slot<S: AsRef<OsStr>>(
        &mut self,
        path: &Path,
        listing: DirectoryListing<'_, S>,
    ) -> Option<u32> {
        let id = self.ensure(path)?;
        self.set_listing(id, listing);
        Some(id)
    }

    pub fn apply_listing<S: AsRef<OsStr>>(
        &mut self,
        path: &Path,
        listing: DirectoryListing<'_, S>,
    ) -> Option<Vec<PathBuf>> {
        let id = self.find(path)?;
        Some(self.set_listing(id, listing))
    }

    fn set_listing<S: AsRef<OsStr>>(
        &mut self,
        id: NodeId,
        listing: DirectoryListing<'_, S>,
    ) -> Vec<PathBuf> {
        let DirectoryListing {
            direct_bytes,
            subdirs,
            denied,
        } = listing;
        let kept_skip = self.facts(id).flags & SKIPPED;
        self.set_own(
            id,
            OwnSize {
                direct: direct_bytes,
                flags: kept_skip | SCANNED | denied_flag(denied),
            },
        );

        let mut listed: Vec<Box<str>> = subdirs
            .iter()
            .map(|subdir| subdir.as_ref().to_string_lossy().into())
            .collect();
        listed.sort();
        listed.dedup();
        let stale: Vec<NodeId> = self
            .children(id)
            .iter()
            .filter(|&child| !is_listed(&listed, self.name(child)))
            .collect();
        for child in stale {
            self.remove(child);
        }

        let base = self.path_of(id);
        let new_names: Vec<Box<str>> = listed
            .into_iter()
            .filter(|name| self.child(id, name).is_none())
            .collect();
        let mut added = Vec::new();
        for name in new_names {
            added.push(base.join(&*name));
            self.child_or_insert(id, name);
        }
        added
    }

    pub fn prune(&mut self, path: &Path) -> bool {
        self.find(path).map(|id| self.remove(id)).is_some()
    }

    pub fn rename(&mut self, from: &Path, to: &Path) -> bool {
        let Some(id) = self.find(from) else {
            return false;
        };
        let (Some(parent), Some(name)) = (
            to.parent().and_then(|parent_path| self.find(parent_path)),
            to.file_name(),
        ) else {
            return false;
        };
        let moved = self.facts(id);
        let is_fixed_in_a_segment = !is_heap(id);
        let is_root = moved.parent == NONE;
        let moves_into_itself = self.is_within(parent, id);
        if is_fixed_in_a_segment || is_root || moves_into_itself {
            return false;
        }
        let replaced = self.resolve(to, false).filter(|&existing| existing != id);
        if replaced.is_some_and(|existing| self.is_within(id, existing)) {
            return false;
        }
        if let Some(existing) = replaced {
            self.remove(existing);
        }
        let subtree = Delta::of_subtree(&moved);
        self.detach(id, subtree);
        self.heap.get_mut(id).name = name.to_string_lossy().into();
        self.attach(parent, id);
        self.bump(parent, subtree);
        self.mark_dirty(id);
        self.structure_changed = true;
        true
    }

    fn skip_target(&mut self, path: &Path, skipped: bool) -> Option<NodeId> {
        if skipped {
            return self.ensure(path);
        }
        self.find(path)
    }

    pub fn set_skipped(&mut self, path: &Path, skipped: bool) -> bool {
        let Some(id) = self.skip_target(path, skipped) else {
            return false;
        };
        let facts = self.facts(id);
        let own = OwnSize {
            direct: facts.direct,
            flags: with_skipped_flag(facts.flags, skipped),
        };
        self.set_own(id, own);
        true
    }

    pub fn exclude_subtree(&mut self, path: &Path) -> Option<ExcludedFolder> {
        let id = self.find(path)?;
        let children: Vec<NodeId> = self.children(id).iter().collect();
        let already_skipped_leaf = children.is_empty() && self.facts(id).own() == EXCLUDED;
        if already_skipped_leaf {
            return Some(ExcludedFolder {
                slot: id,
                changed: false,
            });
        }
        for child in children {
            self.remove(child);
        }
        self.set_own(id, EXCLUDED);
        Some(ExcludedFolder {
            slot: id,
            changed: true,
        })
    }

    pub fn include_subtree(&mut self, path: &Path) -> bool {
        let Some(id) = self.find(path) else {
            return false;
        };
        let is_skipped = self.facts(id).flags & SKIPPED != 0;
        if !is_skipped {
            return false;
        }
        self.set_own(id, UNSCANNED);
        true
    }

    pub fn prune_unscanned(&mut self, path: &Path) -> bool {
        let Some(id) = self.find(path) else {
            return false;
        };
        let mut stack = vec![id];
        while let Some(next) = stack.pop() {
            let children_to_visit = self.prune_if_unscanned(next);
            stack.extend(children_to_visit);
        }
        true
    }

    fn prune_if_unscanned(&mut self, id: NodeId) -> Vec<NodeId> {
        let facts = self.facts(id);
        if facts.pending == 0 {
            return Vec::new();
        }
        let is_unscanned_non_root = awaits_scan(facts.flags) && facts.parent != NONE;
        if is_unscanned_non_root {
            self.remove(id);
            return Vec::new();
        }
        self.children(id).iter().collect()
    }

    pub fn unscanned_tops(&self) -> Vec<PathBuf> {
        let mut walk = UnscannedTopsWalk {
            tops: Vec::new(),
            stack: self.roots.clone(),
        };
        while let Some(id) = walk.stack.pop() {
            self.visit_for_unscanned_tops(id, &mut walk);
        }
        walk.tops
    }

    fn visit_for_unscanned_tops(&self, id: NodeId, walk: &mut UnscannedTopsWalk) {
        let UnscannedTopsWalk { tops, stack } = walk;
        let facts = self.facts(id);
        if facts.pending == 0 {
            return;
        }
        if awaits_scan(facts.flags) {
            tops.push(self.path_of(id));
            return;
        }
        stack.extend(self.children(id).iter());
    }

    pub fn total(&self, path: &Path) -> Option<FolderTotal> {
        let id = self.find(path)?;
        let facts = self.facts(id);
        let own = OwnCounters::of(facts.flags);
        let skipped_below = facts.skipped - own.skipped;
        let denied_below = facts.denied - own.denied;
        let lower_bound = (skipped_below > 0 || denied_below > 0).then(|| {
            let mut skipped = Vec::new();
            self.collect_skipped(id, &mut skipped);
            LowerBound {
                skipped_more: skipped_below - skipped.len() as u32,
                skipped_names: skipped
                    .iter()
                    .map(|&skipped_id| self.name(skipped_id).to_string())
                    .collect(),
                skipped_paths: skipped
                    .iter()
                    .map(|&skipped_id| self.path_of(skipped_id))
                    .collect(),
                denied_count: denied_below,
            }
        });
        Some(FolderTotal {
            bytes: facts.total,
            fully_indexed: facts.pending == 0,
            scanned: facts.flags & SCANNED != 0,
            denied: facts.flags & DENIED != 0,
            skipped: facts.flags & SKIPPED != 0,
            lower_bound,
        })
    }

    pub fn indexed_subtrees_under(&self, path: &Path) -> Vec<PathBuf> {
        let Some(id) = self.find(path) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut stack = vec![(id, self.path_of(id))];
        while let Some((next, base)) = stack.pop() {
            let (indexed, pending): (Vec<_>, Vec<_>) = self
                .children(next)
                .iter()
                .map(|child| (child, base.join(self.name(child))))
                .partition(|(child, _)| self.facts(*child).pending == 0);
            out.extend(indexed.into_iter().map(|(_, child_path)| child_path));
            stack.extend(pending);
        }
        out
    }

    pub fn drain_changed<P: AsRef<Path>>(
        &mut self,
        subscribed: impl IntoIterator<Item = P>,
    ) -> Vec<PathBuf> {
        let hits = subscribed
            .into_iter()
            .filter(|path| {
                self.find(path.as_ref())
                    .is_some_and(|id| self.is_marked_dirty(id))
            })
            .map(|path| path.as_ref().to_path_buf())
            .collect();
        for id in std::mem::take(&mut self.changed) {
            self.clear_dirty(id);
        }
        hits
    }

    fn find(&self, path: &Path) -> Option<NodeId> {
        self.resolve(path, false)
            .or_else(|| self.resolve(path, true))
    }

    fn resolve(&self, path: &Path, fold_case: bool) -> Option<NodeId> {
        let lookup = PathLookup { path, fold_case };
        self.roots
            .iter()
            .find_map(|&root| self.resolve_below_root(root, lookup))
    }

    fn resolve_below_root(&self, root: NodeId, lookup: PathLookup<'_>) -> Option<NodeId> {
        let PathLookup { path, fold_case } = lookup;
        let rest = strip_root(path, self.root_path(root), fold_case)?;
        let mut id = root;
        for component in rest {
            let name = normal_name(component)?;
            id = self.child_matching(
                id,
                NameLookup {
                    name: &name,
                    fold_case,
                },
            )?;
        }
        Some(id)
    }

    fn root_path(&self, root: NodeId) -> &Path {
        Path::new(self.name(root))
    }

    fn strip_any_root<'path>(
        &self,
        path: &'path Path,
        fold_case: bool,
    ) -> Option<RootedRest<'path>> {
        self.roots.iter().find_map(|&root| {
            strip_root(path, self.root_path(root), fold_case).map(|rest| RootedRest { root, rest })
        })
    }

    fn ensure(&mut self, path: &Path) -> Option<NodeId> {
        let RootedRest { root, rest } = [false, true]
            .into_iter()
            .find_map(|fold_case| self.strip_any_root(path, fold_case))?;
        let names = rest.map(normal_name).collect::<Option<Vec<_>>>()?;
        Some(
            names
                .into_iter()
                .fold(root, |id, name| self.child_or_insert(id, name.into())),
        )
    }

    fn child_matching(&self, parent: NodeId, lookup: NameLookup<'_>) -> Option<NodeId> {
        let NameLookup { name, fold_case } = lookup;
        let exact = self.child(parent, name);
        if exact.is_some() {
            return exact;
        }
        if !fold_case {
            return None;
        }
        self.children(parent)
            .iter()
            .find(|&child| eq_fold(self.name(child), name))
    }

    fn child(&self, id: NodeId, name: &str) -> Option<NodeId> {
        let Some(listed) = self.listed_children(id) else {
            return self.segments.child_named(id, name);
        };
        listed
            .binary_search_by(|&child| self.name(child).cmp(name))
            .ok()
            .map(|position| listed[position])
    }

    fn child_or_insert(&mut self, parent: NodeId, name: Box<str>) -> NodeId {
        if let Some(existing) = self.child(parent, &name) {
            return existing;
        }
        let id = self.alloc(parent, name);
        self.attach(parent, id);
        self.bump(id, Delta::own(UNSCANNED));
        id
    }

    fn alloc(&mut self, parent: NodeId, name: Box<str>) -> NodeId {
        self.structure_changed = true;
        self.heap.alloc(HeapNode::named(parent, name))
    }

    fn attach(&mut self, parent: NodeId, id: NodeId) {
        self.list_children_in_overlay(parent);
        let name = self.name(id);
        let siblings = self.listed_children(parent).unwrap_or_default();
        let slot = siblings.partition_point(|&sibling| self.name(sibling) < name);
        self.set_parent(id, parent);
        self.children_mut(parent).insert(slot, id);
    }

    fn list_children_in_overlay(&mut self, id: NodeId) {
        self.children_mut(id);
    }

    fn detach(&mut self, id: NodeId, subtree: Delta) {
        let parent = self.facts(id).parent;
        if parent == NONE {
            self.roots.retain(|&root| root != id);
            return;
        }
        self.children_mut(parent).retain(|&child| child != id);
        self.bump(parent, Delta::default().minus(subtree));
    }

    fn remove(&mut self, id: NodeId) {
        self.detach(id, Delta::of_subtree(&self.facts(id)));
        let mut stack = vec![id];
        while let Some(next) = stack.pop() {
            let attached_below = self.release(next);
            stack.extend(attached_below);
        }
        self.dirty = true;
        self.structure_changed = true;
    }

    fn release(&mut self, id: NodeId) -> Vec<NodeId> {
        if is_heap(id) {
            self.removed.slots.push(id);
            return self.heap.release(id).children;
        }
        self.release_mapped_subtree(id)
    }

    fn release_mapped_subtree(&mut self, id: NodeId) -> Vec<NodeId> {
        let subtree = self.segments.subtree(id);
        let released = self
            .kept_while_replacing
            .uncovered_parts_of(subtree.clone());
        let mut attached_from_outside = Vec::new();
        for ids in released {
            let attached = self.release_mapped_ids(ids);
            attached_from_outside.extend(
                attached
                    .into_iter()
                    .filter(|child| !subtree.contains(child)),
            );
        }
        attached_from_outside
    }

    fn release_mapped_ids(&mut self, ids: Range<NodeId>) -> Vec<NodeId> {
        let patched: Vec<NodeId> = self
            .patches
            .range(ids.clone())
            .map(|(&patched_id, _)| patched_id)
            .collect();
        let listed_children = patched
            .into_iter()
            .filter_map(|patched_id| self.patches.remove(&patched_id)?.children)
            .flatten()
            .collect();
        self.dead.add(ids.clone());
        self.removed.ranges.push(ids);
        listed_children
    }

    fn set_own(&mut self, id: NodeId, own: OwnSize) {
        let previous = self.facts(id).own();
        if own == previous {
            return;
        }
        self.store_own(id, own);
        self.bump(id, Delta::own(own).minus(Delta::own(previous)));
    }

    fn store_own(&mut self, id: NodeId, own: OwnSize) {
        if let Some(node) = self.heap.find_mut(id) {
            node.direct = own.direct;
            node.flags = own.flags;
            return;
        }
        self.patches.entry(id).or_default().own = Some(own);
    }

    fn bump(&mut self, start: NodeId, delta: Delta) {
        if delta == Delta::default() {
            return;
        }
        let mut id = start;
        while id != NONE {
            let parent = self.add_delta(id, delta);
            self.mark_dirty(id);
            id = parent;
        }
    }

    fn add_delta(&mut self, id: NodeId, delta: Delta) -> NodeId {
        if let Some(node) = self.heap.find_mut(id) {
            node.add(delta);
            return node.parent;
        }
        let patch = self.patches.entry(id).or_default();
        patch.delta = patch.delta.plus(delta);
        let moved_to = patch.parent;
        moved_to.unwrap_or_else(|| self.segments.facts(id).parent)
    }

    fn mark_dirty(&mut self, id: NodeId) {
        let was_dirty = std::mem::replace(self.dirty_mark(id), true);
        if !was_dirty {
            self.changed.push(id);
        }
        self.dirty = true;
    }

    fn dirty_mark(&mut self, id: NodeId) -> &mut bool {
        if is_heap(id) {
            return &mut self.heap.get_mut(id).dirty;
        }
        &mut self.patches.entry(id).or_default().dirty
    }

    fn is_marked_dirty(&self, id: NodeId) -> bool {
        if let Some(node) = self.heap.find(id) {
            return node.dirty;
        }
        self.patches.get(&id).is_some_and(|patch| patch.dirty)
    }

    fn clear_dirty(&mut self, id: NodeId) {
        if let Some(node) = self.heap.find_mut(id) {
            node.dirty = false;
            return;
        }
        let Some(patch) = self.patches.get_mut(&id) else {
            return;
        };
        patch.dirty = false;
        if patch.changes_nothing() {
            self.patches.remove(&id);
        }
    }

    fn is_within(&self, id: NodeId, ancestor: NodeId) -> bool {
        self.self_and_ancestors(id)
            .any(|current| current == ancestor)
    }

    fn self_and_ancestors(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let first = (id != NONE).then_some(id);
        std::iter::successors(first, |&current| {
            let parent = self.facts(current).parent;
            (parent != NONE).then_some(parent)
        })
    }

    fn path_of(&self, id: NodeId) -> PathBuf {
        let names: Vec<&str> = self
            .self_and_ancestors(id)
            .map(|current| self.name(current))
            .collect();
        names.into_iter().rev().collect()
    }

    fn collect_skipped(&self, id: NodeId, skipped: &mut Vec<NodeId>) {
        for child in self.children(id).iter() {
            self.collect_skipped_from_child(child, skipped);
        }
    }

    fn collect_skipped_from_child(&self, child: NodeId, skipped: &mut Vec<NodeId>) {
        let facts = self.facts(child);
        let has_room = skipped.len() < MAX_SKIPPED_NAMES;
        if !has_room {
            return;
        }
        if facts.skipped == 0 {
            return;
        }
        if facts.flags & SKIPPED != 0 {
            skipped.push(child);
        }
        self.collect_skipped(child, skipped);
    }

    fn facts(&self, id: NodeId) -> Facts {
        if let Some(node) = self.heap.find(id) {
            return node.facts();
        }
        let base = self.segments.facts(id);
        self.patches
            .get(&id)
            .map_or(base, |patch| patch.applied_to(base))
    }

    fn name(&self, id: NodeId) -> &str {
        if let Some(node) = self.heap.find(id) {
            return &node.name;
        }
        self.segments.name(id)
    }

    fn listed_children(&self, id: NodeId) -> Option<&[NodeId]> {
        if let Some(node) = self.heap.find(id) {
            return Some(&node.children);
        }
        self.patches.get(&id)?.children.as_deref()
    }

    fn children(&self, id: NodeId) -> Children<'_> {
        if let Some(listed) = self.listed_children(id) {
            return Children::listed(listed);
        }
        self.segments.children(id)
    }

    fn children_mut(&mut self, id: NodeId) -> &mut Vec<NodeId> {
        if is_heap(id) {
            return &mut self.heap.get_mut(id).children;
        }
        let segments = &self.segments;
        let patch = self.patches.entry(id).or_default();
        patch
            .children
            .get_or_insert_with(|| segments.children(id).iter().collect())
    }

    fn is_live(&self, id: NodeId) -> bool {
        if let Some(node) = self.heap.find(id) {
            return !node.name.is_empty();
        }
        self.segments.contains(id) && !self.dead.contains(id)
    }

    pub fn heap_ids(&self) -> Range<u32> {
        self.heap.ids()
    }

    pub(crate) fn id_ranges(&self) -> Vec<Range<u32>> {
        self.segments
            .id_ranges()
            .chain([self.heap.ids()])
            .filter(|ids| !ids.is_empty())
            .collect()
    }

    pub(crate) fn ids_from(&self, cursor: u32, limit: usize) -> Option<Range<u32>> {
        let ids = self.id_ranges().into_iter().find(|ids| cursor < ids.end)?;
        let start = cursor.max(ids.start);
        let limit = u32::try_from(limit).unwrap_or(u32::MAX);
        Some(start..ids.end.min(start.saturating_add(limit)))
    }

    pub(crate) fn live_node(&self, id: u32) -> Option<LiveNode<'_>> {
        if !self.is_live(id) {
            return None;
        }
        let parent = self.facts(id).parent;
        Some(LiveNode {
            name: self.name(id),
            parent: (parent != NONE).then_some(parent),
        })
    }

    pub fn folder_signature(&self, id: u32) -> Option<u32> {
        if !self.is_live(id) {
            return None;
        }
        if let Some(node) = self.heap.find(id) {
            return Some(node.signature());
        }
        Some(self.segments.signature(id))
    }

    pub fn hidden_ranges(&self) -> IdRanges {
        let mut hidden = IdRanges::default();
        let segment_ranges = self.segments.hidden_ranges();
        for range in segment_ranges.filter(|range| !self.is_root(range.start)) {
            hidden.add(range);
        }
        let relinked_tops = self.relinked_mapped_tops();
        for top in relinked_tops.filter(|&top| self.lies_in_hidden_folder(top)) {
            hidden.add(self.segments.subtree(top));
        }
        hidden
    }

    fn relinked_mapped_tops(&self) -> impl Iterator<Item = NodeId> + '_ {
        let relinked = self
            .patches
            .iter()
            .filter(|(_, patch)| patch.parent.is_some());
        relinked.map(|(&id, _)| id)
    }

    fn lies_in_hidden_folder(&self, id: NodeId) -> bool {
        self.self_and_ancestors(id)
            .any(|current| self.is_hidden_folder(current))
    }

    fn is_hidden_folder(&self, id: NodeId) -> bool {
        !self.is_root(id) && is_hidden_name(self.name(id).as_bytes())
    }

    fn is_root(&self, id: NodeId) -> bool {
        self.facts(id).parent == NONE
    }

    pub fn lookup(&self, path: &Path) -> Option<u32> {
        self.find(path)
    }

    pub fn subtree_slots(&self, id: u32) -> Vec<u32> {
        let mut slots = Vec::new();
        if !self.is_live(id) {
            return slots;
        }
        let mut stack = vec![id];
        while let Some(next) = stack.pop() {
            slots.push(next);
            stack.extend(self.children(next).iter().rev());
        }
        slots
    }

    pub fn subtree_ranges(&self, id: u32) -> IdRanges {
        let mut ranges = IdRanges::default();
        if !self.is_live(id) {
            return ranges;
        }
        let mut stack = vec![id];
        while let Some(next) = stack.pop() {
            let descend_into = self.add_subtree_part(next, &mut ranges);
            stack.extend(descend_into);
        }
        ranges
    }

    fn add_subtree_part(&self, id: NodeId, ranges: &mut IdRanges) -> Vec<NodeId> {
        if let Some(whole) = self.unrelinked_mapped_subtree(id) {
            ranges.add(whole);
            return Vec::new();
        }
        ranges.add(id..id + 1);
        self.children(id).iter().collect()
    }

    fn unrelinked_mapped_subtree(&self, id: NodeId) -> Option<Range<NodeId>> {
        if is_heap(id) {
            return None;
        }
        let subtree = self.segments.subtree(id);
        let relinked = self
            .patches
            .range(subtree.clone())
            .any(|(_, patch)| patch.children.is_some());
        (!relinked).then_some(subtree)
    }

    pub(crate) fn child_ids(&self, id: u32) -> Children<'_> {
        if !self.is_live(id) {
            return Children::default();
        }
        self.children(id)
    }

    pub fn depth_first(&self) -> DepthFirstCursor {
        let roots = self.roots.iter().rev();
        DepthFirstCursor {
            pending: roots.map(|&id| PendingVisit { id, depth: 0 }).collect(),
        }
    }

    pub fn next_folder(&self, cursor: &mut DepthFirstCursor) -> Option<VisitedFolder<'_>> {
        let visit = cursor.pending.pop()?;
        let children = self.children(visit.id).iter().rev();
        cursor.pending.extend(children.map(|id| PendingVisit {
            id,
            depth: visit.depth + 1,
        }));
        let facts = self.facts(visit.id);
        Some(VisitedFolder {
            slot: visit.id,
            depth: visit.depth,
            name: self.name(visit.id),
            direct: facts.direct,
            flags: NodeFlags::from_bits(facts.flags).unwrap_or_default(),
        })
    }

    pub fn export_slots(&self) -> Vec<u32> {
        let mut cursor = self.depth_first();
        std::iter::from_fn(|| self.next_folder(&mut cursor))
            .map(|folder| folder.slot)
            .collect()
    }

    pub(crate) fn write_overlay(&self, out: &mut Vec<u8>) {
        put_ids(out, &self.roots);
        self.heap.write(out);
        put_count(out, self.patches.len());
        for (&id, patch) in &self.patches {
            put_u32(out, id);
            patch.write(out);
        }
        self.dead.write(out);
    }

    pub(crate) fn read_overlay(&mut self, reader: &mut Reader<'_>) -> Option<()> {
        let roots = reader.ids()?;
        let heap = HeapNodes::read(reader)?;
        let patches = (0..reader.u32()?)
            .map(|_| Some((reader.u32()?, Patch::read(reader)?)))
            .collect::<Option<BTreeMap<NodeId, Patch>>>()?;
        let dead = IdRanges::read(reader)?;
        self.heap = heap;
        let patched_ids_are_mapped = patches.keys().all(|&id| self.segments.contains(id));
        let patched_parents: Vec<NodeId> =
            patches.values().filter_map(|patch| patch.parent).collect();
        let dead_ids_are_mapped = dead.last_end() <= self.segments.end();
        let linked_ids = roots
            .iter()
            .chain(self.heap.live().flat_map(|(_, node)| &node.children))
            .chain(
                patches
                    .values()
                    .filter_map(|patch| patch.children.as_ref())
                    .flatten(),
            );
        let links_resolve = linked_ids.copied().all(|id| self.holds(id))
            && self
                .heap
                .live()
                .all(|(_, node)| self.holds_parent(node.parent))
            && patched_parents.iter().all(|&parent| self.holds(parent));
        if !(patched_ids_are_mapped && dead_ids_are_mapped && links_resolve) {
            return None;
        }
        self.roots = roots;
        self.patches = patches;
        self.dead = dead;
        Some(())
    }

    fn holds(&self, id: NodeId) -> bool {
        self.heap.find(id).is_some() || self.segments.contains(id)
    }

    fn holds_parent(&self, parent: NodeId) -> bool {
        parent == NONE || self.holds(parent)
    }
}

fn is_listed(sorted_names: &[Box<str>], name: &str) -> bool {
    sorted_names
        .binary_search_by(|listed| (**listed).cmp(name))
        .is_ok()
}

fn strip_root<'a>(
    path: &'a Path,
    root: &Path,
    fold_case: bool,
) -> Option<std::path::Components<'a>> {
    let mut rest = path.components();
    for want in root.components() {
        let got = rest.next()?;
        let same = got == want
            || (fold_case
                && eq_fold(
                    &got.as_os_str().to_string_lossy(),
                    &want.as_os_str().to_string_lossy(),
                ));
        if !same {
            return None;
        }
    }
    Some(rest)
}

fn awaits_scan(flags: u8) -> bool {
    flags & (SCANNED | SKIPPED) == 0
}

fn normal_name(component: Component<'_>) -> Option<String> {
    match component {
        Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
        _ => None,
    }
}

fn eq_fold(left: &str, right: &str) -> bool {
    left.chars()
        .flat_map(char::to_lowercase)
        .eq(right.chars().flat_map(char::to_lowercase))
}
