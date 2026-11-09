use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use memmap2::Mmap;

use super::block::{self, BlockSpan, NamesCorrupt, StoredBlock};
use super::{is_packed_list_or_empty, NamesScratch};
use crate::size::index::persist::{self, Reader, TempFile};

pub const NAMES_FILE: &str = "file-names.bin";
pub const NAMES_FORMAT_VERSION: u32 = 1;
pub const SPILL_SUFFIX: &str = ".building";

const MAGIC: &[u8; 8] = b"AXONAME\0";
const HEADER_BYTES: usize = 48;
const DIRECTORY_ENTRY_BYTES: usize = 32;
const CHECKSUM_BYTES: usize = size_of::<u64>();
const RESERVED_HEADER_FIELD: u32 = 0;
const MAX_BLOCK_RAW_BYTES: u32 = 256 * 1024 * 1024;
const CHUNK_LENGTH_BYTES: usize = size_of::<u32>();

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NamesHeader {
    pub event_id: u64,
    pub pair_token: u64,
    pub folder_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DirectoryEntry {
    offset: u64,
    compressed_len: u32,
    raw_len: u32,
    span: BlockSpan,
    checksum: u64,
}

impl DirectoryEntry {
    fn bytes(&self) -> Range<usize> {
        let start = self.offset as usize;
        start..start + self.compressed_len as usize
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.offset.to_le_bytes());
        out.extend_from_slice(&self.compressed_len.to_le_bytes());
        out.extend_from_slice(&self.raw_len.to_le_bytes());
        out.extend_from_slice(&self.span.first_ordinal.to_le_bytes());
        out.extend_from_slice(&self.span.folder_count.to_le_bytes());
        out.extend_from_slice(&self.checksum.to_le_bytes());
    }

    fn decode(reader: &mut Reader<'_>) -> Option<Self> {
        Some(Self {
            offset: reader.u64()?,
            compressed_len: reader.u32()?,
            raw_len: reader.u32()?,
            span: BlockSpan {
                first_ordinal: reader.u32()?,
                folder_count: reader.u32()?,
            },
            checksum: reader.u64()?,
        })
    }
}

struct HeaderFields {
    header: NamesHeader,
    block_count: u32,
    directory_offset: u64,
}

impl HeaderFields {
    fn encode(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&NAMES_FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&self.header.folder_count.to_le_bytes());
        out.extend_from_slice(&self.header.event_id.to_le_bytes());
        out.extend_from_slice(&self.header.pair_token.to_le_bytes());
        out.extend_from_slice(&self.block_count.to_le_bytes());
        out.extend_from_slice(&RESERVED_HEADER_FIELD.to_le_bytes());
        out.extend_from_slice(&self.directory_offset.to_le_bytes());
        out
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        let mut reader = Reader(bytes);
        let magic_matches = reader.take(MAGIC.len())? == MAGIC;
        let version_matches = reader.u32()? == NAMES_FORMAT_VERSION;
        if !(magic_matches && version_matches) {
            return None;
        }
        let folder_count = reader.u32()?;
        let event_id = reader.u64()?;
        let pair_token = reader.u64()?;
        let block_count = reader.u32()?;
        let _reserved = reader.u32()?;
        let directory_offset = reader.u64()?;
        Some(Self {
            header: NamesHeader {
                event_id,
                pair_token,
                folder_count,
            },
            block_count,
            directory_offset,
        })
    }
}

#[derive(Debug)]
pub struct MappedNames {
    map: Mmap,
    header: NamesHeader,
    directory: Vec<DirectoryEntry>,
}

impl MappedNames {
    pub fn open(path: &Path) -> Option<Self> {
        let file = File::open(path).ok()?;
        // SAFETY: the file is only ever replaced by atomic rename, never written in place or truncated.
        let map = unsafe { Mmap::map(&file) }.ok()?;
        let layout = parse_layout(&map)?;
        Some(Self {
            map,
            header: layout.header,
            directory: layout.directory,
        })
    }

    pub fn header(&self) -> NamesHeader {
        self.header
    }

    pub fn block_count(&self) -> usize {
        self.directory.len()
    }

