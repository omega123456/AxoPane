//! Windows Shell thumbnail provider.
//!
//! Shell and GDI objects never leave the two provider-owned MTA workers.  The
//! synchronous provider boundary is intentional for Phase 2: a later
//! scheduler submits work to this already-bounded executor instead of creating
//! another pool or putting COM work on a Tauri worker.

use super::provider::{ProviderCapability, ThumbnailProvider};
use super::types::{
    validated_png_data_url, ThumbnailCandidate, ThumbnailState, MAX_PNG_BYTES,
    MAX_PREVIEW_DIMENSION, MAX_PREVIEW_PIXELS,
};
use base64::Engine;
use png::{BitDepth, ColorType, Encoder};
use std::mem::size_of;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use windows::core::PCWSTR;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_RESIZETOFIT,
};

const WORKER_COUNT: usize = 2;
const QUEUE_CAPACITY: usize = 64;

pub struct WindowsThumbnailProvider {
    workers: Mutex<Option<Workers>>,
}

struct Workers {
    sender: SyncSender<Work>,
    joins: Vec<JoinHandle<()>>,
}

struct Work {
    candidate: ThumbnailCandidate,
    reply: Sender<ThumbnailState>,
}

impl Default for WindowsThumbnailProvider {
    fn default() -> Self {
        Self {
            workers: Mutex::new(Some(Workers::start())),
        }
    }
}

impl Workers {
    fn start() -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Work>(QUEUE_CAPACITY);
        let receiver = std::sync::Arc::new(Mutex::new(receiver));
        let joins = (0..WORKER_COUNT)
            .filter_map(|index| {
                let receiver = std::sync::Arc::clone(&receiver);
                thread::Builder::new()
                    .name(format!("thumbnail-shell-{index}"))
                    .spawn(move || worker_loop(receiver))
                    .ok()
            })
            .collect();
        Self { sender, joins }
    }
}

impl Drop for WindowsThumbnailProvider {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl ThumbnailProvider for WindowsThumbnailProvider {
    fn capability(&self) -> ProviderCapability {
        ProviderCapability::Native
    }

    fn generate(&self, candidate: &ThumbnailCandidate) -> ThumbnailState {
        if candidate.is_directory {
            return ThumbnailState::Unavailable;
        }
        let (reply, result) = mpsc::channel();
        let Ok(guard) = self.workers.lock() else {
            return ThumbnailState::Failed;
        };
        let Some(workers) = guard.as_ref() else {
            return ThumbnailState::Unavailable;
        };
        if workers
            .sender
            .send(Work {
                candidate: candidate.clone(),
                reply,
            })
            .is_err()
        {
            return ThumbnailState::Failed;
        }
        result.recv().unwrap_or(ThumbnailState::Failed)
    }

    fn shutdown(&self) {
        let Ok(mut guard) = self.workers.lock() else {
            return;
        };
        let Some(workers) = guard.take() else {
            return;
        };
        drop(workers.sender);
        for join in workers.joins {
            let _ = join.join();
        }
    }
}

fn worker_loop(receiver: std::sync::Arc<Mutex<Receiver<Work>>>) {
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
    loop {
        let message = match receiver.lock() {
            Ok(receiver) => receiver.recv(),
            Err(_) => break,
        };
        let Ok(work) = message else { break };
        let _ = work.reply.send(extract(&work.candidate));
    }
    if initialized {
        unsafe { CoUninitialize() };
    }
}

fn extract(candidate: &ThumbnailCandidate) -> ThumbnailState {
    let wide = candidate
        .fingerprint
        .path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let item: IShellItemImageFactory =
        match unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) } {
            Ok(item) => item,
            Err(_) => return ThumbnailState::Unavailable,
        };
    let bitmap = match unsafe {
        item.GetImage(
            SIZE {
                cx: MAX_PREVIEW_DIMENSION as i32,
                cy: MAX_PREVIEW_DIMENSION as i32,
            },
            SIIGBF_RESIZETOFIT,
        )
    } {
        Ok(bitmap) => BitmapGuard(bitmap),
        Err(_) => return ThumbnailState::Unavailable,
    };
    let Some(png) = bitmap_to_png(bitmap.0) else {
        return ThumbnailState::Failed;
    };
    if png.len() > MAX_PNG_BYTES {
        return ThumbnailState::Failed;
    }
    validated_png_data_url(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
    .unwrap_or(ThumbnailState::Failed)
}

struct BitmapGuard(HBITMAP);
impl Drop for BitmapGuard {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { DeleteObject(self.0.into()) };
        }
    }
}
struct DcGuard(windows::Win32::Graphics::Gdi::HDC);
impl Drop for DcGuard {
    fn drop(&mut self) {
        let _ = unsafe { DeleteDC(self.0) };
    }
}

fn bitmap_to_png(bitmap: HBITMAP) -> Option<Vec<u8>> {
    let mut object = BITMAP::default();
    if unsafe {
        GetObjectW(
            bitmap.into(),
            i32::try_from(size_of::<BITMAP>()).ok()?,
            Some((&mut object as *mut BITMAP).cast()),
        )
    } == 0
    {
        return None;
    }
    let width = u32::try_from(object.bmWidth).ok()?;
    let height = u32::try_from(object.bmHeight.abs()).ok()?;
    if width == 0
        || height == 0
        || width > MAX_PREVIEW_DIMENSION
        || height > MAX_PREVIEW_DIMENSION
        || width.checked_mul(height)? > MAX_PREVIEW_PIXELS
    {
        return None;
    }
    let stride = width.checked_mul(4)?;
    let len = usize::try_from(stride.checked_mul(height)?).ok()?;
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: len as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bgra = vec![0; len];
    let dc = unsafe { CreateCompatibleDC(None) };
    if dc.0.is_null() {
        return None;
    }
    let dc = DcGuard(dc);
    if unsafe {
        GetDIBits(
            dc.0,
            bitmap,
            0,
            height,
            Some(bgra.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        )
    } == 0
    {
        return None;
    }
    let mut rgba = Vec::with_capacity(len);
    for pixel in bgra.chunks_exact(4) {
        let alpha = u32::from(pixel[3]);
        let unpremultiply = |channel: u8| {
            if alpha == 0 {
                0
            } else {
                ((u32::from(channel) * 255 + alpha / 2) / alpha).min(255) as u8
            }
        };
        rgba.extend_from_slice(&[
            unpremultiply(pixel[2]),
            unpremultiply(pixel[1]),
            unpremultiply(pixel[0]),
            pixel[3],
        ]);
    }
    let mut output = Vec::new();
    let mut encoder = Encoder::new(&mut output, width, height);
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    encoder.write_header().ok()?.write_image_data(&rgba).ok()?;
    Some(output)
}
