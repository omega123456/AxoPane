use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[cfg(all(target_os = "windows", not(feature = "test-utils")))]
use std::any::Any;
#[cfg(all(target_os = "windows", not(feature = "test-utils")))]
use std::sync::mpsc;
#[cfg(all(target_os = "windows", not(feature = "test-utils")))]
use std::sync::mpsc::Sender;
#[cfg(any(not(target_os = "windows"), feature = "test-utils"))]
use std::sync::Mutex;
#[cfg(all(target_os = "windows", not(feature = "test-utils")))]
use std::thread;

#[cfg(all(target_os = "windows", not(feature = "test-utils")))]
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

#[derive(Clone)]
pub struct ShellExecutor {
    inner: Arc<ShellExecutorInner>,
    execution_count: Arc<AtomicU64>,
}

enum ShellExecutorInner {
    #[cfg(any(not(target_os = "windows"), feature = "test-utils"))]
    Direct { serialized: bool, gate: Mutex<()> },
    #[cfg(all(target_os = "windows", not(feature = "test-utils")))]
    Threaded {
        sender: Sender<Box<dyn FnOnce() + Send>>,
    },
}

impl ShellExecutor {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(default_inner()),
            execution_count: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn execute<T, F>(&self, task: F) -> T
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        self.execution_count.fetch_add(1, Ordering::SeqCst);

        match self.inner.as_ref() {
            #[cfg(any(not(target_os = "windows"), feature = "test-utils"))]
            ShellExecutorInner::Direct { serialized, gate } => {
                if *serialized {
                    let _guard = gate.lock().expect("shell executor lock");
                    task()
                } else {
                    task()
                }
            }
            #[cfg(all(target_os = "windows", not(feature = "test-utils")))]
            ShellExecutorInner::Threaded { sender } => {
                let (result_tx, result_rx) = mpsc::sync_channel::<Box<dyn Any + Send>>(0);
                let wrapped = Box::new(move || {
                    let result = task();
                    result_tx
                        .send(Box::new(result))
                        .expect("shell executor result send");
                });
                sender.send(wrapped).expect("shell executor task send");
                *result_rx
                    .recv()
                    .expect("shell executor result receive")
                    .downcast::<T>()
                    .expect("shell executor result type")
            }
        }
    }

    pub fn execution_count(&self) -> u64 {
        self.execution_count.load(Ordering::SeqCst)
    }
}

impl Default for ShellExecutor {
    fn default() -> Self {
        Self::new()
    }
}

fn default_inner() -> ShellExecutorInner {
    #[cfg(all(target_os = "windows", not(feature = "test-utils")))]
    {
        let (sender, receiver) = mpsc::channel::<Box<dyn FnOnce() + Send>>();
        thread::Builder::new()
            .name("native-menu-shell".to_string())
            .spawn(move || {
                let initialized_com =
                    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() };
                while let Ok(task) = receiver.recv() {
                    task();
                }
                if initialized_com {
                    unsafe {
                        CoUninitialize();
                    }
                }
            })
            .expect("native menu shell executor thread");

        ShellExecutorInner::Threaded { sender }
    }

    #[cfg(any(not(target_os = "windows"), feature = "test-utils"))]
    {
        ShellExecutorInner::Direct {
            serialized: true,
            gate: Mutex::new(()),
        }
    }
}
