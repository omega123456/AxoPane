use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use file_explorer_lib::size::index::names::block::{checksum, decode, BlockBuilder, EncodedBlock};
use file_explorer_lib::size::index::names::codec::{DecodeScratch, NamesEncoder};
use file_explorer_lib::size::index::names::file::{
    remove_if_present, spill_path, MappedNames, NamesFileWriter, NamesHeader, NAMES_FILE,
    NAMES_FORMAT_VERSION, SPILL_SUFFIX,
};
use file_explorer_lib::size::index::names::PackedNames;
use file_explorer_lib::size::index::persist::temp_path;
use tempfile::{tempdir, TempDir};

const HEADER_BYTES: usize = 48;
const DIRECTORY_ENTRY_BYTES: usize = 32;

fn packed(names: &[&str]) -> PackedNames {
    PackedNames::from_names(names.iter().map(|name| name.as_bytes()))
}

fn finished(builder: BlockBuilder) -> EncodedBlock {
    let mut encoder = NamesEncoder::new().expect("encoder");
    builder.finish(&mut encoder).expect("compresses")
}

/// One block per folder range: 0..2, 2..5, 5..6.
fn blocks() -> Vec<EncodedBlock> {
    let mut first = BlockBuilder::new(0);
    first.push(0, packed(&["root.txt"]).as_bytes());
    first.push(1, packed(&["a.md", ".env"]).as_bytes());
    let mut second = BlockBuilder::new(2);
    second.push(2, &[]);
    second.push(4, packed(&["deep.rs"]).as_bytes());
    let mut third = BlockBuilder::new(5);
    third.push(5, packed(&["last"]).as_bytes());
    vec![finished(first), finished(second), finished(third)]
}

const HEADER: NamesHeader = NamesHeader {
    event_id: 4242,
    pair_token: 77,
    folder_count: 6,
};

struct Fixture {
    _dir: TempDir,
    path: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("cache").join(NAMES_FILE);
    Fixture { _dir: dir, path }
}

fn write_file(path: &Path, blocks: &[EncodedBlock], header: NamesHeader) {
    let mut writer = NamesFileWriter::create(path).expect("writer");
    for block in blocks {
        writer
            .append(block.stored(), block.span.first_ordinal)
            .expect("append");
    }
    let temp = writer.finish(header).expect("finish");
    assert_eq!(temp, temp_path(path));
    fs::rename(temp, path).expect("rename into place");
}

fn names_at(file: &MappedNames, ordinal: u32) -> Option<Vec<u8>> {
    let index = file.blocks_covering(ordinal..ordinal + 1).start;
    let mut scratch = DecodeScratch::default();
    let decoded = decode(file.block(index)?, &mut scratch).expect("decodes");
    decoded.names_of(ordinal).map(<[u8]>::to_vec)
}

#[test]
fn a_written_file_maps_and_locates_blocks_by_folder_range() {
    let fixture = fixture();
    write_file(&fixture.path, &blocks(), HEADER);
    let file = MappedNames::open(&fixture.path).expect("opens");
    assert_eq!(file.header(), HEADER);
    assert_eq!(file.block_count(), 3);
    assert_eq!(file.span(1).map(|span| span.ordinals()), Some(2..5));
    assert_eq!(file.span(3), None);
    assert!(file.block(3).is_none());
    assert_eq!(file.blocks_covering(0..1), 0..1);
    assert_eq!(file.blocks_covering(1..3), 0..2);
    assert_eq!(file.blocks_covering(3..4), 1..2);
    assert_eq!(file.blocks_covering(0..6), 0..3);
    assert_eq!(
        file.blocks_covering(4..4),
        1..1,
        "an empty range selects nothing"
    );
    assert_eq!(file.blocks_covering(9..12), 3..3);
    assert_eq!(file.block_starting_at(2), Some(1));
    assert_eq!(file.block_starting_at(3), None);
    assert_eq!(
        names_at(&file, 1),
        Some(packed(&["a.md", ".env"]).as_bytes().to_vec())
    );
    assert_eq!(
        names_at(&file, 4),
        Some(packed(&["deep.rs"]).as_bytes().to_vec())
    );
    assert_eq!(names_at(&file, 3), None);
    assert!(!temp_path(&fixture.path).exists());
}

#[test]
fn an_empty_file_is_valid_and_missing_or_foreign_files_are_not() {
    let fixture = fixture();
    let empty = NamesHeader {
        folder_count: 0,
        ..HEADER
    };
    write_file(&fixture.path, &[], empty);
    let file = MappedNames::open(&fixture.path).expect("opens");
    assert_eq!(file.block_count(), 0);
    assert_eq!(file.blocks_covering(0..10), 0..0);

    assert!(MappedNames::open(&fixture.path.with_file_name("missing.bin")).is_none());
    assert!(MappedNames::open(fixture.path.parent().expect("dir")).is_none());
    fs::write(&fixture.path, b"").expect("empty file");
    assert!(MappedNames::open(&fixture.path).is_none());
    fs::write(&fixture.path, b"not a names file at all, just text").expect("text");
    assert!(MappedNames::open(&fixture.path).is_none());
}

