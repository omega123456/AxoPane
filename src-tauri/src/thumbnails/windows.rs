//! Windows Shell thumbnail provider.
//!
//! Shell and GDI objects never leave the two provider-owned MTA workers. The
//! synchronous provider boundary is intentional: the scheduler submits work to
//! this already-bounded executor instead of creating another pool or putting
//! COM work on a Tauri worker.

use super::provider::{ProviderCapability, ThumbnailPreviewCallback, ThumbnailProvider};
use super::types::{
    validated_png_data_url, ThumbnailCandidate, ThumbnailState, MAX_PNG_BYTES,
    MAX_PREVIEW_DIMENSION, MAX_PREVIEW_PIXELS,
};
use crate::pixels::png_data_url;
use crate::win32::com::ComApartment;
use crate::win32::gdi::{bitmap_to_png_bytes, BitmapGuard};
use crate::win32::wide_nul;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::SIZE;
use windows::Win32::System::Com::COINIT_MULTITHREADED;
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_INCACHEONLY, SIIGBF_RESIZETOFIT,
    SIIGBF_THUMBNAILONLY,
};

fn is_past(deadline: Instant) -> bool {
    Instant::now() >= deadline
}

const WORKER_COUNT: usize = 2;
const QUEUE_CAPACITY: usize = 64;
const GENERATION_DEADLINE: Duration = Duration::from_secs(2);

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
    deadline: Instant,
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

impl WindowsThumbnailProvider {
    fn send_work(&self, work: Work) -> Result<(), ThumbnailState> {
        let Ok(guard) = self.workers.lock() else {
            return Err(ThumbnailState::Failed);
        };
        let Some(workers) = guard.as_ref() else {
            return Err(ThumbnailState::Unavailable);
        };
        workers
            .sender
            .try_send(work)
            .map_err(|_| ThumbnailState::Failed)
    }
}

impl ThumbnailProvider for WindowsThumbnailProvider {
    fn capability(&self) -> ProviderCapability {
        ProviderCapability::Native
    }

    fn generate(
        &self,
        candidate: &ThumbnailCandidate,
        _preview: ThumbnailPreviewCallback,
    ) -> ThumbnailState {
        if candidate.is_directory {
            return ThumbnailState::Unavailable;
        }
        let (reply, result) = mpsc::channel();
        let work = Work {
            candidate: candidate.clone(),
            reply,
            deadline: Instant::now() + GENERATION_DEADLINE,
        };
        if let Err(state) = self.send_work(work) {
            return state;
        }
        result
            .recv_timeout(GENERATION_DEADLINE)
            .unwrap_or(ThumbnailState::Failed)
    }

    fn shutdown(&self) {
        let Ok(mut guard) = self.workers.lock() else {
            return;
        };
        let Some(workers) = guard.take() else {
            return;
        };
        drop(workers.sender);
        // `IShellItemImageFactory::GetImage` has no cancellation API and a
        // third-party shell extension can block indefinitely. Dropping the
        // handles lets healthy workers exit normally without hanging app exit.
        drop(workers.joins);
    }
}

fn worker_loop(receiver: std::sync::Arc<Mutex<Receiver<Work>>>) {
    let _apartment = ComApartment::initialize(COINIT_MULTITHREADED);
    loop {
        let message = match receiver.lock() {
            Ok(receiver) => receiver.recv(),
            Err(_) => break,
        };
        let Ok(work) = message else { break };
        if is_past(work.deadline) {
            let _ = work.reply.send(ThumbnailState::Failed);
            continue;
        }
        let _ = work.reply.send(extract(&work.candidate, work.deadline));
    }
}

fn extract(candidate: &ThumbnailCandidate, deadline: Instant) -> ThumbnailState {
    let wide = wide_nul(&candidate.fingerprint.path);
    let item: IShellItemImageFactory =
        match unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) } {
            Ok(item) => item,
            Err(_) => return ThumbnailState::Unavailable,
        };
    let requested_size = SIZE {
        cx: MAX_PREVIEW_DIMENSION as i32,
        cy: MAX_PREVIEW_DIMENSION as i32,
    };
    let cached = unsafe {
        item.GetImage(
            requested_size,
            SIIGBF_RESIZETOFIT | SIIGBF_THUMBNAILONLY | SIIGBF_INCACHEONLY,
        )
    };
    let bitmap = match cached {
        Ok(bitmap) => Ok(bitmap),
        Err(_) if is_past(deadline) => return ThumbnailState::Failed,
        Err(_) => unsafe {
            item.GetImage(requested_size, SIIGBF_RESIZETOFIT | SIIGBF_THUMBNAILONLY)
        },
    };
    let bitmap = match bitmap {
        Ok(bitmap) => BitmapGuard(bitmap),
        Err(_) => return ThumbnailState::Unavailable,
    };
    let Some(png) = bitmap_to_png_bytes(bitmap.0, is_preview_size) else {
        return ThumbnailState::Failed;
    };
    if png.len() > MAX_PNG_BYTES {
        return ThumbnailState::Failed;
    }
    validated_png_data_url(png_data_url(&png)).unwrap_or(ThumbnailState::Failed)
}

fn is_preview_size(width: u32, height: u32) -> bool {
    width <= MAX_PREVIEW_DIMENSION
        && height <= MAX_PREVIEW_DIMENSION
        && width
            .checked_mul(height)
            .is_some_and(|pixels| pixels <= MAX_PREVIEW_PIXELS)
}
