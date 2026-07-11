//! Bounded owner for short, latency-sensitive IPC work.
//!
//! Tauri command handlers only validate and enqueue here.  The fixed workers
//! acquire the process-wide `ResourceCoordinator` permit before touching the
//! filesystem or a platform API, so the IPC dispatcher is never used as an
//! unbounded blocking pool.

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

/// Cooperative cancellation shared with an owned IPC job. Platform helpers
/// with hard-kill semantics retain their own owner; ordinary filesystem work
/// observes this token at operation-defined checkpoints.
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

type Job = Box<dyn FnOnce() + Send + 'static>;

enum Message {
    Run(Job),
}

/// A process-lifetime bounded executor for one-request IPC work.  Long work
/// belongs to its domain owner (`OpsService`, size scheduler, native helper),
/// never this queue.
pub struct IpcExecutor {
    sender: Sender<Message>,
    /// A local receiver clone lets shutdown drain work that no worker has
    /// claimed yet. Keeping the sender alive for normal admission means a
    /// sender-disconnect sentinel cannot be relied on for worker shutdown.
    receiver: Receiver<Message>,
    workers: Mutex<Vec<JoinHandle<()>>>,
    /// Serializes the small check-and-enqueue admission window against
    /// shutdown's flag-and-drain transition. No request can be stranded in
    /// the queue after shutdown has completed its drain.
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

    /// Enqueues one bounded request.  A full queue is an explicit overload
    /// response, rather than silently creating another thread/pool.
    pub async fn latency<T, F>(&self, resource_key: String, work: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, String> + Send + 'static,
    {
        self.latency_cancellable(resource_key, Cancellation::default(), move |_| work())
            .await
    }

    pub async fn latency_cancellable<T, F>(
        &self,
        resource_key: String,
        cancellation: Cancellation,
        work: F,
    ) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(&Cancellation) -> Result<T, String> + Send + 'static,
    {
        self.latency_cancellable_with_deadline(resource_key, cancellation, IPC_DEADLINE, work)
            .await
    }

    /// Runs one bounded request with an owner-specific completion deadline.
    ///
    /// Most IPC work uses [`IPC_DEADLINE`]. A platform API that presents an
    /// OS-owned confirmation UI may need a longer, still finite deadline so
    /// the executor does not report a failure while that UI is active.
    pub async fn latency_cancellable_with_deadline<T, F>(
        &self,
        resource_key: String,
        cancellation: Cancellation,
        deadline: Duration,
        work: F,
    ) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(&Cancellation) -> Result<T, String> + Send + 'static,
    {
        let coordinator = Arc::clone(&self.coordinator);
        let worker_cancellation = cancellation.clone();
        let (reply_tx, reply_rx) = oneshot::channel();
        let message = Message::Run(Box::new(move || {
            if worker_cancellation.is_cancelled() {
                let _ = reply_tx.send(Err("IPC latency request cancelled".to_string()));
                return;
            }
            let result = coordinator
                .submit(JobSpec::new([JobClass::Latency], [resource_key]))
                .map_err(|error| format!("IPC latency admission failed: {error:?}"))
                .and_then(|_permit| {
                    if worker_cancellation.is_cancelled() {
                        Err("IPC latency request cancelled".to_string())
                    } else {
                        work(&worker_cancellation)
                    }
                });
            let _ = reply_tx.send(result);
        }));
        {
            let _lifecycle = self.lifecycle.lock().expect("IPC executor lifecycle lock");
            if self.shutting_down.load(Ordering::Acquire) {
                return Err("IPC executor is shutting down".to_string());
            }
            self.sender.try_send(message).map_err(|error| match error {
                crossbeam_channel::TrySendError::Full(_) => "IPC latency queue is full".to_string(),
                crossbeam_channel::TrySendError::Disconnected(_) => {
                    "IPC executor is shutting down".to_string()
                }
            })?;
        }

        match tokio::time::timeout(deadline, reply_rx).await {
            Ok(reply) => {
                reply.map_err(|_| "IPC executor stopped before completing request".to_string())?
            }
            Err(_) => {
                cancellation.cancel();
                Err("IPC latency request exceeded its deadline".to_string())
            }
        }
    }

    /// Runs an asynchronous platform contract on the same named, bounded
    /// owner as other latency IPC. This is intentionally distinct from merely
    /// declaring a Tauri command `async`: the future is entered only after
    /// queue admission, and its caller observes the executor deadline and
    /// cooperative cancellation token. Native work that cannot be killed
    /// (such as an already-issued LaunchServices request) may finish in the
    /// background after the caller has timed out, but it can neither publish
    /// a result nor start another request through this owner.
    pub async fn latency_async_cancellable<T, F, Fut>(
        &self,
        resource_key: String,
        cancellation: Cancellation,
        work: F,
    ) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(Cancellation) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, String>> + Send + 'static,
    {
        self.latency_async_cancellable_with_deadline(resource_key, cancellation, IPC_DEADLINE, work)
            .await
    }

    /// Asynchronous counterpart to [`Self::latency_cancellable_with_deadline`].
    pub async fn latency_async_cancellable_with_deadline<T, F, Fut>(
        &self,
        resource_key: String,
        cancellation: Cancellation,
        deadline: Duration,
        work: F,
    ) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(Cancellation) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, String>> + Send + 'static,
    {
        self.latency_cancellable_with_deadline(
            resource_key,
            cancellation,
            deadline,
            move |cancel| tauri::async_runtime::block_on(work(cancel.clone())),
        )
        .await
    }

    pub fn shutdown(&self) {
        {
            let _lifecycle = self.lifecycle.lock().expect("IPC executor lifecycle lock");
            if self.shutting_down.swap(true, Ordering::AcqRel) {
                return;
            }

            // Drop all not-yet-started jobs so their reply channels close
            // promptly. This is intentionally independent of queue capacity:
            // saturation cannot make shutdown lose a termination signal.
            while let Ok(message) = self.receiver.try_recv() {
                drop(message);
            }
        }
        let mut workers = self.workers.lock().expect("IPC executor workers lock");
        for worker in workers.drain(..) {
            let _ = worker.join();
        }
    }

    /// Observable queue depth for deterministic integration coverage and
    /// lightweight operational diagnostics. It is only a snapshot; admission
    /// still relies exclusively on `try_send` above.
    pub fn queued_len(&self) -> usize {
        self.sender.len()
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
        .spawn(move || {
            while !shutting_down.load(Ordering::Acquire) {
                let Ok(message) = receiver.recv_timeout(Duration::from_millis(25)) else {
                    continue;
                };
                match message {
                    Message::Run(job) => job(),
                }
            }
        })
        .expect("failed to spawn IPC latency worker")
}
