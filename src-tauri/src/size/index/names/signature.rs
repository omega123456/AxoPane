use std::sync::LazyLock;

pub const DIGIT_BIT: u32 = 1 << 26;
pub const DOT_BIT: u32 = 1 << 27;
pub const DASH_BIT: u32 = 1 << 28;
pub const UNDERSCORE_BIT: u32 = 1 << 29;
pub const OTHER_ASCII_BIT: u32 = 1 << 30;
pub const NON_ASCII_BIT: u32 = 1 << 31;

const FIRST_LETTER: u8 = b'a';
const BYTE_VALUES: usize = 256;

static BYTE_BITS: LazyLock<[u32; BYTE_VALUES]> = LazyLock::new(byte_bit_table);

pub fn byte_bit(byte: u8) -> u32 {
    BYTE_BITS[usize::from(byte)]
}

pub fn signature_of(name: &[u8]) -> u32 {
    let byte_bits = &*BYTE_BITS;
    name.iter().fold(0, |signature, &byte| {
        signature | byte_bits[usize::from(byte)]
    })
}

pub fn is_non_ascii(signature: u32) -> bool {
    signature & NON_ASCII_BIT != 0
}

pub trait SignatureFilter {
    fn admits(&self, signature: u32) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignedName<'name> {
    pub bytes: &'name [u8],
    pub signature: u32,
}

impl<'name> SignedName<'name> {
    pub fn of(bytes: &'name [u8]) -> Self {
        Self {
            bytes,
            signature: signature_of(bytes),
        }
    }
}

fn byte_bit_table() -> [u32; BYTE_VALUES] {
    let mut table = [0; BYTE_VALUES];
    for byte in 0..=u8::MAX {
        table[usize::from(byte)] = classify(byte);
    }
    table
}

fn classify(byte: u8) -> u32 {
    if !byte.is_ascii() {
        return NON_ASCII_BIT;
    }
    if byte.is_ascii_alphabetic() {
        return letter_bit(byte);
    }
    if byte.is_ascii_digit() {
        return DIGIT_BIT;
    }
    punctuation_bit(byte)
}

fn letter_bit(letter: u8) -> u32 {
    1 << (letter.to_ascii_lowercase() - FIRST_LETTER)
}

fn punctuation_bit(byte: u8) -> u32 {
    match byte {
        b'.' => DOT_BIT,
        b'-' => DASH_BIT,
        b'_' => UNDERSCORE_BIT,
        _ => OTHER_ASCII_BIT,
    }
}
