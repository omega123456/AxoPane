use super::builder::{BuildError, SegmentBuilder, SegmentEntry};
use crate::size::index::names::{FileNameStore, NamesCursor, NamesUnreadable, ReusableBlock};
use crate::size::index::tree::{DepthFirstCursor, FolderSizeTree, VisitedFolder};

pub const FOLDERS_PER_REWRITE_STEP: usize = 4_096;

#[derive(Debug)]
pub enum RewriteFailure {
    NamesUnreadable,
    Build(BuildError),
}

impl From<BuildError> for RewriteFailure {
    fn from(error: BuildError) -> Self {
        Self::Build(error)
    }
}

impl From<NamesUnreadable> for RewriteFailure {
    fn from(_unreadable: NamesUnreadable) -> Self {
        Self::NamesUnreadable
    }
}

#[derive(Clone, Copy)]
pub struct RewriteSource<'index> {
    pub tree: &'index FolderSizeTree,
    pub names: &'index FileNameStore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewriteProgress {
    MoreFolders,
    Finished,
}

struct HeldFolder {
    ordinal: u32,
    slot: u32,
}

struct PendingCopy {
    block: ReusableBlock,
    first_ordinal: u32,
    held: Vec<HeldFolder>,
}

impl PendingCopy {
    fn next_slot(&self) -> u32 {
        self.block.first_slot + self.held.len() as u32
    }

    fn is_complete(&self) -> bool {
        self.held.len() as u32 == self.block.folder_count
    }
}

pub struct BaseRewrite {
    folders: DepthFirstCursor,
    names: NamesCursor,
    pending: Option<PendingCopy>,
}

impl BaseRewrite {
    pub fn of(tree: &FolderSizeTree) -> Self {
        Self {
            folders: tree.depth_first(),
            names: NamesCursor::default(),
            pending: None,
        }
    }

    pub fn copy_step(
        &mut self,
        source: RewriteSource<'_>,
        builder: &mut SegmentBuilder,
    ) -> Result<RewriteProgress, RewriteFailure> {
        let mut step = RewriteStep { source, builder };
        let mut progress = RewriteProgress::MoreFolders;
        let mut folders_left_in_step = FOLDERS_PER_REWRITE_STEP;
        while progress == RewriteProgress::MoreFolders && folders_left_in_step > 0 {
            progress = self.copy_next_folder(&mut step)?;
            folders_left_in_step -= 1;
        }
        Ok(progress)
    }

    fn copy_next_folder(
        &mut self,
        step: &mut RewriteStep<'_, '_>,
    ) -> Result<RewriteProgress, RewriteFailure> {
        let Some(folder) = step.source.tree.next_folder(&mut self.folders) else {
            self.write_held_names(step)?;
            return Ok(RewriteProgress::Finished);
        };
        let continues_the_copy = self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.next_slot() == folder.slot);
        if continues_the_copy {
            self.hold(step, &folder)?;
            return Ok(RewriteProgress::MoreFolders);
        }
        self.write_held_names(step)?;
        if let Some(block) = step.source.names.reusable_block_at(folder.slot) {
            self.pending = Some(PendingCopy {
                block,
                first_ordinal: step.builder.node_count(),
                held: Vec::new(),
            });
            self.hold(step, &folder)?;
            return Ok(RewriteProgress::MoreFolders);
        }
        let file_names = step
            .source
            .names
            .names_for_rewrite(folder.slot, &mut self.names)?;
        step.builder.push(entry_of(&folder, file_names))?;
        Ok(RewriteProgress::MoreFolders)
    }

    fn hold(
        &mut self,
        step: &mut RewriteStep<'_, '_>,
        folder: &VisitedFolder<'_>,
    ) -> Result<(), RewriteFailure> {
        let ordinal = step.builder.push_row(entry_of(folder, NO_FILE_NAMES))?;
        if let Some(pending) = &mut self.pending {
            pending.held.push(HeldFolder {
                ordinal,
                slot: folder.slot,
            });
        }
        self.copy_if_complete(step)
    }

    fn copy_if_complete(&mut self, step: &mut RewriteStep<'_, '_>) -> Result<(), RewriteFailure> {
        let complete = self.pending.take_if(|pending| pending.is_complete());
        let Some(copy) = complete else {
            return Ok(());
        };
        let block = step.source.names.copied_block(copy.block.index)?;
        step.builder.copy_names_block(copy.first_ordinal, block)?;
        Ok(())
    }

    fn write_held_names(&mut self, step: &mut RewriteStep<'_, '_>) -> Result<(), RewriteFailure> {
        let Some(pending) = self.pending.take() else {
            return Ok(());
        };
        for held in pending.held {
            let file_names = step
                .source
                .names
                .names_for_rewrite(held.slot, &mut self.names)?;
            step.builder.push_names(held.ordinal, file_names)?;
        }
        Ok(())
    }
}

struct RewriteStep<'step, 'index> {
    source: RewriteSource<'index>,
    builder: &'step mut SegmentBuilder,
}

const NO_FILE_NAMES: &[u8] = &[];

fn entry_of<'folder>(
    folder: &VisitedFolder<'folder>,
    file_names: &'folder [u8],
) -> SegmentEntry<'folder> {
    SegmentEntry {
        depth: folder.depth,
        name: folder.name,
        direct: folder.direct,
        flags: folder.flags,
        file_names,
    }
}
