//! The cross-subsystem resource coordinator: one fixed-size, globally
//! bounded admission service that subsystems (directory sessions, size
//! scheduling, transfer/ops, native-menu warm discovery) submit work
//! through instead of each owning an independent thread pool.
//!
//! # Model
//!
//! The coordinator does not execute jobs itself — subsystems still run
//! their own work (on whatever thread makes sense for them) once admitted.
//! What the coordinator owns is *admission*: a global cap of four
//! latency-sensitive slots, an adaptive throughput cap, and two CPU slots
//! (see [`queue::MAX_LATENCY_SLOTS`], [`max_throughput_slots`], and
//! [`queue::MAX_CPU_SLOTS`]), fair scheduling of that capacity across
//! however many distinct resource keys are in play, and atomic
//! all-or-none multi-resource reservations so no caller can ever observe a
//! job holding part of what it asked for.
//!
//! A caller [`ResourceCoordinator::submit`]s a [`JobSpec`] describing the
//! job classes it needs (latency/throughput/CPU — a job may need more than
//! one, e.g. archive creation needs throughput *and* CPU) and the complete
//! set of resource keys it touches (source, destination, archive path,
//! etc.). `submit` blocks the calling thread until the coordinator grants a
//! [`JobHandle`], the caller cancels via a second thread calling
//! [`ResourceCoordinator::cancel`] with the same [`JobId`], or the
//! coordinator shuts down. Dropping a granted [`JobHandle`] (or calling
//! [`JobHandle::release`] explicitly) releases every class/resource permit
//! it held — deterministically, even on an early return or panic unwind,
//! because release happens in `Drop`.
//!
//! # Deadlock freedom
//!
//! All admission decisions are made by a single dispatcher thread under one
//! lock ([`queue::SchedulerState`]), and every job's resource-key set is
//! canonically sorted before reservation
//! ([`queue::canonicalize_resource_keys`]). A job needing `{A, B}` and a
//! job needing `{B, A}` are therefore always evaluated against the same
//! globally-agreed key order, and because reservation is all-or-none in one
//! scheduler decision, neither job can ever hold `A` while waiting on `B`
//! (or vice versa) — the classic opposing-order deadlock is structurally
//! impossible here, not merely made unlikely.
//!
//! # Test-utils
//!
//! There is no OS-global or platform-specific state anywhere in this
//! module: it is pure in-memory synchronization logic (a dispatcher thread,
//! channels, and a mutex-guarded scheduler). The same implementation runs
//! under `test-utils` and production; the only `test-utils`-gated surface
//! is [`ResourceCoordinator::pending_len`], a queue-depth introspection
//! helper tests use to assert fairness/backlog behavior without racing the
//! dispatcher thread via sleeps.

pub mod queue;

use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread::{available_parallelism, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{bounded, unbounded, Receiver, RecvTimeoutError, Sender};

use queue::{AdmitOutcome, JobRequest, SchedulerState};

const MIN_THROUGHPUT_SLOTS: usize = 2;
const MAX_THROUGHPUT_SLOTS: usize = 8;
const THROUGHPUT_CPU_DIVISOR: usize = 4;

fn throughput_slots_for_parallelism(available: Option<usize>) -> usize {
    available.map_or(MIN_THROUGHPUT_SLOTS, |available| {
        available
            .div_ceil(THROUGHPUT_CPU_DIVISOR)
            .clamp(MIN_THROUGHPUT_SLOTS, MAX_THROUGHPUT_SLOTS)
    })
}

/// Process-wide I/O admission capacity: one slot per four available logical
/// CPUs, bounded so small machines retain current throughput and large ones
/// cannot create an unbounded metadata/I/O burst.
pub fn max_throughput_slots() -> usize {
    static SLOTS: OnceLock<usize> = OnceLock::new();
    *SLOTS.get_or_init(|| {
        throughput_slots_for_parallelism(available_parallelism().ok().map(|value| value.get()))
    })
}

#[cfg(feature = "test-utils")]
pub fn throughput_slots_for_parallelism_for_tests(available: Option<usize>) -> usize {
    throughput_slots_for_parallelism(available)
}

/// A job's declared resource-admission class. A job may declare more than
/// one (e.g. archive creation needs both `Throughput` and `Cpu`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum JobClass {
    /// Interactive/metadata work: directory listing, tree children, item
    /// counts, focus reconcile. Wants to run promptly even while bulk work
    /// saturates throughput/CPU lanes.
    Latency,
    /// Bulk data movement: folder-size traversal, copy/move/archive I/O.
    Throughput,
    /// CPU-bound codec work: archive compression/extraction.
    Cpu,
}

