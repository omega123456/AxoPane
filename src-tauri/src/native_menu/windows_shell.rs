//! Shared low-level Windows shell helpers used by both the classic
//! `IContextMenu` path (`windows.rs`) and the Windows 11 modern
//! `IExplorerCommand` path (`windows_modern.rs`).
//!
//! Everything here is real shell/COM/GDI code, so the module only compiles for
//! the production Windows build. Under `feature = "test-utils"` the providers
//! short-circuit before any of this is reached (see `windows.rs` /
//! `windows_modern.rs`), matching the machine-global-API rule in CLAUDE.md.
#![cfg(all(not(feature = "test-utils"), target_os = "windows"))]

use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use base64::Engine;
use png::{BitDepth, ColorType, Encoder};

use crate::native_menu::types::{LoadNativeMenuRequest, NativeMenuCanonicalActionKind};

use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{ILFree, SHDefExtractIconW, SHParseDisplayName};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

/// Encodes a wide, NUL-terminated UTF-16 buffer suitable for `PCWSTR` use.
pub(crate) fn path_to_wide(path: &str) -> Vec<u16> {
    path.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Parent directory of a path, used to bind to the containing shell folder.
pub(crate) fn parent_directory(path: &str) -> Option<PathBuf> {
    Path::new(path).parent().map(Path::to_path_buf)
}

/// True when the request targets empty folder background rather than items.
pub(crate) fn is_background_target(request: &LoadNativeMenuRequest) -> bool {
    matches!(
        request.target_kind,
        crate::native_menu::types::NativeMenuTargetKind::Background
    )
}

/// The selection the menu should act on: explicit `selected_paths` when present,
/// otherwise the single `target_path`.
pub(crate) fn selected_target_paths(request: &LoadNativeMenuRequest) -> Vec<String> {
    if !request.selected_paths.is_empty() {
        return request.selected_paths.clone();
    }

    request.target_path.iter().cloned().collect()
}

/// Removes the accelerator (`&`) markers and trailing shortcut text Windows
/// stores in menu strings, leaving a clean display label.
pub(crate) fn sanitize_menu_label(raw: String) -> String {
    let without_shortcut = raw.split('\t').next().unwrap_or_default();
    let mut label = String::with_capacity(without_shortcut.len());
    let mut chars = without_shortcut.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '&' {
            if matches!(chars.peek(), Some('&')) {
                label.push('&');
                chars.next();
            }
            continue;
        }
        label.push(character);
    }
    label.trim().to_string()
}

