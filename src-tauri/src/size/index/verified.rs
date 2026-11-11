use std::sync::atomic::{AtomicU64, Ordering};

const BITS_PER_WORD: usize = u64::BITS as usize;

#[derive(Debug)]
pub struct VerifiedSet {
    words: Vec<AtomicU64>,
}

impl VerifiedSet {
    pub fn none_of(count: usize) -> Self {
        let word_count = count.div_ceil(BITS_PER_WORD);
        Self {
            words: (0..word_count).map(|_| AtomicU64::new(0)).collect(),
        }
    }

    pub fn contains(&self, index: usize) -> bool {
        let word = self.words[index / BITS_PER_WORD].load(Ordering::Relaxed);
        word & bit_of(index) != 0
    }

    pub fn insert(&self, index: usize) -> bool {
        let bit = bit_of(index);
        let before = self.words[index / BITS_PER_WORD].fetch_or(bit, Ordering::Relaxed);
        before & bit == 0
    }
}

fn bit_of(index: usize) -> u64 {
    1 << (index % BITS_PER_WORD)
}
