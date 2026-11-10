use std::sync::OnceLock;
use std::thread::available_parallelism;

#[derive(Clone, Copy)]
struct CpuShare {
    cpus_per_unit: usize,
    minimum: usize,
    maximum: usize,
}

impl CpuShare {
    fn units_for(self, available_cpus: Option<usize>) -> usize {
        let Some(available_cpus) = available_cpus else {
            return self.minimum;
        };
        available_cpus
            .div_ceil(self.cpus_per_unit)
            .clamp(self.minimum, self.maximum)
    }
}

fn detected_cpus() -> Option<usize> {
    available_parallelism().ok().map(|cpus| cpus.get())
}

fn detected_units(share: CpuShare, cached_units: &OnceLock<usize>) -> usize {
    *cached_units.get_or_init(|| share.units_for(detected_cpus()))
}

const THROUGHPUT_SLOTS: CpuShare = CpuShare {
    cpus_per_unit: 4,
    minimum: 2,
    maximum: 8,
};

const WALKER_JOB_THREADS: CpuShare = CpuShare {
    cpus_per_unit: 2,
    minimum: 2,
    maximum: 8,
};

const WALKER_HELPER_BUDGET: CpuShare = CpuShare {
    cpus_per_unit: 1,
    minimum: 2,
    maximum: 16,
};

const SCAN_WORKERS: CpuShare = CpuShare {
    cpus_per_unit: 2,
    minimum: 1,
    maximum: 8,
};

pub fn max_throughput_slots() -> usize {
    static SLOTS: OnceLock<usize> = OnceLock::new();
    detected_units(THROUGHPUT_SLOTS, &SLOTS)
}

pub fn walker_job_threads() -> usize {
    static THREADS: OnceLock<usize> = OnceLock::new();
    detected_units(WALKER_JOB_THREADS, &THREADS)
}

pub fn walker_helper_budget() -> usize {
    static HELPERS: OnceLock<usize> = OnceLock::new();
    detected_units(WALKER_HELPER_BUDGET, &HELPERS)
}

pub fn scan_workers() -> usize {
    static WORKERS: OnceLock<usize> = OnceLock::new();
    detected_units(SCAN_WORKERS, &WORKERS)
}

#[cfg(feature = "test-utils")]
pub fn throughput_slots_for_parallelism_for_tests(available_cpus: Option<usize>) -> usize {
    THROUGHPUT_SLOTS.units_for(available_cpus)
}

#[cfg(feature = "test-utils")]
pub fn walker_job_threads_for_tests(available_cpus: Option<usize>) -> usize {
    WALKER_JOB_THREADS.units_for(available_cpus)
}

#[cfg(feature = "test-utils")]
pub fn walker_helper_budget_for_tests(available_cpus: Option<usize>) -> usize {
    WALKER_HELPER_BUDGET.units_for(available_cpus)
}

#[cfg(feature = "test-utils")]
pub fn scan_workers_for_tests(available_cpus: Option<usize>) -> usize {
    SCAN_WORKERS.units_for(available_cpus)
}
