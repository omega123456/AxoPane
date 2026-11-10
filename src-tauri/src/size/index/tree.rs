use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

type NodeId = u32;
const NONE: NodeId = NodeId::MAX;

pub(crate) const SCANNED: u8 = 1;
pub(crate) const DENIED: u8 = 2;
pub(crate) const SKIPPED: u8 = 4;
pub(crate) const PERSISTED_FLAGS: u8 = SCANNED | DENIED | SKIPPED;
const DIRTY: u8 = 8;

const NO_FLAGS: u8 = 0;

const MAX_SKIPPED_NAMES: usize = 3;

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

#[derive(Debug, Default)]
struct Node {
    parent: NodeId,
    name: Box<str>,
    children: Vec<NodeId>,
    direct: u64,
    total: u64,
    pending: u32,
    skipped: u32,
    denied: u32,
    flags: u8,
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
            pending: u32::from(flags & (SCANNED | SKIPPED) == 0),
            skipped: u32::from(flags & SKIPPED != 0),
            denied: u32::from(flags & DENIED != 0),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Delta {
    bytes: i64,
    pending: i32,
    skipped: i32,
    denied: i32,
}

impl Delta {
    fn own(direct: u64, flags: u8) -> Self {
        let own = OwnCounters::of(flags);
        Self {
            bytes: direct as i64,
            pending: own.pending as i32,
            skipped: own.skipped as i32,
            denied: own.denied as i32,
        }
    }

    fn of_subtree(node: &Node) -> Self {
        Self {
            bytes: node.total as i64,
            pending: node.pending as i32,
            skipped: node.skipped as i32,
            denied: node.denied as i32,
        }
    }

    fn minus(self, other: Self) -> Self {
        Self {
            bytes: self.bytes.wrapping_sub(other.bytes),
            pending: self.pending - other.pending,
            skipped: self.skipped - other.skipped,
            denied: self.denied - other.denied,
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

pub(crate) struct ImportedNode {
    pub(crate) parent: u32,
    pub(crate) name: Box<str>,
    pub(crate) direct: u64,
    pub(crate) flags: u8,
}

struct ImportedLink<'name> {
    parent: u32,
    name: &'name str,
}

#[derive(Clone, Copy)]
struct OwnSize {
    direct: u64,
    flags: u8,
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

struct RootedRest<'path> {
    root: NodeId,
    rest: std::path::Components<'path>,
}

struct UnscannedTopsWalk {
    tops: Vec<PathBuf>,
    stack: Vec<NodeId>,
}

#[derive(Clone, Copy)]
struct DepthFirstVisit {
    id: NodeId,
    parent_order: u32,
}

pub(crate) struct LiveNode<'tree> {
    pub(crate) name: &'tree str,
    pub(crate) parent: Option<u32>,
}

#[derive(Debug, Default)]
pub struct FolderSizeTree {
    nodes: Vec<Node>,
    free: Vec<NodeId>,
    roots: Vec<NodeId>,
    changed: Vec<NodeId>,
    removed: Vec<NodeId>,
    dirty: bool,
    structure_changed: bool,
}

impl FolderSizeTree {
    fn has_root(&self, path: &Path) -> bool {
        self.roots
            .iter()
            .any(|&root| Path::new(&*self.node(root).name) == path)
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
        self.bump(id, Delta::own(0, 0));
        true
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len() - self.free.len()
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
        std::mem::take(&mut self.removed)
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
        let flags = (self.node(id).flags & SKIPPED) | SCANNED | denied_flag(denied);
        self.set_own(
            id,
            OwnSize {
                direct: direct_bytes,
                flags,
            },
        );

        let mut listed: Vec<Box<str>> = subdirs
            .iter()
            .map(|subdir| subdir.as_ref().to_string_lossy().into())
            .collect();
        listed.sort();
        listed.dedup();
        let stale: Vec<NodeId> = self
            .node(id)
            .children
            .iter()
            .copied()
            .filter(|&child| listed.binary_search(&self.node(child).name).is_err())
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
        let is_root = self.node(id).parent == NONE;
        let moves_into_itself = self.is_within(parent, id);
        if is_root || moves_into_itself {
            return false;
        }
        let replaced = self.resolve(to, false).filter(|&existing| existing != id);
        if replaced.is_some_and(|existing| self.is_within(id, existing)) {
            return false;
        }
        if let Some(existing) = replaced {
            self.remove(existing);
        }
        let moved = Delta::of_subtree(self.node(id));
        self.detach(id, moved);
        self.node_mut(id).name = name.to_string_lossy().into();
        self.attach(parent, id);
        self.bump(parent, moved);
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
        let node = self.node(id);
        let own = OwnSize {
            direct: node.direct,
            flags: with_skipped_flag(node.flags, skipped),
        };
        self.set_own(id, own);
        true
    }

