//! Folder-only size tree for the macOS index: one node per directory with
//! its direct-file bytes and rolled-up subtree totals. Pure: no I/O and no
//! platform code, so it compiles and is tested on every OS.

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

type NodeId = u32;
const NONE: NodeId = NodeId::MAX;

/// This directory's own listing is known.
const SCANNED: u8 = 1;
/// The OS refused to read this directory.
const DENIED: u8 = 2;
/// An unvisited protected root: never read, size unknown.
const SKIPPED: u8 = 4;
/// The bits a snapshot stores.
pub(crate) const PERSISTED_FLAGS: u8 = SCANNED | DENIED | SKIPPED;
/// Totals changed since the last [`FolderSizeTree::drain_changed`].
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

/// 80 bytes inline; with the name allocation and the parent's 4-byte child
/// slot, about 100 bytes per folder.
#[derive(Debug, Default)]
struct Node {
    parent: NodeId,
    /// A single component, or the full path for a root.
    name: Box<str>,
    /// Sorted by name for binary-search lookup.
    children: Vec<NodeId>,
    direct: u64,
    /// The aggregates below cover this node and every descendant.
    total: u64,
    /// Nodes neither scanned nor skipped.
    pending: u32,
    skipped: u32,
    denied: u32,
    flags: u8,
}

/// A signed change to a node's aggregates, applied to it and every ancestor.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Delta {
    bytes: i64,
    pending: i32,
    skipped: i32,
    denied: i32,
}

