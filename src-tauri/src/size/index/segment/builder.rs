use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::ops::Range;
use std::path::PathBuf;

use memmap2::MmapMut;

use super::format::{
    le_u32, le_u64, Cell, CellRun, Column, Counts, Layout, NodeFlags, SegmentPaths, TreeHeader,
    HEADER_BYTES, MAX_SEGMENT_NODES, NO_PARENT,
};
use crate::size::index::names::block::{self, BLOCK_TARGET_BYTES};
use crate::size::index::names::file::{remove_if_present, NamesHeader};
use crate::size::index::names::is_packed_list_or_empty;
use crate::size::index::names::writer::{BlockOutput, OutputLimits, RewriteError, RewriteStats};
use crate::size::index::persist::{self, Reader};
use crate::size::index::tree::OwnCounters;

const ROWS_EXTENSION: &str = "rows";
const ROW_HEAD_BYTES: usize =
    size_of::<u32>() + size_of::<u64>() + size_of::<u8>() + size_of::<u32>();
const SEGMENT_EVENT_ID: u64 = 0;

#[derive(Debug)]
pub enum BuildError {
    Io(io::Error),
    Names(RewriteError),
    Malformed,
    TooLarge,
    NoRoot,
}

impl From<io::Error> for BuildError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<RewriteError> for BuildError {
    fn from(error: RewriteError) -> Self {
        Self::Names(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentTarget {
    pub paths: SegmentPaths,
    pub token: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentEntry<'entry> {
    pub depth: usize,
    pub name: &'entry str,
    pub direct: u64,
    pub flags: NodeFlags,
    pub file_names: &'entry [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltSegment {
    pub paths: SegmentPaths,
    pub node_count: u32,
    pub names: RewriteStats,
}

struct LoggedRow<'row> {
    parent: u32,
    direct: u64,
    flags: u8,
    name: &'row [u8],
}

struct RowLog {
    file: BufWriter<File>,
    path: PathBuf,
}

impl RowLog {
    fn create(path: PathBuf) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = BufWriter::new(File::create(&path)?);
        Ok(Self { file, path })
    }

    fn append(&mut self, row: &LoggedRow<'_>) -> io::Result<()> {
        self.file.write_all(&row.parent.to_le_bytes())?;
        self.file.write_all(&row.direct.to_le_bytes())?;
        self.file.write_all(&[row.flags])?;
        self.file
            .write_all(&(row.name.len() as u32).to_le_bytes())?;
        self.file.write_all(row.name)
    }

    fn into_reader(mut self) -> io::Result<RowLogReader> {
        self.file.flush()?;
        drop(self.file);
        Ok(RowLogReader {
            file: BufReader::new(File::open(&self.path)?),
            name: Vec::new(),
        })
    }
}

struct RowLogReader {
    file: BufReader<File>,
    name: Vec<u8>,
}

impl RowLogReader {
    fn next_row(&mut self) -> io::Result<LoggedRow<'_>> {
        let mut head = [0_u8; ROW_HEAD_BYTES];
        self.file.read_exact(&mut head)?;
        let mut fields = Reader(&head);
        let parsed = (fields.u32(), fields.u64(), fields.take(1), fields.u32());
        let (Some(parent), Some(direct), Some(flags), Some(name_len)) = parsed else {
            return Err(io::ErrorKind::InvalidData.into());
        };
        self.name.resize(name_len as usize, 0);
        self.file.read_exact(&mut self.name)?;
        Ok(LoggedRow {
            parent,
            direct,
            flags: flags[0],
            name: &self.name,
        })
    }
}

struct OpenFrame {
    ordinal: u32,
    last_child: String,
}

struct TempFiles {
    rows: PathBuf,
    tree: PathBuf,
    names: PathBuf,
}

impl TempFiles {
    fn of(paths: &SegmentPaths) -> Self {
        Self {
            rows: paths.tree.with_extension(ROWS_EXTENSION),
            tree: persist::temp_path(&paths.tree),
            names: persist::temp_path(&paths.names),
        }
    }

    fn remove_leftovers(&self) {
        for leftover in [&self.rows, &self.tree, &self.names] {
            let _ = remove_if_present(leftover);
        }
    }
}

pub struct SegmentBuilder {
    target: SegmentTarget,
    temps: TempFiles,
    rows: RowLog,
    names: BlockOutput,
    open: Vec<OpenFrame>,
    counts: Counts,
}

impl SegmentBuilder {
    pub fn create(target: SegmentTarget) -> Result<Self, BuildError> {
        let temps = TempFiles::of(&target.paths);
        let limits = OutputLimits {
            block_target: BLOCK_TARGET_BYTES,
            deadline: None,
        };
        let rows = RowLog::create(temps.rows.clone())?;
        let names = match BlockOutput::create(&target.paths.names, limits) {
            Ok(names) => names,
            Err(error) => {
                temps.remove_leftovers();
                return Err(error.into());
            }
        };
        Ok(Self {
            target,
            temps,
            rows,
            names,
            open: Vec::new(),
            counts: Counts::default(),
        })
    }

