//! Quick Look thumbnail provider.
//!
//! The serial owner is the only thread that owns `QLThumbnailGenerator`,
//! requests, retained completion blocks, and cancellation. Quick Look chooses
//! the callback queue, so callbacks immediately copy into an owned bounded
//! RGBA buffer and only send Rust data back to the owner.

use super::provider::{ProviderCapability, ThumbnailPreviewCallback, ThumbnailProvider};
use super::types::{
    validated_png_data_url, ThumbnailCandidate, ThumbnailFingerprint, ThumbnailState,
    MAX_PNG_BYTES, MAX_PREVIEW_DIMENSION, MAX_PREVIEW_PIXELS,
};
use base64::Engine;
use block2::RcBlock;
use crossbeam_channel::{bounded, select, Receiver, Sender};
use objc2::rc::Retained;
use objc2::AnyThread;
use objc2_core_foundation::{CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo,
};
use objc2_foundation::{NSError, NSString, NSURL};
use objc2_quick_look_thumbnailing::{
    QLThumbnailGenerationRequest, QLThumbnailGenerationRequestRepresentationTypes,
    QLThumbnailGenerator, QLThumbnailRepresentation, QLThumbnailRepresentationType,
};
use png::{BitDepth, ColorType, Encoder};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const MAX_IN_FLIGHT: usize = 2;
const COMMAND_CAPACITY: usize = 64;
const COMPLETION_CAPACITY: usize = 8;
const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(2);
const GENERATION_DEADLINE: Duration = Duration::from_secs(2);

pub struct MacosThumbnailProvider {
    owner: Mutex<Option<Owner>>,
    next_id: AtomicU64,
}

struct Owner {
    commands: Sender<Command>,
    join: JoinHandle<()>,
}
enum Command {
    Generate {
        id: u64,
        candidate: ThumbnailCandidate,
        reply: Sender<ProviderUpdate>,
    },
    Cancel {
        fingerprint: ThumbnailFingerprint,
    },
    Shutdown,
}
struct Completion {
    id: u64,
    payload: ProviderPayload,
    is_final: bool,
}
struct ProviderUpdate {
    payload: ProviderPayload,
    is_final: bool,
}
enum ProviderPayload {
    Frame(RawFrame),
    State(ThumbnailState),
}
struct RawFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    quality: crate::ipc::types::ThumbnailQuality,
}
struct CallbackState {
    cancelled: AtomicBool,
}
struct Pending {
    candidate: ThumbnailCandidate,
    request: Retained<QLThumbnailGenerationRequest>,
    _completion: RcBlock<
        dyn Fn(*mut QLThumbnailRepresentation, QLThumbnailRepresentationType, *mut NSError),
    >,
    reply: Sender<ProviderUpdate>,
    callback: std::sync::Arc<CallbackState>,
}

impl Default for MacosThumbnailProvider {
    fn default() -> Self {
        let (commands, receiver) = bounded(COMMAND_CAPACITY);
        let join = thread::Builder::new()
            .name("thumbnail-quick-look".into())
            .spawn(move || owner_loop(receiver))
            .expect("thumbnail Quick Look owner thread");
        Self {
            owner: Mutex::new(Some(Owner { commands, join })),
            next_id: AtomicU64::new(1),
        }
    }
}

impl Drop for MacosThumbnailProvider {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl ThumbnailProvider for MacosThumbnailProvider {
    fn capability(&self) -> ProviderCapability {
        ProviderCapability::Native
    }

    fn generate(
        &self,
        candidate: &ThumbnailCandidate,
        preview: ThumbnailPreviewCallback,
    ) -> ThumbnailState {
        if candidate.is_directory {
            return ThumbnailState::Unavailable;
        }
        let (reply, result) = bounded(2);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        {
            let Ok(owner) = self.owner.lock() else {
                return ThumbnailState::Failed;
            };
            let Some(owner) = owner.as_ref() else {
                return ThumbnailState::Unavailable;
            };
            if owner
                .commands
                .send(Command::Generate {
                    id,
                    candidate: candidate.clone(),
                    reply,
                })
                .is_err()
            {
                return ThumbnailState::Failed;
            }
        }
        let deadline = Instant::now() + GENERATION_DEADLINE;
        loop {
            match result.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(update) if update.is_final => {
                    return encode_payload(update.payload);
                }
                Ok(update) => {
                    let state = encode_payload(update.payload);
                    if matches!(state, ThumbnailState::Ready { .. }) {
                        preview(state);
                    }
                }
                Err(_) => {
                    self.cancel(candidate);
                    return ThumbnailState::Failed;
                }
            }
        }
    }

