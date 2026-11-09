//! Pure parser for the undocumented `.DS_Store` "buddy allocator" + B-tree
//! format, scoped to exactly what macOS's Trash "put back" bookkeeping needs:
//! the `ptbL` (put-back directory) and `ptbN` (put-back name) records Finder
//! writes into `~/.Trash/.DS_Store` for every item it moves to the trash.
//!
//! This module never touches the filesystem — it only maps a byte slice to
//! put-back records — so it can be unit-tested against a committed fixture
//! and counted toward coverage regardless of which OS the test suite runs on.
//!
//! Layout (all integers big-endian):
//! - Header (36 bytes): `0x00000001`, magic `"Bud1"`, a root block offset
//!   (relative to byte 4) and size (duplicated once), then 16 reserved bytes.
//! - Root block: `count`, a zeroed `u32`, then a 256-slot address table
//!   (`count` of which are populated) encoding `(block_offset, block_size)`
//!   pairs, followed by a table of contents mapping names (e.g. `"DSDB"`) to
//!   block ids.
//! - The `DSDB` block holds `(root_node_id, internal_levels, record_count,
//!   block_count, 0x1000)`, pointing at the root of the actual B-tree.
//! - B-tree nodes: `rightmost_child` (`0` = leaf) + `count`, then either
//!   `count` records (leaf) or `count` `(child_block_id, record)` pairs
//!   followed by the `rightmost_child` subtree (internal node, walked in order).
//! - Records: a UTF-16BE filename, a 4-byte struct id (e.g. `"ptbL"`), a
//!   4-byte data type (e.g. `"ustr"`), then a type-specific value.

use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::path::Path;

const U32_BYTES: usize = 4;
const FOUR_CHAR_CODE_BYTES: usize = 4;
const UTF16_UNIT_BYTES: usize = 2;
const HEADER_VERSION: u32 = 1;
const HEADER_MAGIC: &[u8] = b"Bud1";
const HEADER_MAGIC_OFFSET: usize = 4;
const HEADER_ROOT_OFFSET_POSITION: usize = 8;
/// Block offsets in the header and address table are relative to byte 4.
const BLOCK_OFFSET_BASE: usize = 4;
/// `count` plus the zeroed `u32` that precede the address table.
const ADDRESS_TABLE_PREAMBLE_BYTES: usize = 8;
/// The low five bits of an address-table entry encode the block size.
const BLOCK_SIZE_BITS_MASK: u32 = 0x1F;
/// `rightmost_child` plus `count` at the start of every B-tree node.
const NODE_HEADER_BYTES: usize = 8;
const LEAF_NODE_MARKER: u32 = 0;
const BOOL_VALUE_BYTES: usize = 1;
const TIMESTAMP_VALUE_BYTES: usize = 8;
const PRINTABLE_ASCII: RangeInclusive<u8> = 0x20..=0x7e;

/// A recovered put-back record: the original parent directory (volume
/// relative, no leading slash) and the original file name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PutBack {
    pub original_dir: String,
    pub original_name: String,
}

/// Parses a `.DS_Store` byte buffer into a map of trashed name -> put-back
/// record. Never panics: any structural problem (truncated, malformed, or
/// empty input) simply yields an empty map, since this fallback is strictly
/// best-effort.
pub fn parse_put_back(bytes: &[u8]) -> HashMap<String, PutBack> {
    parse(bytes).unwrap_or_default()
}

/// Joins a volume-relative `ptbL` directory and a `ptbN` name into an
/// absolute path under the given volume root (home trash's volume root is
/// `"/"`). `dir` may or may not carry a leading/trailing slash; both are
/// normalized away before joining.
pub fn resolve_original_path(volume_root: &Path, dir: &str, name: &str) -> String {
    let mut root = volume_root.to_string_lossy().into_owned();
    if !root.ends_with('/') {
        root.push('/');
    }

    let dir = dir.trim_start_matches('/');
    let mut path = format!("{root}{dir}");
    if !path.ends_with('/') {
        path.push('/');
    }
    path.push_str(name);
    path
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn u32(&self, position: usize) -> Option<u32> {
        let slice = self.slice(position, U32_BYTES)?;
        Some(u32::from_be_bytes(slice.try_into().ok()?))
    }

    fn slice(&self, position: usize, length: usize) -> Option<&'a [u8]> {
        self.bytes.get(position..position.checked_add(length)?)
    }

    fn four_char_code(&self, position: usize) -> Option<&'a str> {
        std::str::from_utf8(self.slice(position, FOUR_CHAR_CODE_BYTES)?).ok()
    }

    fn utf16be(&self, position: usize, code_units: usize) -> Option<String> {
        let raw = self.slice(position, code_units.checked_mul(UTF16_UNIT_BYTES)?)?;
        let units: Vec<u16> = raw
            .chunks_exact(UTF16_UNIT_BYTES)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16(&units).ok()
    }
}