    pub fn node_count(&self) -> u32 {
        self.counts.nodes
    }

    pub fn push(&mut self, entry: SegmentEntry<'_>) -> Result<u32, BuildError> {
        let ordinal = self.counts.nodes;
        let name_heap_bytes = u32::try_from(entry.name.len())
            .ok()
            .and_then(|name_bytes| self.counts.name_heap_bytes.checked_add(name_bytes));
        let (Some(name_heap_bytes), true) = (name_heap_bytes, ordinal < MAX_SEGMENT_NODES) else {
            return Err(BuildError::TooLarge);
        };
        if !is_packed_list_or_empty(entry.file_names) {
            return Err(BuildError::Malformed);
        }
        let parent = self.enter(entry.depth, entry.name)?;
        self.rows.append(&LoggedRow {
            parent,
            direct: entry.direct,
            flags: entry.flags.bits(),
            name: entry.name.as_bytes(),
        })?;
        self.names.push(ordinal, entry.file_names)?;
        self.counts.nodes += 1;
        self.counts.top_level += u32::from(parent == NO_PARENT);
        self.counts.name_heap_bytes = name_heap_bytes;
        self.open.push(OpenFrame {
            ordinal,
            last_child: String::new(),
        });
        Ok(ordinal)
    }

    fn enter(&mut self, depth: usize, name: &str) -> Result<u32, BuildError> {
        let skips_a_level = depth > self.open.len();
        if name.is_empty() || skips_a_level {
            return Err(BuildError::Malformed);
        }
        self.open.truncate(depth);
        let Some(parent) = self.open.last_mut() else {
            return Ok(NO_PARENT);
        };
        let follows_previous_sibling = parent.last_child.as_str() < name;
        if !follows_previous_sibling {
            return Err(BuildError::Malformed);
        }
        parent.last_child.clear();
        parent.last_child.push_str(name);
        Ok(parent.ordinal)
    }

    pub fn finish(self) -> Result<BuiltSegment, BuildError> {
        let Self {
            target,
            temps,
            rows,
            names,
            counts,
            ..
        } = self;
        let segment = PendingSegment {
            target,
            temps: &temps,
            counts,
        };
        let built = segment.write(rows, names);
        temps.remove_leftovers();
        built
    }

    pub fn discard(self) {
        let Self {
            temps, rows, names, ..
        } = self;
        names.discard();
        drop(rows);
        temps.remove_leftovers();
    }
}

struct PendingSegment<'temps> {
    target: SegmentTarget,
    temps: &'temps TempFiles,
    counts: Counts,
}

impl PendingSegment<'_> {
    fn write(self, rows: RowLog, names: BlockOutput) -> Result<BuiltSegment, BuildError> {
        let finished_names = names.finish(NamesHeader {
            event_id: SEGMENT_EVENT_ID,
            pair_token: self.target.token,
            folder_count: self.counts.nodes,
        })?;
        self.write_tree(rows.into_reader()?)?;
        fs::rename(&self.temps.tree, &self.target.paths.tree)?;
        fs::rename(&finished_names.temp, &self.target.paths.names)?;
        Ok(BuiltSegment {
            paths: self.target.paths,
            node_count: self.counts.nodes,
            names: finished_names.stats,
        })
    }

    fn write_tree(&self, rows: RowLogReader) -> Result<(), BuildError> {
        let layout = Layout::for_counts(self.counts).ok_or(BuildError::TooLarge)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.temps.tree)?;
        file.set_len(layout.file_len() as u64)?;
        // SAFETY: the temp file was just created by this builder and nothing else opens it.
        let mut map = unsafe { MmapMut::map_mut(&file) }?;
        let mut columns = ColumnsMut {
            bytes: &mut map,
            layout: &layout,
            node_count: self.counts.nodes,
        };
        columns.fill_rows(rows)?;
        columns.roll_up();
        columns.assign_child_offsets();
        columns.list_children();
        columns.seal(TreeHeader {
            counts: self.counts,
            token: self.target.token,
        });
        map.flush()?;
        drop(map);
        Ok(persist::sync(&file)?)
    }
}

struct RowPlace {
    ordinal: u32,
    name_start: u32,
}

struct ColumnsMut<'map> {
    bytes: &'map mut [u8],
    layout: &'map Layout,
    node_count: u32,
}