/// Lower-cased, alphanumeric-only form of a verb/label used for canonical
/// classification and de-duplication.
pub(crate) fn normalize_verb(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Maps a shell verb (and, as a fallback, the label) to an app-owned canonical
/// action so the existing dedupe can drop duplicates of our own rows.
pub(crate) fn classify_canonical_action(
    normalized_verb: Option<&str>,
    label: &str,
) -> Option<NativeMenuCanonicalActionKind> {
    let verb = normalized_verb.unwrap_or_default();
    match verb {
        "open" => Some(NativeMenuCanonicalActionKind::Open),
        "openwith" => Some(NativeMenuCanonicalActionKind::OpenWith),
        "copy" => Some(NativeMenuCanonicalActionKind::Copy),
        "copyaspath" => Some(NativeMenuCanonicalActionKind::CopyAsPath),
        "cut" => Some(NativeMenuCanonicalActionKind::Cut),
        "paste" => Some(NativeMenuCanonicalActionKind::Paste),
        "rename" => Some(NativeMenuCanonicalActionKind::Rename),
        "delete" => Some(NativeMenuCanonicalActionKind::Delete),
        "properties" => Some(NativeMenuCanonicalActionKind::Properties),
        "share" => Some(NativeMenuCanonicalActionKind::Share),
        "compress" | "windowscompresszipfile" => Some(NativeMenuCanonicalActionKind::Compress),
        "extract" => Some(NativeMenuCanonicalActionKind::Extract),
        "refresh" => Some(NativeMenuCanonicalActionKind::Refresh),
        "new" | "newfolder" => Some(NativeMenuCanonicalActionKind::NewFolder),
        "selectall" => Some(NativeMenuCanonicalActionKind::SelectAll),
        _ => {
            let normalized_label = normalize_verb(label);
            match normalized_label.as_str() {
                value if value.starts_with("openwith") => {
                    Some(NativeMenuCanonicalActionKind::OpenWith)
                }
                "delete" => Some(NativeMenuCanonicalActionKind::Delete),
                "properties" => Some(NativeMenuCanonicalActionKind::Properties),
                "copy" => Some(NativeMenuCanonicalActionKind::Copy),
                "copyaspath" => Some(NativeMenuCanonicalActionKind::CopyAsPath),
                "cut" => Some(NativeMenuCanonicalActionKind::Cut),
                "paste" => Some(NativeMenuCanonicalActionKind::Paste),
                "rename" => Some(NativeMenuCanonicalActionKind::Rename),
                "refresh" => Some(NativeMenuCanonicalActionKind::Refresh),
                "selectall" => Some(NativeMenuCanonicalActionKind::SelectAll),
                _ => None,
            }
        }
    }
}

/// An absolute PIDL parsed from a filesystem path, freed on drop.
pub(crate) struct OwnedPidl(*mut ITEMIDLIST);

impl OwnedPidl {
    pub(crate) fn from_path(path: &str) -> windows::core::Result<Self> {
        let wide = path_to_wide(path);
        let mut pidl = null_mut();
        unsafe {
            SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None)?;
        }
        Ok(Self(pidl))
    }

    pub(crate) fn as_ptr(&self) -> *const ITEMIDLIST {
        self.0
    }
}

impl Drop for OwnedPidl {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                ILFree(Some(self.0.cast_const()));
            }
        }
    }
}

struct DeviceContextGuard(HDC);

impl Drop for DeviceContextGuard {
    fn drop(&mut self) {
        let _ = unsafe { DeleteDC(self.0) };
    }
}

/// A device-independent color HBITMAP that is destroyed on drop. Used to own the
/// color bitmap extracted from an HICON.
struct OwnedBitmap(HBITMAP);

impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        if !self.0 .0.is_null() {
            let _ = unsafe { DeleteObject(self.0.into()) };
        }
    }
}

/// Wraps PNG bytes as a `data:` URL string.
pub(crate) fn bitmap_to_data_url(bitmap: HBITMAP) -> Option<String> {
    let bitmap_bytes = bitmap_to_png_bytes(bitmap)?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bitmap_bytes)
    ))
}