#[test]
fn any_header_or_directory_corruption_rejects_the_file() {
    let fixture = fixture();
    write_file(&fixture.path, &blocks(), HEADER);
    let bytes = fs::read(&fixture.path).expect("written");
    let blocks_len: usize = blocks().iter().map(|block| block.compressed.len()).sum();
    let directory_start = HEADER_BYTES + blocks_len;
    let covered = (0..HEADER_BYTES).chain(directory_start..bytes.len());
    for at in covered {
        let mut corrupt = bytes.clone();
        corrupt[at] ^= 0x41;
        fs::write(&fixture.path, &corrupt).expect("corrupt");
        assert!(MappedNames::open(&fixture.path).is_none(), "flip at {at}");
    }
    for len in [0, HEADER_BYTES - 1, HEADER_BYTES, bytes.len() - 1] {
        fs::write(&fixture.path, &bytes[..len]).expect("truncated");
        assert!(
            MappedNames::open(&fixture.path).is_none(),
            "truncated to {len}"
        );
    }
    let mut longer = bytes.clone();
    longer.push(0);
    fs::write(&fixture.path, &longer).expect("longer");
    assert!(MappedNames::open(&fixture.path).is_none());
}

#[test]
fn a_corrupt_block_still_maps_but_fails_its_own_checksum() {
    let fixture = fixture();
    write_file(&fixture.path, &blocks(), HEADER);
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&fixture.path)
        .expect("open");
    file.seek(SeekFrom::Start(HEADER_BYTES as u64))
        .expect("seek");
    file.write_all(&[0xEE]).expect("corrupt block byte");
    drop(file);
    let mapped = MappedNames::open(&fixture.path).expect("header still valid");
    let mut scratch = DecodeScratch::default();
    assert!(decode(mapped.block(0).expect("block"), &mut scratch).is_err());
    assert!(decode(mapped.block(1).expect("block"), &mut scratch).is_ok());
}

struct CraftedEntry {
    offset: u64,
    compressed_len: u32,
    raw_len: u32,
    first: u32,
    count: u32,
}

struct Crafted {
    magic: &'static [u8; 8],
    version: u32,
    folder_count: u32,
    directory_offset: Option<u64>,
    entries: Vec<CraftedEntry>,
}

fn one_block() -> EncodedBlock {
    let mut builder = BlockBuilder::new(0);
    builder.push(0, packed(&["x"]).as_bytes());
    builder.push(1, &[]);
    finished(builder)
}

fn valid_crafted() -> Crafted {
    let block = one_block();
    Crafted {
        magic: b"AXONAME\0",
        version: NAMES_FORMAT_VERSION,
        folder_count: 2,
        directory_offset: None,
        entries: vec![CraftedEntry {
            offset: HEADER_BYTES as u64,
            compressed_len: block.compressed.len() as u32,
            raw_len: block.raw_len,
            first: 0,
            count: 2,
        }],
    }
}

/// Lays out a names file by hand with a correct header checksum.
fn crafted_bytes(crafted: &Crafted) -> Vec<u8> {
    let block = one_block();
    let natural_offset = (HEADER_BYTES + block.compressed.len()) as u64;
    let mut header = crafted.magic.to_vec();
    header.extend_from_slice(&crafted.version.to_le_bytes());
    header.extend_from_slice(&crafted.folder_count.to_le_bytes());
    header.extend_from_slice(&1_u64.to_le_bytes());
    header.extend_from_slice(&2_u64.to_le_bytes());
    header.extend_from_slice(&(crafted.entries.len() as u32).to_le_bytes());
    header.extend_from_slice(&0_u32.to_le_bytes());
    header.extend_from_slice(
        &crafted
            .directory_offset
            .unwrap_or(natural_offset)
            .to_le_bytes(),
    );
    let mut directory = Vec::new();
    for entry in &crafted.entries {
        directory.extend_from_slice(&entry.offset.to_le_bytes());
        directory.extend_from_slice(&entry.compressed_len.to_le_bytes());
        directory.extend_from_slice(&entry.raw_len.to_le_bytes());
        directory.extend_from_slice(&entry.first.to_le_bytes());
        directory.extend_from_slice(&entry.count.to_le_bytes());
        directory.extend_from_slice(&block.checksum.to_le_bytes());
    }
    let mut covered = header.clone();
    covered.extend_from_slice(&directory);
    let mut out = header;
    out.extend_from_slice(&block.compressed);
    out.extend_from_slice(&directory);
    out.extend_from_slice(&checksum(&covered).to_le_bytes());
    out
}