/// Opaque, process-unique job identifier. Obtained from
/// [`ResourceCoordinator::submit_cancellable`] (or from a granted
/// [`JobHandle::id`]/[`AdmissionAwait::id`]) and used to
/// [`ResourceCoordinator::cancel`] a still-queued or already-granted job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId(u64);

/// A caller-declared job: the classes it needs admitted and the complete
/// set of resource keys (source/destination/etc.) it touches. Resource keys
/// should be the stable keys `VolumeRegistry` publishes
/// (`VolumeRecord::resource_key`); duplicate keys are collapsed and the
/// whole set is canonically sorted before reservation, so callers do not
/// need to pre-sort or dedupe themselves.
#[derive(Debug, Clone)]
pub struct JobSpec {
    pub classes: BTreeSet<JobClass>,
    pub resource_keys: Vec<String>,
    /// Optional coalescing identity: a second `submit` sharing the same key
    /// while an equivalent job is still queued or holding its grant is
    /// folded into the existing job's handle rather than being separately
    /// admitted. `None` disables coalescing for this job.
    pub coalesce_key: Option<String>,
}

impl JobSpec {
    pub fn new(
        classes: impl IntoIterator<Item = JobClass>,
        resource_keys: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            classes: classes.into_iter().collect(),
            resource_keys: queue::canonicalize_resource_keys(resource_keys),
            coalesce_key: None,
        }
    }

    pub fn with_coalesce_key(mut self, key: impl Into<String>) -> Self {
        self.coalesce_key = Some(key.into());
        self
    }
}

/// Why a [`ResourceCoordinator::submit`] call did not return a granted
/// [`JobHandle`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitError {
    /// The bounded wait queue was already full ([`queue::MAX_QUEUED_JOBS`]).
    QueueFull,
    /// The job (or, for a coalesced submission, the job it was folded onto)
    /// was cancelled before it could be granted.
    Cancelled,
    /// The coordinator was shut down while this job was queued or waiting.
    ShuttingDown,
}

/// A granted admission. Holding one means every declared class/resource
/// permit in the originating [`JobSpec`] is currently reserved. Dropping
/// (or explicitly calling [`JobHandle::release`]) releases the complete
/// reservation in one step — there is no way to partially release a
/// `JobHandle`, matching the all-or-none reservation contract.
pub struct JobHandle {
    id: JobId,
    coordinator: Arc<Inner>,
    released: bool,
}

impl JobHandle {
    pub fn id(&self) -> JobId {
        self.id
    }

    /// Releases every permit this handle holds. Idempotent: calling this
    /// and then letting the handle drop (or calling it twice) only releases
    /// once.
    pub fn release(mut self) {
        self.release_inner();
    }

    fn release_inner(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        self.coordinator.release(self.id);
    }
}

impl Drop for JobHandle {
    fn drop(&mut self) {
        // Deterministic release even on an early return, cancellation
        // race, or panic unwind: nothing about permit release depends on
        // the caller remembering to call `release()`.
        self.release_inner();
    }
}

