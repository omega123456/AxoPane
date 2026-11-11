use file_explorer_lib::size::index::names::block::{checksum, BlockBuilder, BlockView};
use file_explorer_lib::size::index::names::signature::{
    byte_bit, is_non_ascii, signature_of, DASH_BIT, DIGIT_BIT, DOT_BIT, NON_ASCII_BIT,
    OTHER_ASCII_BIT, UNDERSCORE_BIT,
};
use file_explorer_lib::size::index::names::PackedNames;

const LETTER_COUNT: u32 = 26;
const FOLDER_COUNT_BYTES: usize = 4;
const FOLDER_ENTRY_BYTES: usize = 8;
const ALL_BITS_FLIPPED: u8 = 0xFF;
const CONTROL_BYTE: u8 = 0x01;
const DELETE_BYTE: u8 = 0x7F;
const FIRST_NON_ASCII_BYTE: u8 = 0x80;

fn letters() -> impl Iterator<Item = (u32, u8)> {
    (0..LETTER_COUNT).zip(b'a'..=b'z')
}

#[test]
fn each_letter_owns_one_low_bit_regardless_of_case() {
    for (position, letter) in letters() {
        let expected = 1 << position;
        assert_eq!(byte_bit(letter), expected, "{}", letter as char);
        assert_eq!(byte_bit(letter.to_ascii_uppercase()), expected);
    }
}

#[test]
fn digits_punctuation_and_other_bytes_share_their_class_bits() {
    for digit in b'0'..=b'9' {
        assert_eq!(byte_bit(digit), DIGIT_BIT);
    }
    assert_eq!(byte_bit(b'.'), DOT_BIT);
    assert_eq!(byte_bit(b'-'), DASH_BIT);
    assert_eq!(byte_bit(b'_'), UNDERSCORE_BIT);
    for other in [b' ', b'!', b'~', b'(', b'/', CONTROL_BYTE, DELETE_BYTE] {
        assert_eq!(byte_bit(other), OTHER_ASCII_BIT, "{other:#x}");
    }
    for high in FIRST_NON_ASCII_BYTE..=u8::MAX {
        assert_eq!(byte_bit(high), NON_ASCII_BIT, "{high:#x}");
    }
}

#[test]
fn the_classes_fill_all_thirty_two_bits_without_overlap() {
    let bits = [
        DIGIT_BIT,
        DOT_BIT,
        DASH_BIT,
        UNDERSCORE_BIT,
        OTHER_ASCII_BIT,
        NON_ASCII_BIT,
    ];
    let letter_bits = letters().fold(0, |all, (_, letter)| all | byte_bit(letter));
    let class_bits = bits.iter().fold(0, |all, bit| all | bit);
    assert_eq!(letter_bits & class_bits, 0);
    assert_eq!(letter_bits | class_bits, u32::MAX);
    let one_bit_per_class = u32::try_from(bits.len()).expect("class count fits in u32");
    let total_class_bits: u32 = bits.iter().map(|bit| bit.count_ones()).sum();
    assert_eq!(total_class_bits, one_bit_per_class);
}

#[test]
fn a_signature_is_the_union_of_its_bytes() {
    assert_eq!(signature_of(b""), 0);
    assert_eq!(signature_of(b"aA"), byte_bit(b'a'));
    let name = b"Report-2_v.PDF";
    let expected = name.iter().fold(0, |all, &byte| all | byte_bit(byte));
    assert_eq!(signature_of(name), expected);
    assert_eq!(
        signature_of(b".-_9 "),
        DOT_BIT | DASH_BIT | UNDERSCORE_BIT | DIGIT_BIT | OTHER_ASCII_BIT
    );
    assert!(!is_non_ascii(signature_of(name)));
    assert!(is_non_ascii(signature_of("Résumé".as_bytes())));
    assert!(
        is_non_ascii(signature_of("\u{212A}".as_bytes())),
        "Kelvin sign"
    );
}

#[test]
fn the_block_builder_stores_the_shared_signature_of_every_name_in_order() {
    let folders = [
        PackedNames::from_names(["Zeta.TXT", "a-b_c.rs", "résumé.pdf"]),
        PackedNames::from_names(["\u{212A}elvin", "notes 1.md"]),
    ];
    let mut builder = BlockBuilder::new(0);
    builder.push(0, folders[0].as_bytes());
    builder.push(1, folders[1].as_bytes());
    let block = builder.finish();
    let view = BlockView::verified(block.stored()).expect("verifies");
    for (folder, names) in view.folders().zip(&folders) {
        let expected: Vec<u32> = names.names().map(signature_of).collect();
        assert_eq!(folder.signatures.iter().collect::<Vec<u32>>(), expected);
    }
}

#[test]
fn verification_trusts_stored_signatures_without_recomputing_them() {
    let mut builder = BlockBuilder::new(0);
    builder.push(0, PackedNames::from_names(["abc"]).as_bytes());
    let mut block = builder.finish();
    let first_signature_byte = FOLDER_COUNT_BYTES + FOLDER_ENTRY_BYTES;
    block.bytes[first_signature_byte] ^= ALL_BITS_FLIPPED;
    block.checksum = checksum(&block.bytes);
    let view = BlockView::verified(block.stored()).expect("structure still holds");
    let folder = view.folders().next().expect("folder");
    assert_ne!(
        folder.signatures.iter().next(),
        Some(signature_of(b"abc")),
        "signature correctness is a builder invariant"
    );
}
