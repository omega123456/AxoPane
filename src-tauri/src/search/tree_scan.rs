use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use super::file_scan::{self, FileScan, FileSelection};
use super::matcher::NameKind;
use super::path_style::PathStyle;
use super::query::{ParsedQuery, Segment};
use super::results::{
    rank_candidates, CandidatePool, Tiered, TieredCandidate, TieredPool, BROWSE_PARENT_CAP,
};
use super::source::FolderTreeSource;
use super::{Candidate, CandidateBatch, SearchError, SearchRun, SearchScope};
use crate::resource_coordinator::scan_pool::{map_parts_in_order, split_range};
use crate::size::index::tree::FolderSizeTree;
use crate::size::index::IndexRead;

pub const SLOTS_PER_READ: usize = 32_768;

const FIRST_ID: u32 = 0;
const HIDDEN_NAME_PREFIX: char = '.';
const PATH_SEPARATOR: char = '/';

pub type TreeSearchRun<'run> = SearchRun<'run, dyn FolderTreeSource + 'run>;

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
    let first = attempt(run, query, slots_per_read)?;
    if first.saw_one_generation {
        return Ok(first.batch);
    }
    let mut second = attempt(run, query, slots_per_read)?;
    second.batch.truncated |= !second.saw_one_generation;
    Ok(second.batch)
}

struct Attempt {
    batch: CandidateBatch,
    saw_one_generation: bool,
}

fn attempt(
    run: &TreeSearchRun<'_>,
    query: &ParsedQuery,
    slots_per_read: usize,
) -> Result<Attempt, SearchError> {
    let watched = GenerationWatch::over(run.source);
    let watched_run = SearchRun {
        source: &watched as &dyn FolderTreeSource,
        options: run.options,
        control: run.control,
    };
    let batch = collect_candidates(&watched_run, query, slots_per_read)?;
    Ok(Attempt {
        batch,
        saw_one_generation: !watched.changed.load(Ordering::Relaxed),
    })
}

struct GenerationWatch<'source> {
    source: &'source dyn FolderTreeSource,
    first_seen: Mutex<Option<u64>>,
    changed: AtomicBool,
}

impl<'source> GenerationWatch<'source> {
    fn over(source: &'source dyn FolderTreeSource) -> Self {
        Self {
            source,
            first_seen: Mutex::new(None),
            changed: AtomicBool::new(false),
        }
    }

    fn observe(&self, generation: u64) {
        let mut first_seen = self
            .first_seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let first = *first_seen.get_or_insert(generation);
        if first != generation {
            self.changed.store(true, Ordering::Relaxed);
        }
    }
}

impl FolderTreeSource for GenerationWatch<'_> {
    fn read(&self, read: &mut dyn FnMut(IndexRead<'_>)) -> Result<(), SearchError> {
        self.source.read(&mut |view| {
            self.observe(view.generation);
            read(view);
        })
    }
}

fn collect_candidates(
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
            found: TieredPool::default(),
            roots: Vec::new(),
        };
        if pass.is_idle() {
            return Ok(sweep.roots);
        }
        let mut next_start = Some(FIRST_ID);
        while let Some(start) = next_start {
            next_start = self.sweep_chunk(&mut sweep, start)?;
        }
        self.add_folder_hits(sweep.found, pool)?;
        Ok(sweep.roots)
    }

    fn add_folder_hits(
        &self,
        found: TieredPool<u32>,
        pool: &mut CandidatePool,
    ) -> Result<(), SearchError> {
        let kept = found.into_kept();
        if kept.truncated {
            pool.mark_truncated();
        }
        let include_hidden = self.run.options.include_hidden;
        let mut hits = Vec::new();
        self.run.source.read(&mut |view| {
            let visible = VisibleTree::new(&view, include_hidden);
            hits = folder_hits(&visible, &kept.items);
        })?;
        pool.extend(hits);
        Ok(())
    }

    fn sweep_chunk(
        &self,
        sweep: &mut FolderSweep<'_>,
        start: u32,
    ) -> Result<Option<u32>, SearchError> {
        if !self.run.control.proceed()? {
            sweep.found.mark_truncated();
            return Ok(None);
        }
        let chunk = SlotChunk {
            start,
            len: self.slots_per_read,
        };
        let read = self.read_slot_chunk(sweep.pass, chunk)?;
        sweep.found.extend(read.hits);
        sweep.roots.extend(read.roots);
        Ok(read.next_start)
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

struct FolderSweep<'query> {
    pass: FolderPass<'query>,
    found: TieredPool<u32>,
    roots: Vec<u32>,
}

