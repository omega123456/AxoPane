use std::io;
use std::path::PathBuf;
use std::time::Instant;

use super::block::{self, BlockBuilder, BlockSpan, EntryRange, StoredBlock};
use super::file::{MappedNames, NamesFileWriter, NamesHeader, SpillFile, SpillLocation};
use super::{MappedState, NamesScratch, Overlay, Spill};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RewriteStats {
    pub blocks_reencoded: u64,
    pub blocks_copied: u64,
    pub peak_open_blocks: u64,
}

impl RewriteStats {
    pub fn absorb(&mut self, other: Self) {
        self.blocks_reencoded += other.blocks_reencoded;
        self.blocks_copied += other.blocks_copied;
        self.peak_open_blocks = self.peak_open_blocks.max(other.peak_open_blocks);
    }
}

#[derive(Debug)]
pub struct PairCapture {
    pub(crate) path: PathBuf,
    pub(crate) block_target: usize,
    pub(crate) order: Vec<u32>,
    pub(crate) overlay: Overlay,
    pub(crate) spill: Option<Spill>,
    pub(crate) mapped: Option<MappedState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairStamp {
    pub event_id: u64,
    pub pair_token: u64,
    pub deadline: Option<Instant>,
}

#[derive(Debug)]
pub struct WrittenNames {
    pub temp: PathBuf,
    pub order: Vec<u32>,
    pub header: NamesHeader,
    pub stats: RewriteStats,
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

pub fn write_names(capture: PairCapture, stamp: PairStamp) -> Result<WrittenNames, RewriteError> {
    let mut rewrite = Rewrite {
        sources: Sources {
            overlay: &capture.overlay,
            spill: capture.spill.as_ref(),
            mapped: capture.mapped.as_ref(),
        },
        reader: NameReader::default(),
        output: BlockOutput {
            writer: NamesFileWriter::create(&capture.path)?,
            open: None,
            block_target: capture.block_target,
            deadline: stamp.deadline,
            stats: RewriteStats::default(),
            open_blocks: 0,
        },
    };
    let result = rewrite.run(&capture.order);
    let BlockOutput { writer, stats, .. } = rewrite.output;
    if let Err(error) = result {
        writer.discard();
        return Err(error);
    }
    let header = NamesHeader {
        event_id: stamp.event_id,
        pair_token: stamp.pair_token,
        folder_count: capture.order.len() as u32,
    };
    let temp = finish_output(writer, header, stamp.deadline)?;
    Ok(WrittenNames {
        temp,
        order: capture.order,
        header,
        stats,
    })
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

#[derive(Clone, Copy)]
struct Sources<'capture> {
    overlay: &'capture Overlay,
    spill: Option<&'capture Spill>,
    mapped: Option<&'capture MappedState>,
}

struct SpillHit<'capture> {
    file: &'capture SpillFile,
    location: SpillLocation,
}

impl<'capture> Sources<'capture> {
    fn spill_hit(&self, slot: u32) -> Option<SpillHit<'capture>> {
        let spill = self.spill?;
        let location = spill.index.location(slot)?;
        Some(SpillHit {
            file: &spill.file,
            location,
        })
    }

    fn overrides(&self, slot: u32) -> bool {
        let in_overlay = self.overlay.get(slot).is_some();
        let in_spill = self.spill_hit(slot).is_some();
        in_overlay || in_spill
    }
}

struct Rewrite<'capture> {
    sources: Sources<'capture>,
    reader: NameReader,
    output: BlockOutput,
}

struct ReusableBlock {
    index: usize,
    span: BlockSpan,
}

