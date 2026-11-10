use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::size::index::names::block;
use crate::size::index::persist::Reader;
use crate::size::index::tree::{DENIED, PERSISTED_FLAGS, SCANNED, SKIPPED};

pub const TREE_FORMAT_VERSION: u32 = 1;
pub const CHUNK_BYTES: usize = 64 * 1024;
pub const MAX_SEGMENT_NODES: u32 = 1 << 31;

pub(crate) const NO_PARENT: u32 = u32::MAX;
pub(crate) const HEADER_BYTES: usize = 48;

const MAGIC: &[u8; 8] = b"AXOTREE\0";
const CHECKSUM_BYTES: usize = size_of::<u64>();
const HEADER_FIELD_BYTES: usize = HEADER_BYTES - CHECKSUM_BYTES;
const RESERVED_HEADER_FIELD: u32 = 0;
const TREE_FILE_PREFIX: &str = "folder-tree-";
const NAMES_FILE_PREFIX: &str = "file-names-";
const SEGMENT_FILE_SUFFIX: &str = ".bin";
const EXTENSION_SEPARATOR: char = '.';
const NO_FLAGS: u8 = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentPaths {
    pub tree: PathBuf,
    pub names: PathBuf,
}

impl SegmentPaths {
    pub fn numbered(directory: &Path, sequence: u64) -> Self {
        let file =
            |prefix: &str| directory.join(format!("{prefix}{sequence:06}{SEGMENT_FILE_SUFFIX}"));
        Self {
            tree: file(TREE_FILE_PREFIX),
            names: file(NAMES_FILE_PREFIX),
        }
    }
}

