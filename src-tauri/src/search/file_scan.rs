use std::iter;
use std::ops::ControlFlow;

use super::matcher::NameKind;
use super::path_style::PathStyle;
use super::query::Segment;
use super::results::{CandidatePool, TieredCandidate, LISTED_TIER};
use super::tree_scan::{TreeSearchRun, VisibleTree};
use super::{Candidate, SearchError};
use crate::size::index::names::{FileNameStore, FolderNames, NamesScratch, NamesUnreadable};
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
        scratch: NamesScratch::default(),
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
    scratch: NamesScratch,
    pool: &'scan mut CandidatePool,
}

impl FileScanner<'_, '_> {
    fn plan(&self, selection: &FileSelection) -> Result<Option<ScanPlan>, SearchError> {
        let mut plan = None;
        self.run
            .source
            .read(&mut |view| plan = ScanPlan::for_view(&view, selection))?;
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
        let scratch = &mut self.scratch;
        let mut collected = Collected::default();
        self.run.source.read(&mut |view| {
            let matcher = FileMatcher {
                visible: VisibleTree::new(&view, include_hidden),
                name,
                members: plan.members.as_ref(),
            };
            let reader = NamesReader {
                names: view.names,
                scratch: &mut *scratch,
            };
            collected = collect(chunk, reader, &matcher);
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
    blocks: Vec<usize>,
}

impl ScanPlan {
    fn for_view(view: &IndexRead<'_>, selection: &FileSelection) -> Option<Self> {
        if !view.names.is_available() {
            return None;
        }
        let members = selected_members(view.tree, selection);
        let blocks = selected_blocks(view.names, members.as_ref());
        Some(Self { members, blocks })
    }

    fn chunks(&self) -> impl Iterator<Item = NamesChunk<'_>> {
        let blocks = self.blocks.chunks(BLOCKS_PER_READ).map(NamesChunk::Blocks);
        iter::once(NamesChunk::Overlay).chain(blocks)
    }
}

fn selected_blocks(names: &FileNameStore, members: Option<&IdRanges>) -> Vec<usize> {
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

struct NamesReader<'read> {
    names: &'read FileNameStore,
    scratch: &'read mut NamesScratch,
}

impl NamesReader<'_> {
    fn visit(
        &mut self,
        chunk: NamesChunk<'_>,
        visit: &mut dyn FnMut(FolderNames<'_>),
    ) -> Result<(), NamesUnreadable> {
        match chunk {
            NamesChunk::Overlay => {
                self.names.visit_overlay(visit);
                Ok(())
            }
            NamesChunk::Blocks(indexes) => self.visit_blocks(indexes, visit),
        }
    }

    fn visit_blocks(
        &mut self,
        indexes: &[usize],
        visit: &mut dyn FnMut(FolderNames<'_>),
    ) -> Result<(), NamesUnreadable> {
        let block_count = self.names.block_count();
        let existing = indexes.iter().filter(|&&index| index < block_count);
        for &index in existing {
            let block = self.names.read_block(index, self.scratch)?;
            block.folders().for_each(&mut *visit);
        }
        Ok(())
    }
}

#[derive(Default)]
struct Collected {
    hits: Vec<TieredCandidate>,
    unreadable: bool,
}

fn collect(
    chunk: NamesChunk<'_>,
    mut reader: NamesReader<'_>,
    matcher: &FileMatcher<'_>,
) -> Collected {
    let mut hits = Vec::new();
    let visited = reader.visit(chunk, &mut |folder| matcher.collect_hits(folder, &mut hits));
    Collected {
        hits,
        unreadable: visited.is_err(),
    }
}

struct FileMatcher<'scan> {
    visible: VisibleTree<'scan>,
    name: Option<&'scan Segment>,
    members: Option<&'scan IdRanges>,
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
        let matched: Vec<NameHit> = folder
            .names()
            .filter_map(|raw| self.match_name(raw))
            .collect();
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
        self.members.is_none_or(|members| members.contains(slot))
    }

    fn match_name(&self, raw: &[u8]) -> Option<NameHit> {
        let name = String::from_utf8_lossy(raw);
        if self.visible.hides_name(&name) {
            return None;
        }
        let tier = self.tier_of(&name)?;
        Some(NameHit {
            name: name.into_owned(),
            tier,
        })
    }

    fn tier_of(&self, name: &str) -> Option<u8> {
        let Some(segment) = self.name else {
            return Some(LISTED_TIER);
        };
        segment
            .match_name(name, NameKind::File)
            .map(|found| found.tier)
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