impl Delta {
    fn own(direct: u64, flags: u8) -> Self {
        Self {
            bytes: direct as i64,
            pending: i32::from(flags & (SCANNED | SKIPPED) == 0),
            skipped: i32::from(flags & SKIPPED != 0),
            denied: i32::from(flags & DENIED != 0),
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

/// Why a total is only a lower bound: skipped protected roots or denied
/// directories somewhere strictly below the queried folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LowerBound {
    /// Display names of up to three skipped roots, in name order.
    pub skipped_names: Vec<String>,
    /// Skipped roots beyond `skipped_names`.
    pub skipped_more: u32,
    /// Denied directories below the folder.
    pub denied_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderTotal {
    pub bytes: u64,
    /// No directory in the subtree is still waiting for a scan, so the
    /// index can answer without filesystem work.
    pub fully_indexed: bool,
    /// This directory's own listing is known (live events for unscanned
    /// directories are left to the scan).
    pub scanned: bool,
    pub denied: bool,
    pub skipped: bool,
    pub lower_bound: Option<LowerBound>,
}

/// Arena of folder nodes with u32 indices; freed slots are reused.
#[derive(Debug, Default)]
pub struct FolderSizeTree {
    nodes: Vec<Node>,
    free: Vec<NodeId>,
    roots: Vec<NodeId>,
    /// Nodes carrying the `DIRTY` bit.
    changed: Vec<NodeId>,
    dirty: bool,
}

impl FolderSizeTree {
    /// Adds an index root such as `/Users`; returns false if it exists or
    /// is empty (an empty name marks a freed slot).
    pub fn add_root(&mut self, path: &Path) -> bool {
        if path.as_os_str().is_empty()
            || self
                .roots
                .iter()
                .any(|&root| Path::new(&*self.node(root).name) == path)
        {
            return false;
        }
        let id = self.alloc(NONE, path.to_string_lossy().into());
        self.roots.push(id);
        self.bump(id, Delta::own(0, 0));
        true
    }

    /// Live nodes, roots included.
    pub fn node_count(&self) -> usize {
        self.nodes.len() - self.free.len()
    }

    /// Whether anything changed since [`Self::mark_clean`] (snapshot writes).
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Merges one scanned directory as an authoritative listing: sets its
    /// direct bytes and denied state, prunes children no longer listed and
    /// adds new ones as not yet scanned. Missing ancestors are created as not
    /// yet scanned. Returns false for paths outside every root.
    pub fn merge_record<S: AsRef<OsStr>>(
        &mut self,
        path: &Path,
        direct_bytes: u64,
        subdirs: &[S],
        denied: bool,
    ) -> bool {
        let Some(id) = self.ensure(path) else {
            return false;
        };
        self.set_listing(id, direct_bytes, subdirs, denied);
        true
    }

    /// Applies a single-directory re-read like [`Self::merge_record`], but
    /// only to a directory already in the tree. Returns the added children's
    /// paths (to be scanned), or `None` if the directory is not in the tree.
    pub fn apply_listing<S: AsRef<OsStr>>(
        &mut self,
        path: &Path,
        direct_bytes: u64,
        subdirs: &[S],
        denied: bool,
    ) -> Option<Vec<PathBuf>> {
        let id = self.find(path)?;
        Some(self.set_listing(id, direct_bytes, subdirs, denied))
    }

    fn set_listing<S: AsRef<OsStr>>(
        &mut self,
        id: NodeId,
        direct_bytes: u64,
        subdirs: &[S],
        denied: bool,
    ) -> Vec<PathBuf> {
        let flags = (self.node(id).flags & SKIPPED) | SCANNED | denied_flag(denied);
        self.set_own(id, direct_bytes, flags);

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

    /// Removes a directory and its subtree, subtracting its totals from the
    /// ancestors once. Returns false if it is not in the tree.
    pub fn prune(&mut self, path: &Path) -> bool {
        self.find(path).map(|id| self.remove(id)).is_some()
    }

    /// Moves a directory (and its subtree) to `to`, replacing any existing
    /// node there. Returns false if `from` is missing or a root, `to`'s
    /// parent is not in the tree, or `to` lies inside `from`.
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
        if self.node(id).parent == NONE || self.is_within(parent, id) {
            return false;
        }
        // Exact only: a case-insensitive hit may be a distinct sibling on a
        // case-sensitive volume.
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
        true
    }

    fn skip_target(&mut self, path: &Path, skipped: bool) -> Option<NodeId> {
        if skipped {
            return self.ensure(path);
        }
        self.find(path)
    }

    /// Marks or clears an unvisited protected root. Marking creates the node
    /// (and missing ancestors); clearing leaves it waiting for a scan.
    pub fn set_skipped(&mut self, path: &Path, skipped: bool) -> bool {
        let Some(id) = self.skip_target(path, skipped) else {
            return false;
        };
        let node = self.node(id);
        let flags = with_skipped_flag(node.flags, skipped);
        self.set_own(id, node.direct, flags);
        true
    }

    /// After a completed walk of `path`: removes every directory at or below
    /// it (index roots excepted) that is still waiting for a scan. The walk
    /// refused those (vanished, excluded or on another device), so they would
    /// otherwise stay pending forever.
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

    /// The topmost directories still waiting for a scan, e.g. after loading
    /// a snapshot written mid-scan.
    pub fn unscanned_tops(&self) -> Vec<PathBuf> {
        let mut tops = Vec::new();
        let mut stack = self.roots.clone();
        while let Some(id) = stack.pop() {
            self.visit_for_unscanned_tops(id, &mut tops, &mut stack);
        }
        tops
    }

    fn visit_for_unscanned_tops(
        &self,
        id: NodeId,
        tops: &mut Vec<PathBuf>,
        stack: &mut Vec<NodeId>,
    ) {
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

    /// The indexed total for a folder, or `None` if it is not in the tree.
    pub fn total(&self, path: &Path) -> Option<FolderTotal> {
        let id = self.find(path)?;
        let node = self.node(id);
        let own = Delta::own(node.direct, node.flags);
        let skipped_below = node.skipped - own.skipped as u32;
        let denied_below = node.denied - own.denied as u32;
        let lower_bound = (skipped_below > 0 || denied_below > 0).then(|| {
            let mut skipped_names = Vec::new();
            self.collect_skipped_names(id, &mut skipped_names);
            LowerBound {
                skipped_more: skipped_below - skipped_names.len() as u32,
                skipped_names,
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

    /// The largest fully indexed subtrees strictly below `path`; a boost
    /// excludes these from its walk.
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

    /// Returns the subscribed paths whose totals changed since the last
    /// call, then resets change tracking. Call once per applied batch.
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

    /// Exact match first; only if that fails, a case-insensitive match
    /// (still preferring exact per component).
    fn find(&self, path: &Path) -> Option<NodeId> {
        self.resolve(path, false)
            .or_else(|| self.resolve(path, true))
    }

    fn resolve(&self, path: &Path, fold_case: bool) -> Option<NodeId> {
        self.roots
            .iter()
            .find_map(|&root| self.resolve_below_root(root, path, fold_case))
    }

    fn resolve_below_root(&self, root: NodeId, path: &Path, fold_case: bool) -> Option<NodeId> {
        let rest = strip_root(path, self.root_path(root), fold_case)?;
        let mut id = root;
        for component in rest {
            let name = normal_name(component)?;
            id = self.child_matching(id, &name, fold_case)?;
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
    ) -> Option<(NodeId, std::path::Components<'path>)> {
        self.roots.iter().find_map(|&root| {
            strip_root(path, self.root_path(root), fold_case).map(|rest| (root, rest))
        })
    }

    /// Finds `path` (exact match), creating missing nodes below its root.
    fn ensure(&mut self, path: &Path) -> Option<NodeId> {
        let (root, rest) = [false, true]
            .into_iter()
            .find_map(|fold_case| self.strip_any_root(path, fold_case))?;
        let names = rest.map(normal_name).collect::<Option<Vec<_>>>()?;
        Some(
            names
                .into_iter()
                .fold(root, |id, name| self.child_or_insert(id, name.into())),
        )
    }

    fn child_matching(&self, parent: NodeId, name: &str, fold_case: bool) -> Option<NodeId> {
        let exact = self.child(parent, name);
        if exact.is_some() || !fold_case {
            return exact;
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

    /// Unlinks `id` from its parent (or the roots) and subtracts `subtree`
    /// from the ancestors.
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
        }
        self.dirty = true;
    }

    fn set_own(&mut self, id: NodeId, direct: u64, flags: u8) {
        let node = self.node(id);
        let delta = Delta::own(direct, flags).minus(Delta::own(node.direct, node.flags));
        let node = self.node_mut(id);
        node.direct = direct;
        node.flags = (node.flags & !PERSISTED_FLAGS) | flags;
        self.bump(id, delta);
    }

    /// Applies `delta` to `id` and every ancestor, marking each changed.
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

    /// Whether `id` is `ancestor` or below it.
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

    fn collect_skipped_names(&self, id: NodeId, names: &mut Vec<String>) {
        for &child in &self.node(id).children {
            self.collect_skipped_names_from_child(child, names);
        }
    }

    fn collect_skipped_names_from_child(&self, child: NodeId, names: &mut Vec<String>) {
        let node = self.node(child);
        let has_room = names.len() < MAX_SKIPPED_NAMES;
        if !has_room || node.skipped == 0 {
            return;
        }
        if node.flags & SKIPPED != 0 {
            names.push(node.name.to_string());
        }
        self.collect_skipped_names(child, names);
    }

    /// Arena slots, live or freed: node ids run `0..slot_len()`.
    pub(crate) fn slot_len(&self) -> usize {
        self.nodes.len()
    }

    /// A live node's name and parent (`None` for a root); `None` for a freed
    /// or out-of-range slot.
    pub(crate) fn live_node(&self, id: u32) -> Option<(&str, Option<u32>)> {
        let node = self.nodes.get(id as usize)?;
        if node.name.is_empty() {
            return None;
        }
        Some((&node.name, (node.parent != NONE).then_some(node.parent)))
    }

    pub(crate) fn lookup(&self, path: &Path) -> Option<u32> {
        self.find(path)
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

    /// Visits live nodes parents-first, each with its parent's position in
    /// the visit order (`u32::MAX` for roots). Children come in name order,
    /// so the sequence is deterministic.
    pub(crate) fn export(&self, mut visit: impl FnMut(u32, &str, u64, u8)) {
        let mut order = 0_u32;
        let mut stack: Vec<(NodeId, u32)> =
            self.roots.iter().rev().map(|&root| (root, NONE)).collect();
        while let Some((id, parent_order)) = stack.pop() {
            let node = self.node(id);
            visit(
                parent_order,
                &node.name,
                node.direct,
                node.flags & PERSISTED_FLAGS,
            );
            stack.extend(node.children.iter().rev().map(|&child| (child, order)));
            order += 1;
        }
    }

    /// Appends one exported node; `None` rejects invalid input (forward
    /// parent, bad flags, empty or duplicate name).
    pub(crate) fn import_node(
        &mut self,
        parent: u32,
        name: Box<str>,
        direct: u64,
        flags: u8,
    ) -> Option<()> {
        if name.is_empty() || flags & !PERSISTED_FLAGS != 0 {
            return None;
        }
        let id = self.nodes.len() as NodeId;
        self.attach_imported(parent, id, &name)?;
        self.nodes.push(Node {
            parent,
            name,
            direct,
            flags,
            ..Node::default()
        });
        Some(())
    }

    fn attach_imported(&mut self, parent: u32, id: NodeId, name: &str) -> Option<()> {
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

    /// Recomputes every aggregate after [`Self::import_node`] calls.
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

/// The components of `path` after `root`, if `root` is a prefix of it.
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