impl Rewrite<'_> {
    fn run(&mut self, order: &[u32]) -> Result<(), RewriteError> {
        let mut ordinal = 0;
        while ordinal < order.len() {
            ordinal += self.write_from(order, ordinal)?;
        }
        self.output.close_open_block()
    }

    fn write_from(&mut self, order: &[u32], ordinal: usize) -> Result<usize, RewriteError> {
        if let Some(reusable) = self.reusable_block_at(order, ordinal) {
            return self.copy_block(reusable, ordinal);
        }
        let slot = order[ordinal];
        let names = self.reader.names_of(self.sources, slot)?;
        self.output.push(ordinal as u32, names)?;
        Ok(1)
    }

    fn reusable_block_at(&self, order: &[u32], ordinal: usize) -> Option<ReusableBlock> {
        let mapped = self.sources.mapped?;
        let old_first = mapped.translation.ordinal_of(order[ordinal])?;
        let index = mapped.file.block_starting_at(old_first)?;
        let span = mapped.file.span(index)?;
        let run = order.get(ordinal..ordinal + span.folder_count as usize)?;
        let unchanged = run.iter().zip(span.ordinals()).all(|(&slot, old_ordinal)| {
            let same_position = mapped.translation.ordinal_of(slot) == Some(old_ordinal);
            same_position && !self.sources.overrides(slot)
        });
        unchanged.then_some(ReusableBlock { index, span })
    }

    fn copy_block(
        &mut self,
        reusable: ReusableBlock,
        ordinal: usize,
    ) -> Result<usize, RewriteError> {
        let mapped = self.sources.mapped.ok_or(RewriteError::Unreadable)?;
        let stored = mapped
            .file
            .block(reusable.index)
            .ok_or(RewriteError::Unreadable)?;
        if block::checksum(stored.compressed) != stored.checksum {
            return Err(RewriteError::Unreadable);
        }
        self.output.close_open_block()?;
        self.output.append_copy(stored, ordinal as u32)?;
        Ok(reusable.span.folder_count as usize)
    }
}

struct LoadedBlock {
    index: usize,
    span: BlockSpan,
    entries: Vec<EntryRange>,
}

#[derive(Default)]
struct NameReader {
    block_scratch: NamesScratch,
    spill_scratch: NamesScratch,
    loaded: Option<LoadedBlock>,
}

impl NameReader {
    fn names_of<'read>(
        &'read mut self,
        sources: Sources<'read>,
        slot: u32,
    ) -> Result<&'read [u8], RewriteError> {
        if let Some(entry) = sources.overlay.get(slot) {
            return Ok(entry.packed());
        }
        if let Some(hit) = sources.spill_hit(slot) {
            return hit
                .file
                .read(hit.location, &mut self.spill_scratch)
                .map_err(|_| RewriteError::Unreadable);
        }
        let Some(mapped) = sources.mapped else {
            return Ok(&[]);
        };
        let Some(ordinal) = mapped.translation.ordinal_of(slot) else {
            return Ok(&[]);
        };
        self.block_names(&mapped.file, ordinal)
    }

    fn block_names(&mut self, file: &MappedNames, ordinal: u32) -> Result<&[u8], RewriteError> {
        let index = file.blocks_covering(ordinal..ordinal + 1).start;
        let is_loaded = self
            .loaded
            .as_ref()
            .is_some_and(|loaded| loaded.index == index);
        if !is_loaded {
            self.load(file, index)?;
        }
        let Some(loaded) = &self.loaded else {
            return Ok(&[]);
        };
        let relative_id = ordinal - loaded.span.first_ordinal;
        let found = loaded
            .entries
            .binary_search_by_key(&relative_id, |entry| entry.relative_id)
            .ok()
            .map(|position| loaded.entries[position].names.clone());
        Ok(found.map_or(&[], |range| &self.block_scratch.raw[range]))
    }

    fn load(&mut self, file: &MappedNames, index: usize) -> Result<(), RewriteError> {
        self.loaded = None;
        let stored = file.block(index).ok_or(RewriteError::Unreadable)?;
        let decoded = block::decode(stored, &mut self.block_scratch.raw)
            .map_err(|_| RewriteError::Unreadable)?;
        self.loaded = Some(LoadedBlock {
            index,
            span: decoded.span,
            entries: decoded.entry_ranges(),
        });
        Ok(())
    }
}

struct BlockOutput {
    writer: NamesFileWriter,
    open: Option<BlockBuilder>,
    block_target: usize,
    deadline: Option<Instant>,
    stats: RewriteStats,
    open_blocks: u64,
}

impl BlockOutput {
    fn start_block(&mut self, ordinal: u32) -> BlockBuilder {
        self.open_blocks += 1;
        self.stats.peak_open_blocks = self.stats.peak_open_blocks.max(self.open_blocks);
        BlockBuilder::new(ordinal)
    }

    fn push(&mut self, ordinal: u32, names: &[u8]) -> Result<(), RewriteError> {
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
        let encoded = builder.finish();
        self.writer
            .append(encoded.stored(), encoded.span.first_ordinal)?;
        self.open_blocks -= 1;
        self.stats.blocks_reencoded += 1;
        Ok(())
    }

    fn append_copy(
        &mut self,
        stored: StoredBlock<'_>,
        first_ordinal: u32,
    ) -> Result<(), RewriteError> {
        self.writer.append(stored, first_ordinal)?;
        self.stats.blocks_copied += 1;
        Ok(())
    }
}