fn crafted_opens(crafted: &Crafted) -> bool {
    let fixture = fixture();
    fs::create_dir_all(fixture.path.parent().expect("dir")).expect("cache dir");
    fs::write(&fixture.path, crafted_bytes(crafted)).expect("crafted");
    MappedNames::open(&fixture.path).is_some()
}

fn with_entry(edit: impl FnOnce(&mut CraftedEntry)) -> Crafted {
    let mut crafted = valid_crafted();
    edit(&mut crafted.entries[0]);
    crafted
}

#[test]
fn header_violations_are_rejected_even_with_a_valid_checksum() {
    assert!(crafted_opens(&valid_crafted()), "control");
    assert_eq!(DIRECTORY_ENTRY_BYTES, 32);
    assert_eq!(NAMES_FORMAT_VERSION, 2, "zstd blocks");
    let rejected = [
        (
            Crafted {
                magic: b"AXOSIZE\0",
                ..valid_crafted()
            },
            "foreign magic",
        ),
        (
            Crafted {
                version: NAMES_FORMAT_VERSION + 1,
                ..valid_crafted()
            },
            "newer version",
        ),
        (
            Crafted {
                version: NAMES_FORMAT_VERSION - 1,
                ..valid_crafted()
            },
            "older LZ4 version",
        ),
        (
            Crafted {
                folder_count: 3,
                ..valid_crafted()
            },
            "blocks do not cover every folder",
        ),
        (
            Crafted {
                directory_offset: Some(8),
                ..valid_crafted()
            },
            "directory overlaps the header",
        ),
        (
            Crafted {
                directory_offset: Some(u64::MAX),
                ..valid_crafted()
            },
            "directory beyond the file",
        ),
    ];
    for (crafted, what) in rejected {
        assert!(!crafted_opens(&crafted), "{what}");
    }
}

#[test]
fn directory_entry_violations_are_rejected_even_with_a_valid_checksum() {
    let rejected = [
        (with_entry(|entry| entry.first = 1), "first block not at 0"),
        (with_entry(|entry| entry.count = 0), "an empty span"),
        (
            with_entry(|entry| entry.offset = 0),
            "block inside the header",
        ),
        (
            with_entry(|entry| entry.compressed_len += 4096),
            "block beyond the block area",
        ),
        (
            with_entry(|entry| entry.raw_len = u32::MAX),
            "implausible raw length",
        ),
        (
            with_entry(|entry| {
                entry.first = u32::MAX;
                entry.count = 2;
            }),
            "overflowing span",
        ),
    ];
    for (crafted, what) in rejected {
        assert!(!crafted_opens(&crafted), "{what}");
    }
}

#[test]
fn a_gap_between_blocks_is_rejected() {
    let block = one_block();
    let mut crafted = valid_crafted();
    crafted.folder_count = 5;
    crafted.entries.push(CraftedEntry {
        offset: HEADER_BYTES as u64,
        compressed_len: block.compressed.len() as u32,
        raw_len: block.raw_len,
        first: 3,
        count: 2,
    });
    assert!(!crafted_opens(&crafted));
    crafted.entries[1].first = 2;
    crafted.folder_count = 4;
    assert!(crafted_opens(&crafted), "contiguous blocks may share bytes");
}

#[test]
fn a_discarded_writer_leaves_no_temp_file() {
    let fixture = fixture();
    let mut writer = NamesFileWriter::create(&fixture.path).expect("writer");
    let block = one_block();
    writer.append(block.stored(), 0).expect("append");
    assert!(temp_path(&fixture.path).exists());
    writer.discard();
    assert!(!temp_path(&fixture.path).exists());
    assert!(!fixture.path.exists());

    let blocked = fixture.path.parent().expect("dir").join("file");
    fs::write(&blocked, b"x").expect("file");
    assert!(NamesFileWriter::create(&blocked.join(NAMES_FILE)).is_err());
}

#[test]
fn the_legacy_spill_sits_beside_the_names_file_and_removal_tolerates_absence() {
    let fixture = fixture();
    let path = spill_path(&fixture.path);
    assert!(path.to_string_lossy().ends_with(SPILL_SUFFIX));
    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some("file-names.bin.building")
    );
    fs::create_dir_all(path.parent().expect("dir")).expect("dir");
    fs::write(&path, b"stale content").expect("stale spill");
    remove_if_present(&path).expect("removed");
    assert!(!path.exists());
    remove_if_present(&path).expect("already gone is fine");
    assert!(remove_if_present(fixture.path.parent().expect("dir")).is_err());
}