    pub fn block(&self, index: usize) -> Option<StoredBlock<'_>> {
        let entry = self.directory.get(index)?;
        Some(StoredBlock {
            span: entry.span,
            raw_len: entry.raw_len,
            checksum: entry.checksum,
            compressed: &self.map[entry.bytes()],
        })
    }

    pub fn span(&self, index: usize) -> Option<BlockSpan> {
        self.directory.get(index).map(|entry| entry.span)
    }

    pub fn blocks_covering(&self, ordinals: Range<u32>) -> Range<usize> {
        let start = self
            .directory
            .partition_point(|entry| entry.span.end() <= ordinals.start);
        if ordinals.is_empty() {
            return start..start;
        }
        let end = self
            .directory
            .partition_point(|entry| entry.span.first_ordinal < ordinals.end);
        start..end.max(start)
    }

    pub fn block_starting_at(&self, ordinal: u32) -> Option<usize> {
        self.directory
            .binary_search_by_key(&ordinal, |entry| entry.span.first_ordinal)
            .ok()
    }
}

struct Layout {
    header: NamesHeader,
    directory: Vec<DirectoryEntry>,
}

fn parse_layout(bytes: &[u8]) -> Option<Layout> {
    let header_bytes = bytes.get(..HEADER_BYTES)?;
    let fields = HeaderFields::decode(header_bytes)?;
    let directory_start = usize::try_from(fields.directory_offset).ok()?;
    let directory_len = (fields.block_count as usize).checked_mul(DIRECTORY_ENTRY_BYTES)?;
    let directory_end = directory_start.checked_add(directory_len)?;
    let file_ends_after_checksum = directory_end.checked_add(CHECKSUM_BYTES)? == bytes.len();
    let directory_after_header = directory_start >= HEADER_BYTES;
    if !(file_ends_after_checksum && directory_after_header) {
        return None;
    }
    let directory_bytes = &bytes[directory_start..directory_end];
    let stored_checksum = u64::from_le_bytes(bytes[directory_end..].try_into().ok()?);
    if header_checksum(header_bytes, directory_bytes) != stored_checksum {
        return None;
    }
    let mut reader = Reader(directory_bytes);
    let directory = (0..fields.block_count)
        .map(|_| DirectoryEntry::decode(&mut reader))
        .collect::<Option<Vec<_>>>()?;
    let bounds = DirectoryBounds {
        blocks_end: directory_start,
        folder_count: fields.header.folder_count,
    };
    bounds.accepts(&directory).then_some(Layout {
        header: fields.header,
        directory,
    })
}

struct DirectoryBounds {
    blocks_end: usize,
    folder_count: u32,
}

impl DirectoryBounds {
    fn accepts(&self, directory: &[DirectoryEntry]) -> bool {
        let expected_firsts =
            std::iter::once(0).chain(directory.iter().map(|entry| entry.span.end()));
        let tiled = directory
            .iter()
            .zip(expected_firsts)
            .all(|(entry, expected_first)| self.accepts_entry(entry, expected_first));
        let covered_end = directory.last().map_or(0, |entry| entry.span.end());
        tiled && covered_end == self.folder_count
    }

    fn accepts_entry(&self, entry: &DirectoryEntry, next_ordinal: u32) -> bool {
        let range = entry.bytes();
        let inside_block_area = range.start >= HEADER_BYTES && range.end <= self.blocks_end;
        let contiguous = entry.span.first_ordinal == next_ordinal;
        let covers_folders = entry.span.folder_count > 0;
        let span_fits = entry
            .span
            .first_ordinal
            .checked_add(entry.span.folder_count)
            .is_some();
        let raw_len_sane = entry.raw_len <= MAX_BLOCK_RAW_BYTES;
        inside_block_area && contiguous && covers_folders && span_fits && raw_len_sane
    }
}

fn header_checksum(header: &[u8], directory: &[u8]) -> u64 {
    let mut covered = header.to_vec();
    covered.extend_from_slice(directory);
    block::checksum(&covered)
}

pub struct NamesFileWriter {
    file: BufWriter<File>,
    temp: PathBuf,
    offset: u64,
    directory: Vec<DirectoryEntry>,
}

impl NamesFileWriter {
    pub fn create(final_path: &Path) -> io::Result<Self> {
        let TempFile { file, path } = TempFile::create(final_path)?;
        let mut file = BufWriter::new(file);
        file.write_all(&[0; HEADER_BYTES])?;
        Ok(Self {
            file,
            temp: path,
            offset: HEADER_BYTES as u64,
            directory: Vec::new(),
        })
    }

