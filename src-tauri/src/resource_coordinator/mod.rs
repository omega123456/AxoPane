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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum JobClass {
    Latency,
    Throughput,
    Cpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId(u64);

#[derive(Debug, Clone)]
pub struct JobSpec {
    pub classes: BTreeSet<JobClass>,
    pub resource_keys: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitError {
    QueueFull,
    Cancelled,
    ShuttingDown,
}

pub struct JobHandle {
    id: JobId,
    coordinator: Arc<Inner>,
    released: bool,
}

impl JobHandle {
    pub fn id(&self) -> JobId {
        self.id
    }

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
        self.release_inner();
    }
}

enum Message {
    Submit {
        request: JobRequest,
        reply: Sender<Result<JobId, SubmitError>>,
    },
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
    #[cfg(feature = "test-utils")]
    state_for_introspection: Arc<Mutex<SchedulerState>>,
}

impl Inner {
    fn release(&self, id: JobId) {
        let _ = self.sender.send(Message::Release { id });
    }
}

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

    pub fn submit(&self, spec: JobSpec) -> Result<JobHandle, SubmitError> {
        self.submit_cancellable(spec).recv()
    }

    pub fn submit_cancellable(&self, spec: JobSpec) -> AdmissionAwait {
        let id = JobId(self.inner.next_id.fetch_add(1, Ordering::SeqCst));
        let request = JobRequest {
            id,
            classes: spec.classes,
            resource_keys: spec.resource_keys,
            coalesce_key: spec.coalesce_key,
        };

        let (reply_sender, reply_receiver) = bounded(1);
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
        if let Ok(false) = reply_receiver.recv() {
            self.inner.release(id);
        }
    }

    #[cfg(feature = "test-utils")]
    pub fn pending_len(&self) -> usize {
        self.inner
            .state_for_introspection
            .lock()
            .expect("resource coordinator state lock")
            .queued_len()
    }

    pub fn shutdown(&self) {
        let mut guard = self
            .dispatcher
            .lock()
            .expect("resource coordinator dispatcher lock");
        let Some(handle) = guard.take() else {
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
            let _ = reply_receiver.recv();
        }

        let _ = handle.join();
    }
}

impl Drop for ResourceCoordinator {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub struct AdmissionAwait {
    id: JobId,
    reply_receiver: Receiver<Result<JobId, SubmitError>>,
    coordinator: Arc<Inner>,
}

impl AdmissionAwait {
    pub fn id(&self) -> JobId {
        self.id
    }

    pub fn recv(self) -> Result<JobHandle, SubmitError> {
        match self.reply_receiver.recv() {
            Ok(Ok(id)) => Ok(JobHandle {
                id,
                coordinator: self.coordinator,
                released: false,
            }),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(SubmitError::ShuttingDown),
        }
    }

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

fn run_dispatcher(receiver: Receiver<Message>, state: Arc<Mutex<SchedulerState>>) {
    let mut dispatcher = Dispatcher {
        state,
        waiting_replies: HashMap::new(),
    };
    loop {
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

    fn shutdown(&mut self, reply: &Sender<()>) {
        self.lock_state().clear_all();
        for (_, sender) in self.waiting_replies.drain() {
            let _ = sender.send(Err(SubmitError::ShuttingDown));
        }
        let _ = reply.send(());
    }

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
