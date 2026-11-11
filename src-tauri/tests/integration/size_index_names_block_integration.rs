use file_explorer_lib::size::index::names::block::{
    checksum, BlockBuilder, BlockFolder, BlockSpan, BlockView, BuiltBlock, NamesCorrupt,
    StoredBlock, BLOCK_TARGET_BYTES,
};
use file_explorer_lib::size::index::names::signature::signature_of;
use file_explorer_lib::size::index::names::{names_in, PackedNames};

fn packed(names: &[&str]) -> PackedNames {
    PackedNames::from_names(names.iter().map(|name| name.as_bytes()))
}

/// Folders 10..16 where 11, 13 and 15 hold no files.
fn sample_block() -> BuiltBlock {
    let mut builder = BlockBuilder::new(10);
    builder.push(10, packed(&[".hidden", "a.txt"]).as_bytes());
    builder.push(11, &[]);
    builder.push(12, packed(&["invoice.pdf"]).as_bytes());
    builder.push(14, packed(&["b", "Ünïcödé.md"]).as_bytes());
    builder.push(15, &[]);
    builder.finish()
}

fn verified(block: &BuiltBlock) -> BlockView<'_> {
    BlockView::verified(block.stored()).expect("verifies")
}

fn signatures_of(packed: &[u8]) -> Vec<u32> {
    names_in(packed).map(signature_of).collect()
}

struct RawBlock {
    folders: Vec<(u32, u32)>,
    offsets: Vec<u32>,
    names: Vec<u8>,
    name_count: u32,
    folder_count: u32,
}

impl RawBlock {
    fn valid() -> Self {
        Self {
            folders: vec![(0, 0), (2, 2)],
            offsets: vec![0, 2, 4],
            names: b"a\0b\0c\0".to_vec(),
            name_count: 3,
            folder_count: 3,
        }
    }

    fn built(&self) -> BuiltBlock {
        let mut bytes = (self.folders.len() as u32).to_le_bytes().to_vec();
        for (relative_id, first_name) in &self.folders {
            bytes.extend_from_slice(&relative_id.to_le_bytes());
            bytes.extend_from_slice(&first_name.to_le_bytes());
        }
        for _ in 0..self.offsets.len() {
            bytes.extend_from_slice(&0_u32.to_le_bytes());
        }
        for offset in &self.offsets {
            bytes.extend_from_slice(&offset.to_le_bytes());
        }
        bytes.extend_from_slice(&self.names);
        BuiltBlock {
            span: BlockSpan {
                first_ordinal: 0,
                folder_count: self.folder_count,
            },
            name_count: self.name_count,
            checksum: checksum(&bytes),
            bytes,
        }
    }

    fn verifies(&self) -> bool {
        BlockView::verified(self.built().stored()).is_ok()
    }
}

#[test]
fn a_block_round_trips_folders_names_and_signatures_relative_to_its_first_folder() {
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
    assert_eq!(block.name_count, 5);

    let view = verified(&block);
    let folders: Vec<BlockFolder<'_>> = view.folders().collect();
    assert_eq!(
        folders
            .iter()
            .map(|folder| folder.relative_id)
            .collect::<Vec<_>>(),
        [0, 2, 4],
        "empty folders take no entry"
    );
    let expected = [
        packed(&[".hidden", "a.txt"]),
        packed(&["invoice.pdf"]),
        packed(&["b", "Ünïcödé.md"]),
    ];
    for (folder, names) in folders.iter().zip(&expected) {
        assert_eq!(folder.packed, names.as_bytes());
        assert_eq!(folder.signatures.len(), names.names().count());
        assert!(!folder.signatures.is_empty());
        assert_eq!(
            folder.signatures.iter().collect::<Vec<_>>(),
            signatures_of(names.as_bytes())
        );
    }
    assert_eq!(view.names_of(12), Some(expected[1].as_bytes()));
    assert_eq!(view.names_of(14), Some(expected[2].as_bytes()));
    assert_eq!(view.names_of(10), Some(expected[0].as_bytes()));
    assert_eq!(view.names_of(11), None);
    assert_eq!(view.names_of(15), None);
    assert_eq!(view.names_of(99), None, "past the block");
    assert_eq!(view.names_of(9), None, "before the block");
    assert_eq!(
        view.folder(4).map(|folder| folder.packed),
        Some(expected[2].as_bytes())
    );
}

#[test]
fn a_block_copied_to_another_position_keeps_its_bytes() {
    let block = sample_block();
    let moved = StoredBlock {
        span: BlockSpan {
            first_ordinal: 500,
            ..block.span
        },
        ..block.stored()
    };
    let view = BlockView::verified(moved).expect("verifies");
    assert_eq!(
        view.names_of(502),
        Some(packed(&["invoice.pdf"]).as_bytes())
    );
}