pub fn sequence_of_segment_file(file_name: &str) -> Option<u64> {
    let numbered = [TREE_FILE_PREFIX, NAMES_FILE_PREFIX]
        .into_iter()
        .find_map(|prefix| file_name.strip_prefix(prefix))?;
    let (sequence, _extension) = numbered.split_once(EXTENSION_SEPARATOR)?;
    sequence.parse().ok()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NodeFlags {
    pub scanned: bool,
    pub denied: bool,
    pub skipped: bool,
}

impl NodeFlags {
    pub(crate) fn bits(self) -> u8 {
        bit_if(self.scanned, SCANNED) | bit_if(self.denied, DENIED) | bit_if(self.skipped, SKIPPED)
    }

    pub(crate) fn from_bits(bits: u8) -> Option<Self> {
        let has_unknown_bits = bits & !PERSISTED_FLAGS != 0;
        if has_unknown_bits {
            return None;
        }
        Some(Self {
            scanned: bits & SCANNED != 0,
            denied: bits & DENIED != 0,
            skipped: bits & SKIPPED != 0,
        })
    }
}

fn bit_if(set: bool, bit: u8) -> u8 {
    if set {
        return bit;
    }
    NO_FLAGS
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) nodes: u32,
    pub(crate) top_level: u32,
    pub(crate) name_heap_bytes: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum Column {
    Direct,
    Total,
    Pending,
    Skipped,
    Denied,
    Parent,
    SubtreeEnd,
    ChildOffset,
    ChildCount,
    Children,
    NameEnds,
    Flags,
    NameHeap,
}

const COLUMN_COUNT: usize = 13;

impl Column {
    const IN_FILE_ORDER: [Self; COLUMN_COUNT] = [
        Self::Direct,
        Self::Total,
        Self::Pending,
        Self::Skipped,
        Self::Denied,
        Self::Parent,
        Self::SubtreeEnd,
        Self::ChildOffset,
        Self::ChildCount,
        Self::Children,
        Self::NameEnds,
        Self::Flags,
        Self::NameHeap,
    ];

    pub(crate) fn at(self, index: u32) -> Cell {
        Cell {
            column: self,
            index,
        }
    }

    fn entry_bytes(self) -> usize {
        match self {
            Self::Direct | Self::Total => size_of::<u64>(),
            Self::Flags | Self::NameHeap => size_of::<u8>(),
            _ => size_of::<u32>(),
        }
    }

    fn entry_count(self, counts: Counts) -> u64 {
        match self {
            Self::Children => u64::from(counts.nodes - counts.top_level),
            Self::NameEnds => u64::from(counts.nodes) + 1,
            Self::NameHeap => u64::from(counts.name_heap_bytes),
            _ => u64::from(counts.nodes),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cell {
    pub(crate) column: Column,
    pub(crate) index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CellRun {
    pub(crate) column: Column,
    pub(crate) first: u32,
    pub(crate) count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layout {
    sections: [Range<usize>; COLUMN_COUNT],
    chunk_table: Range<usize>,
}

impl Layout {
    pub(crate) fn for_counts(counts: Counts) -> Option<Self> {
        if counts.top_level > counts.nodes {
            return None;
        }
        let mut sections = Vec::with_capacity(COLUMN_COUNT);
        let mut offset = HEADER_BYTES;
        for column in Column::IN_FILE_ORDER {
            let entries = usize::try_from(column.entry_count(counts)).ok()?;
            let end = offset.checked_add(entries.checked_mul(column.entry_bytes())?)?;
            sections.push(offset..end);
            offset = end;
        }
        let chunk_count = (offset - HEADER_BYTES).div_ceil(CHUNK_BYTES);
        let table_end = offset.checked_add(chunk_count.checked_mul(CHECKSUM_BYTES)?)?;
        Some(Self {
            sections: sections.try_into().ok()?,
            chunk_table: offset..table_end,
        })
    }

    pub(crate) fn file_len(&self) -> usize {
        self.chunk_table.end
    }

    pub(crate) fn chunk_table(&self) -> Range<usize> {
        self.chunk_table.clone()
    }

    pub(crate) fn chunk_count(&self) -> usize {
        self.chunk_table.len() / CHECKSUM_BYTES
    }

    pub(crate) fn chunk(&self, index: usize) -> Range<usize> {
        let start = HEADER_BYTES + index * CHUNK_BYTES;
        let end = (start + CHUNK_BYTES).min(self.chunk_table.start);
        start..end
    }

    pub(crate) fn chunk_checksum(&self, index: usize) -> Range<usize> {
        let start = self.chunk_table.start + index * CHECKSUM_BYTES;
        start..start + CHECKSUM_BYTES
    }

    pub(crate) fn chunks_covering(&self, bytes: &Range<usize>) -> Range<usize> {
        if bytes.is_empty() {
            return 0..0;
        }
        let first = (bytes.start - HEADER_BYTES) / CHUNK_BYTES;
        let last = (bytes.end - 1 - HEADER_BYTES) / CHUNK_BYTES;
        first..last + 1
    }

    pub(crate) fn cell(&self, cell: Cell) -> Option<Range<usize>> {
        self.run(CellRun {
            column: cell.column,
            first: cell.index,
            count: 1,
        })
    }

    pub(crate) fn run(&self, run: CellRun) -> Option<Range<usize>> {
        let section = &self.sections[run.column as usize];
        let entry_bytes = run.column.entry_bytes();
        let start = section
            .start
            .checked_add((run.first as usize).checked_mul(entry_bytes)?)?;
        let end = start.checked_add((run.count as usize).checked_mul(entry_bytes)?)?;
        (end <= section.end).then_some(start..end)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TreeHeader {
    pub(crate) counts: Counts,
    pub(crate) token: u64,
}

pub(crate) struct OpenedTree {
    pub(crate) header: TreeHeader,
    pub(crate) layout: Layout,
}

impl TreeHeader {
    fn encode_fields(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&TREE_FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&self.counts.nodes.to_le_bytes());
        out.extend_from_slice(&self.counts.top_level.to_le_bytes());
        out.extend_from_slice(&self.counts.name_heap_bytes.to_le_bytes());
        out.extend_from_slice(&self.token.to_le_bytes());
        out.extend_from_slice(&RESERVED_HEADER_FIELD.to_le_bytes());
        out.extend_from_slice(&RESERVED_HEADER_FIELD.to_le_bytes());
        out
    }

    pub(crate) fn encode(&self, chunk_table: &[u8]) -> Vec<u8> {
        let mut out = self.encode_fields();
        let checksum = header_checksum(&out, chunk_table);
        out.extend_from_slice(&checksum.to_le_bytes());
        out
    }

    fn decode_fields(fields: &[u8]) -> Option<Self> {
        let mut reader = Reader(fields);
        let magic_matches = reader.take(MAGIC.len())? == MAGIC;
        let version_matches = reader.u32()? == TREE_FORMAT_VERSION;
        if !(magic_matches && version_matches) {
            return None;
        }
        let counts = Counts {
            nodes: reader.u32()?,
            top_level: reader.u32()?,
            name_heap_bytes: reader.u32()?,
        };
        let token = reader.u64()?;
        Some(Self { counts, token })
    }

    pub(crate) fn open(file: &[u8]) -> Option<OpenedTree> {
        let (fields, rest) = file.split_at_checked(HEADER_FIELD_BYTES)?;
        let stored_checksum = le_u64(rest.get(..CHECKSUM_BYTES)?)?;
        let header = Self::decode_fields(fields)?;
        let within_node_limit = header.counts.nodes <= MAX_SEGMENT_NODES;
        let layout = Layout::for_counts(header.counts)?;
        let length_matches = layout.file_len() == file.len();
        if !(within_node_limit && length_matches) {
            return None;
        }
        let checksum_matches =
            header_checksum(fields, &file[layout.chunk_table()]) == stored_checksum;
        checksum_matches.then_some(OpenedTree { header, layout })
    }
}

fn header_checksum(fields: &[u8], chunk_table: &[u8]) -> u64 {
    let mut covered = fields.to_vec();
    covered.extend_from_slice(chunk_table);
    block::checksum(&covered)
}

pub(crate) fn le_u32(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

pub(crate) fn le_u64(bytes: &[u8]) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}
