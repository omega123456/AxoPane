//! Supervision boundary for untrusted Windows shell extensions.

use std::time::Duration;

#[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
use std::io::{BufReader, BufWriter};
#[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
use std::process::{Child, Command, Stdio};
#[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
use std::sync::mpsc;
#[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
use std::sync::OnceLock;
#[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
use std::sync::{Arc, Mutex};
#[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
use std::thread;

#[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
use super::helper_entry::HELPER_ARGUMENT;
#[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
use super::helper_protocol::{read_frame, write_frame, HelperRequest, HelperResponse};
use super::helper_protocol::{HelperOperation, HelperResult};

pub const DISCOVERY_DEADLINE: Duration = Duration::from_secs(2);
pub const INVOCATION_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelperRole {
    Interactive,
    Warm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperFailure {
    Unsupported,
    Deadline,
    Crashed,
    Malformed,
}

/// One lazily-created helper per role. Calls for the same role are serialized
/// because shell extensions are apartment-bound; warm and interactive roles
/// have independent processes and therefore cannot starve one another.
#[derive(Default)]
pub struct HelperSupervisor {
    #[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
    next_request_id: AtomicU64,
    #[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
    interactive: Mutex<Option<PersistentHelper>>,
    #[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
    warm: Mutex<Option<PersistentHelper>>,
}

/// The app-wide Windows shell boundary. Keeping this singleton means every
/// native-menu command uses the same two persistent, independently supervised
/// helpers instead of accidentally creating a one-shot supervisor per call.
#[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
pub fn shared() -> &'static HelperSupervisor {
    static SUPERVISOR: OnceLock<HelperSupervisor> = OnceLock::new();
    SUPERVISOR.get_or_init(HelperSupervisor::default)
}

pub struct HelperCall {
    pub operation: HelperOperation,
    pub deadline: Duration,
}

impl HelperSupervisor {
    pub fn call(&self, role: HelperRole, call: HelperCall) -> Result<HelperResult, HelperFailure> {
        #[cfg(any(feature = "test-utils", not(target_os = "windows")))]
        {
            let _ = (role, call);
            Err(HelperFailure::Unsupported)
        }
        #[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
        {
            self.call_persistent_helper(role, call)
        }
    }

    #[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
    fn call_persistent_helper(
        &self,
        role: HelperRole,
        call: HelperCall,
    ) -> Result<HelperResult, HelperFailure> {
        let HelperCall {
            operation,
            deadline,
        } = call;
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed) + 1;
        let mut helper = self.helper_slot(role).lock().expect("native helper lock");
        if helper.is_none() {
            *helper = Some(PersistentHelper::spawn(role)?);
        }
        let outcome = helper.as_ref().expect("native helper initialized").call(
            HelperRequest {
                request_id,
                operation,
            },
            deadline,
        );
        if outcome.is_err() {
            // A deadline, protocol error, or child crash contaminates
            // only this role. The next request lazily creates a fresh
            // helper while the other role remains untouched.
            helper.take();
        }
        outcome
    }

    #[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
    fn helper_slot(&self, role: HelperRole) -> &Mutex<Option<PersistentHelper>> {
        match role {
            HelperRole::Interactive => &self.interactive,
            HelperRole::Warm => &self.warm,
        }
    }
}

#[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
struct PersistentHelper {
    requests: mpsc::SyncSender<HelperRequest>,
    responses: mpsc::Receiver<Result<HelperResponse, HelperFailure>>,
    child: Arc<Mutex<Child>>,
}

#[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
impl PersistentHelper {
    fn spawn(role: HelperRole) -> Result<Self, HelperFailure> {
        let mut child = Command::new(std::env::current_exe().map_err(|_| HelperFailure::Crashed)?)
            .arg(HELPER_ARGUMENT)
            .arg(match role {
                HelperRole::Interactive => "--role=interactive",
                HelperRole::Warm => "--role=warm",
            })
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| HelperFailure::Crashed)?;
        let stdin = child.stdin.take().ok_or(HelperFailure::Crashed)?;
        let stdout = child.stdout.take().ok_or(HelperFailure::Crashed)?;
        let child = Arc::new(Mutex::new(child));
        let (request_sender, request_receiver) = mpsc::sync_channel(1);
        let (response_sender, response_receiver) = mpsc::sync_channel(1);
        thread::spawn(move || forward_requests(&request_receiver, stdin));
        thread::spawn(move || forward_responses(&response_sender, stdout));
        Ok(Self {
            requests: request_sender,
            responses: response_receiver,
            child,
        })
    }

    fn call(
        &self,
        request: HelperRequest,
        deadline: Duration,
    ) -> Result<HelperResult, HelperFailure> {
        let request_id = request.request_id;
        self.requests
            .send(request)
            .map_err(|_| HelperFailure::Crashed)?;
        match self.responses.recv_timeout(deadline) {
            Ok(Ok(response)) if response.request_id == request_id => Ok(response.result),
            Ok(Ok(_)) | Ok(Err(_)) => Err(HelperFailure::Malformed),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(HelperFailure::Deadline),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(HelperFailure::Crashed),
        }
    }
}

#[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
fn forward_requests(
    request_receiver: &mpsc::Receiver<HelperRequest>,
    stdin: std::process::ChildStdin,
) {
    let mut writer = BufWriter::new(stdin);
    while let Ok(request) = request_receiver.recv() {
        if write_frame(&mut writer, &request).is_err() {
            break;
        }
    }
}

/// Stops after the first malformed frame, once it has been reported.
#[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
fn forward_responses(
    response_sender: &mpsc::SyncSender<Result<HelperResponse, HelperFailure>>,
    stdout: std::process::ChildStdout,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        let response = read_frame(&mut reader).map_err(|_| HelperFailure::Malformed);
        let failed = response.is_err();
        let receiver_gone = response_sender.send(response).is_err();
        if receiver_gone || failed {
            break;
        }
    }
}

#[cfg(all(not(feature = "test-utils"), target_os = "windows"))]
impl Drop for PersistentHelper {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
