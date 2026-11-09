use std::io;
use std::ops::Range;

use super::codec::{DecodeScratch, NamesEncoder};
use super::is_packed_list_or_empty;

pub const BLOCK_TARGET_BYTES: usize = 256 * 1024;

const ENTRY_FIELD_BYTES: usize = size_of::<u32>();
const ENTRY_HEADER_BYTES: usize = 2 * ENTRY_FIELD_BYTES;
const CHECKSUM_WORD_BYTES: usize = size_of::<u64>();
const CHECKSUM_SEED: u64 = 0x9e37_79b9_7f4a_7c15;
const CHECKSUM_MULTIPLIER: u64 = 0xff51_afd7_ed55_8ccd;
const CHECKSUM_ROTATION: u32 = 29;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamesCorrupt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockSpan {
    pub first_ordinal: u32,
    pub folder_count: u32,
}

impl BlockSpan {
    pub fn end(&self) -> u32 {
        self.first_ordinal.saturating_add(self.folder_count)
    }

    pub fn ordinals(&self) -> Range<u32> {
        self.first_ordinal..self.end()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedBlock {
    pub span: BlockSpan,
    pub raw_len: u32,
    pub checksum: u64,
    pub compressed: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredBlock<'bytes> {
    pub span: BlockSpan,
    pub raw_len: u32,
    pub checksum: u64,
    pub compressed: &'bytes [u8],
}

impl EncodedBlock {
    pub fn stored(&self) -> StoredBlock<'_> {
        StoredBlock {
            span: self.span,
            raw_len: self.raw_len,
            checksum: self.checksum,
            compressed: &self.compressed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockEntry<'raw> {
    pub relative_id: u32,
    pub names: &'raw [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryRange {
    pub relative_id: u32,
    pub names: Range<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct DecodedBlock<'raw> {
    pub span: BlockSpan,
    raw: &'raw [u8],
}

impl<'raw> DecodedBlock<'raw> {
    pub fn raw(&self) -> &'raw [u8] {
        self.raw
    }

    pub fn entry_ranges(&self) -> Vec<EntryRange> {
        let mut ranges = Vec::new();
        let mut offset = 0;
        for entry in self.entries() {
            let start = offset + ENTRY_HEADER_BYTES;
            let end = start + entry.names.len();
            ranges.push(EntryRange {
                relative_id: entry.relative_id,
                names: start..end,
            });
            offset = end;
        }
        ranges
    }

    pub fn entries(&self) -> impl Iterator<Item = BlockEntry<'raw>> {
        let mut rest = self.raw;
        std::iter::from_fn(move || {
            let (entry, remaining) = split_entry(rest)?;
            rest = remaining;
            Some(entry)
        })
    }

    pub fn names_of(&self, ordinal: u32) -> Option<&'raw [u8]> {
        let relative_id = ordinal.checked_sub(self.span.first_ordinal)?;
        self.entries()
            .find(|entry| entry.relative_id == relative_id)
            .map(|entry| entry.names)
    }
}

#[derive(Debug)]
pub struct BlockBuilder {
    span: BlockSpan,
    raw: Vec<u8>,
}

impl BlockBuilder {
    pub fn new(first_ordinal: u32) -> Self {
        Self {
            span: BlockSpan {
                first_ordinal,
                folder_count: 0,
            },
            raw: Vec::new(),
        }
    }

    pub fn raw_len(&self) -> usize {
        self.raw.len()
    }

    pub fn push(&mut self, ordinal: u32, names: &[u8]) {
        let relative_id = ordinal - self.span.first_ordinal;
        self.span.folder_count = relative_id + 1;
        if names.is_empty() {
            return;
        }
        self.raw.extend_from_slice(&relative_id.to_le_bytes());
        self.raw
            .extend_from_slice(&(names.len() as u32).to_le_bytes());
        self.raw.extend_from_slice(names);
    }

    pub fn finish(self, encoder: &mut NamesEncoder) -> io::Result<EncodedBlock> {
        let compressed = encoder.compress(&self.raw)?;
        Ok(EncodedBlock {
            span: self.span,
            raw_len: self.raw.len() as u32,
            checksum: checksum(&compressed),
            compressed,
        })
    }
}

pub fn decode<'raw>(
    stored: StoredBlock<'_>,
    scratch: &'raw mut DecodeScratch,
) -> Result<DecodedBlock<'raw>, NamesCorrupt> {
    if checksum(stored.compressed) != stored.checksum {
        return Err(NamesCorrupt);
    }
    let raw = scratch.decompress(stored.compressed, stored.raw_len as usize)?;
    let decoded = DecodedBlock {
        span: stored.span,
        raw,
    };
    validate(&decoded)?;
    Ok(decoded)
}

fn validate(decoded: &DecodedBlock<'_>) -> Result<(), NamesCorrupt> {
    let mut rest = decoded.raw;
    let mut next_allowed = 0_u32;
    while !rest.is_empty() {
        let (entry, remaining) = split_entry(rest).ok_or(NamesCorrupt)?;
        let in_order = entry.relative_id >= next_allowed;
        let in_span = entry.relative_id < decoded.span.folder_count;
        let has_names = !entry.names.is_empty() && is_packed_list_or_empty(entry.names);
        if !(in_order && in_span && has_names) {
            return Err(NamesCorrupt);
        }
        next_allowed = entry.relative_id + 1;
        rest = remaining;
    }
    Ok(())
}

fn split_entry(raw: &[u8]) -> Option<(BlockEntry<'_>, &[u8])> {
    let (header, rest) = raw.split_at_checked(ENTRY_HEADER_BYTES)?;
    let (id_bytes, length_bytes) = header.split_at(ENTRY_FIELD_BYTES);
    let relative_id = u32::from_le_bytes(id_bytes.try_into().ok()?);
    let length = u32::from_le_bytes(length_bytes.try_into().ok()?) as usize;
    let (names, rest) = rest.split_at_checked(length)?;
    Some((BlockEntry { relative_id, names }, rest))
}

pub fn checksum(bytes: &[u8]) -> u64 {
    let mut words = bytes.chunks_exact(CHECKSUM_WORD_BYTES);
    let seeded = CHECKSUM_SEED ^ bytes.len() as u64;
    let hash = words.by_ref().fold(seeded, mix);
    mix(hash, words.remainder())
}

fn mix(hash: u64, word: &[u8]) -> u64 {
    let mut padded = [0_u8; CHECKSUM_WORD_BYTES];
    padded[..word.len()].copy_from_slice(word);
    (hash ^ u64::from_le_bytes(padded))
        .wrapping_mul(CHECKSUM_MULTIPLIER)
        .rotate_left(CHECKSUM_ROTATION)
}