/// One dispatcher-thread instruction.
enum Message {
    Submit {
        request: JobRequest,
        reply: Sender<Result<JobId, SubmitError>>,
    },
    /// Cancels a job that has not yet been granted (or is a coalesce
    /// target being cancelled, which also cancels everyone coalesced onto
    /// it). A no-op if the job was already granted or already finished —
    /// cancelling a granted job is `ResourceCoordinator::cancel` calling
    /// `release` instead, handled by the caller side, not this message.
    /// Replies with whether `id` itself was found queued (used to decide
    /// whether the caller should fall back to a release for an already
    /// granted job).
    CancelQueued {
        id: JobId,
        reply: Sender<bool>,
    },
    Release {
        id: JobId,
    },
    Shutdown {
        reply: Sender<()>,
    },
}

struct Inner {
    sender: Sender<Message>,
    next_id: AtomicU64,
    /// Guarded purely so `pending_len` (test-only introspection) can read
    /// queue depth without sending a round-trip message through the
    /// dispatcher; the dispatcher thread is still the only mutator. Only
    /// read outside production builds.
    #[cfg(feature = "test-utils")]
    state_for_introspection: Arc<Mutex<SchedulerState>>,
}

impl Inner {
    fn release(&self, id: JobId) {
        // Best-effort: if the dispatcher has already shut down the channel
        // is closed and there is nothing left to release against (shutdown
        // itself drains all occupancy).
        let _ = self.sender.send(Message::Release { id });
    }
}

/// The process-wide resource coordinator. Constructed once (e.g. as Tauri
/// managed state, `Arc<ResourceCoordinator>`, alongside `VolumeRegistry`)
/// and shared by every subsystem that submits work through it.
pub struct ResourceCoordinator {
    inner: Arc<Inner>,
    dispatcher: Mutex<Option<JoinHandle<()>>>,
}

impl Default for ResourceCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceCoordinator {
    pub fn new() -> Self {
        let (sender, receiver) = unbounded::<Message>();
        let state = Arc::new(Mutex::new(SchedulerState::default()));
        let dispatcher_state = Arc::clone(&state);

        let dispatcher = std::thread::Builder::new()
            .name("resource-coordinator".to_string())
            .spawn(move || run_dispatcher(receiver, dispatcher_state))
            .expect("failed to spawn resource coordinator dispatcher thread");

        Self {
            inner: Arc::new(Inner {
                sender,
                next_id: AtomicU64::new(1),
                #[cfg(feature = "test-utils")]
                state_for_introspection: state,
            }),
            dispatcher: Mutex::new(Some(dispatcher)),
        }
    }

    /// Submits `spec` and blocks the calling thread until it is granted, a
    /// concurrent [`ResourceCoordinator::cancel`] call for the returned id
    /// (obtained via the ticket race described below) removes it from the
    /// queue, or the coordinator shuts down.
    ///
    /// Because the [`JobId`] a caller would need to cancel with is only
    /// known once `submit` has enqueued the job, callers that need to
    /// cancel a job that might still be waiting should use
    /// [`ResourceCoordinator::submit_cancellable`] instead, which returns
    /// a handle to await that already knows the id.
    pub fn submit(&self, spec: JobSpec) -> Result<JobHandle, SubmitError> {
        self.submit_cancellable(spec).recv()
    }

    /// Like [`ResourceCoordinator::submit`], but returns immediately (before
    /// admission is decided) with an [`AdmissionAwait`] that exposes the
    /// assigned [`JobId`] and that the caller blocks on separately. This lets one
    /// thread submit while another thread cancels the same job by id while
    /// it may still be queued.
    pub fn submit_cancellable(&self, spec: JobSpec) -> AdmissionAwait {
        let id = JobId(self.inner.next_id.fetch_add(1, Ordering::SeqCst));
        let request = JobRequest {
            id,
            classes: spec.classes,
            resource_keys: spec.resource_keys,
            coalesce_key: spec.coalesce_key,
        };

        let (reply_sender, reply_receiver) = bounded(1);
        // A closed dispatcher channel (post-shutdown) means the coordinator
        // is gone; report that directly rather than blocking forever on a
        // reply that will never arrive.
        if self
            .inner
            .sender
            .send(Message::Submit {
                request,
                reply: reply_sender,
            })
            .is_err()
        {
            let (immediate_sender, immediate_receiver) = bounded(1);
            let _ = immediate_sender.send(Err(SubmitError::ShuttingDown));
            return AdmissionAwait {
                id,
                reply_receiver: immediate_receiver,
                coordinator: Arc::clone(&self.inner),
            };
        }

        AdmissionAwait {
            id,
            reply_receiver,
            coordinator: Arc::clone(&self.inner),
        }
    }

