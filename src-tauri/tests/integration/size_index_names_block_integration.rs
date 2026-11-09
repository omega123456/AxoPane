use file_explorer_lib::size::index::names::block::{
    checksum, decode, BlockBuilder, BlockEntry, BlockSpan, EncodedBlock, NamesCorrupt,
    BLOCK_TARGET_BYTES,
};
use file_explorer_lib::size::index::names::PackedNames;

fn packed(names: &[&str]) -> PackedNames {
    PackedNames::from_names(names.iter().map(|name| name.as_bytes()))
}

/// Folders 10..16 where 11, 13 and 15 hold no files.
fn sample_block() -> EncodedBlock {
    let mut builder = BlockBuilder::new(10);
    builder.push(10, packed(&["a.txt", ".hidden"]).as_bytes());
    builder.push(11, &[]);
    builder.push(12, packed(&["invoice.pdf"]).as_bytes());
    builder.push(14, packed(&["Ünïcödé.md", "b"]).as_bytes());
    builder.push(15, &[]);
    builder.finish()
}

fn raw_entry(relative_id: u32, names: &[u8]) -> Vec<u8> {
    let mut out = relative_id.to_le_bytes().to_vec();
    out.extend_from_slice(&(names.len() as u32).to_le_bytes());
    out.extend_from_slice(names);
    out
}

/// A block with a valid checksum over arbitrary raw content.
fn crafted(raw: &[u8], folder_count: u32) -> EncodedBlock {
    let compressed = lz4_flex::block::compress(raw);
    EncodedBlock {
        span: BlockSpan {
            first_ordinal: 0,
            folder_count,
        },
        raw_len: raw.len() as u32,
        checksum: checksum(&compressed),
        compressed,
    }
}

fn decodes(block: &EncodedBlock) -> bool {
    let mut scratch = Vec::new();
    decode(block.stored(), &mut scratch).is_ok()
}

#[test]
fn a_block_round_trips_with_ids_relative_to_its_first_folder() {
    let block = sample_block();
    assert_eq!(
        block.span,
        BlockSpan {
            first_ordinal: 10,
            folder_count: 6
        }
    );
    assert_eq!(block.span.end(), 16);
    assert_eq!(block.span.ordinals(), 10..16);

    let mut scratch = Vec::new();
    let decoded = decode(block.stored(), &mut scratch).expect("decodes");
    let entries: Vec<BlockEntry<'_>> = decoded.entries().collect();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.relative_id)
            .collect::<Vec<_>>(),
        [0, 2, 4],
        "empty folders take no entry"
    );
    assert_eq!(
        decoded.names_of(12),
        Some(packed(&["invoice.pdf"]).as_bytes())
    );
    assert_eq!(decoded.names_of(11), None);
    assert_eq!(decoded.names_of(9), None, "before the block");
    let ranges = decoded.entry_ranges();
    assert_eq!(ranges.len(), 3);
    for (range, entry) in ranges.iter().zip(&entries) {
        assert_eq!(range.relative_id, entry.relative_id);
        assert_eq!(&decoded.raw()[range.names.clone()], entry.names);
    }
}

#[test]
fn a_block_copied_to_another_position_keeps_its_bytes() {
    let block = sample_block();
    let moved = EncodedBlock {
        span: BlockSpan {
            first_ordinal: 500,
            ..block.span
        },
        ..block.clone()
    };
    let mut scratch = Vec::new();
    let decoded = decode(moved.stored(), &mut scratch).expect("decodes");
    assert_eq!(
        decoded.names_of(502),
        Some(packed(&["invoice.pdf"]).as_bytes())
    );
}

#[test]
fn checksum_and_decode_failures_are_rejected() {
    let block = sample_block();
    assert!(decodes(&block));
    let mut flipped = block.clone();
    flipped.compressed[0] ^= 0x41;
    let mut scratch = Vec::new();
    assert_eq!(
        decode(flipped.stored(), &mut scratch).map(|_| ()),
        Err(NamesCorrupt)
    );

    let mut garbage = block.clone();
    garbage.compressed = vec![0xF0; 4];
    garbage.checksum = checksum(&garbage.compressed);
    assert!(!decodes(&garbage), "not an LZ4 block");

    let mut short = block.clone();
    short.raw_len += 7;
    assert!(!decodes(&short), "fewer bytes than the directory promised");
}

#[test]
fn malformed_entries_are_rejected_even_with_a_valid_checksum() {
    let good = raw_entry(0, b"a\0b\0");
    assert!(decodes(&crafted(&good, 1)), "control");
    assert!(decodes(&crafted(&[], 3)), "a block of empty folders");

    let out_of_order = [raw_entry(1, b"a\0"), raw_entry(0, b"b\0")].concat();
    let repeated = [raw_entry(0, b"a\0"), raw_entry(0, b"b\0")].concat();
    for (raw, folder_count, what) in [
        (out_of_order, 2, "out of order"),
        (repeated, 2, "repeated folder"),
        (raw_entry(3, b"a\0"), 2, "outside the span"),
        (raw_entry(0, b""), 1, "an empty list is never stored"),
        (raw_entry(0, b"a"), 1, "unterminated"),
        (raw_entry(0, b"\0a\0"), 1, "empty first name"),
        (raw_entry(0, b"a\0\0b\0"), 1, "empty middle name"),
        (good[..good.len() - 1].to_vec(), 1, "truncated names"),
        (good[..5].to_vec(), 1, "truncated header"),
    ] {
        assert!(!decodes(&crafted(&raw, folder_count)), "{what}");
    }
}

#[test]
fn checksums_cover_every_byte_and_the_length() {
    let bytes: Vec<u8> = (0..=40).collect();
    assert_eq!(checksum(&bytes), checksum(&bytes));
    for at in 0..bytes.len() {
        let mut changed = bytes.clone();
        changed[at] ^= 1;
        assert_ne!(checksum(&changed), checksum(&bytes), "byte {at}");
    }
    assert_ne!(checksum(&[]), checksum(&[0]));
    assert_ne!(checksum(&[0; 8]), checksum(&[0; 9]));
}

#[test]
fn the_block_target_is_a_quarter_mebibyte_of_names() {
    assert_eq!(BLOCK_TARGET_BYTES, 256 * 1024);
    let mut builder = BlockBuilder::new(0);
    assert_eq!(builder.raw_len(), 0);
    builder.push(0, packed(&["x"]).as_bytes());
    assert_eq!(builder.raw_len(), 8 + 2);
    let span = BlockSpan {
        first_ordinal: u32::MAX,
        folder_count: 5,
    };
    assert_eq!(span.end(), u32::MAX, "a hostile span never overflows");
}