    fn cancel(&self, candidate: &ThumbnailCandidate) {
        if let Ok(owner) = self.owner.lock() {
            if let Some(owner) = owner.as_ref() {
                let _ = owner.commands.try_send(Command::Cancel {
                    fingerprint: candidate.fingerprint.clone(),
                });
            }
        }
    }

    fn shutdown(&self) {
        let Ok(mut owner) = self.owner.lock() else {
            return;
        };
        let Some(owner) = owner.take() else { return };
        let _ = owner.commands.send(Command::Shutdown);
        // The owner performs its bounded drain before releasing QL objects.
        // Joining is deterministic once the native framework honours cancel.
        let _ = owner.join.join();
    }
}

fn owner_loop(commands: Receiver<Command>) {
    let generator = unsafe { QLThumbnailGenerator::sharedGenerator() };
    let (completion_tx, completion_rx) = bounded::<Completion>(COMPLETION_CAPACITY);
    let mut pending = HashMap::<u64, Pending>::new();
    let mut accepting = true;
    while accepting || !pending.is_empty() {
        select! {
            recv(completion_rx) -> completion => if let Ok(completion) = completion {
                if completion.is_final {
                    if let Some(pending) = pending.remove(&completion.id) {
                        let _ = pending.reply.send(ProviderUpdate { payload: completion.payload, is_final: true });
                    }
                } else if let Some(pending) = pending.get(&completion.id) {
                    let _ = pending.reply.send(ProviderUpdate { payload: completion.payload, is_final: false });
                }
            },
            recv(commands) -> command => match command {
                Ok(Command::Generate { id, candidate, reply }) if accepting && pending.len() < MAX_IN_FLIGHT => {
                    begin(&generator, id, candidate, reply, completion_tx.clone(), &mut pending);
                }
                Ok(Command::Generate { reply, .. }) => { let _ = reply.send(ProviderUpdate { payload: ProviderPayload::State(ThumbnailState::Unavailable), is_final: true }); }
                Ok(Command::Cancel { fingerprint }) => cancel_fingerprint(&generator, &mut pending, &fingerprint),
                Ok(Command::Shutdown) | Err(_) => {
                    accepting = false;
                    for pending in pending.values() { pending.callback.cancelled.store(true, Ordering::Release); unsafe { generator.cancelRequest(&pending.request); } }
                    // A framework callback owns the terminal acknowledgement. Do not retain its
                    // representation; drain only briefly, then safely close receiver state.
                    let deadline = std::time::Instant::now() + SHUTDOWN_DEADLINE;
                    while !pending.is_empty() && std::time::Instant::now() < deadline {
                        if let Ok(completion) = completion_rx.recv_timeout(Duration::from_millis(10)) {
                            if let Some(pending) = pending.remove(&completion.id) { let _ = pending.reply.send(ProviderUpdate { payload: ProviderPayload::State(ThumbnailState::Unavailable), is_final: true }); }
                        }
                    }
                    for (_, pending) in pending.drain() { let _ = pending.reply.send(ProviderUpdate { payload: ProviderPayload::State(ThumbnailState::Unavailable), is_final: true }); }
                }
            }
        }
    }
}