#[derive(Clone, Copy)]
struct SlotChunk {
    start: u32,
    len: usize,
}

struct SlotChunkRead<'query> {
    pass: FolderPass<'query>,
    chunk: SlotChunk,
    next_start: Option<u32>,
    hits: Vec<Tiered<u32>>,
    roots: Vec<u32>,
}

impl<'query> SlotChunkRead<'query> {
    fn new(pass: FolderPass<'query>, chunk: SlotChunk) -> Self {
        Self {
            pass,
            chunk,
            next_start: None,
            hits: Vec::new(),
            roots: Vec::new(),
        }
    }

    fn collect(&mut self, visible: &VisibleTree<'_>) {
        let Some(ids) = visible.tree.ids_from(self.chunk.start, self.chunk.len) else {
            return;
        };
        let has_more = visible.tree.ids_from(ids.end, self.chunk.len).is_some();
        self.next_start = has_more.then_some(ids.end);
        let scan = PartScan {
            pass: self.pass,
            visible,
        };
        for part in inspect_in_parallel(scan, ids) {
            self.hits.extend(part.hits);
            self.roots.extend(part.roots);
        }
    }
}

#[derive(Default)]
struct InspectedPart {
    hits: Vec<Tiered<u32>>,
    roots: Vec<u32>,
}

#[derive(Clone, Copy)]
struct PartScan<'scan, 'query> {
    pass: FolderPass<'query>,
    visible: &'scan VisibleTree<'scan>,
}

impl PartScan<'_, '_> {
    fn inspect_range(self, ids: Range<u32>) -> InspectedPart {
        let mut part = InspectedPart::default();
        for id in ids {
            self.inspect(id, &mut part);
        }
        part
    }

    fn inspect(self, id: u32, part: &mut InspectedPart) {
        let Some(node) = self.visible.tree.live_node(id) else {
            return;
        };
        if self.pass.is_root(node.name) {
            part.roots.push(id);
        }
        let Some(name) = self.pass.name else {
            return;
        };
        let Some(found) = name.match_name(node.name, NameKind::Folder) else {
            return;
        };
        if self.visible.shows(id) {
            part.hits.push(Tiered {
                tier: found.tier,
                item: id,
            });
        }
    }
}

fn inspect_in_parallel(scan: PartScan<'_, '_>, ids: Range<u32>) -> Vec<InspectedPart> {
    map_parts_in_order(split_range(ids), |part| scan.inspect_range(part))
}

fn folder_hits(visible: &VisibleTree<'_>, kept: &[Tiered<u32>]) -> Vec<TieredCandidate> {
    kept.iter()
        .filter_map(|hit| {
            let path = visible.path_of(hit.item)?;
            Some(Tiered {
                tier: hit.tier,
                item: folder(path),
            })
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineageStep {
    Shown,
    Hidden,
    Up(u32),
}

pub(crate) struct VisibleTree<'tree> {
    pub(crate) tree: &'tree FolderSizeTree,
    blocked: Vec<String>,
    blocked_ids: Vec<u32>,
    include_hidden: bool,
}

impl<'tree> VisibleTree<'tree> {
    pub(crate) fn new(view: &IndexRead<'tree>, include_hidden: bool) -> Self {
        Self {
            tree: view.tree,
            blocked: blocked_prefixes(view.blocked),
            blocked_ids: view
                .blocked
                .iter()
                .filter_map(|root| view.tree.lookup(root))
                .collect(),
            include_hidden,
        }
    }

    fn shows(&self, id: u32) -> bool {
        let mut step = self.lineage_step(id, id);
        while let LineageStep::Up(parent) = step {
            step = self.lineage_step(id, parent);
        }
        step == LineageStep::Shown
    }

    fn lineage_step(&self, id: u32, current: u32) -> LineageStep {
        let Some(node) = self.tree.live_node(current) else {
            return LineageStep::Hidden;
        };
        let is_blocked_ancestor = current != id && self.blocked_ids.contains(&current);
        if is_blocked_ancestor {
            return LineageStep::Hidden;
        }
        let Some(parent) = node.parent else {
            return LineageStep::Shown;
        };
        if self.hides_name(node.name) {
            return LineageStep::Hidden;
        }
        LineageStep::Up(parent)
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
        .flat_map(|&parent| visible.tree.child_ids(parent).iter())
        .filter_map(|child| visible.path_of(child));
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
