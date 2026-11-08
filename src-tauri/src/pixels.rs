//! Pure pixel-format conversion and PNG encoding shared by the native icon and
//! thumbnail providers.

use base64::Engine;
use png::{BitDepth, ColorType, Encoder};

pub const RGBA_BYTES_PER_PIXEL: usize = 4;
const CHANNEL_MAX: u32 = 255;

/// Straight-alpha value of one premultiplied channel, rounded to nearest.
pub fn unpremultiply(channel: u8, alpha: u8) -> u8 {
    let alpha = u32::from(alpha);
    (u32::from(channel) * CHANNEL_MAX + alpha / 2)
        .checked_div(alpha)
        .map_or(0, |value| value.min(CHANNEL_MAX) as u8)
}

/// Converts premultiplied BGRA (as `GetDIBits` returns for 32bpp) to
/// straight-alpha RGBA, or `None` when `pixels` is not whole pixels.
pub fn premultiplied_bgra_to_rgba(pixels: &[u8]) -> Option<Vec<u8>> {
    let whole_pixels = pixels.len().is_multiple_of(RGBA_BYTES_PER_PIXEL);
    if !whole_pixels {
        return None;
    }
    let mut rgba = Vec::with_capacity(pixels.len());
    for pixel in pixels.chunks_exact(RGBA_BYTES_PER_PIXEL) {
        let [blue, green, red, alpha] = [pixel[0], pixel[1], pixel[2], pixel[3]];
        rgba.extend_from_slice(&[
            unpremultiply(red, alpha),
            unpremultiply(green, alpha),
            unpremultiply(blue, alpha),
            alpha,
        ]);
    }
    Some(rgba)
}

pub fn encode_rgba_png(width: u32, height: u32, rgba_pixels: &[u8]) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(&mut bytes, width, height);
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    encoder
        .write_header()
        .ok()?
        .write_image_data(rgba_pixels)
        .ok()?;
    Some(bytes)
}

pub fn png_data_url(png_bytes: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png_bytes)
    )
}