fn begin(
    generator: &QLThumbnailGenerator,
    id: u64,
    candidate: ThumbnailCandidate,
    reply: Sender<ProviderUpdate>,
    completion_tx: Sender<Completion>,
    pending: &mut HashMap<u64, Pending>,
) {
    let path = NSString::from_str(&candidate.fingerprint.path.to_string_lossy());
    let url = NSURL::fileURLWithPath(&path);
    let size = CGSize {
        width: MAX_PREVIEW_DIMENSION as f64,
        height: MAX_PREVIEW_DIMENSION as f64,
    };
    // This selector is not generated by objc2 0.3 on every deployment
    // target, although it is available on our macOS 12 baseline.
    let request: Retained<QLThumbnailGenerationRequest> = unsafe {
        objc2::msg_send![QLThumbnailGenerationRequest::alloc(), initWithFileAtURL: &*url, size: size, scale: 1.0f64, representationTypes: QLThumbnailGenerationRequestRepresentationTypes::LowQualityThumbnail | QLThumbnailGenerationRequestRepresentationTypes::Thumbnail]
    };
    let callback = std::sync::Arc::new(CallbackState {
        cancelled: AtomicBool::new(false),
    });
    let callback_state = std::sync::Arc::clone(&callback);
    let completion = RcBlock::new(
        move |representation: *mut QLThumbnailRepresentation,
              representation_type: QLThumbnailRepresentationType,
              error: *mut NSError| {
            let is_final = representation_type == QLThumbnailRepresentationType::Thumbnail;
            let payload = if callback_state.cancelled.load(Ordering::Acquire) {
                ProviderPayload::State(ThumbnailState::Unavailable)
            } else if representation.is_null() || !error.is_null() {
                ProviderPayload::State(ThumbnailState::Unavailable)
            } else {
                unsafe {
                    rgba_frame_from_representation(&*representation, representation_type).map_or(
                        ProviderPayload::State(ThumbnailState::Failed),
                        ProviderPayload::Frame,
                    )
                }
            };
            if is_final || matches!(payload, ProviderPayload::Frame(_)) {
                let _ = completion_tx.try_send(Completion {
                    id,
                    payload,
                    is_final,
                });
            }
        },
    );
    unsafe {
        generator.generateRepresentationsForRequest_updateHandler(&request, Some(&completion));
    }
    pending.insert(
        id,
        Pending {
            candidate,
            request,
            _completion: completion,
            reply,
            callback,
        },
    );
}

fn cancel_fingerprint(
    generator: &QLThumbnailGenerator,
    pending: &mut HashMap<u64, Pending>,
    fingerprint: &ThumbnailFingerprint,
) {
    let ids: Vec<_> = pending
        .iter()
        .filter_map(|(id, pending)| (pending.candidate.fingerprint == *fingerprint).then_some(*id))
        .collect();
    for id in ids {
        if let Some(pending) = pending.get(&id) {
            pending.callback.cancelled.store(true, Ordering::Release);
            unsafe {
                generator.cancelRequest(&pending.request);
            }
        }
    }
}

/// Copies arbitrary Quick Look CGImage formats through our fixed RGBA context;
/// provider-owned bytes are never assumed to be directly usable RGBA.
unsafe fn rgba_frame_from_representation(
    representation: &QLThumbnailRepresentation,
    representation_type: QLThumbnailRepresentationType,
) -> Option<RawFrame> {
    let image = representation.CGImage();
    let width = CGImage::width(Some(&image));
    let height = CGImage::height(Some(&image));
    if width == 0
        || height == 0
        || width > MAX_PREVIEW_DIMENSION as usize
        || height > MAX_PREVIEW_DIMENSION as usize
        || width.checked_mul(height)? > MAX_PREVIEW_PIXELS as usize
    {
        return None;
    }
    let row = width.checked_mul(4)?;
    let mut rgba = vec![0; row.checked_mul(height)?];
    let color_space = CGColorSpace::new_device_rgb()?;
    let context = CGBitmapContextCreate(
        rgba.as_mut_ptr().cast(),
        width,
        height,
        8,
        row,
        Some(&color_space),
        CGImageAlphaInfo::PremultipliedLast.0,
    )?;
    CGContext::draw_image(
        Some(&context),
        CGRect {
            origin: Default::default(),
            size: CGSize {
                width: width as f64,
                height: height as f64,
            },
        },
        Some(&image),
    );
    Some(RawFrame {
        width: width as u32,
        height: height as u32,
        rgba,
        quality: if representation_type == QLThumbnailRepresentationType::Thumbnail {
            crate::ipc::types::ThumbnailQuality::High
        } else {
            crate::ipc::types::ThumbnailQuality::Low
        },
    })
}

fn encode_payload(payload: ProviderPayload) -> ThumbnailState {
    let frame = match payload {
        ProviderPayload::Frame(frame) => frame,
        ProviderPayload::State(state) => return state,
    };
    let mut png = Vec::new();
    let mut encoder = Encoder::new(&mut png, frame.width, frame.height);
    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);
    let encoded = encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&frame.rgba));
    if encoded.is_err() {
        return ThumbnailState::Failed;
    }
    if png.len() > MAX_PNG_BYTES {
        return ThumbnailState::Failed;
    }
    let Ok(validated) = validated_png_data_url(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )) else {
        return ThumbnailState::Failed;
    };
    match validated {
        ThumbnailState::Ready { data_url, .. } => ThumbnailState::Ready {
            data_url,
            quality: frame.quality,
        },
        _ => ThumbnailState::Failed,
    }
}
