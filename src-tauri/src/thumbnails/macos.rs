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
use crossbeam_channel::{bounded, select, Receiver, RecvError, Sender};
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
const SHUTDOWN_DRAIN_POLL: Duration = Duration::from_millis(10);
/// One progressive preview plus the final representation.
const REPLY_CAPACITY: usize = 2;
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
    _completion: CompletionBlock,
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

impl MacosThumbnailProvider {
    fn send_generate(&self, command: Command) -> Result<(), ThumbnailState> {
        let Ok(owner) = self.owner.lock() else {
            return Err(ThumbnailState::Failed);
        };
        let Some(owner) = owner.as_ref() else {
            return Err(ThumbnailState::Unavailable);
        };
        owner
            .commands
            .send(command)
            .map_err(|_| ThumbnailState::Failed)
    }

    fn await_generation(
        &self,
        candidate: &ThumbnailCandidate,
        result: &Receiver<ProviderUpdate>,
        preview: &ThumbnailPreviewCallback,
    ) -> ThumbnailState {
        let deadline = Instant::now() + GENERATION_DEADLINE;
        loop {
            match result.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(update) if update.is_final => return encode_payload(update.payload),
                Ok(update) => forward_ready_preview(preview, update.payload),
                Err(_) => break,
            }
        }
        self.cancel(candidate);
        ThumbnailState::Failed
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
        let (reply, result) = bounded(REPLY_CAPACITY);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let command = Command::Generate {
            id,
            candidate: candidate.clone(),
            reply,
        };
        if let Err(state) = self.send_generate(command) {
            return state;
        }
        self.await_generation(candidate, &result, &preview)
    }

    fn cancel(&self, candidate: &ThumbnailCandidate) {
        let Ok(owner) = self.owner.lock() else {
            return;
        };
        let Some(owner) = owner.as_ref() else {
            return;
        };
        let _ = owner.commands.try_send(Command::Cancel {
            fingerprint: candidate.fingerprint.clone(),
        });
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

fn unavailable_update() -> ProviderUpdate {
    ProviderUpdate::final_update(ProviderPayload::State(ThumbnailState::Unavailable))
}

impl ProviderUpdate {
    fn final_update(payload: ProviderPayload) -> Self {
        Self {
            payload,
            is_final: true,
        }
    }

    fn preview(payload: ProviderPayload) -> Self {
        Self {
            payload,
            is_final: false,
        }
    }
}

fn owner_loop(commands: Receiver<Command>) {
    let (completion_sender, completions) = bounded::<Completion>(COMPLETION_CAPACITY);
    let mut owner = QuickLookOwner::new(completion_sender);
    while owner.is_running() {
        select! {
            recv(completions) -> completion => owner.on_completion(completion),
            recv(commands) -> command => owner.on_command(command, &completions),
        }
    }
}

struct QuickLookOwner {
    generator: Retained<QLThumbnailGenerator>,
    completion_sender: Sender<Completion>,
    pending: HashMap<u64, Pending>,
    accepting: bool,
}

impl QuickLookOwner {
    fn new(completion_sender: Sender<Completion>) -> Self {
        Self {
            generator: unsafe { QLThumbnailGenerator::sharedGenerator() },
            completion_sender,
            pending: HashMap::new(),
            accepting: true,
        }
    }

    fn is_running(&self) -> bool {
        self.accepting || !self.pending.is_empty()
    }

    fn has_capacity(&self) -> bool {
        self.accepting && self.pending.len() < MAX_IN_FLIGHT
    }

    fn on_completion(&mut self, completion: Result<Completion, RecvError>) {
        let Ok(completion) = completion else {
            return;
        };
        if completion.is_final {
            self.deliver_final(completion);
            return;
        }
        self.deliver_preview(completion);
    }

    fn deliver_final(&mut self, completion: Completion) {
        let Some(pending) = self.pending.remove(&completion.id) else {
            return;
        };
        let _ = pending
            .reply
            .send(ProviderUpdate::final_update(completion.payload));
    }

    fn deliver_preview(&self, completion: Completion) {
        let Some(pending) = self.pending.get(&completion.id) else {
            return;
        };
        let _ = pending
            .reply
            .send(ProviderUpdate::preview(completion.payload));
    }

    fn on_command(
        &mut self,
        command: Result<Command, RecvError>,
        completions: &Receiver<Completion>,
    ) {
        match command {
            Ok(Command::Generate {
                id,
                candidate,
                reply,
            }) => self.on_generate(id, candidate, reply),
            Ok(Command::Cancel { fingerprint }) => self.cancel_fingerprint(&fingerprint),
            Ok(Command::Shutdown) | Err(_) => self.drain_on_shutdown(completions),
        }
    }

    fn on_generate(
        &mut self,
        id: u64,
        candidate: ThumbnailCandidate,
        reply: Sender<ProviderUpdate>,
    ) {
        if !self.has_capacity() {
            let _ = reply.send(unavailable_update());
            return;
        }
        self.begin(id, candidate, reply);
    }

    fn drain_on_shutdown(&mut self, completions: &Receiver<Completion>) {
        self.accepting = false;
        for pending in self.pending.values() {
            self.cancel_pending(pending);
        }
        // A framework callback owns the terminal acknowledgement, so its representation is not retained.
        let deadline = Instant::now() + SHUTDOWN_DEADLINE;
        while !self.pending.is_empty() && Instant::now() < deadline {
            self.settle_drained_completion(completions);
        }
        for (_, pending) in self.pending.drain() {
            let _ = pending.reply.send(unavailable_update());
        }
    }

    fn settle_drained_completion(&mut self, completions: &Receiver<Completion>) {
        let Ok(completion) = completions.recv_timeout(SHUTDOWN_DRAIN_POLL) else {
            return;
        };
        let Some(pending) = self.pending.remove(&completion.id) else {
            return;
        };
        let _ = pending.reply.send(unavailable_update());
    }

    fn cancel_pending(&self, pending: &Pending) {
        pending.callback.cancelled.store(true, Ordering::Release);
        unsafe {
            self.generator.cancelRequest(&pending.request);
        }
    }

    fn cancel_fingerprint(&self, fingerprint: &ThumbnailFingerprint) {
        let matching = self
            .pending
            .values()
            .filter(|pending| pending.candidate.fingerprint == *fingerprint);
        for pending in matching {
            self.cancel_pending(pending);
        }
    }

    fn begin(&mut self, id: u64, candidate: ThumbnailCandidate, reply: Sender<ProviderUpdate>) {
        let request = thumbnail_request(&candidate);
        let callback = std::sync::Arc::new(CallbackState {
            cancelled: AtomicBool::new(false),
        });
        let callback_state = std::sync::Arc::clone(&callback);
        let completion = completion_block(id, callback_state, self.completion_sender.clone());
        unsafe {
            self.generator
                .generateRepresentationsForRequest_updateHandler(&request, Some(&completion));
        }
        self.pending.insert(
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
}

type CompletionBlock =
    RcBlock<dyn Fn(*mut QLThumbnailRepresentation, QLThumbnailRepresentationType, *mut NSError)>;

fn completion_block(
    id: u64,
    callback_state: std::sync::Arc<CallbackState>,
    completion_sender: Sender<Completion>,
) -> CompletionBlock {
    RcBlock::new(
        move |representation: *mut QLThumbnailRepresentation,
              representation_type: QLThumbnailRepresentationType,
              error: *mut NSError| {
            let is_final = representation_type == QLThumbnailRepresentationType::Thumbnail;
            let payload =
                completion_payload(&callback_state, representation, representation_type, error);
            let worth_sending = is_final || matches!(payload, ProviderPayload::Frame(_));
            if worth_sending {
                let _ = completion_sender.try_send(Completion {
                    id,
                    payload,
                    is_final,
                });
            }
        },
    )
}

fn thumbnail_request(candidate: &ThumbnailCandidate) -> Retained<QLThumbnailGenerationRequest> {
    let path = NSString::from_str(&candidate.fingerprint.path.to_string_lossy());
    let url = NSURL::fileURLWithPath(&path);
    let size = CGSize {
        width: MAX_PREVIEW_DIMENSION as f64,
        height: MAX_PREVIEW_DIMENSION as f64,
    };
    // This selector is not generated by objc2 0.3 on every deployment target, although it is available on our macOS 12 baseline.
    unsafe {
        objc2::msg_send![QLThumbnailGenerationRequest::alloc(), initWithFileAtURL: &*url, size: size, scale: 1.0f64, representationTypes: QLThumbnailGenerationRequestRepresentationTypes::LowQualityThumbnail | QLThumbnailGenerationRequestRepresentationTypes::Thumbnail]
    }
}

fn completion_payload(
    callback_state: &CallbackState,
    representation: *mut QLThumbnailRepresentation,
    representation_type: QLThumbnailRepresentationType,
    error: *mut NSError,
) -> ProviderPayload {
    let produced_nothing = callback_state.cancelled.load(Ordering::Acquire)
        || representation.is_null()
        || !error.is_null();
    if produced_nothing {
        return ProviderPayload::State(ThumbnailState::Unavailable);
    }
    unsafe {
        rgba_frame_from_representation(&*representation, representation_type).map_or(
            ProviderPayload::State(ThumbnailState::Failed),
            ProviderPayload::Frame,
        )
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
        quality: quality_of(representation_type),
    })
}

fn quality_of(
    representation_type: QLThumbnailRepresentationType,
) -> crate::ipc::types::ThumbnailQuality {
    if representation_type == QLThumbnailRepresentationType::Thumbnail {
        return crate::ipc::types::ThumbnailQuality::High;
    }
    crate::ipc::types::ThumbnailQuality::Low
}

fn forward_ready_preview(preview: &ThumbnailPreviewCallback, payload: ProviderPayload) {
    let state = encode_payload(payload);
    if matches!(state, ThumbnailState::Ready { .. }) {
        preview(state);
    }
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