/// Renders an `HBITMAP` (assumed 32bpp BGRA with premultiplied alpha, as the
/// shell provides for menu item bitmaps) into PNG bytes.
pub(crate) fn bitmap_to_png_bytes(bitmap: HBITMAP) -> Option<Vec<u8>> {
    let mut object = BITMAP::default();
    let object_size = i32::try_from(std::mem::size_of::<BITMAP>()).ok()?;
    let copied = unsafe {
        GetObjectW(
            bitmap.into(),
            object_size,
            Some((&mut object as *mut BITMAP).cast()),
        )
    };
    if copied == 0 {
        return None;
    }

    let width = u32::try_from(object.bmWidth).ok()?;
    let height = u32::try_from(object.bmHeight.abs()).ok()?;
    if width == 0 || height == 0 {
        return None;
    }

    let stride = width.checked_mul(4)?;
    let pixel_bytes_len = stride.checked_mul(height)?;
    let pixel_bytes_len = usize::try_from(pixel_bytes_len).ok()?;

    let header = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: i32::try_from(width).ok()?,
        biHeight: -i32::try_from(height).ok()?,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        biSizeImage: u32::try_from(pixel_bytes_len).ok()?,
        ..Default::default()
    };
    let mut info = BITMAPINFO {
        bmiHeader: header,
        ..Default::default()
    };
    let mut pixels = vec![0u8; pixel_bytes_len];

    let device_context = unsafe { CreateCompatibleDC(None) };
    if device_context.0.is_null() {
        return None;
    }
    let dc_guard = DeviceContextGuard(device_context);

    let scan_lines = unsafe {
        GetDIBits(
            dc_guard.0,
            bitmap,
            0,
            height,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    if scan_lines == 0 {
        return None;
    }

    let rgba_pixels = bgra_to_rgba(&pixels)?;
    encode_rgba_png(width, height, &rgba_pixels)
}

/// Extracts an icon from an `IExplorerCommand::GetIcon` resource reference
/// (e.g. `"C:\\path\\app.dll,-123"` or a bare image path) and renders it to a
/// PNG `data:` URL. Returns `None` on any failure so callers degrade to a label
/// without an icon.
pub(crate) fn icon_resource_to_data_url(resource: &str) -> Option<String> {
    let trimmed = resource.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (module, index) = split_icon_resource(trimmed);
    let module_wide = path_to_wide(module);
    let mut large: HICON = HICON(null_mut());
    // LOWORD = large icon size; request 32px for a crisp menu glyph.
    let icon_size: u32 = 32;
    let extracted = unsafe {
        SHDefExtractIconW(
            PCWSTR(module_wide.as_ptr()),
            index,
            0,
            Some(&mut large),
            None,
            icon_size,
        )
    };
    if extracted.is_err() || large.0.is_null() {
        return None;
    }
    let _icon_guard = IconGuard(large);
    hicon_to_data_url(large)
}

struct IconGuard(HICON);

impl Drop for IconGuard {
    fn drop(&mut self) {
        if !self.0 .0.is_null() {
            let _ = unsafe { DestroyIcon(self.0) };
        }
    }
}

fn split_icon_resource(resource: &str) -> (&str, i32) {
    match resource.rsplit_once(',') {
        Some((module, index)) => {
            let parsed = index.trim().parse::<i32>().unwrap_or(0);
            (module.trim(), parsed)
        }
        None => (resource, 0),
    }
}

/// Converts an `HICON` to a PNG `data:` URL via its color bitmap.
pub(crate) fn hicon_to_data_url(icon: HICON) -> Option<String> {
    let mut info = ICONINFO::default();
    if unsafe { GetIconInfo(icon, &mut info) }.is_err() {
        return None;
    }
    // GetIconInfo creates two bitmaps we now own and must free.
    let _color_guard = OwnedBitmap(info.hbmColor);
    let _mask_guard = OwnedBitmap(info.hbmMask);
    if info.hbmColor.0.is_null() {
        return None;
    }
    bitmap_to_data_url(info.hbmColor)
}

fn encode_rgba_png(width: u32, height: u32, rgba_pixels: &[u8]) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(&mut bytes, width, height);
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(rgba_pixels).ok()?;
    drop(writer);
    Some(bytes)
}

/// Converts premultiplied BGRA bytes (as `GetDIBits` returns for 32bpp) to
/// straight-alpha RGBA for PNG encoding.
pub(crate) fn bgra_to_rgba(pixels: &[u8]) -> Option<Vec<u8>> {
    if pixels.len() % 4 != 0 {
        return None;
    }

    let mut rgba = Vec::with_capacity(pixels.len());
    for chunk in pixels.chunks_exact(4) {
        let blue = u32::from(chunk[0]);
        let green = u32::from(chunk[1]);
        let red = u32::from(chunk[2]);
        let alpha = u32::from(chunk[3]);

        if alpha == 0 {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }

        let unpremultiply =
            |channel: u32| -> u8 { ((channel * 255 + (alpha / 2)) / alpha).min(255) as u8 };

        rgba.push(unpremultiply(red));
        rgba.push(unpremultiply(green));
        rgba.push(unpremultiply(blue));
        rgba.push(alpha as u8);
    }
    Some(rgba)
}
