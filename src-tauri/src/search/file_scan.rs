use std::iter;
use std::ops::ControlFlow;

use super::matcher::NameKind;
use super::path_style::PathStyle;
use super::query::Segment;
use super::results::{CandidatePool, TieredCandidate, LISTED_TIER};
use super::tree_scan::{TreeSearchRun, VisibleTree};
use super::{Candidate, SearchError};
use crate::resource_coordinator::scan_pool::{map_parts_in_order, split_slice};
use crate::size::index::names::signature::SignedName;
use crate::size::index::names::{FileNameStore, FolderNames, NamesUnreadable};
use crate::size::index::tree::{FolderSizeTree, IdRanges};
use crate::size::index::IndexRead;

pub const BLOCKS_PER_READ: usize = 8;

pub enum FileSelection {
    Everywhere,
    Below(Vec<u32>),
    Directly(Vec<u32>),
}

impl FileSelection {
    pub fn nothing() -> Self {
        Self::Directly(Vec::new())
    }

    fn reaches_subtrees(&self) -> bool {
        matches!(self, Self::Everywhere | Self::Below(_))
    }
}

pub struct FileScan<'scan> {
    pub selection: FileSelection,
    pub name: Option<&'scan Segment>,
}

pub fn scan_files(
    run: &TreeSearchRun<'_>,
    scan: FileScan<'_>,
    pool: &mut CandidatePool,
) -> Result<(), SearchError> {
    let mut scanner = FileScanner {
        run,
        name: scan.name,
        pool,
    };
    let Some(plan) = scanner.plan(&scan.selection)? else {
        return Ok(());
    };
    for chunk in plan.chunks() {
        let progress = scanner.scan_chunk(&plan, chunk)?;
        if progress.is_break() {
            return Ok(());
        }
    }
    Ok(())
}

struct FileScanner<'scan, 'run> {
    run: &'scan TreeSearchRun<'run>,
    name: Option<&'scan Segment>,
    pool: &'scan mut CandidatePool,
}

impl FileScanner<'_, '_> {
    fn plan(&self, selection: &FileSelection) -> Result<Option<ScanPlan>, SearchError> {
        let include_hidden = self.run.options.include_hidden;
        let mut plan = None;
        self.run.source.read(&mut |view| {
            plan = ScanPlan::for_view(&view, selection, include_hidden);
        })?;
        Ok(plan)
    }

    fn scan_chunk(
        &mut self,
        plan: &ScanPlan,
        chunk: NamesChunk<'_>,
    ) -> Result<ControlFlow<()>, SearchError> {
        if !self.run.control.proceed()? {
            self.pool.mark_truncated();
            return Ok(ControlFlow::Break(()));
        }
        let include_hidden = self.run.options.include_hidden;
        let name = self.name;
        let mut collected = Collected::default();
        self.run.source.read(&mut |view| {
            let matcher = FileMatcher {
                visible: VisibleTree::new(&view, include_hidden),
                name,
                members: plan.members.as_ref(),
                excluded: &plan.excluded,
            };
            collected = collect(chunk, view.names, &matcher);
        })?;
        Ok(self.absorb(collected))
    }

    fn absorb(&mut self, collected: Collected) -> ControlFlow<()> {
        self.pool.extend(collected.hits);
        if collected.unreadable {
            self.pool.mark_truncated();
            return ControlFlow::Break(());
        }
        let listing_overflowed = self.name.is_none() && self.pool.is_over_cap();
        if listing_overflowed {
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    }
}

struct ScanPlan {
    members: Option<IdRanges>,
    excluded: IdRanges,
    blocks: Vec<usize>,
}

impl ScanPlan {
    fn for_view(
        view: &IndexRead<'_>,
        selection: &FileSelection,
        include_hidden: bool,
    ) -> Option<Self> {
        if !view.names.is_available() {
            return None;
        }
        let members = selected_members(view.tree, selection);
        let excluded = excluded_ranges(view, selection, include_hidden);
        let blocks = selected_blocks(view.names, members.as_ref(), &excluded);
        Some(Self {
            members,
            excluded,
            blocks,
        })
    }

    fn chunks(&self) -> impl Iterator<Item = NamesChunk<'_>> {
        let blocks = self.blocks.chunks(BLOCKS_PER_READ).map(NamesChunk::Blocks);
        iter::once(NamesChunk::Overlay).chain(blocks)
    }
}

fn excluded_ranges(
    view: &IndexRead<'_>,
    selection: &FileSelection,
    include_hidden: bool,
) -> IdRanges {
    let mut excluded = view.names.dead_slots().clone();
    let skips_hidden_subtrees = !include_hidden && selection.reaches_subtrees();
    if skips_hidden_subtrees {
        excluded.add_all(view.tree.hidden_ranges());
    }
    excluded
}

fn selected_blocks(
    names: &FileNameStore,
    members: Option<&IdRanges>,
    excluded: &IdRanges,
) -> Vec<usize> {
    let excluded_blocks = names.blocks_within(excluded);
    let candidates = member_blocks(names, members).into_iter();
    let kept = candidates.filter(|block| excluded_blocks.binary_search(block).is_err());
    kept.collect()
}

fn member_blocks(names: &FileNameStore, members: Option<&IdRanges>) -> Vec<usize> {
    let Some(members) = members else {
        return (0..names.block_count()).collect();
    };
    names.blocks_covering(members.ranges())
}