#[derive(Debug, Clone)]
enum Value {
    Str(String),
    Blob(Vec<u8>),
    Other,
}

struct Record {
    name: String,
    struct_id: String,
    value: Value,
}

const ADDRESS_TABLE_SLOTS: usize = 256;

fn parse(bytes: &[u8]) -> Option<HashMap<String, PutBack>> {
    let reader = Reader { bytes };

    let version = reader.u32(0)?;
    let magic = reader.slice(HEADER_MAGIC_OFFSET, HEADER_MAGIC.len())?;
    let header_matches = version == HEADER_VERSION && magic == HEADER_MAGIC;
    if !header_matches {
        return None;
    }

    let root_offset = reader.u32(HEADER_ROOT_OFFSET_POSITION)? as usize;
    let root_position = root_offset.checked_add(BLOCK_OFFSET_BASE)?;
    let address_table_start = root_position.checked_add(ADDRESS_TABLE_PREAMBLE_BYTES)?;
    let blocks = block_positions(&reader, root_position, address_table_start)?;

    let table_of_contents_position = address_table_start + ADDRESS_TABLE_SLOTS * U32_BYTES;
    let dsdb_block = dsdb_block_id(&reader, table_of_contents_position)?;
    let dsdb_position = *blocks.get(&dsdb_block)?;
    let root_node_id = reader.u32(dsdb_position)?;

    let mut tree = NodeTree {
        reader: &reader,
        blocks: &blocks,
        records: Vec::new(),
    };
    tree.walk(root_node_id)?;
    Some(put_backs(tree.records))
}

fn block_positions(
    reader: &Reader<'_>,
    root_position: usize,
    address_table_start: usize,
) -> Option<HashMap<u32, usize>> {
    let address_count = reader.u32(root_position)? as usize;
    if address_count > ADDRESS_TABLE_SLOTS {
        return None;
    }
    let mut blocks: HashMap<u32, usize> = HashMap::with_capacity(address_count);
    for index in 0..address_count {
        let value = reader.u32(address_table_start + index * U32_BYTES)?;
        let offset = (value & !BLOCK_SIZE_BITS_MASK) as usize;
        let block_position = offset.checked_add(BLOCK_OFFSET_BASE)?;
        blocks.insert(index as u32, block_position);
    }
    Some(blocks)
}

/// The last table-of-contents entry named `DSDB`.
fn dsdb_block_id(reader: &Reader<'_>, table_of_contents_position: usize) -> Option<u32> {
    let table_of_contents_count = reader.u32(table_of_contents_position)?;
    let mut cursor = table_of_contents_position + U32_BYTES;
    let mut dsdb_block: Option<u32> = None;
    for _ in 0..table_of_contents_count {
        let name_length = *reader.bytes.get(cursor)? as usize;
        cursor += 1;
        let name = std::str::from_utf8(reader.slice(cursor, name_length)?).ok()?;
        cursor += name_length;
        let block_id = reader.u32(cursor)?;
        cursor += U32_BYTES;
        if name == "DSDB" {
            dsdb_block = Some(block_id);
        }
    }
    dsdb_block
}

#[derive(Default)]
struct PendingPutBack {
    original_dir: Option<String>,
    original_name: Option<String>,
}

impl PendingPutBack {
    fn absorb(&mut self, record: Record) {
        match (record.struct_id.as_str(), record.value) {
            ("ptbL", Value::Str(dir)) => self.original_dir = Some(dir),
            ("ptbL", Value::Blob(blob)) => self.absorb_bookmark(&blob),
            ("ptbN", Value::Str(name)) => self.original_name = Some(name),
            _ => {}
        }
    }

    fn absorb_bookmark(&mut self, blob: &[u8]) {
        if let Some(dir) = extract_bookmark_path(blob) {
            self.original_dir = Some(dir);
        }
    }

