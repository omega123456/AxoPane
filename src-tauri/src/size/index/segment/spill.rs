use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

use super::assemble;
use super::builder::{BuildError, BuiltSegment, SegmentTarget};
use crate::size::index::names::file::remove_if_present;
use crate::size::index::names::{PackedNames, NAME_TERMINATOR};
use crate::size::index::persist::Reader;
use crate::size::index::tree::DirectoryListing;

pub const RUN_BYTES: usize = 64 * 1024 * 1024;

pub(crate) const KEY_SEPARATOR: u8 = 0;

const RUN_FILE_MARKER: &str = ".run";
const LENGTH_BYTES: usize = size_of::<u32>();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpillOptions {
    pub runs_path: PathBuf,
    pub root: PathBuf,
    pub run_bytes: usize,
}

pub struct SpillRecord<'record, S> {
    pub path: &'record Path,
    pub listing: DirectoryListing<'record, S>,
    pub file_names: &'record [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraftTarget {
    pub segment: SegmentTarget,
    pub root_name: String,
}

pub(crate) struct SortedRecord<'bytes> {
    pub(crate) key: &'bytes [u8],
    pub(crate) direct: u64,
    pub(crate) denied: bool,
    pub(crate) file_names: &'bytes [u8],
    pub(crate) subdirs: &'bytes [u8],
}

impl<'bytes> SortedRecord<'bytes> {
    pub(crate) fn decode(bytes: &'bytes [u8]) -> Option<Self> {
        let mut reader = Reader(bytes);
        let key_len = reader.u32()? as usize;
        let key = reader.take(key_len)?;
        let direct = reader.u64()?;
        let denied = reader.take(1)?[0] != 0;
        let names_len = reader.u32()? as usize;
        let file_names = reader.take(names_len)?;
        Some(Self {
            key,
            direct,
            denied,
            file_names,
            subdirs: reader.0,
        })
    }
}

pub(crate) fn key_of(record: &[u8]) -> Option<&[u8]> {
    let mut reader = Reader(record);
    let key_len = reader.u32()? as usize;
    reader.take(key_len)
}

pub(crate) trait SortedRecords {
    fn advance(&mut self) -> io::Result<bool>;
    fn current(&self) -> &[u8];
}

struct RunRecord {
    bytes: Range<usize>,
    key: Range<usize>,
}

#[derive(Default)]
struct Run {
    arena: Vec<u8>,
    records: Vec<RunRecord>,
}

struct EncodedFields<'fields> {
    key: &'fields [u8],
    direct: u64,
    denied: bool,
    file_names: &'fields [u8],
    subdirs: &'fields [u8],
}

impl Run {
    fn push(&mut self, fields: &EncodedFields<'_>) {
        let start = self.arena.len();
        self.arena
            .extend_from_slice(&(fields.key.len() as u32).to_le_bytes());
        let key_start = self.arena.len();
        self.arena.extend_from_slice(fields.key);
        let key = key_start..self.arena.len();
        self.arena.extend_from_slice(&fields.direct.to_le_bytes());
        self.arena.push(u8::from(fields.denied));
        self.arena
            .extend_from_slice(&(fields.file_names.len() as u32).to_le_bytes());
        self.arena.extend_from_slice(fields.file_names);
        self.arena.extend_from_slice(fields.subdirs);
        self.records.push(RunRecord {
            bytes: start..self.arena.len(),
            key,
        });
    }

    fn sort(&mut self) {
        let Self { arena, records } = self;
        records.sort_by(|left, right| arena[left.key.clone()].cmp(&arena[right.key.clone()]));
    }

    fn record(&self, position: usize) -> &[u8] {
        &self.arena[self.records[position].bytes.clone()]
    }

    fn clear(&mut self) {
        self.arena.clear();
        self.records.clear();
    }
}

struct RunFiles {
    base: PathBuf,
    count: usize,
}

impl RunFiles {
    fn path(&self, index: usize) -> PathBuf {
        let mut name = self.base.as_os_str().to_os_string();
        name.push(format!("{RUN_FILE_MARKER}{index}"));
        PathBuf::from(name)
    }

    fn write(&mut self, run: &Run) -> io::Result<()> {
        let path = self.path(self.count);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        self.count += 1;
        let mut file = BufWriter::new(File::create(path)?);
        for position in 0..run.records.len() {
            let record = run.record(position);
            file.write_all(&(record.len() as u32).to_le_bytes())?;
            file.write_all(record)?;
        }
        file.flush()
    }

    fn readers(&self) -> io::Result<Vec<RunReader>> {
        (0..self.count)
            .map(|index| RunReader::open(&self.path(index)))
            .collect()
    }
}

impl Drop for RunFiles {
    fn drop(&mut self) {
        for index in 0..self.count {
            let _ = remove_if_present(&self.path(index));
        }
    }
}

struct MemoryRun {
    run: Run,
    position: Option<usize>,
}

impl SortedRecords for MemoryRun {
    fn advance(&mut self) -> io::Result<bool> {
        let next = self.position.map_or(0, |position| position + 1);
        if next >= self.run.records.len() {
            return Ok(false);
        }
        self.position = Some(next);
        Ok(true)
    }

    fn current(&self) -> &[u8] {
        self.position
            .map_or(&[], |position| self.run.record(position))
    }
}