    pub fn exclude_subtree(&mut self, path: &Path) -> Option<ExcludedFolder> {
        let id = self.find(path)?;
        let node = self.node(id);
        let already_skipped_leaf =
            node.children.is_empty() && node.direct == 0 && node.flags & PERSISTED_FLAGS == SKIPPED;
        if already_skipped_leaf {
            return Some(ExcludedFolder {
                slot: id,
                changed: false,
            });
        }
        for child in node.children.clone() {
            self.remove(child);
        }
        self.set_own(
            id,
            OwnSize {
                direct: 0,
                flags: SKIPPED,
            },
        );
        Some(ExcludedFolder {
            slot: id,
            changed: true,
        })
    }

    pub fn include_subtree(&mut self, path: &Path) -> bool {
        let Some(id) = self.find(path) else {
            return false;
        };
        let is_skipped = self.node(id).flags & SKIPPED != 0;
        if !is_skipped {
            return false;
        }
        self.set_own(
            id,
            OwnSize {
                direct: 0,
                flags: NO_FLAGS,
            },
        );
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
        let node = self.node(id);
        if node.pending == 0 {
            return Vec::new();
        }
        let is_unscanned_non_root = awaits_scan(node) && node.parent != NONE;
        if is_unscanned_non_root {
            self.remove(id);
            return Vec::new();
        }
        node.children.clone()
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
        let node = self.node(id);
        if node.pending == 0 {
            return;
        }
        if awaits_scan(node) {
            tops.push(self.path_of(id));
            return;
        }
        stack.extend_from_slice(&node.children);
    }

