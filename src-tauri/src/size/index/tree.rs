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

const MAX_SKIPPED_NAMES: usize = 3;

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
    /// Adds an index root such as `/Users`; returns false if it exists.
    pub fn add_root(&mut self, path: &Path) -> bool {
        if self
            .roots
            .iter()
            .any(|&r| Path::new(&*self.node(r).name) == path)
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
        let flags = (self.node(id).flags & SKIPPED) | SCANNED | if denied { DENIED } else { 0 };
        self.set_own(id, direct_bytes, flags);

        let mut listed: Vec<Box<str>> = subdirs
            .iter()
            .map(|s| s.as_ref().to_string_lossy().into())
            .collect();
        listed.sort();
        listed.dedup();
        let stale: Vec<NodeId> = self
            .node(id)
            .children
            .iter()
            .copied()
            .filter(|&c| listed.binary_search(&self.node(c).name).is_err())
            .collect();
        for child in stale {
            self.remove(child);
        }

        let base = self.path_of(id);
        let mut added = Vec::new();
        for name in listed {
            if self.child(id, &name).is_none() {
                added.push(base.join(&*name));
                self.child_or_insert(id, name);
            }
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
        let (Some(parent), Some(name)) = (to.parent().and_then(|p| self.find(p)), to.file_name())
        else {
            return false;
        };
        if self.node(id).parent == NONE || self.is_within(parent, id) {
            return false;
        }
        // Exact only: a case-insensitive hit may be a distinct sibling on a
        // case-sensitive volume.
        if let Some(existing) = self.resolve(to, false) {
            if existing != id {
                if self.is_within(id, existing) {
                    return false;
                }
                self.remove(existing);
            }
        }
        let moved = Delta::of_subtree(self.node(id));
        self.detach(id, moved);
        self.node_mut(id).name = name.to_string_lossy().into();
        self.attach(parent, id);
        self.bump(parent, moved);
        self.mark_dirty(id);
        true
    }

    /// Marks or clears an unvisited protected root. Marking creates the node
    /// (and missing ancestors); clearing leaves it waiting for a scan.
    pub fn set_skipped(&mut self, path: &Path, skipped: bool) -> bool {
        let id = if skipped {
            self.ensure(path)
        } else {
            self.find(path)
        };
        let Some(id) = id else {
            return false;
        };
        let node = self.node(id);
        let flags = if skipped {
            node.flags | SKIPPED
        } else {
            node.flags & !SKIPPED
        };
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
            let node = self.node(next);
            if node.pending == 0 {
                continue;
            }
            if node.flags & (SCANNED | SKIPPED) == 0 && node.parent != NONE {
                self.remove(next);
            } else {
                stack.extend_from_slice(&node.children);
            }
        }
        true
    }

    /// The topmost directories still waiting for a scan, e.g. after loading
    /// a snapshot written mid-scan.
    pub fn unscanned_tops(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = self.roots.clone();
        while let Some(id) = stack.pop() {
            let node = self.node(id);
            if node.pending == 0 {
                continue;
            }
            if node.flags & (SCANNED | SKIPPED) == 0 {
                out.push(self.path_of(id));
            } else {
                stack.extend_from_slice(&node.children);
            }
        }
        out
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
        let mut out = Vec::new();
        if let Some(id) = self.find(path) {
            let mut stack = vec![(id, self.path_of(id))];
            while let Some((next, base)) = stack.pop() {
                for &child in &self.node(next).children {
                    let node = self.node(child);
                    let child_path = base.join(&*node.name);
                    if node.pending == 0 {
                        out.push(child_path);
                    } else {
                        stack.push((child, child_path));
                    }
                }
            }
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
            .filter(|p| {
                self.find(p.as_ref())
                    .is_some_and(|id| self.node(id).flags & DIRTY != 0)
            })
            .map(|p| p.as_ref().to_path_buf())
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
        self.roots.iter().find_map(|&root| {
            let rest = strip_root(path, Path::new(&*self.node(root).name), fold_case)?;
            let mut id = root;
            for component in rest {
                let name = normal_name(component)?;
                id = self.child(id, &name).or_else(|| {
                    fold_case
                        .then(|| {
                            self.node(id)
                                .children
                                .iter()
                                .copied()
                                .find(|&c| eq_fold(&self.node(c).name, &name))
                        })
                        .flatten()
                })?;
            }
            Some(id)
        })
    }

    /// Finds `path` (exact match), creating missing nodes below its root.
    fn ensure(&mut self, path: &Path) -> Option<NodeId> {
        let (root, rest) = [false, true].into_iter().find_map(|fold_case| {
            self.roots.iter().find_map(|&root| {
                strip_root(path, Path::new(&*self.node(root).name), fold_case)
                    .map(|rest| (root, rest))
            })
        })?;
        let names = rest.map(normal_name).collect::<Option<Vec<_>>>()?;
        Some(
            names
                .into_iter()
                .fold(root, |id, name| self.child_or_insert(id, name.into())),
        )
    }

    fn child(&self, id: NodeId, name: &str) -> Option<NodeId> {
        let children = &self.node(id).children;
        children
            .binary_search_by(|&c| (*self.node(c).name).cmp(name))
            .ok()
            .map(|i| children[i])
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
            .partition_point(|&c| self.node(c).name < *name);
        self.node_mut(id).parent = parent;
        self.node_mut(parent).children.insert(slot, id);
    }

    /// Unlinks `id` from its parent (or the roots) and subtracts `subtree`
    /// from the ancestors.
    fn detach(&mut self, id: NodeId, subtree: Delta) {
        let parent = self.node(id).parent;
        if parent == NONE {
            self.roots.retain(|&r| r != id);
            return;
        }
        self.node_mut(parent).children.retain(|&c| c != id);
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
    fn is_within(&self, mut id: NodeId, ancestor: NodeId) -> bool {
        while id != NONE {
            if id == ancestor {
                return true;
            }
            id = self.node(id).parent;
        }
        false
    }

    fn path_of(&self, mut id: NodeId) -> PathBuf {
        let mut names = Vec::new();
        while id != NONE {
            names.push(&*self.node(id).name);
            id = self.node(id).parent;
        }
        names.into_iter().rev().collect()
    }

    fn collect_skipped_names(&self, id: NodeId, out: &mut Vec<String>) {
        for &child in &self.node(id).children {
            let node = self.node(child);
            if out.len() == MAX_SKIPPED_NAMES {
                return;
            }
            if node.skipped == 0 {
                continue;
            }
            if node.flags & SKIPPED != 0 {
                out.push(node.name.to_string());
            }
            self.collect_skipped_names(child, out);
        }
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
        let mut stack: Vec<(NodeId, u32)> = self.roots.iter().rev().map(|&r| (r, NONE)).collect();
        while let Some((id, parent_order)) = stack.pop() {
            let node = self.node(id);
            visit(
                parent_order,
                &node.name,
                node.direct,
                node.flags & PERSISTED_FLAGS,
            );
            stack.extend(node.children.iter().rev().map(|&c| (c, order)));
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
        if parent == NONE {
            self.roots.push(id);
        } else {
            let siblings = &self.nodes.get(parent as usize)?.children;
            let slot = siblings
                .binary_search_by(|&c| (*self.node(c).name).cmp(&name))
                .err()?;
            self.nodes[parent as usize].children.insert(slot, id);
        }
        self.nodes.push(Node {
            parent,
            name,
            direct,
            flags,
            ..Node::default()
        });
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
            let node = &self.nodes[id];
            let (parent, total, pending, skipped, denied) = (
                node.parent,
                node.total,
                node.pending,
                node.skipped,
                node.denied,
            );
            if parent != NONE {
                let parent = &mut self.nodes[parent as usize];
                parent.total = parent.total.wrapping_add(total);
                parent.pending = parent.pending.wrapping_add(pending);
                parent.skipped = parent.skipped.wrapping_add(skipped);
                parent.denied = parent.denied.wrapping_add(denied);
            }
        }
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

fn normal_name(component: Component<'_>) -> Option<String> {
    match component {
        Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
        _ => None,
    }
}

fn eq_fold(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_lowercase)
        .eq(b.chars().flat_map(char::to_lowercase))
}