fn selected_members(tree: &FolderSizeTree, selection: &FileSelection) -> Option<IdRanges> {
    match selection {
        FileSelection::Everywhere => None,
        FileSelection::Below(roots) => Some(subtrees_below(tree, roots)),
        FileSelection::Directly(folders) => Some(live_folders(tree, folders)),
    }
}

fn subtrees_below(tree: &FolderSizeTree, roots: &[u32]) -> IdRanges {
    let mut members = IdRanges::default();
    for &root in roots {
        if !members.contains(root) {
            members.add_all(tree.subtree_ranges(root));
        }
    }
    members
}

fn live_folders(tree: &FolderSizeTree, folders: &[u32]) -> IdRanges {
    let mut members = IdRanges::default();
    let live = folders
        .iter()
        .copied()
        .filter(|&folder| tree.live_node(folder).is_some());
    for folder in live {
        members.add(folder..folder + 1);
    }
    members
}

#[derive(Clone, Copy)]
enum NamesChunk<'plan> {
    Overlay,
    Blocks(&'plan [usize]),
}

#[derive(Default)]
struct Collected {
    hits: Vec<TieredCandidate>,
    unreadable: bool,
}

impl Collected {
    fn joined(parts: Vec<Self>) -> Self {
        let unreadable = parts.iter().any(|part| part.unreadable);
        let hits = parts.into_iter().flat_map(|part| part.hits).collect();
        Self { hits, unreadable }
    }
}

fn collect(chunk: NamesChunk<'_>, names: &FileNameStore, matcher: &FileMatcher<'_>) -> Collected {
    match chunk {
        NamesChunk::Overlay => collect_overlay(names, matcher),
        NamesChunk::Blocks(indexes) => collect_blocks(names, indexes, matcher),
    }
}

fn collect_overlay(names: &FileNameStore, matcher: &FileMatcher<'_>) -> Collected {
    let mut hits = Vec::new();
    names.visit_overlay(&mut |folder| matcher.collect_hits(folder, &mut hits));
    Collected {
        hits,
        unreadable: false,
    }
}

fn collect_blocks(
    names: &FileNameStore,
    indexes: &[usize],
    matcher: &FileMatcher<'_>,
) -> Collected {
    let parts = map_parts_in_order(split_slice(indexes), |part| {
        collect_block_part(names, part, matcher)
    });
    Collected::joined(parts)
}

fn collect_block_part(
    names: &FileNameStore,
    indexes: &[usize],
    matcher: &FileMatcher<'_>,
) -> Collected {
    let mut hits = Vec::new();
    let block_count = names.block_count();
    let mut existing = indexes.iter().filter(|&&index| index < block_count);
    let visited = existing.try_for_each(|&index| {
        let block = names.read_block(index)?;
        block
            .folders()
            .for_each(|folder| matcher.collect_hits(folder, &mut hits));
        Ok::<(), NamesUnreadable>(())
    });
    Collected {
        hits,
        unreadable: visited.is_err(),
    }
}

struct FileMatcher<'scan> {
    visible: VisibleTree<'scan>,
    name: Option<&'scan Segment>,
    members: Option<&'scan IdRanges>,
    excluded: &'scan IdRanges,
}

struct NameHit {
    name: String,
    tier: u8,
}

impl FileMatcher<'_> {
    fn collect_hits(&self, folder: FolderNames<'_>, hits: &mut Vec<TieredCandidate>) {
        if !self.selects(folder.slot) {
            return;
        }
        let matched = self.matched_names(folder);
        if matched.is_empty() {
            return;
        }
        let Some(folder_path) = self.visible.path_of(folder.slot) else {
            return;
        };
        let candidates = matched
            .into_iter()
            .filter_map(|hit| self.candidate(&folder_path, hit));
        hits.extend(candidates);
    }

    fn selects(&self, slot: u32) -> bool {
        if self.excluded.contains(slot) {
            return false;
        }
        self.members.is_none_or(|members| members.contains(slot))
    }

    fn matched_names(&self, folder: FolderNames<'_>) -> Vec<NameHit> {
        let Some(segment) = self.name else {
            return self.listed_names(folder);
        };
        let mut matched = Vec::new();
        folder.visit_admitted(segment.prefilter(), &mut |name| {
            matched.extend(self.matched_name(segment, name));
        });
        matched
    }

    fn listed_names(&self, folder: FolderNames<'_>) -> Vec<NameHit> {
        let listed = folder
            .names()
            .filter_map(|raw| self.shown_hit(raw, LISTED_TIER));
        listed.collect()
    }

    fn matched_name(&self, segment: &Segment, name: SignedName<'_>) -> Option<NameHit> {
        if self.visible.hides_name(name.bytes) {
            return None;
        }
        let matched = segment.match_raw(name, NameKind::File)?;
        Some(NameHit {
            name: matched.text.into_owned(),
            tier: matched.found.tier,
        })
    }

    fn shown_hit(&self, raw: &[u8], tier: u8) -> Option<NameHit> {
        if self.visible.hides_name(raw) {
            return None;
        }
        let name = String::from_utf8_lossy(raw);
        Some(NameHit {
            name: name.into_owned(),
            tier,
        })
    }

    fn candidate(&self, folder_path: &str, hit: NameHit) -> Option<TieredCandidate> {
        let path = PathStyle::Posix.join(folder_path, &hit.name);
        if self.visible.is_blocked(&path) {
            return None;
        }
        Some(TieredCandidate {
            tier: hit.tier,
            item: Candidate {
                path,
                is_dir: false,
                modified: None,
            },
        })
    }
}
