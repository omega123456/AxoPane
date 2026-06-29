use std::path::Path;

pub fn icon_data_url_for_path(path: &Path, is_dir: bool) -> Option<String> {
    if is_dir || !should_use_native_icon(path) {
        return None;
    }

    #[cfg(all(windows, not(feature = "test-utils")))]
    {
        return windows_impl::icon_data_url_for_path(path);
    }

    #[cfg(any(not(windows), feature = "test-utils"))]
    {
        let _ = path;
        None
    }
}

fn should_use_native_icon(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };

    matches!(
        extension.to_ascii_lowercase().as_str(),
        "exe" | "com" | "scr" | "msi" | "msix" | "dll"
    )
}

#[cfg(all(windows, not(feature = "test-utils")))]
mod windows_impl {
    use super::*;
    use base64::Engine;
    use png::{BitDepth, ColorType, Encoder};
    use std::mem::size_of;

    use windows::core::PCWSTR;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
    };
    use windows::Win32::UI::Shell::{
        SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON,
    };
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON};

    pub(super) fn icon_data_url_for_path(path: &Path) -> Option<String> {
        let wide_path = wide(path);
        let mut info = SHFILEINFOW::default();
        let result = unsafe {
            SHGetFileInfoW(
                PCWSTR(wide_path.as_ptr()),
                Default::default(),
                Some(&mut info),
                size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_SMALLICON,
            )
        };
        if result == 0 || info.hIcon.is_invalid() {
            return None;
        }

        let icon_guard = IconGuard(info.hIcon);
        hicon_to_data_url(icon_guard.0)
    }

    struct IconGuard(HICON);

    impl Drop for IconGuard {
        fn drop(&mut self) {
            let _ = unsafe { DestroyIcon(self.0) };
        }
    }

    struct BitmapGuard(HBITMAP);

    impl Drop for BitmapGuard {
        fn drop(&mut self) {
            if !self.0.is_invalid() {
                let _ = unsafe { DeleteObject(self.0.into()) };
            }
        }
    }

    struct DeviceContextGuard(windows::Win32::Graphics::Gdi::HDC);

    impl Drop for DeviceContextGuard {
        fn drop(&mut self) {
            let _ = unsafe { DeleteDC(self.0) };
        }
    }

    fn hicon_to_data_url(icon: HICON) -> Option<String> {
        let mut icon_info = Default::default();
        if unsafe { GetIconInfo(icon, &mut icon_info) }.is_err() {
            return None;
        }

        let color_bitmap = BitmapGuard(icon_info.hbmColor);
        let mask_bitmap = BitmapGuard(icon_info.hbmMask);
        let bitmap = if !color_bitmap.0.is_invalid() {
            color_bitmap.0
        } else {
            mask_bitmap.0
        };
        if bitmap.is_invalid() {
            return None;
        }

        let bitmap_bytes = bitmap_to_png_bytes(bitmap)?;
        Some(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bitmap_bytes)
        ))
    }

    fn bitmap_to_png_bytes(bitmap: HBITMAP) -> Option<Vec<u8>> {
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
        if width == 0 || height == 0 {
            return None;
        }

        let stride = width.checked_mul(4)?;
        let pixel_bytes_len = usize::try_from(stride.checked_mul(height)?).ok()?;
        let header = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
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
        let mut bytes = Vec::new();
        let mut encoder = Encoder::new(&mut bytes, width, height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&rgba_pixels).ok()?;
        drop(writer);
        Some(bytes)
    }

    fn bgra_to_rgba(pixels: &[u8]) -> Option<Vec<u8>> {
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

    fn wide(path: &Path) -> Vec<u16> {
        path.to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect()
    }
}
