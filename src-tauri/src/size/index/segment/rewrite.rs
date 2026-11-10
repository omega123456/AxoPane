use super::builder::{BuildError, SegmentBuilder, SegmentEntry};
use crate::size::index::names::{FileNameStore, NamesCursor, NamesUnreadable};
use crate::size::index::tree::{DepthFirstCursor, FolderSizeTree};

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

pub struct BaseRewrite {
    folders: DepthFirstCursor,
    names: NamesCursor,
}

impl BaseRewrite {
    pub fn of(tree: &FolderSizeTree) -> Self {
        Self {
            folders: tree.depth_first(),
            names: NamesCursor::default(),
        }
    }

    pub fn copy_step(
        &mut self,
        source: RewriteSource<'_>,
        builder: &mut SegmentBuilder,
    ) -> Result<RewriteProgress, RewriteFailure> {
        let mut progress = RewriteProgress::MoreFolders;
        let mut folders_left_in_step = FOLDERS_PER_REWRITE_STEP;
        while progress == RewriteProgress::MoreFolders && folders_left_in_step > 0 {
            progress = self.copy_next_folder(source, builder)?;
            folders_left_in_step -= 1;
        }
        Ok(progress)
    }

    fn copy_next_folder(
        &mut self,
        source: RewriteSource<'_>,
        builder: &mut SegmentBuilder,
    ) -> Result<RewriteProgress, RewriteFailure> {
        let Some(folder) = source.tree.next_folder(&mut self.folders) else {
            return Ok(RewriteProgress::Finished);
        };
        let file_names = source
            .names
            .names_for_rewrite(folder.slot, &mut self.names)?;
        builder.push(SegmentEntry {
            depth: folder.depth,
            name: folder.name,
            direct: folder.direct,
            flags: folder.flags,
            file_names,
        })?;
        Ok(RewriteProgress::MoreFolders)
    }
}
