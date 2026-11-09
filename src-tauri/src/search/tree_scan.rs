use std::path::{Path, PathBuf};

use super::file_scan::{self, FileScan, FileSelection};
use super::matcher::NameKind;
use super::path_style::PathStyle;
use super::query::{ParsedQuery, Segment};
use super::results::{rank_candidates, CandidatePool, TieredCandidate, BROWSE_PARENT_CAP};
use super::source::FolderTreeSource;
use super::{Candidate, CandidateBatch, SearchError, SearchRun, SearchScope};
use crate::size::index::tree::FolderSizeTree;
use crate::size::index::IndexRead;

pub const SLOTS_PER_READ: usize = 32_768;

const HIDDEN_NAME_PREFIX: char = '.';
const PATH_SEPARATOR: char = '/';

pub type TreeSearchRun<'run> = SearchRun<'run, dyn FolderTreeSource>;

pub fn candidates(
    run: &TreeSearchRun<'_>,
    query: &ParsedQuery,
) -> Result<CandidateBatch, SearchError> {
    candidates_in_chunks(run, query, SLOTS_PER_READ)
}

pub fn candidates_in_chunks(
    run: &TreeSearchRun<'_>,
    query: &ParsedQuery,
    slots_per_read: usize,
) -> Result<CandidateBatch, SearchError> {
    let folders = FolderScan {
        run,
        slots_per_read: slots_per_read.max(1),
    };
    let mut pool = CandidatePool::default();
    let selection = folders.folder_pass(query, &mut pool)?;
    if run.options.scope != SearchScope::Folders {
        let files = FileScan {
            selection,
            name: query.name_seg.as_ref(),
        };
        file_scan::scan_files(run, files, &mut pool)?;
    }
    Ok(pool.into_batch())
}

struct FolderScan<'scan, 'run> {
    run: &'scan TreeSearchRun<'run>,
    slots_per_read: usize,
}

impl FolderScan<'_, '_> {
    fn folder_pass(
        &self,
        query: &ParsedQuery,
        pool: &mut CandidatePool,
    ) -> Result<FileSelection, SearchError> {
        if query.name_seg.is_some() {
            return self.name_pass(query, pool);
        }
        if let Some(anchor) = &query.anchor {
            return self.list_parents(&[anchor.path(PathStyle::Posix)], pool);
        }
        if query.browse {
            return self.browse_pass(query, pool);
        }
        Ok(FileSelection::nothing())
    }

    fn name_pass(
        &self,
        query: &ParsedQuery,
        pool: &mut CandidatePool,
    ) -> Result<FileSelection, SearchError> {
        let pass = FolderPass {
            name: query.name_seg.as_ref().filter(|_| self.lists_folders()),
            root_segment: query.folder_segs.last(),
        };
        let roots = self.scan(pass, pool)?;
        if let Some(anchor) = &query.anchor {
            let anchor_slots = self.resolve(&[anchor.path(PathStyle::Posix)])?;
            return Ok(FileSelection::Below(anchor_slots));
        }
        if query.folder_segs.is_empty() {
            return Ok(FileSelection::Everywhere);
        }
        Ok(FileSelection::Below(roots))
    }

    fn browse_pass(
        &self,
        query: &ParsedQuery,
        pool: &mut CandidatePool,
    ) -> Result<FileSelection, SearchError> {
        let parent_query = query.browse_parents();
        let parent_pass = FolderPass {
            name: parent_query.name_seg.as_ref(),
            root_segment: None,
        };
        let mut parents = CandidatePool::default();
        self.scan(parent_pass, &mut parents)?;
        let top: Vec<String> = rank_candidates(
            parents.into_batch().candidates,
            &parent_query,
            PathStyle::Posix,
        )
        .into_iter()
        .take(BROWSE_PARENT_CAP)
        .map(|hit| hit.path)
        .collect();
        self.list_parents(&top, pool)
    }

    fn list_parents(
        &self,
        parents: &[String],
        pool: &mut CandidatePool,
    ) -> Result<FileSelection, SearchError> {
        let parent_slots = self.resolve(parents)?;
        self.list_folders_in(&parent_slots, pool)?;
        Ok(FileSelection::Directly(parent_slots))
    }

    fn lists_folders(&self) -> bool {
        self.run.options.scope != SearchScope::Files
    }

    fn resolve(&self, paths: &[String]) -> Result<Vec<u32>, SearchError> {
        let mut slots = Vec::new();
        self.run.source.read(&mut |view| {
            slots = paths
                .iter()
                .filter_map(|path| view.tree.lookup(Path::new(path)))
                .collect();
        })?;
        Ok(slots)
    }

    fn list_folders_in(
        &self,
        parents: &[u32],
        pool: &mut CandidatePool,
    ) -> Result<(), SearchError> {
        if !self.lists_folders() {
            return Ok(());
        }
        let include_hidden = self.run.options.include_hidden;
        self.run.source.read(&mut |view| {
            let visible = VisibleTree::new(&view, include_hidden);
            push_children(&visible, parents, pool);
        })
    }

    fn scan(
        &self,
        pass: FolderPass<'_>,
        pool: &mut CandidatePool,
    ) -> Result<Vec<u32>, SearchError> {
        let mut sweep = FolderSweep {
            pass,
            pool,
            roots: Vec::new(),
        };
        if pass.is_idle() {
            return Ok(sweep.roots);
        }
        let mut next_start = Some(0);
        while let Some(start) = next_start {
            next_start = self.sweep_chunk(&mut sweep, start)?;
        }
        Ok(sweep.roots)
    }

