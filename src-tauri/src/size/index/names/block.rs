use std::cmp::Ordering;
use std::ops::Range;

use super::signature::{signature_of, SignatureFilter, SignedName};
use super::{names_in, NAME_TERMINATOR};
use crate::size::index::persist::{put_u32, Reader};
use crate::size::index::segment::format::le_u32;

pub const BLOCK_TARGET_BYTES: usize = 256 * 1024;

const FIELD_BYTES: usize = size_of::<u32>();
const FOLDER_ENTRY_BYTES: usize = 2 * FIELD_BYTES;
const FOLDER_COUNT_BYTES: usize = FIELD_BYTES;
const BYTES_PER_NAME: usize = 2 * FIELD_BYTES;
const FIRST_NAME_OF_A_BLOCK: u32 = 0;
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
pub struct BuiltBlock {
    pub span: BlockSpan,
    pub name_count: u32,
    pub checksum: u64,
    pub bytes: Vec<u8>,
}

impl BuiltBlock {
    pub fn stored(&self) -> StoredBlock<'_> {
        StoredBlock {
            span: self.span,
            name_count: self.name_count,
            checksum: self.checksum,
            bytes: &self.bytes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredBlock<'bytes> {
    pub span: BlockSpan,
    pub name_count: u32,
    pub checksum: u64,
    pub bytes: &'bytes [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NameSignatures<'bytes> {
    bytes: &'bytes [u8],
}

impl<'bytes> NameSignatures<'bytes> {
    pub fn len(&self) -> usize {
        self.bytes.len() / FIELD_BYTES
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = u32> + 'bytes {
        self.bytes.chunks_exact(FIELD_BYTES).filter_map(le_u32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NameOffsets<'bytes> {
    bytes: &'bytes [u8],
    first: u32,
}

impl NameOffsets<'_> {
    fn start_in_folder(&self, name: usize) -> Option<usize> {
        let start = name.checked_mul(FIELD_BYTES)?;
        let offset = le_u32(self.bytes.get(start..start + FIELD_BYTES)?)?;
        let start_in_folder = offset.checked_sub(self.first)?;
        Some(start_in_folder as usize)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockFolder<'bytes> {
    pub relative_id: u32,
    pub signatures: NameSignatures<'bytes>,
    pub offsets: NameOffsets<'bytes>,
    pub packed: &'bytes [u8],
}

impl<'bytes> BlockFolder<'bytes> {
    pub fn name(&self, index: usize) -> Option<&'bytes [u8]> {
        let start = self.offsets.start_in_folder(index)?;
        let end = self
            .offsets
            .start_in_folder(index + 1)
            .unwrap_or(self.packed.len());
        let terminated = self.packed.get(start..end)?;
        terminated.strip_suffix(&[NAME_TERMINATOR])
    }

    pub fn admitted_names<'filter>(
        self,
        filter: &'filter dyn SignatureFilter,
    ) -> impl Iterator<Item = SignedName<'bytes>> + 'filter
    where
        'bytes: 'filter,
    {
        let numbered = self.signatures.iter().enumerate();
        let admitted = numbered.filter(move |&(_, signature)| filter.admits(signature));
        admitted.filter_map(move |(index, signature)| {
            Some(SignedName {
                bytes: self.name(index)?,
                signature,
            })
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FolderEntry {
    relative_id: u32,
    first_name: u32,
}

#[derive(Clone, Copy)]
struct EntryLimits {
    folder_count: u32,
    name_count: u32,
}

impl FolderEntry {
    fn follows(&self, previous: Option<FolderEntry>, limits: EntryLimits) -> bool {
        let inside = self.relative_id < limits.folder_count && self.first_name < limits.name_count;
        let Some(previous) = previous else {
            let opens_the_block = self.first_name == FIRST_NAME_OF_A_BLOCK;
            return inside && opens_the_block;
        };
        let ids_ascend = self.relative_id > previous.relative_id;
        let names_ascend = self.first_name > previous.first_name;
        inside && ids_ascend && names_ascend
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BlockView<'bytes> {
    pub span: BlockSpan,
    name_count: u32,
    folders: &'bytes [u8],
    signatures: &'bytes [u8],
    offsets: &'bytes [u8],
    names: &'bytes [u8],
}

impl<'bytes> BlockView<'bytes> {
    pub fn verified(stored: StoredBlock<'bytes>) -> Result<Self, NamesCorrupt> {
        if checksum(stored.bytes) != stored.checksum {
            return Err(NamesCorrupt);
        }
        let view = Self::trusting(stored)?;
        if !view.has_valid_structure() {
            return Err(NamesCorrupt);
        }
        Ok(view)
    }

    pub fn trusting(stored: StoredBlock<'bytes>) -> Result<Self, NamesCorrupt> {
        split_regions(stored).ok_or(NamesCorrupt)
    }

    pub fn folders(&self) -> impl Iterator<Item = BlockFolder<'bytes>> {
        let view = *self;
        (0..self.folder_count()).map_while(move |position| view.folder_at(position))
    }

    pub fn folder(&self, relative_id: u32) -> Option<BlockFolder<'bytes>> {
        self.folder_at(self.position_of(relative_id)?)
    }

    pub fn names_of(&self, ordinal: u32) -> Option<&'bytes [u8]> {
        let relative_id = ordinal.checked_sub(self.span.first_ordinal)?;
        self.folder(relative_id).map(|folder| folder.packed)
    }

    fn folder_count(&self) -> usize {
        self.folders.len() / FOLDER_ENTRY_BYTES
    }

    fn folder_entry(&self, position: usize) -> Option<FolderEntry> {
        let start = position.checked_mul(FOLDER_ENTRY_BYTES)?;
        let entry = self.folders.get(start..start + FOLDER_ENTRY_BYTES)?;
        let (id_bytes, first_name_bytes) = entry.split_at(FIELD_BYTES);
        Some(FolderEntry {
            relative_id: le_u32(id_bytes)?,
            first_name: le_u32(first_name_bytes)?,
        })
    }

    fn first_name_after(&self, position: usize) -> u32 {
        self.folder_entry(position + 1)
            .map_or(self.name_count, |next| next.first_name)
    }

    fn boundary_before(&self, name: u32) -> Option<usize> {
        if name == self.name_count {
            return Some(self.names.len());
        }
        let start = (name as usize).checked_mul(FIELD_BYTES)?;
        let offset = le_u32(self.offsets.get(start..start + FIELD_BYTES)?)?;
        Some(offset as usize)
    }

    fn name_bytes(&self, name: u32) -> Option<&'bytes [u8]> {
        let start = self.boundary_before(name)?;
        let end = self.boundary_before(name + 1)?;
        self.names.get(start..end)
    }

    fn folder_at(&self, position: usize) -> Option<BlockFolder<'bytes>> {
        let entry = self.folder_entry(position)?;
        let names = entry.first_name..self.first_name_after(position);
        let table_bytes = names.start as usize * FIELD_BYTES..names.end as usize * FIELD_BYTES;
        let name_bytes = self.boundary_before(names.start)?..self.boundary_before(names.end)?;
        Some(BlockFolder {
            relative_id: entry.relative_id,
            signatures: NameSignatures {
                bytes: self.signatures.get(table_bytes.clone())?,
            },
            offsets: NameOffsets {
                bytes: self.offsets.get(table_bytes)?,
                first: name_bytes.start as u32,
            },
            packed: self.names.get(name_bytes)?,
        })
    }

    fn position_of(&self, relative_id: u32) -> Option<usize> {
        let mut candidates = 0..self.folder_count();
        while !candidates.is_empty() {
            let middle = candidates.start + candidates.len() / 2;
            let entry = self.folder_entry(middle)?;
            match entry.relative_id.cmp(&relative_id) {
                Ordering::Equal => return Some(middle),
                Ordering::Less => candidates.start = middle + 1,
                Ordering::Greater => candidates.end = middle,
            }
        }
        None
    }

    fn has_valid_structure(&self) -> bool {
        self.folders_are_valid() && self.names_are_valid()
    }

    fn folders_are_valid(&self) -> bool {
        let limits = EntryLimits {
            folder_count: self.span.folder_count,
            name_count: self.name_count,
        };
        let in_order =
            (0..self.folder_count()).all(|position| self.entry_is_in_order(position, limits));
        let holds_folders = self.folder_count() > 0;
        let holds_names = self.name_count > 0;
        let folders_match_names = holds_folders == holds_names;
        in_order && folders_match_names
    }

    fn entry_is_in_order(&self, position: usize, limits: EntryLimits) -> bool {
        let Some(entry) = self.folder_entry(position) else {
            return false;
        };
        let previous = position
            .checked_sub(1)
            .and_then(|before| self.folder_entry(before));
        entry.follows(previous, limits)
    }

    fn names_are_valid(&self) -> bool {
        let area_starts_with_first_name = self.boundary_before(FIRST_NAME_OF_A_BLOCK) == Some(0);
        let every_name_is_whole = (0..self.name_count)
            .all(|name| self.name_bytes(name).is_some_and(is_one_terminated_name));
        area_starts_with_first_name && every_name_is_whole
    }
}

fn is_one_terminated_name(bytes: &[u8]) -> bool {
    let Some((&last, name)) = bytes.split_last() else {
        return false;
    };
    let terminated = last == NAME_TERMINATOR;
    let has_characters = !name.is_empty();
    let holds_one_name = !name.contains(&NAME_TERMINATOR);
    terminated && has_characters && holds_one_name
}

fn split_regions(stored: StoredBlock<'_>) -> Option<BlockView<'_>> {
    let mut reader = Reader(stored.bytes);
    let folder_count = reader.u32()? as usize;
    let name_count = stored.name_count as usize;
    let name_table_bytes = name_count.checked_mul(FIELD_BYTES)?;
    let folders = reader.take(folder_count.checked_mul(FOLDER_ENTRY_BYTES)?)?;
    let signatures = reader.take(name_table_bytes)?;
    let offsets = reader.take(name_table_bytes)?;
    Some(BlockView {
        span: stored.span,
        name_count: stored.name_count,
        folders,
        signatures,
        offsets,
        names: reader.0,
    })
}

#[derive(Debug)]
pub struct BlockBuilder {
    span: BlockSpan,
    folder_entries: u32,
    name_count: u32,
    folders: Vec<u8>,
    signatures: Vec<u8>,
    offsets: Vec<u8>,
    names: Vec<u8>,
}

impl BlockBuilder {
    pub fn new(first_ordinal: u32) -> Self {
        Self {
            span: BlockSpan {
                first_ordinal,
                folder_count: 0,
            },
            folder_entries: 0,
            name_count: 0,
            folders: Vec::new(),
            signatures: Vec::new(),
            offsets: Vec::new(),
            names: Vec::new(),
        }
    }

    pub fn payload_len(&self) -> usize {
        FOLDER_COUNT_BYTES + self.folders.len() + self.name_tables_len() + self.names.len()
    }

    fn name_tables_len(&self) -> usize {
        self.name_count as usize * BYTES_PER_NAME
    }

    pub fn push(&mut self, ordinal: u32, packed: &[u8]) {
        let relative_id = ordinal - self.span.first_ordinal;
        self.span.folder_count = relative_id + 1;
        if packed.is_empty() {
            return;
        }
        put_u32(&mut self.folders, relative_id);
        put_u32(&mut self.folders, self.name_count);
        self.folder_entries += 1;
        for name in names_in(packed) {
            self.push_name(name);
        }
    }

    fn push_name(&mut self, name: &[u8]) {
        put_u32(&mut self.signatures, signature_of(name));
        put_u32(&mut self.offsets, self.names.len() as u32);
        self.names.extend_from_slice(name);
        self.names.push(NAME_TERMINATOR);
        self.name_count += 1;
    }

    pub fn finish(self) -> BuiltBlock {
        let mut bytes = Vec::with_capacity(self.payload_len());
        put_u32(&mut bytes, self.folder_entries);
        bytes.extend_from_slice(&self.folders);
        bytes.extend_from_slice(&self.signatures);
        bytes.extend_from_slice(&self.offsets);
        bytes.extend_from_slice(&self.names);
        BuiltBlock {
            span: self.span,
            name_count: self.name_count,
            checksum: checksum(&bytes),
            bytes,
        }
    }
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
