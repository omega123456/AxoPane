use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, Sender};
use tokio::sync::oneshot;

use crate::resource_coordinator::{JobClass, JobSpec, ResourceCoordinator};

pub const IPC_QUEUE_CAPACITY: usize = 32;
pub const IPC_WORKER_COUNT: usize = 4;
pub const IPC_DEADLINE: Duration = Duration::from_secs(2);
const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(25);
const REQUEST_CANCELLED: &str = "IPC latency request cancelled";
const SHUTTING_DOWN_ERROR: &str = "IPC executor is shutting down";

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

pub struct LatencyRequest {
    pub resource_key: String,
    pub cancellation: Cancellation,
    pub deadline: Duration,
}

impl LatencyRequest {
    pub fn new(resource_key: String) -> Self {
        Self::cancellable(resource_key, Cancellation::default())
    }

    pub fn cancellable(resource_key: String, cancellation: Cancellation) -> Self {
        Self {
            resource_key,
            cancellation,
            deadline: IPC_DEADLINE,
        }
    }

    pub fn with_deadline(self, deadline: Duration) -> Self {
        Self { deadline, ..self }
    }
}

type Job = Box<dyn FnOnce() + Send + 'static>;

enum Message {
    Run(Job),
}

pub struct IpcExecutor {
    sender: Sender<Message>,
    receiver: Receiver<Message>,
    workers: Mutex<Vec<JoinHandle<()>>>,
    lifecycle: Mutex<()>,
    coordinator: Arc<ResourceCoordinator>,
    shutting_down: Arc<AtomicBool>,
}

impl IpcExecutor {
    pub fn new(coordinator: Arc<ResourceCoordinator>) -> Self {
        let (sender, receiver) = bounded(IPC_QUEUE_CAPACITY);
        let shutting_down = Arc::new(AtomicBool::new(false));
        let workers = (0..IPC_WORKER_COUNT)
            .map(|index| spawn_worker(index, receiver.clone(), Arc::clone(&shutting_down)))
            .collect();
        Self {
            sender,
            receiver,
            workers: Mutex::new(workers),
            lifecycle: Mutex::new(()),
            coordinator,
            shutting_down,
        }
    }

    pub async fn latency<T, F>(&self, resource_key: String, work: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, String> + Send + 'static,
    {
        self.latency_cancellable(LatencyRequest::new(resource_key), move |_| work())
            .await
    }

    pub async fn latency_cancellable<T, F>(
        &self,
        request: LatencyRequest,
        work: F,
    ) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(&Cancellation) -> Result<T, String> + Send + 'static,
    {
        let LatencyRequest {
            resource_key,
            cancellation,
            deadline,
        } = request;
        let coordinator = Arc::clone(&self.coordinator);
        let worker_cancellation = cancellation.clone();
        let (reply_sender, reply_receiver) = oneshot::channel();
        let message = Message::Run(Box::new(move || {
            let job = AdmittedJob {
                coordinator: &coordinator,
                resource_key,
                cancellation: &worker_cancellation,
            };
            let result = run_admitted(job, work);
            let _ = reply_sender.send(result);
        }));
        self.admit(message)?;

        match tokio::time::timeout(deadline, reply_receiver).await {
            Ok(reply) => {
                reply.map_err(|_| "IPC executor stopped before completing request".to_string())?
            }
            Err(_) => {
                cancellation.cancel();
                Err("IPC latency request exceeded its deadline".to_string())
            }
        }
    }

    pub async fn latency_async_cancellable<T, F, Fut>(
        &self,
        request: LatencyRequest,
        work: F,
    ) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(Cancellation) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, String>> + Send + 'static,
    {
        self.latency_cancellable(request, move |cancel| {
            tauri::async_runtime::block_on(work(cancel.clone()))
        })
        .await
    }

    fn admit(&self, message: Message) -> Result<(), String> {
        let _lifecycle = self.lifecycle.lock().expect("IPC executor lifecycle lock");
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(SHUTTING_DOWN_ERROR.to_string());
        }
        self.sender.try_send(message).map_err(admission_error)
    }

    pub fn shutdown(&self) {
        let newly_stopped = self.stop_admission();
        if !newly_stopped {
            return;
        }
        let mut workers = self.workers.lock().expect("IPC executor workers lock");
        for worker in workers.drain(..) {
            let _ = worker.join();
        }
    }

    fn stop_admission(&self) -> bool {
        let _lifecycle = self.lifecycle.lock().expect("IPC executor lifecycle lock");
        if self.shutting_down.swap(true, Ordering::AcqRel) {
            return false;
        }
        while let Ok(message) = self.receiver.try_recv() {
            drop(message);
        }
        true
    }

    pub fn queued_len(&self) -> usize {
        self.sender.len()
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::Acquire)
    }
}

impl Drop for IpcExecutor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn spawn_worker(
    index: usize,
    receiver: Receiver<Message>,
    shutting_down: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name(format!("ipc-latency-{index}"))
        .spawn(move || run_worker_loop(&receiver, &shutting_down))
        .expect("failed to spawn IPC latency worker")
}

fn run_worker_loop(receiver: &Receiver<Message>, shutting_down: &AtomicBool) {
    while !shutting_down.load(Ordering::Acquire) {
        let Ok(message) = receiver.recv_timeout(SHUTDOWN_POLL_INTERVAL) else {
            continue;
        };
        if shutting_down.load(Ordering::Acquire) {
            drop(message);
            break;
        }
        match message {
            Message::Run(job) => job(),
        }
    }
}

fn admission_error(error: crossbeam_channel::TrySendError<Message>) -> String {
    match error {
        crossbeam_channel::TrySendError::Full(_) => "IPC latency queue is full".to_string(),
        crossbeam_channel::TrySendError::Disconnected(_) => SHUTTING_DOWN_ERROR.to_string(),
    }
}

struct AdmittedJob<'job> {
    coordinator: &'job ResourceCoordinator,
    resource_key: String,
    cancellation: &'job Cancellation,
}

fn run_admitted<T>(
    job: AdmittedJob<'_>,
    work: impl FnOnce(&Cancellation) -> Result<T, String>,
) -> Result<T, String> {
    let AdmittedJob {
        coordinator,
        resource_key,
        cancellation,
    } = job;
    if cancellation.is_cancelled() {
        return Err(REQUEST_CANCELLED.to_string());
    }
    let _permit = coordinator
        .submit(JobSpec::new([JobClass::Latency], [resource_key]))
        .map_err(|error| format!("IPC latency admission failed: {error:?}"))?;
    if cancellation.is_cancelled() {
        return Err(REQUEST_CANCELLED.to_string());
    }
    work(cancellation)
}
