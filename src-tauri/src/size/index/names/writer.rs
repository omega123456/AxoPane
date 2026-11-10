use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::block::BlockBuilder;
use super::codec::NamesEncoder;
use super::file::{NamesFileWriter, NamesHeader};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RewriteStats {
    pub blocks_reencoded: u64,
    pub peak_open_blocks: u64,
}

impl RewriteStats {
    pub fn absorb(&mut self, other: Self) {
        self.blocks_reencoded += other.blocks_reencoded;
        self.peak_open_blocks = self.peak_open_blocks.max(other.peak_open_blocks);
    }
}

#[derive(Debug)]
pub enum RewriteError {
    Io(io::Error),
    Unreadable,
    DeadlinePassed,
}

impl From<io::Error> for RewriteError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

fn finish_output(
    output: NamesFileWriter,
    header: NamesHeader,
    deadline: Option<Instant>,
) -> Result<PathBuf, RewriteError> {
    if is_past(deadline) {
        output.discard();
        return Err(RewriteError::DeadlinePassed);
    }
    Ok(output.finish(header)?)
}

pub fn is_past(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|deadline| Instant::now() >= deadline)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct OutputLimits {
    pub(crate) block_target: usize,
    pub(crate) deadline: Option<Instant>,
}

#[derive(Debug)]
pub(crate) struct FinishedNames {
    pub(crate) temp: PathBuf,
    pub(crate) stats: RewriteStats,
}

pub(crate) struct BlockOutput {
    writer: NamesFileWriter,
    encoder: NamesEncoder,
    open: Option<BlockBuilder>,
    block_target: usize,
    deadline: Option<Instant>,
    stats: RewriteStats,
    open_blocks: u64,
}

impl BlockOutput {
    pub(crate) fn create(path: &Path, limits: OutputLimits) -> io::Result<Self> {
        Ok(Self {
            writer: NamesFileWriter::create(path)?,
            encoder: NamesEncoder::new()?,
            open: None,
            block_target: limits.block_target,
            deadline: limits.deadline,
            stats: RewriteStats::default(),
            open_blocks: 0,
        })
    }

    pub(crate) fn finish(mut self, header: NamesHeader) -> Result<FinishedNames, RewriteError> {
        if let Err(error) = self.close_open_block() {
            self.discard();
            return Err(error);
        }
        let stats = self.stats;
        let temp = finish_output(self.writer, header, self.deadline)?;
        Ok(FinishedNames { temp, stats })
    }

    pub(crate) fn discard(self) {
        self.writer.discard();
    }

    fn start_block(&mut self, ordinal: u32) -> BlockBuilder {
        self.open_blocks += 1;
        self.stats.peak_open_blocks = self.stats.peak_open_blocks.max(self.open_blocks);
        BlockBuilder::new(ordinal)
    }

    pub(crate) fn push(&mut self, ordinal: u32, names: &[u8]) -> Result<(), RewriteError> {
        let mut builder = match self.open.take() {
            Some(open) => open,
            None => self.start_block(ordinal),
        };
        builder.push(ordinal, names);
        if builder.raw_len() >= self.block_target {
            return self.finish_block(builder);
        }
        self.open = Some(builder);
        Ok(())
    }

    fn close_open_block(&mut self) -> Result<(), RewriteError> {
        let Some(builder) = self.open.take() else {
            return Ok(());
        };
        self.finish_block(builder)
    }

    fn finish_block(&mut self, builder: BlockBuilder) -> Result<(), RewriteError> {
        if is_past(self.deadline) {
            return Err(RewriteError::DeadlinePassed);
        }
        let encoded = builder.finish(&mut self.encoder)?;
        self.writer
            .append(encoded.stored(), encoded.span.first_ordinal)?;
        self.open_blocks -= 1;
        self.stats.blocks_reencoded += 1;
        Ok(())
    }
}