    fn complete(self) -> Option<PutBack> {
        Some(PutBack {
            original_dir: self.original_dir?,
            original_name: self.original_name?,
        })
    }
}

fn put_backs(records: Vec<Record>) -> HashMap<String, PutBack> {
    let mut pending: HashMap<String, PendingPutBack> = HashMap::new();
    for record in records {
        pending
            .entry(record.name.clone())
            .or_default()
            .absorb(record);
    }
    pending
        .into_iter()
        .filter_map(|(name, put_back)| Some((name, put_back.complete()?)))
        .collect()
}

struct NodeTree<'tree> {
    reader: &'tree Reader<'tree>,
    blocks: &'tree HashMap<u32, usize>,
    records: Vec<Record>,
}

impl NodeTree<'_> {
    fn walk(&mut self, block_id: u32) -> Option<()> {
        let position = *self.blocks.get(&block_id)?;
        let rightmost_child = self.reader.u32(position)?;
        let count = self.reader.u32(position + U32_BYTES)?;
        let mut cursor = position + NODE_HEADER_BYTES;

        let is_internal_node = rightmost_child != LEAF_NODE_MARKER;
        for _ in 0..count {
            cursor = self.walk_entry(cursor, is_internal_node)?;
        }
        if is_internal_node {
            self.walk(rightmost_child)?;
        }

        Some(())
    }

    fn walk_entry(&mut self, cursor: usize, is_internal_node: bool) -> Option<usize> {
        let mut record_position = cursor;
        if is_internal_node {
            let child = self.reader.u32(cursor)?;
            record_position += U32_BYTES;
            self.walk(child)?;
        }
        let parsed = parse_record(self.reader, record_position)?;
        self.records.push(parsed.record);
        Some(parsed.next)
    }
}

struct ParsedRecord {
    record: Record,
    next: usize,
}

fn parse_record(reader: &Reader<'_>, position: usize) -> Option<ParsedRecord> {
    let name_length = reader.u32(position)? as usize;
    let mut cursor = position + U32_BYTES;
    let name = reader.utf16be(cursor, name_length)?;
    cursor += name_length * UTF16_UNIT_BYTES;

    let struct_id = reader.four_char_code(cursor)?.to_string();
    cursor += FOUR_CHAR_CODE_BYTES;
    let data_type = reader.four_char_code(cursor)?;
    cursor += FOUR_CHAR_CODE_BYTES;

    let (value, next) = match data_type {
        "ustr" => {
            let code_units = reader.u32(cursor)? as usize;
            let text = reader.utf16be(cursor + U32_BYTES, code_units)?;
            (
                Value::Str(text),
                cursor + U32_BYTES + code_units * UTF16_UNIT_BYTES,
            )
        }
        "blob" => {
            let byte_count = reader.u32(cursor)? as usize;
            let bytes = reader.slice(cursor + U32_BYTES, byte_count)?.to_vec();
            (Value::Blob(bytes), cursor + U32_BYTES + byte_count)
        }
        "long" | "shor" | "type" => (Value::Other, cursor + U32_BYTES),
        "bool" => (Value::Other, cursor + BOOL_VALUE_BYTES),
        "comp" | "dutc" => (Value::Other, cursor + TIMESTAMP_VALUE_BYTES),
        _ => return None,
    };

    Some(ParsedRecord {
        record: Record {
            name,
            struct_id,
            value,
        },
        next,
    })
}

/// Older macOS releases stored `ptbL` as an alias/bookmark blob instead of a
/// plain `ustr`. Fully parsing that format is out of scope for a best-effort
/// fallback, so this scans for the longest printable-ASCII run containing a
/// `/` and treats it as the embedded POSIX path, matching how bookmark data
/// embeds path components in cleartext. Returns `None` (never guesses) if no
/// such run is found.
fn extract_bookmark_path(blob: &[u8]) -> Option<String> {
    let best = blob
        .split(|byte| !PRINTABLE_ASCII.contains(byte))
        .filter(|run| run.contains(&b'/'))
        .fold(None, keep_longer_run);
    best.map(|run| String::from_utf8_lossy(run).into_owned())
}

fn keep_longer_run<'blob>(best: Option<&'blob [u8]>, run: &'blob [u8]) -> Option<&'blob [u8]> {
    let is_longer = best.is_none_or(|current| run.len() > current.len());
    if is_longer {
        return Some(run);
    }
    best
}
