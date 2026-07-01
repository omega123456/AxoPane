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
//! - B-tree nodes: `mode` (`0` = leaf) + `count`, then either `count` records
//!   (leaf) or `count` `(child_block_id, record)` pairs (internal node,
//!   walked in order — there is no trailing child beyond the last pair).
//! - Records: a UTF-16BE filename, a 4-byte struct id (e.g. `"ptbL"`), a
//!   4-byte data type (e.g. `"ustr"`), then a type-specific value.

use std::collections::HashMap;
use std::path::Path;

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
    fn u32(&self, pos: usize) -> Option<u32> {
        let slice = self.bytes.get(pos..pos.checked_add(4)?)?;
        Some(u32::from_be_bytes(slice.try_into().ok()?))
    }

    fn slice(&self, pos: usize, len: usize) -> Option<&'a [u8]> {
        self.bytes.get(pos..pos.checked_add(len)?)
    }

    fn ascii4(&self, pos: usize) -> Option<&'a str> {
        std::str::from_utf8(self.slice(pos, 4)?).ok()
    }

    fn utf16be(&self, pos: usize, code_units: usize) -> Option<String> {
        let raw = self.slice(pos, code_units.checked_mul(2)?)?;
        let units: Vec<u16> = raw
            .chunks_exact(2)
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

    let magic1 = reader.u32(0)?;
    let magic2 = reader.slice(4, 4)?;
    if magic1 != 1 || magic2 != b"Bud1" {
        return None;
    }

    let root_offset = reader.u32(8)? as usize;
    let root_pos = root_offset.checked_add(4)?;

    let addr_count = reader.u32(root_pos)? as usize;
    let addr_table_start = root_pos.checked_add(8)?;
    if addr_count > ADDRESS_TABLE_SLOTS {
        return None;
    }

    let mut blocks: HashMap<u32, usize> = HashMap::with_capacity(addr_count);
    for index in 0..addr_count {
        let value = reader.u32(addr_table_start + index * 4)?;
        let offset = (value & !0x1F) as usize;
        let block_pos = offset.checked_add(4)?;
        blocks.insert(index as u32, block_pos);
    }

    let toc_pos = addr_table_start + ADDRESS_TABLE_SLOTS * 4;
    let toc_count = reader.u32(toc_pos)?;
    let mut cursor = toc_pos + 4;
    let mut dsdb_block: Option<u32> = None;
    for _ in 0..toc_count {
        let name_len = *bytes.get(cursor)? as usize;
        cursor += 1;
        let name = std::str::from_utf8(reader.slice(cursor, name_len)?).ok()?;
        cursor += name_len;
        let block_id = reader.u32(cursor)?;
        cursor += 4;
        if name == "DSDB" {
            dsdb_block = Some(block_id);
        }
    }

    let dsdb_pos = *blocks.get(&dsdb_block?)?;
    let root_node_id = reader.u32(dsdb_pos)?;

    let mut records = Vec::new();
    walk(&reader, &blocks, root_node_id, &mut records)?;

    let mut pending: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    for record in records {
        let entry = pending.entry(record.name).or_default();
        match (record.struct_id.as_str(), record.value) {
            ("ptbL", Value::Str(dir)) => entry.0 = Some(dir),
            ("ptbL", Value::Blob(blob)) => {
                if let Some(dir) = extract_bookmark_path(&blob) {
                    entry.0 = Some(dir);
                }
            }
            ("ptbN", Value::Str(name)) => entry.1 = Some(name),
            _ => {}
        }
    }

    Some(
        pending
            .into_iter()
            .filter_map(|(name, (dir, put_back_name))| {
                Some((
                    name,
                    PutBack {
                        original_dir: dir?,
                        original_name: put_back_name?,
                    },
                ))
            })
            .collect(),
    )
}

fn walk(
    reader: &Reader<'_>,
    blocks: &HashMap<u32, usize>,
    block_id: u32,
    out: &mut Vec<Record>,
) -> Option<()> {
    let pos = *blocks.get(&block_id)?;
    let mode = reader.u32(pos)?;
    let count = reader.u32(pos + 4)?;
    let mut cursor = pos + 8;

    if mode == 0 {
        for _ in 0..count {
            let (record, next) = parse_record(reader, cursor)?;
            out.push(record);
            cursor = next;
        }
    } else {
        for _ in 0..count {
            let child = reader.u32(cursor)?;
            cursor += 4;
            walk(reader, blocks, child, out)?;
            let (record, next) = parse_record(reader, cursor)?;
            out.push(record);
            cursor = next;
        }
    }

    Some(())
}

fn parse_record(reader: &Reader<'_>, pos: usize) -> Option<(Record, usize)> {
    let name_len = reader.u32(pos)? as usize;
    let mut cursor = pos + 4;
    let name = reader.utf16be(cursor, name_len)?;
    cursor += name_len * 2;

    let struct_id = reader.ascii4(cursor)?.to_string();
    cursor += 4;
    let data_type = reader.ascii4(cursor)?;
    cursor += 4;

    let (value, next) = match data_type {
        "ustr" => {
            let len = reader.u32(cursor)? as usize;
            let text = reader.utf16be(cursor + 4, len)?;
            (Value::Str(text), cursor + 4 + len * 2)
        }
        "blob" => {
            let len = reader.u32(cursor)? as usize;
            let bytes = reader.slice(cursor + 4, len)?.to_vec();
            (Value::Blob(bytes), cursor + 4 + len)
        }
        "long" | "shor" | "type" => (Value::Other, cursor + 4),
        "bool" => (Value::Other, cursor + 1),
        "comp" | "dutc" => (Value::Other, cursor + 8),
        _ => return None,
    };

    Some((
        Record {
            name,
            struct_id,
            value,
        },
        next,
    ))
}

/// Older macOS releases stored `ptbL` as an alias/bookmark blob instead of a
/// plain `ustr`. Fully parsing that format is out of scope for a best-effort
/// fallback, so this scans for the longest printable-ASCII run containing a
/// `/` and treats it as the embedded POSIX path, matching how bookmark data
/// embeds path components in cleartext. Returns `None` (never guesses) if no
/// such run is found.
fn extract_bookmark_path(blob: &[u8]) -> Option<String> {
    let mut best: Option<&[u8]> = None;
    let mut start = 0;
    let is_printable = |byte: u8| (0x20..=0x7e).contains(&byte);

    for (index, &byte) in blob.iter().enumerate() {
        if !is_printable(byte) {
            let run = &blob[start..index];
            if run.contains(&b'/') && best.is_none_or(|current| run.len() > current.len()) {
                best = Some(run);
            }
            start = index + 1;
        }
    }
    let run = &blob[start..];
    if run.contains(&b'/') && best.is_none_or(|current| run.len() > current.len()) {
        best = Some(run);
    }

    best.map(|run| String::from_utf8_lossy(run).into_owned())
}