#[test]
fn a_flipped_byte_anywhere_fails_verification_but_not_a_trusting_read() {
    let block = sample_block();
    for at in 0..block.bytes.len() {
        let mut flipped = block.clone();
        flipped.bytes[at] ^= 0x41;
        assert_eq!(
            BlockView::verified(flipped.stored()).map(|_| ()),
            Err(NamesCorrupt),
            "flip at {at}"
        );
    }
    let mut renamed = block.clone();
    let last_name_byte = renamed.bytes.len() - 2;
    renamed.bytes[last_name_byte] = b'X';
    let trusted = BlockView::trusting(renamed.stored()).expect("trusted as is");
    assert_eq!(
        trusted.names_of(14),
        Some("b\0Ünïcödé.mX\0".as_bytes()),
        "a trusting read never hashes"
    );
}

#[test]
fn layouts_too_short_for_their_tables_are_refused() {
    let block = sample_block();
    let mut more_names = block.clone();
    more_names.name_count = 1_000;
    assert!(BlockView::trusting(more_names.stored()).is_err());
    let empty = BuiltBlock {
        bytes: vec![0, 0],
        checksum: checksum(&[0, 0]),
        ..block.clone()
    };
    assert!(BlockView::trusting(empty.stored()).is_err());
    let mut many_folders = block;
    many_folders.bytes[0] = 0xFF;
    assert!(BlockView::trusting(many_folders.stored()).is_err());
}

#[test]
fn an_empty_block_and_a_crafted_block_verify() {
    assert!(RawBlock::valid().verifies(), "control");
    let empty = RawBlock {
        folders: Vec::new(),
        offsets: Vec::new(),
        names: Vec::new(),
        name_count: 0,
        folder_count: 4,
    };
    assert!(empty.verifies(), "a block of empty folders");
    let built = empty.built();
    let view = BlockView::verified(built.stored()).expect("verifies");
    assert_eq!(view.folders().count(), 0);
}

#[test]
fn malformed_folder_directories_are_rejected_even_with_a_valid_checksum() {
    let rejected = [
        (vec![(2, 0), (0, 2)], "ids out of order"),
        (vec![(0, 0), (0, 2)], "repeated folder"),
        (vec![(0, 0), (3, 2)], "outside the span"),
        (vec![(0, 1), (2, 2)], "first folder skips names"),
        (vec![(0, 0), (2, 0)], "a folder without names"),
        (vec![(0, 0), (2, 3)], "first name past the name count"),
    ];
    for (folders, what) in rejected {
        let raw = RawBlock {
            folders,
            ..RawBlock::valid()
        };
        assert!(!raw.verifies(), "{what}");
    }
    let names_without_folders = RawBlock {
        folders: Vec::new(),
        ..RawBlock::valid()
    };
    assert!(!names_without_folders.verifies(), "names without folders");
    let folders_without_names = RawBlock {
        offsets: Vec::new(),
        names: Vec::new(),
        name_count: 0,
        ..RawBlock::valid()
    };
    assert!(!folders_without_names.verifies(), "folders without names");
}

#[test]
fn malformed_name_areas_are_rejected_even_with_a_valid_checksum() {
    let rejected = [
        (
            vec![1, 2, 4],
            b"a\0b\0c\0".to_vec(),
            "first name not at zero",
        ),
        (vec![0, 3, 4], b"a\0b\0c\0".to_vec(), "offset inside a name"),
        (vec![0, 4, 2], b"a\0b\0c\0".to_vec(), "offsets out of order"),
        (
            vec![0, 2, 40],
            b"a\0b\0c\0".to_vec(),
            "offset beyond the area",
        ),
        (vec![0, 2, 4], b"a\0b\0c".to_vec(), "unterminated last name"),
        (vec![0, 1, 3], b"\0b\0c\0".to_vec(), "empty first name"),
        (vec![0, 2, 4], b"a\0b\0c\0d\0".to_vec(), "trailing bytes"),
        (
            vec![0, 2, 6],
            b"a\0b\0x\0c\0".to_vec(),
            "two names behind one offset",
        ),
    ];
    for (offsets, names, what) in rejected {
        let raw = RawBlock {
            offsets,
            names,
            ..RawBlock::valid()
        };
        assert!(!raw.verifies(), "{what}");
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
fn the_payload_counts_directory_tables_and_names_against_the_target() {
    assert_eq!(BLOCK_TARGET_BYTES, 256 * 1024);
    let mut builder = BlockBuilder::new(0);
    assert_eq!(builder.payload_len(), 4, "the folder count");
    builder.push(0, packed(&["x"]).as_bytes());
    assert_eq!(builder.payload_len(), 4 + 8 + 4 + 4 + 2);
    let block = builder.finish();
    assert_eq!(block.bytes.len(), 22);
    let span = BlockSpan {
        first_ordinal: u32::MAX,
        folder_count: 5,
    };
    assert_eq!(span.end(), u32::MAX, "a hostile span never overflows");
}
