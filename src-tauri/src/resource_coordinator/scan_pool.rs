use std::ops::Range;
use std::sync::OnceLock;

use rayon::prelude::*;
use rayon::{ThreadPool, ThreadPoolBuilder};

use super::parallelism::scan_workers;

fn scan_pool() -> Option<&'static ThreadPool> {
    static POOL: OnceLock<Option<ThreadPool>> = OnceLock::new();
    POOL.get_or_init(|| {
        ThreadPoolBuilder::new()
            .num_threads(scan_workers())
            .thread_name(|index| format!("scan-worker-{index}"))
            .build()
            .ok()
    })
    .as_ref()
}

pub fn run_on_scan_workers<Output: Send>(work: impl FnOnce() -> Output + Send) -> Output {
    let Some(pool) = scan_pool() else {
        return work();
    };
    pool.install(work)
}

pub fn map_parts_in_order<Part, Output>(
    parts: Vec<Part>,
    work: impl Fn(Part) -> Output + Sync + Send,
) -> Vec<Output>
where
    Part: Send,
    Output: Send,
{
    run_on_scan_workers(|| parts.into_par_iter().map(work).collect())
}

pub fn split_range(ids: Range<u32>) -> Vec<Range<u32>> {
    let part_len = ids.len().div_ceil(scan_workers()).max(1);
    let part_len = u32::try_from(part_len).unwrap_or(u32::MAX);
    let starts = (ids.start..ids.end).step_by(part_len as usize);
    starts
        .map(|start| start..start.saturating_add(part_len).min(ids.end))
        .collect()
}

pub fn split_slice<Item>(items: &[Item]) -> Vec<&[Item]> {
    let part_len = items.len().div_ceil(scan_workers()).max(1);
    items.chunks(part_len).collect()
}