    pub fn total(&self, path: &Path) -> Option<FolderTotal> {
        let id = self.find(path)?;
        let node = self.node(id);
        let own = Delta::own(node.direct, node.flags);
        let skipped_below = node.skipped - own.skipped as u32;
        let denied_below = node.denied - own.denied as u32;
        let lower_bound = (skipped_below > 0 || denied_below > 0).then(|| {
            let mut skipped = Vec::new();
            self.collect_skipped(id, &mut skipped);
            LowerBound {
                skipped_more: skipped_below - skipped.len() as u32,
                skipped_names: skipped
                    .iter()
                    .map(|&skipped_id| self.node(skipped_id).name.to_string())
                    .collect(),
                skipped_paths: skipped
                    .iter()
                    .map(|&skipped_id| self.path_of(skipped_id))
                    .collect(),
                denied_count: denied_below,
            }
        });
        Some(FolderTotal {
            bytes: node.total,
            fully_indexed: node.pending == 0,
            scanned: node.flags & SCANNED != 0,
            denied: node.flags & DENIED != 0,
            skipped: node.flags & SKIPPED != 0,
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
                .node(next)
                .children
                .iter()
                .map(|&child| (child, base.join(&*self.node(child).name)))
                .partition(|(child, _)| self.node(*child).pending == 0);
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
                    .is_some_and(|id| self.node(id).flags & DIRTY != 0)
            })
            .map(|path| path.as_ref().to_path_buf())
            .collect();
        for id in std::mem::take(&mut self.changed) {
            self.node_mut(id).flags &= !DIRTY;
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
        Path::new(&*self.node(root).name)
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
        self.node(parent)
            .children
            .iter()
            .copied()
            .find(|&child| eq_fold(&self.node(child).name, name))
    }

    fn child(&self, id: NodeId, name: &str) -> Option<NodeId> {
        let children = &self.node(id).children;
        children
            .binary_search_by(|&child| (*self.node(child).name).cmp(name))
            .ok()
            .map(|position| children[position])
    }

    fn child_or_insert(&mut self, parent: NodeId, name: Box<str>) -> NodeId {
        if let Some(existing) = self.child(parent, &name) {
            return existing;
        }
        let id = self.alloc(parent, name);
        self.attach(parent, id);
        self.bump(id, Delta::own(0, 0));
        id
    }

    fn alloc(&mut self, parent: NodeId, name: Box<str>) -> NodeId {
        let node = Node {
            parent,
            name,
            ..Node::default()
        };
        self.structure_changed = true;
        match self.free.pop() {
            Some(id) => {
                self.nodes[id as usize] = node;
                id
            }
            None => {
                self.nodes.push(node);
                (self.nodes.len() - 1) as NodeId
            }
        }
    }

    fn attach(&mut self, parent: NodeId, id: NodeId) {
        let name = &self.node(id).name;
        let slot = self
            .node(parent)
            .children
            .partition_point(|&child| self.node(child).name < *name);
        self.node_mut(id).parent = parent;
        self.node_mut(parent).children.insert(slot, id);
    }

    fn detach(&mut self, id: NodeId, subtree: Delta) {
        let parent = self.node(id).parent;
        if parent == NONE {
            self.roots.retain(|&root| root != id);
            return;
        }
        self.node_mut(parent).children.retain(|&child| child != id);
        self.bump(parent, Delta::default().minus(subtree));
    }

    fn remove(&mut self, id: NodeId) {
        self.detach(id, Delta::of_subtree(self.node(id)));
        let mut stack = vec![id];
        while let Some(next) = stack.pop() {
            let node = std::mem::take(self.node_mut(next));
            stack.extend(node.children);
            self.free.push(next);
            self.removed.push(next);
        }
        self.dirty = true;
        self.structure_changed = true;
    }

    fn set_own(&mut self, id: NodeId, own: OwnSize) {
        let OwnSize { direct, flags } = own;
        let node = self.node(id);
        let delta = Delta::own(direct, flags).minus(Delta::own(node.direct, node.flags));
        let node = self.node_mut(id);
        node.direct = direct;
        node.flags = (node.flags & !PERSISTED_FLAGS) | flags;
        self.bump(id, delta);
    }

    fn bump(&mut self, mut id: NodeId, delta: Delta) {
        if delta == Delta::default() {
            return;
        }
        while id != NONE {
            let node = self.node_mut(id);
            node.total = node.total.wrapping_add_signed(delta.bytes);
            node.pending = node.pending.wrapping_add_signed(delta.pending);
            node.skipped = node.skipped.wrapping_add_signed(delta.skipped);
            node.denied = node.denied.wrapping_add_signed(delta.denied);
            let parent = node.parent;
            self.mark_dirty(id);
            id = parent;
        }
    }

    fn mark_dirty(&mut self, id: NodeId) {
        let node = self.node_mut(id);
        if node.flags & DIRTY == 0 {
            node.flags |= DIRTY;
            self.changed.push(id);
        }
        self.dirty = true;
    }

    fn is_within(&self, id: NodeId, ancestor: NodeId) -> bool {
        self.self_and_ancestors(id)
            .any(|current| current == ancestor)
    }

    fn self_and_ancestors(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let first = (id != NONE).then_some(id);
        std::iter::successors(first, |&current| {
            let parent = self.node(current).parent;
            (parent != NONE).then_some(parent)
        })
    }

    fn path_of(&self, mut id: NodeId) -> PathBuf {
        let mut names = Vec::new();
        while id != NONE {
            names.push(&*self.node(id).name);
            id = self.node(id).parent;
        }
        names.into_iter().rev().collect()
    }

    fn collect_skipped(&self, id: NodeId, skipped: &mut Vec<NodeId>) {
        for &child in &self.node(id).children {
            self.collect_skipped_from_child(child, skipped);
        }
    }

    fn collect_skipped_from_child(&self, child: NodeId, skipped: &mut Vec<NodeId>) {
        let node = self.node(child);
        let has_room = skipped.len() < MAX_SKIPPED_NAMES;
        if !has_room {
            return;
        }
        if node.skipped == 0 {
            return;
        }
        if node.flags & SKIPPED != 0 {
            skipped.push(child);
        }
        self.collect_skipped(child, skipped);
    }

    pub(crate) fn slot_len(&self) -> usize {
        self.nodes.len()
    }

    pub(crate) fn live_node(&self, id: u32) -> Option<LiveNode<'_>> {
        let node = self.nodes.get(id as usize)?;
        if node.name.is_empty() {
            return None;
        }
        Some(LiveNode {
            name: &node.name,
            parent: (node.parent != NONE).then_some(node.parent),
        })
    }