struct RunReader {
    file: BufReader<File>,
    record: Vec<u8>,
}

impl RunReader {
    fn open(path: &Path) -> io::Result<Self> {
        Ok(Self {
            file: BufReader::new(File::open(path)?),
            record: Vec::new(),
        })
    }

    fn advance(&mut self) -> io::Result<bool> {
        if self.file.fill_buf()?.is_empty() {
            return Ok(false);
        }
        let mut length = [0_u8; LENGTH_BYTES];
        self.file.read_exact(&mut length)?;
        self.record.resize(u32::from_le_bytes(length) as usize, 0);
        self.file.read_exact(&mut self.record)?;
        Ok(true)
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Head {
    key: Vec<u8>,
    run: usize,
}

struct Merge {
    readers: Vec<RunReader>,
    heads: BinaryHeap<Reverse<Head>>,
    current: Option<usize>,
    spare_key: Vec<u8>,
    _files: RunFiles,
}

impl Merge {
    fn over(files: RunFiles) -> io::Result<Self> {
        let mut merge = Self {
            readers: files.readers()?,
            heads: BinaryHeap::new(),
            current: None,
            spare_key: Vec::new(),
            _files: files,
        };
        for run in 0..merge.readers.len() {
            merge.queue_next_of(run)?;
        }
        Ok(merge)
    }

    fn queue_next_of(&mut self, run: usize) -> io::Result<()> {
        let reader = &mut self.readers[run];
        if !reader.advance()? {
            return Ok(());
        }
        let mut key = std::mem::take(&mut self.spare_key);
        key.clear();
        key.extend_from_slice(key_of(&reader.record).unwrap_or_default());
        self.heads.push(Reverse(Head { key, run }));
        Ok(())
    }
}

impl SortedRecords for Merge {
    fn advance(&mut self) -> io::Result<bool> {
        if let Some(run) = self.current.take() {
            self.queue_next_of(run)?;
        }
        let Some(Reverse(head)) = self.heads.pop() else {
            return Ok(false);
        };
        self.current = Some(head.run);
        self.spare_key = head.key;
        Ok(true)
    }

    fn current(&self) -> &[u8] {
        self.current.map_or(&[], |run| &self.readers[run].record)
    }
}

pub struct WalkSpill {
    root: PathBuf,
    run_bytes: usize,
    run: Run,
    files: RunFiles,
    record_count: u64,
}

impl WalkSpill {
    pub fn create(options: SpillOptions) -> Self {
        Self {
            root: options.root,
            run_bytes: options.run_bytes,
            run: Run::default(),
            files: RunFiles {
                base: options.runs_path,
                count: 0,
            },
            record_count: 0,
        }
    }

    pub fn record_count(&self) -> u64 {
        self.record_count
    }

    pub fn run_file_count(&self) -> usize {
        self.files.count
    }

    pub fn append<S: AsRef<OsStr>>(
        &mut self,
        record: SpillRecord<'_, S>,
    ) -> Result<(), BuildError> {
        let key = relative_key(&self.root, record.path).ok_or(BuildError::Malformed)?;
        let subdirs = sorted_unique_names(record.listing.subdirs);
        self.run.push(&EncodedFields {
            key: &key,
            direct: record.listing.direct_bytes,
            denied: record.listing.denied,
            file_names: record.file_names,
            subdirs: subdirs.as_bytes(),
        });
        self.record_count += 1;
        if self.run.arena.len() >= self.run_bytes {
            self.write_run()?;
        }
        Ok(())
    }

    fn write_run(&mut self) -> io::Result<()> {
        self.run.sort();
        self.files.write(&self.run)?;
        self.run.clear();
        Ok(())
    }

    pub fn build(self, target: GraftTarget) -> Result<BuiltSegment, BuildError> {
        let mut sorted = self.into_sorted()?;
        assemble::build_graft(sorted.as_mut(), target)
    }

    fn into_sorted(mut self) -> io::Result<Box<dyn SortedRecords>> {
        if self.files.count == 0 {
            self.run.sort();
            return Ok(Box::new(MemoryRun {
                run: self.run,
                position: None,
            }));
        }
        if !self.run.records.is_empty() {
            self.write_run()?;
        }
        Ok(Box::new(Merge::over(self.files)?))
    }
}

fn relative_key(root: &Path, path: &Path) -> Option<Vec<u8>> {
    let mut key = Vec::new();
    for component in path.strip_prefix(root).ok()?.components() {
        let Component::Normal(name) = component else {
            return None;
        };
        let name = name.to_string_lossy();
        if name.as_bytes().contains(&KEY_SEPARATOR) {
            return None;
        }
        if !key.is_empty() {
            key.push(KEY_SEPARATOR);
        }
        key.extend_from_slice(name.as_bytes());
    }
    Some(key)
}

fn sorted_unique_names<S: AsRef<OsStr>>(subdirs: &[S]) -> PackedNames {
    let mut names: Vec<String> = subdirs
        .iter()
        .map(|subdir| subdir.as_ref().to_string_lossy().into_owned())
        .filter(|name| !name.as_bytes().contains(&NAME_TERMINATOR))
        .collect();
    names.sort();
    names.dedup();
    PackedNames::from_names(names)
}