    /// Cancels `id`. If the job is still queued (not yet granted), it is
    /// removed from the wait list and any job coalesced onto it is
    /// cancelled too; the corresponding [`AdmissionAwait::recv`] call
    /// resolves to `Err(SubmitError::Cancelled)`. If the job was already
    /// granted, this releases its permits exactly as dropping the
    /// [`JobHandle`] would (idempotent with an explicit `release`/`drop`).
    /// Returns promptly regardless of how much bulk work is currently
    /// queued or running: cancellation only mutates the in-memory wait
    /// list/occupancy under the dispatcher's single lock, and is processed
    /// via the same unbounded control channel `submit`/`release` use rather
    /// than being blocked behind job *execution* (which happens entirely
    /// outside the coordinator).
    pub fn cancel(&self, id: JobId) {
        let (reply_sender, reply_receiver) = bounded(1);
        if self
            .inner
            .sender
            .send(Message::CancelQueued {
                id,
                reply: reply_sender,
            })
            .is_err()
        {
            return;
        }
        // If it was not queued (already granted, or unknown), fall back to
        // a release — this makes `cancel` safe to call regardless of
        // whether the caller knows the job's current phase.
        if let Ok(false) = reply_receiver.recv() {
            self.inner.release(id);
        }
    }

    /// Number of jobs currently queued (submitted, not yet granted).
    /// `test-utils`-gated: production code should not need to poll queue
    /// depth, and tests should prefer blocking on [`AdmissionAwait::recv`]
    /// where possible; this exists for the handful of fairness/backlog
    /// assertions that need to observe queue depth directly. Reads
    /// scheduler state directly rather than round-tripping through the
    /// dispatcher's message channel, so a caller that just called `submit`
    /// on another thread may briefly observe a count that has not yet
    /// caught up with that submission — callers needing a precise
    /// after-submit backlog size should poll with a bounded condition wait
    /// rather than treating one read as authoritative.
    #[cfg(feature = "test-utils")]
    pub fn pending_len(&self) -> usize {
        self.inner
            .state_for_introspection
            .lock()
            .expect("resource coordinator state lock")
            .queued_len()
    }

    /// Shuts the coordinator down: every queued job is cancelled, every
    /// granted job's occupancy is cleared, the dispatcher thread is told to
    /// stop, and this call blocks until that thread has actually exited.
    /// Safe to call more than once. After shutdown, `submit` returns
    /// `Err(SubmitError::ShuttingDown)` immediately instead of blocking.
    pub fn shutdown(&self) {
        let mut guard = self
            .dispatcher
            .lock()
            .expect("resource coordinator dispatcher lock");
        let Some(handle) = guard.take() else {
            // Already shut down.
            return;
        };

        let (reply_sender, reply_receiver) = bounded(1);
        if self
            .inner
            .sender
            .send(Message::Shutdown {
                reply: reply_sender,
            })
            .is_ok()
        {
            // Wait for the dispatcher to acknowledge it has drained all
            // state before joining, so no caller can observe a shutdown
            // that raced ahead of queued-job cancellation.
            let _ = reply_receiver.recv();
        }

        // Join unconditionally: even if the send failed (channel already
        // closed somehow), the dispatcher's receive loop exits when the
        // sender is dropped, so join still completes promptly.
        let _ = handle.join();
    }
}