    pub fn lookup(&self, path: &Path) -> Option<u32> {
        self.find(path)
    }

    pub fn subtree_slots(&self, id: u32) -> Vec<u32> {
        let mut slots = Vec::new();
        if self.live_node(id).is_none() {
            return slots;
        }
        let mut stack = vec![id];
        while let Some(next) = stack.pop() {
            slots.push(next);
            stack.extend(self.node(next).children.iter().rev());
        }
        slots
    }

    pub(crate) fn child_ids(&self, id: u32) -> &[u32] {
        self.nodes
            .get(id as usize)
            .map_or(&[], |node| &node.children)
    }

    fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id as usize]
    }

    fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id as usize]
    }

    pub(crate) fn export(&self, mut visit: impl FnMut(u32, &str, u64, u8)) {
        self.visit_depth_first(|visited| {
            let node = self.node(visited.id);
            visit(
                visited.parent_order,
                &node.name,
                node.direct,
                node.flags & PERSISTED_FLAGS,
            );
        });
    }

    pub fn export_slots(&self) -> Vec<u32> {
        let mut slots = Vec::with_capacity(self.node_count());
        self.visit_depth_first(|visited| slots.push(visited.id));
        slots
    }

    fn visit_depth_first(&self, mut visit: impl FnMut(DepthFirstVisit)) {
        let mut order = 0_u32;
        let mut stack: Vec<DepthFirstVisit> = self
            .roots
            .iter()
            .rev()
            .map(|&root| DepthFirstVisit {
                id: root,
                parent_order: NONE,
            })
            .collect();
        while let Some(visited) = stack.pop() {
            let children = self.node(visited.id).children.iter().rev();
            stack.extend(children.map(|&child| DepthFirstVisit {
                id: child,
                parent_order: order,
            }));
            visit(visited);
            order += 1;
        }
    }

    pub(crate) fn import_node(&mut self, imported: ImportedNode) -> Option<()> {
        let ImportedNode {
            parent,
            name,
            direct,
            flags,
        } = imported;
        if name.is_empty() {
            return None;
        }
        let has_unknown_flags = flags & !PERSISTED_FLAGS != 0;
        if has_unknown_flags {
            return None;
        }
        let id = self.nodes.len() as NodeId;
        self.attach_imported(
            id,
            ImportedLink {
                parent,
                name: &name,
            },
        )?;
        self.nodes.push(Node {
            parent,
            name,
            direct,
            flags,
            ..Node::default()
        });
        Some(())
    }

    fn attach_imported(&mut self, id: NodeId, link: ImportedLink<'_>) -> Option<()> {
        let ImportedLink { parent, name } = link;
        if parent == NONE {
            self.roots.push(id);
            return Some(());
        }
        let siblings = &self.nodes.get(parent as usize)?.children;
        let slot = siblings
            .binary_search_by(|&child| (*self.node(child).name).cmp(name))
            .err()?;
        self.nodes[parent as usize].children.insert(slot, id);
        Some(())
    }

    pub(crate) fn finish_import(&mut self) {
        for node in &mut self.nodes {
            let own = Delta::own(node.direct, node.flags);
            node.total = node.direct;
            node.pending = own.pending as u32;
            node.skipped = own.skipped as u32;
            node.denied = own.denied as u32;
        }
        for id in (0..self.nodes.len()).rev() {
            self.roll_up_into_parent(id);
        }
    }

    fn roll_up_into_parent(&mut self, id: usize) {
        let child = &self.nodes[id];
        let parent_id = child.parent;
        if parent_id == NONE {
            return;
        }
        let (total, pending, skipped, denied) =
            (child.total, child.pending, child.skipped, child.denied);
        let parent = &mut self.nodes[parent_id as usize];
        parent.total = parent.total.wrapping_add(total);
        parent.pending = parent.pending.wrapping_add(pending);
        parent.skipped = parent.skipped.wrapping_add(skipped);
        parent.denied = parent.denied.wrapping_add(denied);
    }
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

fn awaits_scan(node: &Node) -> bool {
    node.flags & (SCANNED | SKIPPED) == 0
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
