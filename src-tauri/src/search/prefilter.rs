use super::matcher::Token;
use crate::size::index::names::signature::{byte_bit, is_non_ascii, SignatureFilter, SignedName};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefilter {
    required: u32,
    token_bytes: Vec<Vec<u8>>,
    wants_non_ascii: bool,
}

impl Prefilter {
    pub fn of(tokens: &[Token]) -> Self {
        let literals: Vec<Vec<char>> = tokens
            .iter()
            .map(|token| token.literal_chars().collect())
            .collect();
        let wants_non_ascii = literals
            .iter()
            .flatten()
            .any(|character| !character.is_ascii());
        Self {
            required: required_mask(&literals),
            token_bytes: literals.iter().map(|chars| ascii_bytes(chars)).collect(),
            wants_non_ascii,
        }
    }

    pub fn admits_signature(&self, signature: u32) -> bool {
        if is_non_ascii(signature) {
            return true;
        }
        if self.wants_non_ascii {
            return false;
        }
        holds_every_class(signature, self.required)
    }

    pub fn admits_bytes(&self, name: SignedName<'_>) -> bool {
        if is_non_ascii(name.signature) {
            return true;
        }
        self.token_bytes
            .iter()
            .all(|wanted| holds_in_order(name.bytes, wanted))
    }

    pub fn admits_name(&self, name: SignedName<'_>) -> bool {
        if !self.admits_signature(name.signature) {
            return false;
        }
        self.admits_bytes(name)
    }
}

impl SignatureFilter for Prefilter {
    fn admits(&self, signature: u32) -> bool {
        self.admits_signature(signature)
    }
}

fn holds_every_class(signature: u32, required: u32) -> bool {
    signature & required == required
}

fn ascii_bytes(chars: &[char]) -> Vec<u8> {
    let ascii = chars
        .iter()
        .filter_map(|&character| u8::try_from(character).ok());
    ascii.filter(u8::is_ascii).collect()
}

fn required_mask(literals: &[Vec<char>]) -> u32 {
    let bytes = literals.iter().flat_map(|chars| ascii_bytes(chars));
    bytes.fold(0, |mask, byte| mask | byte_bit(byte))
}

fn holds_in_order(name: &[u8], wanted: &[u8]) -> bool {
    let mut lowered = name.iter().map(u8::to_ascii_lowercase);
    wanted
        .iter()
        .all(|&byte| lowered.any(|candidate| candidate == byte))
}