    fn sweep_chunk(
        &self,
        sweep: &mut FolderSweep<'_, '_>,
        start: usize,
    ) -> Result<Option<usize>, SearchError> {
        if !self.run.control.proceed()? {
            sweep.pool.mark_truncated();
            return Ok(None);
        }
        let chunk = SlotChunk {
            start,
            len: self.slots_per_read,
        };
        let read = self.read_slot_chunk(sweep.pass, chunk)?;
        sweep.pool.extend(read.hits);
        sweep.roots.extend(read.roots);
        let reached_end = read.end >= read.slot_count;
        Ok((!reached_end).then_some(read.end))
    }

    fn read_slot_chunk<'query>(
        &self,
        pass: FolderPass<'query>,
        chunk: SlotChunk,
    ) -> Result<SlotChunkRead<'query>, SearchError> {
        let include_hidden = self.run.options.include_hidden;
        let mut read = SlotChunkRead::new(pass, chunk);
        self.run.source.read(&mut |view| {
            let visible = VisibleTree::new(&view, include_hidden);
            read.collect(&visible);
        })?;
        Ok(read)
    }
}

#[derive(Clone, Copy)]
struct FolderPass<'query> {
    name: Option<&'query Segment>,
    root_segment: Option<&'query Segment>,
}

impl FolderPass<'_> {
    fn is_idle(&self) -> bool {
        self.name.is_none() && self.root_segment.is_none()
    }

    fn is_root(&self, name: &str) -> bool {
        self.root_segment
            .is_some_and(|segment| segment.match_name(name, NameKind::Folder).is_some())
    }
}

struct FolderSweep<'query, 'pool> {
    pass: FolderPass<'query>,
    pool: &'pool mut CandidatePool,
    roots: Vec<u32>,
}

#[derive(Clone, Copy)]
struct SlotChunk {
    start: usize,
    len: usize,
}

struct SlotChunkRead<'query> {
    pass: FolderPass<'query>,
    chunk: SlotChunk,
    end: usize,
    slot_count: usize,
    hits: Vec<TieredCandidate>,
    roots: Vec<u32>,
}

impl<'query> SlotChunkRead<'query> {
    fn new(pass: FolderPass<'query>, chunk: SlotChunk) -> Self {
        Self {
            pass,
            chunk,
            end: chunk.start,
            slot_count: 0,
            hits: Vec::new(),
            roots: Vec::new(),
        }
    }

    fn collect(&mut self, visible: &VisibleTree<'_>) {
        self.slot_count = visible.tree.slot_len();
        self.end = (self.chunk.start + self.chunk.len).min(self.slot_count);
        for id in self.chunk.start..self.end {
            self.inspect(visible, id as u32);
        }
    }

    fn inspect(&mut self, visible: &VisibleTree<'_>, id: u32) {
        let Some(node) = visible.tree.live_node(id) else {
            return;
        };
        if self.pass.is_root(node.name) {
            self.roots.push(id);
        }
        let Some(name) = self.pass.name else {
            return;
        };
        let Some(found) = name.match_name(node.name, NameKind::Folder) else {
            return;
        };
        let Some(path) = visible.path_of(id) else {
            return;
        };
        self.hits.push(TieredCandidate {
            tier: found.tier,
            candidate: folder(path),
        });
    }
}

pub(crate) struct VisibleTree<'tree> {
    pub(crate) tree: &'tree FolderSizeTree,
    blocked: Vec<String>,
    include_hidden: bool,
}

impl<'tree> VisibleTree<'tree> {
    pub(crate) fn new(view: &IndexRead<'tree>, include_hidden: bool) -> Self {
        Self {
            tree: view.tree,
            blocked: blocked_prefixes(view.blocked),
            include_hidden,
        }
    }

    pub(crate) fn hides_name(&self, name: &str) -> bool {
        !self.include_hidden && name.starts_with(HIDDEN_NAME_PREFIX)
    }

    pub(crate) fn is_blocked(&self, path: &str) -> bool {
        self.blocked
            .iter()
            .any(|prefix| path.starts_with(prefix.as_str()))
    }

    pub(crate) fn path_of(&self, id: u32) -> Option<String> {
        let mut names = Vec::new();
        let mut cursor = Some(id);
        while let Some(current) = cursor {
            let node = self.tree.live_node(current)?;
            names.push(node.name);
            cursor = node.parent;
        }
        let (root, rest) = names.split_last()?;
        let components: Vec<&str> = rest.iter().rev().copied().collect();
        let has_hidden_component = components.iter().any(|name| self.hides_name(name));
        if has_hidden_component {
            return None;
        }
        let path = PathStyle::Posix.compose(root, &components);
        if self.is_blocked(&path) {
            return None;
        }
        Some(path)
    }
}

fn push_children(visible: &VisibleTree<'_>, parents: &[u32], pool: &mut CandidatePool) {
    let child_paths = parents
        .iter()
        .flat_map(|&parent| visible.tree.child_ids(parent))
        .filter_map(|&child| visible.path_of(child));
    for path in child_paths {
        let has_room = pool.push_listed(folder(path));
        if !has_room {
            return;
        }
    }
}

fn blocked_prefixes(blocked: &[PathBuf]) -> Vec<String> {
    blocked
        .iter()
        .map(|root| {
            let trimmed = root.to_string_lossy();
            format!(
                "{}{PATH_SEPARATOR}",
                trimmed.trim_end_matches(PATH_SEPARATOR)
            )
        })
        .collect()
}

fn folder(path: String) -> Candidate {
    Candidate {
        path,
        is_dir: true,
        modified: None,
    }
}
