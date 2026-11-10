use super::builder::{BuildError, BuiltSegment, SegmentBuilder, SegmentEntry};
use super::format::NodeFlags;
use super::spill::{key_of, GraftTarget, SortedRecord, SortedRecords, KEY_SEPARATOR};
use crate::size::index::names::NAME_TERMINATOR;

const ROOT_DEPTH: usize = 0;
const NO_FILE_NAMES: &[u8] = &[];

pub(crate) fn build_graft(
    sorted: &mut dyn SortedRecords,
    target: GraftTarget,
) -> Result<BuiltSegment, BuildError> {
    let mut assembler = Assembler {
        builder: SegmentBuilder::create(target.segment)?,
        root_name: target.root_name,
        frames: Vec::new(),
    };
    if let Err(error) = assembler.place_all(sorted) {
        assembler.builder.discard();
        return Err(error);
    }
    assembler.builder.finish()
}

#[derive(Default)]
struct PendingNames {
    packed: Vec<u8>,
    consumed: usize,
}

impl PendingNames {
    fn peek(&self) -> Option<&str> {
        let rest = &self.packed[self.consumed..];
        let length = rest.iter().position(|&byte| byte == NAME_TERMINATOR)?;
        std::str::from_utf8(&rest[..length]).ok()
    }

    fn take_before(&mut self, bound: Option<&str>) -> Option<String> {
        let next = self.peek()?;
        let past_bound = bound.is_some_and(|bound| next >= bound);
        if past_bound {
            return None;
        }
        let taken = next.to_string();
        self.consumed += taken.len() + 1;
        Some(taken)
    }

    fn skip(&mut self, name: &str) {
        if self.peek() == Some(name) {
            self.consumed += name.len() + 1;
        }
    }
}

struct Frame {
    name: String,
    pending: PendingNames,
}

struct Own<'record> {
    direct: u64,
    flags: NodeFlags,
    file_names: &'record [u8],
    subdirs: &'record [u8],
}

impl<'record> Own<'record> {
    const UNSCANNED: Self = Self {
        direct: 0,
        flags: NodeFlags {
            scanned: false,
            denied: false,
            skipped: false,
        },
        file_names: NO_FILE_NAMES,
        subdirs: &[],
    };

    fn of(record: &SortedRecord<'record>) -> Self {
        Self {
            direct: record.direct,
            flags: NodeFlags {
                scanned: true,
                denied: record.denied,
                skipped: false,
            },
            file_names: record.file_names,
            subdirs: record.subdirs,
        }
    }
}

struct Assembler {
    builder: SegmentBuilder,
    root_name: String,
    frames: Vec<Frame>,
}

impl Assembler {
    fn place_all(&mut self, sorted: &mut dyn SortedRecords) -> Result<(), BuildError> {
        let mut held: Vec<u8> = Vec::new();
        while sorted.advance()? {
            let next = sorted.current();
            self.place_held_unless_replaced(&held, next)?;
            held.clear();
            held.extend_from_slice(next);
        }
        if held.is_empty() {
            return Err(BuildError::NoRoot);
        }
        self.place(&held)?;
        self.close_to(ROOT_DEPTH)
    }

    fn place_held_unless_replaced(&mut self, held: &[u8], next: &[u8]) -> Result<(), BuildError> {
        let replaced_by_next = held.is_empty() || key_of(next) == key_of(held);
        if replaced_by_next {
            return Ok(());
        }
        self.place(held)
    }

    fn place(&mut self, bytes: &[u8]) -> Result<(), BuildError> {
        let record = SortedRecord::decode(bytes).ok_or(BuildError::Malformed)?;
        let key = std::str::from_utf8(record.key).map_err(|_| BuildError::Malformed)?;
        let own = Own::of(&record);
        if self.frames.is_empty() {
            return self.place_root(key, &own);
        }
        let mut path: Vec<&str> = key.split(char::from(KEY_SEPARATOR)).collect();
        let name = path.pop().ok_or(BuildError::Malformed)?;
        let shared = self.shared_depth(&path);
        self.close_to(shared + 1)?;
        for missing_parent in &path[shared..] {
            self.open_child(missing_parent, &Own::UNSCANNED)?;
        }
        self.open_child(name, &own)
    }

    fn place_root(&mut self, key: &str, own: &Own<'_>) -> Result<(), BuildError> {
        if !key.is_empty() {
            return Err(BuildError::NoRoot);
        }
        let root_name = std::mem::take(&mut self.root_name);
        self.push_frame(&root_name, own)
    }

    fn shared_depth(&self, parents: &[&str]) -> usize {
        let open_below_root = self.frames.iter().skip(1);
        open_below_root
            .zip(parents)
            .take_while(|(frame, parent)| frame.name == **parent)
            .count()
    }

    fn close_to(&mut self, depth: usize) -> Result<(), BuildError> {
        while self.frames.len() > depth {
            self.emit_pending_before(None)?;
            self.frames.pop();
        }
        Ok(())
    }

    fn open_child(&mut self, name: &str, own: &Own<'_>) -> Result<(), BuildError> {
        self.emit_pending_before(Some(name))?;
        if let Some(parent) = self.frames.last_mut() {
            parent.pending.skip(name);
        }
        self.push_frame(name, own)
    }

    fn emit_pending_before(&mut self, bound: Option<&str>) -> Result<(), BuildError> {
        while let Some(leaf) = self.take_pending_before(bound) {
            self.push_entry(&leaf, &Own::UNSCANNED)?;
        }
        Ok(())
    }

    fn take_pending_before(&mut self, bound: Option<&str>) -> Option<String> {
        self.frames.last_mut()?.pending.take_before(bound)
    }

    fn push_frame(&mut self, name: &str, own: &Own<'_>) -> Result<(), BuildError> {
        self.push_entry(name, own)?;
        self.frames.push(Frame {
            name: name.to_string(),
            pending: PendingNames {
                packed: own.subdirs.to_vec(),
                consumed: 0,
            },
        });
        Ok(())
    }

    fn push_entry(&mut self, name: &str, own: &Own<'_>) -> Result<(), BuildError> {
        self.builder.push(SegmentEntry {
            depth: self.frames.len(),
            name,
            direct: own.direct,
            flags: own.flags,
            file_names: own.file_names,
        })?;
        Ok(())
    }
}
