use std::fmt;
use std::io;

use zstd::bulk::{Compressor, Decompressor};

use super::block::NamesCorrupt;

pub const COMPRESSION_LEVEL: i32 = 1;

pub struct NamesEncoder {
    context: Compressor<'static>,
}

impl NamesEncoder {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            context: Compressor::new(COMPRESSION_LEVEL)?,
        })
    }

    pub fn compress(&mut self, raw: &[u8]) -> io::Result<Vec<u8>> {
        self.context.compress(raw)
    }
}

impl fmt::Debug for NamesEncoder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NamesEncoder")
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
pub struct DecodeScratch {
    context: Option<Decompressor<'static>>,
    raw: Vec<u8>,
}

impl DecodeScratch {
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    pub fn decompress(&mut self, compressed: &[u8], raw_len: usize) -> Result<&[u8], NamesCorrupt> {
        let Self { context, raw } = self;
        let decompressor = ready_context(context)?;
        raw.clear();
        raw.resize(raw_len, 0);
        let written = decompressor
            .decompress_to_buffer(compressed, raw.as_mut_slice())
            .map_err(|_| NamesCorrupt)?;
        if written != raw_len {
            return Err(NamesCorrupt);
        }
        Ok(raw.as_slice())
    }
}

impl fmt::Debug for DecodeScratch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DecodeScratch")
            .field("raw_len", &self.raw.len())
            .finish_non_exhaustive()
    }
}

fn ready_context<'scratch>(
    context: &'scratch mut Option<Decompressor<'static>>,
) -> Result<&'scratch mut Decompressor<'static>, NamesCorrupt> {
    if context.is_none() {
        let created = Decompressor::new().map_err(|_| NamesCorrupt)?;
        *context = Some(created);
    }
    context.as_mut().ok_or(NamesCorrupt)
}
