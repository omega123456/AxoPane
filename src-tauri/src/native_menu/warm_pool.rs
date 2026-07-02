use std::sync::OnceLock;

use rayon::{ThreadPool, ThreadPoolBuilder};

/// Lazily-initialized, process-lifetime rayon pool dedicated to driving
/// background native-menu warm batches off the Tauri IPC thread. Mirrors
/// `file_icons::icon_pool` but is scoped to native-menu warming.
///
/// Kept small (2 threads): the dedicated warm [`super::shell_executor::ShellExecutor`]
/// already serializes the actual shell enumeration on its own COM apartment
/// thread, so this pool's job is only to keep the `warm_native_menus` IPC
/// command non-blocking and to bound background scheduling — not to
/// parallelize shell work.
///
/// Compiled only in the non-`test-utils` build: under `test-utils` the
/// `warm_native_menus` command warms synchronously against the in-memory
/// fake provider (no COM, no real thread pool needed), matching the
/// established `request_icons` / `icon_pool` pattern.
pub fn warm_pool() -> Option<&'static ThreadPool> {
    static POOL: OnceLock<Option<ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        ThreadPoolBuilder::new()
            .num_threads(2)
            .thread_name(|index| format!("fe-native-menu-warm-{index}"))
            .build()
            .ok()
    })
    .as_ref()
}
