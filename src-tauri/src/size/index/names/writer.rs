use std::io;
use std::path::{Path, PathBuf};

use super::block::{BlockBuilder, StoredBlock};
use super::file::{NamesFileWriter, NamesHeader};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RewriteStats {
    pub blocks_rebuilt: u64,
    pub blocks_copied: u64,
}

impl RewriteStats {
    pub fn absorb(&mut self, other: Self) {
        self.blocks_rebuilt += other.blocks_rebuilt;
        self.blocks_copied += other.blocks_copied;
    }
}

#[derive(Debug)]
pub enum RewriteError {
    Io(io::Error),
    Unreadable,
}

impl From<io::Error> for RewriteError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug)]
pub(crate) struct FinishedNames {
    pub(crate) temp: PathBuf,
    pub(crate) stats: RewriteStats,
}

pub(crate) struct BlockOutput {
    writer: NamesFileWriter,
    open: Option<BlockBuilder>,
    block_target: usize,
    stats: RewriteStats,
}

impl BlockOutput {
    pub(crate) fn create(path: &Path, block_target: usize) -> io::Result<Self> {
        Ok(Self {
            writer: NamesFileWriter::create(path)?,
            open: None,
            block_target,
            stats: RewriteStats::default(),
        })
    }

    pub(crate) fn finish(mut self, header: NamesHeader) -> Result<FinishedNames, RewriteError> {
        if let Err(error) = self.close_open_block() {
            self.discard();
            return Err(error);
        }
        let stats = self.stats;
        let temp = self.writer.finish(header)?;
        Ok(FinishedNames { temp, stats })
    }

    pub(crate) fn discard(self) {
        self.writer.discard();
    }

    pub(crate) fn push(&mut self, ordinal: u32, names: &[u8]) -> Result<(), RewriteError> {
        let mut builder = self
            .open
            .take()
            .unwrap_or_else(|| BlockBuilder::new(ordinal));
        builder.push(ordinal, names);
        if builder.payload_len() < self.block_target {
            self.open = Some(builder);
            return Ok(());
        }
        self.write_built(builder)
    }

    pub(crate) fn append_copy(
        &mut self,
        first_ordinal: u32,
        block: StoredBlock<'_>,
    ) -> Result<(), RewriteError> {
        self.close_open_block()?;
        self.writer.append(block, first_ordinal)?;
        self.stats.blocks_copied += 1;
        Ok(())
    }

    fn close_open_block(&mut self) -> Result<(), RewriteError> {
        let Some(open) = self.open.take() else {
            return Ok(());
        };
        self.write_built(open)
    }

    fn write_built(&mut self, builder: BlockBuilder) -> Result<(), RewriteError> {
        let block = builder.finish();
        self.writer
            .append(block.stored(), block.span.first_ordinal)?;
        self.stats.blocks_rebuilt += 1;
        Ok(())
    }
}
