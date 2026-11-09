use std::mem::size_of;

use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

use crate::pixels::{
    encode_rgba_png, png_data_url, premultiplied_bgra_to_rgba, RGBA_BYTES_PER_PIXEL,
};

const BGRA_BITS_PER_PIXEL: u16 = 32;

pub struct IconGuard(pub HICON);

impl Drop for IconGuard {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { DestroyIcon(self.0) };
        }
    }
}

pub struct BitmapGuard(pub HBITMAP);

impl Drop for BitmapGuard {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { DeleteObject(self.0.into()) };
        }
    }
}

pub struct DeviceContextGuard(pub HDC);

impl Drop for DeviceContextGuard {
    fn drop(&mut self) {
        let _ = unsafe { DeleteDC(self.0) };
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IconBitmapFallback {
    Mask,
    None,
}

#[derive(Clone, Copy)]
struct BitmapSize {
    width: u32,
    height: u32,
}

struct BgraBitmap {
    size: BitmapSize,
    pixels: Vec<u8>,
}

fn bitmap_dimensions(bitmap: HBITMAP) -> Option<BitmapSize> {
    let mut object = BITMAP::default();
    let copied = unsafe {
        GetObjectW(
            bitmap.into(),
            i32::try_from(size_of::<BITMAP>()).ok()?,
            Some((&mut object as *mut BITMAP).cast()),
        )
    };
    if copied == 0 {
        return None;
    }
    let width = u32::try_from(object.bmWidth).ok()?;
    let height = u32::try_from(object.bmHeight.abs()).ok()?;
    (width != 0 && height != 0).then_some(BitmapSize { width, height })
}

fn read_bgra(bitmap: HBITMAP, accept: impl Fn(u32, u32) -> bool) -> Option<BgraBitmap> {
    let size = bitmap_dimensions(bitmap)?;
    let BitmapSize { width, height } = size;
    if !accept(width, height) {
        return None;
    }
    let stride = width.checked_mul(RGBA_BYTES_PER_PIXEL as u32)?;
    let pixel_bytes_len = usize::try_from(stride.checked_mul(height)?).ok()?;
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: i32::try_from(width).ok()?,
            biHeight: -i32::try_from(height).ok()?,
            biPlanes: 1,
            biBitCount: BGRA_BITS_PER_PIXEL,
            biCompression: BI_RGB.0,
            biSizeImage: u32::try_from(pixel_bytes_len).ok()?,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; pixel_bytes_len];
    let device_context = unsafe { CreateCompatibleDC(None) };
    if device_context.0.is_null() {
        return None;
    }
    let device_context = DeviceContextGuard(device_context);
    let scan_lines = unsafe {
        GetDIBits(
            device_context.0,
            bitmap,
            0,
            height,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    (scan_lines != 0).then_some(BgraBitmap { size, pixels })
}

pub fn bitmap_to_png_bytes(bitmap: HBITMAP, accept: impl Fn(u32, u32) -> bool) -> Option<Vec<u8>> {
    let bgra = read_bgra(bitmap, accept)?;
    let rgba = premultiplied_bgra_to_rgba(&bgra.pixels)?;
    encode_rgba_png(bgra.size.width, bgra.size.height, &rgba)
}

pub fn accept_any_size(_width: u32, _height: u32) -> bool {
    true
}

pub fn bitmap_to_data_url(bitmap: HBITMAP) -> Option<String> {
    bitmap_to_png_bytes(bitmap, accept_any_size).map(|bytes| png_data_url(&bytes))
}

pub fn hicon_to_data_url(icon: HICON, fallback: IconBitmapFallback) -> Option<String> {
    let mut info = ICONINFO::default();
    if unsafe { GetIconInfo(icon, &mut info) }.is_err() {
        return None;
    }
    let color = BitmapGuard(info.hbmColor);
    let mask = BitmapGuard(info.hbmMask);
    let bitmap = match (color.0.is_invalid(), fallback) {
        (false, _) => color.0,
        (true, IconBitmapFallback::Mask) => mask.0,
        (true, IconBitmapFallback::None) => return None,
    };
    if bitmap.is_invalid() {
        return None;
    }
    bitmap_to_data_url(bitmap)
}
