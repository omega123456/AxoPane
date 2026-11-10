use std::io;
use std::path::{Path, PathBuf};

use super::block::{BlockBuilder, EncodedBlock};
use super::codec::NamesEncoder;
use super::file::{NamesFileWriter, NamesHeader};
use crate::resource_coordinator::scan_pool::map_parts_in_order;

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
}

impl From<io::Error> for RewriteError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub(crate) const ONE_BLOCK_AT_A_TIME: usize = 1;

#[derive(Debug, Clone, Copy)]
pub(crate) struct OutputLimits {
    pub(crate) block_target: usize,
    pub(crate) compress_batch: usize,
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
    filled: Vec<BlockBuilder>,
    block_target: usize,
    compress_batch: usize,
    stats: RewriteStats,
    open_blocks: u64,
}

fn encode_with_own_encoder(builder: BlockBuilder) -> io::Result<EncodedBlock> {
    let mut encoder = NamesEncoder::new()?;
    builder.finish(&mut encoder)
}

impl BlockOutput {
    pub(crate) fn create(path: &Path, limits: OutputLimits) -> io::Result<Self> {
        Ok(Self {
            writer: NamesFileWriter::create(path)?,
            encoder: NamesEncoder::new()?,
            open: None,
            filled: Vec::new(),
            block_target: limits.block_target,
            compress_batch: limits.compress_batch.max(ONE_BLOCK_AT_A_TIME),
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
        let temp = self.writer.finish(header)?;
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
        if builder.raw_len() < self.block_target {
            self.open = Some(builder);
            return Ok(());
        }
        self.filled.push(builder);
        if self.filled.len() >= self.compress_batch {
            return self.write_filled();
        }
        Ok(())
    }

    fn close_open_block(&mut self) -> Result<(), RewriteError> {
        self.filled.extend(self.open.take());
        self.write_filled()
    }

    fn write_filled(&mut self) -> Result<(), RewriteError> {
        let filled = std::mem::take(&mut self.filled);
        let written = filled.len() as u64;
        for encoded in self.encode(filled)? {
            self.writer
                .append(encoded.stored(), encoded.span.first_ordinal)?;
        }
        self.open_blocks -= written;
        self.stats.blocks_reencoded += written;
        Ok(())
    }

    fn encode(&mut self, filled: Vec<BlockBuilder>) -> io::Result<Vec<EncodedBlock>> {
        if self.compress_batch == ONE_BLOCK_AT_A_TIME {
            return filled
                .into_iter()
                .map(|builder| builder.finish(&mut self.encoder))
                .collect();
        }
        map_parts_in_order(filled, encode_with_own_encoder)
            .into_iter()
            .collect()
    }
}