    pub fn append(&mut self, block: StoredBlock<'_>, first_ordinal: u32) -> io::Result<()> {
        self.file.write_all(block.compressed)?;
        self.directory.push(DirectoryEntry {
            offset: self.offset,
            compressed_len: block.compressed.len() as u32,
            raw_len: block.raw_len,
            span: BlockSpan {
                first_ordinal,
                folder_count: block.span.folder_count,
            },
            checksum: block.checksum,
        });
        self.offset += block.compressed.len() as u64;
        Ok(())
    }

    pub fn finish(mut self, header: NamesHeader) -> io::Result<PathBuf> {
        let fields = HeaderFields {
            header,
            block_count: self.directory.len() as u32,
            directory_offset: self.offset,
        };
        let header_bytes = fields.encode();
        let mut directory_bytes = Vec::with_capacity(self.directory.len() * DIRECTORY_ENTRY_BYTES);
        for entry in &self.directory {
            entry.encode(&mut directory_bytes);
        }
        let checksum = header_checksum(&header_bytes, &directory_bytes);
        self.file.write_all(&directory_bytes)?;
        self.file.write_all(&checksum.to_le_bytes())?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header_bytes)?;
        let file = self
            .file
            .into_inner()
            .map_err(io::IntoInnerError::into_error)?;
        persist::sync(&file)?;
        Ok(self.temp)
    }

    pub fn discard(self) {
        let temp = self.temp.clone();
        drop(self);
        let _ = fs::remove_file(temp);
    }
}

pub fn spill_path(names_path: &Path) -> PathBuf {
    let mut spill = names_path.as_os_str().to_os_string();
    spill.push(SPILL_SUFFIX);
    PathBuf::from(spill)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpillLocation {
    offset: u64,
    length: u32,
}

impl SpillLocation {
    pub const ABSENT: Self = Self {
        offset: 0,
        length: 0,
    };

    pub fn is_present(&self) -> bool {
        self.length > 0
    }
}

#[derive(Debug)]
pub struct SpillFile {
    file: File,
    path: PathBuf,
    length: AtomicU64,
}

impl SpillFile {
    pub fn create(path: &Path) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        remove_if_present(path)?;
        let file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(path)?;
        Ok(Self {
            file,
            path: path.to_path_buf(),
            length: AtomicU64::new(0),
        })
    }

    pub fn len(&self) -> u64 {
        self.length.load(Ordering::Acquire)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn append(&self, names: &[u8]) -> io::Result<SpillLocation> {
        let mut chunk = (names.len() as u32).to_le_bytes().to_vec();
        chunk.extend_from_slice(&lz4_flex::block::compress(names));
        (&self.file).write_all(&chunk)?;
        let offset = self.length.fetch_add(chunk.len() as u64, Ordering::AcqRel);
        Ok(SpillLocation {
            offset,
            length: chunk.len() as u32,
        })
    }

    pub fn read<'scratch>(
        &self,
        location: SpillLocation,
        scratch: &'scratch mut NamesScratch,
    ) -> Result<&'scratch [u8], NamesCorrupt> {
        let NamesScratch { compressed, raw } = scratch;
        compressed.clear();
        compressed.resize(location.length as usize, 0);
        read_exact_at(&self.file, compressed, location.offset).map_err(|_| NamesCorrupt)?;
        let (length_bytes, body) = compressed
            .split_at_checked(CHUNK_LENGTH_BYTES)
            .ok_or(NamesCorrupt)?;
        let raw_len = u32::from_le_bytes(length_bytes.try_into().map_err(|_| NamesCorrupt)?);
        if raw_len > MAX_BLOCK_RAW_BYTES {
            return Err(NamesCorrupt);
        }
        raw.clear();
        raw.resize(raw_len as usize, 0);
        let written = lz4_flex::block::decompress_into(body, raw).map_err(|_| NamesCorrupt)?;
        let complete = written == raw.len() && is_packed_list_or_empty(raw);
        complete.then_some(raw.as_slice()).ok_or(NamesCorrupt)
    }

    pub fn close_and_remove(self) -> io::Result<()> {
        let Self { file, path, .. } = self;
        drop(file);
        remove_if_present(&path)
    }
}

pub fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

#[cfg(unix)]
fn read_exact_at(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(file, buffer, offset)
}

#[cfg(windows)]
fn read_exact_at(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    let mut filled = 0;
    while filled < buffer.len() {
        let read = file.seek_read(&mut buffer[filled..], offset + filled as u64)?;
        if read == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        filled += read;
    }
    Ok(())
}