impl ColumnsMut<'_> {
    fn cell(&self, cell: Cell) -> Range<usize> {
        self.layout.cell(cell).expect("cell inside its column")
    }

    fn u32(&self, cell: Cell) -> u32 {
        le_u32(&self.bytes[self.cell(cell)]).expect("four byte cell")
    }

    fn set_u32(&mut self, cell: Cell, value: u32) {
        let range = self.cell(cell);
        self.bytes[range].copy_from_slice(&value.to_le_bytes());
    }

    fn add_u32(&mut self, cell: Cell, amount: u32) {
        self.set_u32(cell, self.u32(cell).wrapping_add(amount));
    }

    fn u64(&self, cell: Cell) -> u64 {
        le_u64(&self.bytes[self.cell(cell)]).expect("eight byte cell")
    }

    fn set_u64(&mut self, cell: Cell, value: u64) {
        let range = self.cell(cell);
        self.bytes[range].copy_from_slice(&value.to_le_bytes());
    }

    fn fill_rows(&mut self, mut rows: RowLogReader) -> io::Result<()> {
        let mut name_start = 0;
        for ordinal in 0..self.node_count {
            let row = rows.next_row()?;
            name_start = self.fill_row(
                RowPlace {
                    ordinal,
                    name_start,
                },
                &row,
            );
        }
        Ok(())
    }

    fn fill_row(&mut self, place: RowPlace, row: &LoggedRow<'_>) -> u32 {
        let RowPlace {
            ordinal,
            name_start,
        } = place;
        let own = OwnCounters::of(row.flags);
        let name_end = name_start + row.name.len() as u32;
        let flags = self.cell(Column::Flags.at(ordinal));
        self.bytes[flags].copy_from_slice(&[row.flags]);
        self.set_u32(Column::Parent.at(ordinal), row.parent);
        self.set_u64(Column::Direct.at(ordinal), row.direct);
        self.set_u64(Column::Total.at(ordinal), row.direct);
        self.set_u32(Column::Pending.at(ordinal), own.pending);
        self.set_u32(Column::Skipped.at(ordinal), own.skipped);
        self.set_u32(Column::Denied.at(ordinal), own.denied);
        self.set_u32(Column::SubtreeEnd.at(ordinal), ordinal + 1);
        self.set_u32(Column::NameEnds.at(ordinal + 1), name_end);
        let name = self.layout.run(CellRun {
            column: Column::NameHeap,
            first: name_start,
            count: row.name.len() as u32,
        });
        self.bytes[name.expect("name inside the heap")].copy_from_slice(row.name);
        name_end
    }

    fn roll_up(&mut self) {
        for ordinal in (0..self.node_count).rev() {
            self.roll_up_into_parent(ordinal);
        }
    }

    fn roll_up_into_parent(&mut self, ordinal: u32) {
        let parent = self.u32(Column::Parent.at(ordinal));
        if parent == NO_PARENT {
            return;
        }
        for counter in [Column::Pending, Column::Skipped, Column::Denied] {
            self.add_u32(counter.at(parent), self.u32(counter.at(ordinal)));
        }
        let total = self
            .u64(Column::Total.at(parent))
            .wrapping_add(self.u64(Column::Total.at(ordinal)));
        self.set_u64(Column::Total.at(parent), total);
        let subtree_end = self
            .u32(Column::SubtreeEnd.at(parent))
            .max(self.u32(Column::SubtreeEnd.at(ordinal)));
        self.set_u32(Column::SubtreeEnd.at(parent), subtree_end);
        self.add_u32(Column::ChildCount.at(parent), 1);
    }

    fn assign_child_offsets(&mut self) {
        let mut next_offset = 0;
        for ordinal in 0..self.node_count {
            self.set_u32(Column::ChildOffset.at(ordinal), next_offset);
            next_offset += self.u32(Column::ChildCount.at(ordinal));
            self.set_u32(Column::ChildCount.at(ordinal), 0);
        }
    }

    fn list_children(&mut self) {
        for ordinal in 0..self.node_count {
            self.list_under_parent(ordinal);
        }
    }

    fn list_under_parent(&mut self, ordinal: u32) {
        let parent = self.u32(Column::Parent.at(ordinal));
        if parent == NO_PARENT {
            return;
        }
        let listed_so_far = self.u32(Column::ChildCount.at(parent));
        let position = self.u32(Column::ChildOffset.at(parent)) + listed_so_far;
        self.set_u32(Column::Children.at(position), ordinal);
        self.add_u32(Column::ChildCount.at(parent), 1);
    }

    fn seal(&mut self, header: TreeHeader) {
        for chunk in 0..self.layout.chunk_count() {
            let checksum = block::checksum(&self.bytes[self.layout.chunk(chunk)]);
            self.bytes[self.layout.chunk_checksum(chunk)].copy_from_slice(&checksum.to_le_bytes());
        }
        let encoded = header.encode(&self.bytes[self.layout.chunk_table()]);
        self.bytes[..HEADER_BYTES].copy_from_slice(&encoded);
    }
}