impl Drop for ResourceCoordinator {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The caller-visible half of a submitted job: blocks on the eventual
/// admission decision.
pub struct AdmissionAwait {
    id: JobId,
    reply_receiver: Receiver<Result<JobId, SubmitError>>,
    coordinator: Arc<Inner>,
}

impl AdmissionAwait {
    pub fn id(&self) -> JobId {
        self.id
    }

    /// Blocks until the job is granted, cancelled, or the coordinator shuts
    /// down.
    pub fn recv(self) -> Result<JobHandle, SubmitError> {
        match self.reply_receiver.recv() {
            Ok(Ok(id)) => Ok(JobHandle {
                id,
                coordinator: self.coordinator,
                released: false,
            }),
            Ok(Err(error)) => Err(error),
            // The dispatcher thread dropped the reply sender without
            // answering, which only happens if it panicked or exited
            // mid-flight; treat that the same as an explicit shutdown so
            // callers never block forever.
            Err(_) => Err(SubmitError::ShuttingDown),
        }
    }

    /// Blocks up to `timeout` for the admission decision. Used by tests
    /// that need to assert a job is still pending (has *not* resolved yet)
    /// without an unbounded wait; production callers should generally
    /// prefer [`AdmissionAwait::recv`].
    pub fn recv_timeout(self, timeout: Duration) -> Result<Result<JobHandle, SubmitError>, Self> {
        match self.reply_receiver.recv_timeout(timeout) {
            Ok(Ok(id)) => Ok(Ok(JobHandle {
                id,
                coordinator: self.coordinator,
                released: false,
            })),
            Ok(Err(error)) => Ok(Err(error)),
            Err(RecvTimeoutError::Timeout) => Err(self),
            Err(RecvTimeoutError::Disconnected) => Ok(Err(SubmitError::ShuttingDown)),
        }
    }
}

/// The dispatcher thread's whole life: pull one control message at a time
/// (submit/cancel/release/shutdown), mutate `state` under its lock, and —
/// after every mutation — try to admit as many currently-runnable queued
/// jobs as possible before waiting for the next message. Single-threaded by
/// construction (only this thread ever touches `state`'s admission
/// decisions), which is what makes every reservation atomic without needing
/// per-call locking gymnastics from callers.
fn run_dispatcher(receiver: Receiver<Message>, state: Arc<Mutex<SchedulerState>>) {
    let mut dispatcher = Dispatcher {
        state,
        waiting_replies: HashMap::new(),
    };
    loop {
        // All senders dropped (coordinator dropped without an explicit
        // shutdown reaching the channel first) — clear state and exit.
        let Ok(message) = receiver.recv() else {
            dispatcher.lock_state().clear_all();
            return;
        };
        match dispatcher.handle(message) {
            AfterMessage::AdmitRunnable => dispatcher.admit_runnable(),
            AfterMessage::AwaitNext => {}
            AfterMessage::Stop => return,
        }
    }
}

type AdmissionReply = Sender<Result<JobId, SubmitError>>;

struct AdmittedJob {
    id: JobId,
    coalesced: Vec<JobId>,
}

struct Dispatcher {
    state: Arc<Mutex<SchedulerState>>,
    /// Pending reply channels for jobs that are queued but not yet granted,
    /// so a later admission or cancellation can find who to notify.
    waiting_replies: HashMap<JobId, AdmissionReply>,
}

enum AfterMessage {
    AdmitRunnable,
    AwaitNext,
    Stop,
}

impl Dispatcher {
    fn lock_state(&self) -> MutexGuard<'_, SchedulerState> {
        self.state.lock().expect("resource coordinator state lock")
    }

    fn handle(&mut self, message: Message) -> AfterMessage {
        match message {
            Message::Submit { request, reply } => self.submit(request, reply),
            Message::CancelQueued { id, reply } => {
                self.cancel_queued(id, &reply);
                AfterMessage::AdmitRunnable
            }
            Message::Release { id } => {
                self.lock_state().release_granted(id);
                AfterMessage::AdmitRunnable
            }
            Message::Shutdown { reply } => {
                self.shutdown(&reply);
                AfterMessage::Stop
            }
        }
    }

