use std::sync::OnceLock;

use rayon::{ThreadPool, ThreadPoolBuilder};

const WARM_POOL_THREADS: usize = 2;

pub fn warm_pool() -> Option<&'static ThreadPool> {
    static POOL: OnceLock<Option<ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        ThreadPoolBuilder::new()
            .num_threads(WARM_POOL_THREADS)
            .thread_name(|index| format!("fe-native-menu-warm-{index}"))
            .build()
            .ok()
    })
    .as_ref()
}