    fn submit(&mut self, request: JobRequest, reply: AdmissionReply) -> AfterMessage {
        let id = request.id;
        let mut state = self.state.lock().expect("resource coordinator state lock");
        let coalesce_target = request
            .coalesce_key
            .as_ref()
            .and_then(|coalesce_key| state.find_coalesce_target(coalesce_key))
            .filter(|&target_id| target_id != id);
        if let Some(target_id) = coalesce_target {
            let coalesced = CoalescedRequest {
                id,
                target_id,
                reply,
            };
            coalesce(&mut state, &mut self.waiting_replies, coalesced);
            return AfterMessage::AwaitNext;
        }
        if !state.enqueue(request) {
            let _ = reply.send(Err(SubmitError::QueueFull));
            return AfterMessage::AwaitNext;
        }
        self.waiting_replies.insert(id, reply);
        AfterMessage::AdmitRunnable
    }

    fn cancel_queued(&mut self, id: JobId, reply: &Sender<bool>) {
        let affected = self.lock_state().cancel_queued(id);
        let removed_self = affected.contains(&id);
        for affected_id in affected {
            self.reply_to(affected_id, Err(SubmitError::Cancelled));
        }
        let _ = reply.send(removed_self);
    }

    /// Notifies every still-waiting caller, not just ids `clear_all`
    /// reported: a coalesced job never held its own queue/grant slot but
    /// still has a caller blocked on `AdmissionAwait::recv`.
    fn shutdown(&mut self, reply: &Sender<()>) {
        self.lock_state().clear_all();
        for (_, sender) in self.waiting_replies.drain() {
            let _ = sender.send(Err(SubmitError::ShuttingDown));
        }
        let _ = reply.send(());
    }

    /// Greedily admits every queued job that can currently be granted, so a
    /// burst of independent resources all become runnable together. Every id
    /// coalesced onto a granted job resolves at the same moment as an equal
    /// live holder of its reservation.
    fn admit_runnable(&mut self) {
        while let Some(admitted) = self.admit_next() {
            self.reply_admitted(admitted);
        }
    }

    fn admit_next(&mut self) -> Option<AdmittedJob> {
        let mut state = self.lock_state();
        let AdmitOutcome::Granted(id) = state.try_admit_next() else {
            return None;
        };
        let coalesced = state.coalesced_onto_target(id);
        for _ in 0..coalesced.len() {
            state.add_granted_reference(id);
        }
        Some(AdmittedJob { id, coalesced })
    }

    fn reply_admitted(&mut self, admitted: AdmittedJob) {
        self.reply_to(admitted.id, Ok(admitted.id));
        for coalesced_id in admitted.coalesced {
            self.reply_to(coalesced_id, Ok(admitted.id));
        }
    }

    fn reply_to(&mut self, id: JobId, result: Result<JobId, SubmitError>) {
        if let Some(sender) = self.waiting_replies.remove(&id) {
            let _ = sender.send(result);
        }
    }
}

/// A target that already holds its reservation resolves the coalesced caller
/// at once as a live co-holder, so its later release does not pull the
/// reservation out from under the target. A still-queued target makes the
/// caller wait for the target's own admission, so it never observes a
/// `JobHandle` before the underlying resources are reserved.
struct CoalescedRequest {
    id: JobId,
    target_id: JobId,
    reply: AdmissionReply,
}

fn coalesce(
    state: &mut SchedulerState,
    waiting_replies: &mut HashMap<JobId, AdmissionReply>,
    coalesced: CoalescedRequest,
) {
    let CoalescedRequest {
        id,
        target_id,
        reply,
    } = coalesced;
    state.record_coalesced(id, target_id);
    if !state.is_granted(target_id) {
        waiting_replies.insert(id, reply);
        return;
    }
    state.add_granted_reference(target_id);
    let _ = reply.send(Ok(target_id));
}
