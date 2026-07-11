//! Scheduling primitives for [`super::ResourceCoordinator`]: the fixed
//! global lane capacities, canonical resource-key normalization, and the
//! pure admission-decision logic (`SchedulerState`) the coordinator's
//! dispatcher thread drives.
//!
//! This module intentionally contains no threads, channels, or I/O — it is
//! the deterministic bookkeeping the coordinator's single dispatcher thread
//! mutates under one lock, kept separate so the admission/fairness/deadlock
//! -freedom properties can be reasoned about (and unit-exercised through the
//! coordinator's public API) without any concurrency in the type itself.

use std::collections::{BTreeSet, HashMap, VecDeque};

use super::{JobClass, JobId};

/// Fixed global concurrency cap for latency-sensitive jobs (directory
/// listing, tree children, item counts, focus reconcile). Kept generous
/// relative to throughput/CPU so interactive metadata requests are rarely
/// queued behind bulk work.
pub const MAX_LATENCY_SLOTS: usize = 4;

/// Fixed global concurrency cap for throughput jobs (folder size traversal,
/// copy/move/archive I/O).
pub const MAX_THROUGHPUT_SLOTS: usize = 2;

/// Fixed global concurrency cap for CPU-bound jobs (archive
/// compression/extraction codec work).
pub const MAX_CPU_SLOTS: usize = 2;

/// Maximum number of jobs that may be queued (submitted but not yet
/// admitted) at once. Submission beyond this bound is rejected immediately
/// rather than growing the queue without limit.
pub const MAX_QUEUED_JOBS: usize = 256;

/// A resource may hold at most this many concurrently granted slots per job
/// class. The plan fixes this at one: a resource occupies at most one
/// latency slot and one throughput slot at a time, so a saturated bulk
/// throughput job on a volume cannot also consume every latency slot for
/// that same volume.
pub const MAX_SLOTS_PER_RESOURCE_PER_CLASS: usize = 1;

/// Normalizes a caller-declared resource-key set into the canonical form the
/// coordinator reserves against: duplicates collapsed, and sorted so that a
/// job declaring `{A, B}` and a job declaring `{B, A}` always acquire their
/// keys in the same order. This canonical ordering is what prevents
/// classic lock-ordering deadlocks between opposing-direction jobs (e.g. a
/// copy from A to B racing a copy from B to A) — both jobs contend for the
/// same globally-agreed acquisition order instead of each blocking on the
/// other's first-held key.
pub fn canonicalize_resource_keys(keys: impl IntoIterator<Item = String>) -> Vec<String> {
    let set: BTreeSet<String> = keys.into_iter().collect();
    set.into_iter().collect()
}

/// One queued or granted job's admission requirements, already normalized.
#[derive(Debug, Clone)]
pub struct JobRequest {
    pub id: JobId,
    pub classes: BTreeSet<JobClass>,
    pub resource_keys: Vec<String>,
    /// Optional coalescing identity. A second submission sharing the same
    /// key while an equivalent job is still queued or running is folded
    /// into the existing job instead of being separately admitted.
    pub coalesce_key: Option<String>,
}

/// Per-class global occupancy plus per-resource-per-class occupancy. Pure
/// bookkeeping: does not itself decide fairness ordering (that is the
/// waiting queue's job) — it only answers "would granting `request` exceed
/// any cap right now".
#[derive(Debug, Default)]
struct Occupancy {
    global: HashMap<JobClass, usize>,
    per_resource: HashMap<(String, JobClass), usize>,
}

impl Occupancy {
    fn global_cap(class: JobClass) -> usize {
        match class {
            JobClass::Latency => MAX_LATENCY_SLOTS,
            JobClass::Throughput => MAX_THROUGHPUT_SLOTS,
            JobClass::Cpu => MAX_CPU_SLOTS,
        }
    }

    /// Whether granting `request` right now would keep every global class
    /// cap and every per-resource-per-class cap within bounds. Resource
    /// keys are only rate-limited for `Latency`/`Throughput` classes per the
    /// plan ("at most one latency-sensitive slot AND one throughput slot
    /// occupied per stable resource key"); `Cpu` admission is a pure global
    /// lane with no per-resource sub-limit.
    fn can_grant(&self, request: &JobRequest) -> bool {
        for &class in &request.classes {
            let current_global = *self.global.get(&class).unwrap_or(&0);
            if current_global + 1 > Self::global_cap(class) {
                return false;
            }

            if matches!(class, JobClass::Latency | JobClass::Throughput) {
                for key in &request.resource_keys {
                    let current_per_resource =
                        *self.per_resource.get(&(key.clone(), class)).unwrap_or(&0);
                    if current_per_resource + 1 > MAX_SLOTS_PER_RESOURCE_PER_CLASS {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn grant(&mut self, request: &JobRequest) {
        for &class in &request.classes {
            *self.global.entry(class).or_insert(0) += 1;
            if matches!(class, JobClass::Latency | JobClass::Throughput) {
                for key in &request.resource_keys {
                    *self.per_resource.entry((key.clone(), class)).or_insert(0) += 1;
                }
            }
        }
    }

    fn release(&mut self, request: &JobRequest) {
        for &class in &request.classes {
            if let Some(count) = self.global.get_mut(&class) {
                *count = count.saturating_sub(1);
            }
            if matches!(class, JobClass::Latency | JobClass::Throughput) {
                for key in &request.resource_keys {
                    if let Some(count) = self.per_resource.get_mut(&(key.clone(), class)) {
                        *count = count.saturating_sub(1);
                    }
                }
            }
        }
    }
}

/// Outcome of attempting to admit the next runnable job from the waiting
/// queue.
pub enum AdmitOutcome {
    /// No queued job could be granted right now (queue empty, or every
    /// queued job is blocked on a saturated class/resource).
    NoneRunnable,
    /// `id` was granted; its reservation is now reflected in occupancy.
    Granted(JobId),
}

/// Pure scheduler state: the waiting queue (FIFO submission order) plus
/// current occupancy. Owns no threads or channels — the coordinator's
/// dispatcher thread is the sole mutator, always under one lock, which is
/// what makes the all-or-none reservation decision atomic.
#[derive(Default)]
pub struct SchedulerState {
    waiting: VecDeque<JobRequest>,
    /// Jobs granted right now, keyed by id, so `release` can look up what to
    /// give back without the caller re-declaring its requirements.
    granted: HashMap<JobId, JobRequest>,
    /// Live-holder count per granted id. A coalesced caller's `JobHandle`
    /// carries the *target's* id, not its own (see `record_coalesced`), so
    /// every co-holder releases through the exact same id. Without this
    /// count, whichever holder dropped first would release the whole
    /// reservation out from under every other still-alive holder. Occupancy
    /// is only actually given back once this reaches zero.
    granted_refcount: HashMap<JobId, usize>,
    occupancy: Occupancy,
    /// Ids currently coalesced onto another (still-queued or running) job's
    /// coalesce key, so a later cancel/release for the coalesced id is a
    /// no-op against occupancy (it never held its own reservation).
    coalesced_onto: HashMap<JobId, JobId>,
}

impl SchedulerState {
    pub fn queued_len(&self) -> usize {
        self.waiting.len()
    }

    /// Looks for an already-queued-or-granted job sharing `coalesce_key` and
    /// returns its id if found, so the caller can fold the new submission
    /// into the existing one instead of enqueueing a duplicate.
    pub fn find_coalesce_target(&self, coalesce_key: &str) -> Option<JobId> {
        self.granted
            .values()
            .find(|request| request.coalesce_key.as_deref() == Some(coalesce_key))
            .map(|request| request.id)
            .or_else(|| {
                self.waiting
                    .iter()
                    .find(|request| request.coalesce_key.as_deref() == Some(coalesce_key))
                    .map(|request| request.id)
            })
    }

    /// Records that `coalesced_id` was folded onto `target_id` rather than
    /// queued independently.
    pub fn record_coalesced(&mut self, coalesced_id: JobId, target_id: JobId) {
        self.coalesced_onto.insert(coalesced_id, target_id);
    }

    /// Whether `id` currently holds a granted reservation (as opposed to
    /// still being queued). Used to decide whether a coalescing submission
    /// can reply immediately (target already granted) or must wait for the
    /// target's own admission (target still queued) — the latter matters
    /// so a coalesced caller never receives a `JobHandle` before its
    /// resources are actually reserved.
    pub fn is_granted(&self, id: JobId) -> bool {
        self.granted.contains_key(&id)
    }

    /// Every id currently coalesced onto `target_id`. Used by the
    /// dispatcher to notify coalesced callers at the same moment the
    /// target itself is granted.
    pub fn coalesced_onto_target(&self, target_id: JobId) -> Vec<JobId> {
        self.coalesced_onto
            .iter()
            .filter(|(_, target)| **target == target_id)
            .map(|(coalesced, _)| *coalesced)
            .collect()
    }

    /// Enqueues `request` at the back of the FIFO waiting list. Returns
    /// `false` (and does not enqueue) if the bounded queue is already full.
    pub fn enqueue(&mut self, request: JobRequest) -> bool {
        if self.waiting.len() >= MAX_QUEUED_JOBS {
            return false;
        }
        self.waiting.push_back(request);
        true
    }

    /// Removes a still-queued job by id (used by cancellation of a job that
    /// has not yet been granted). Returns every id that must be notified as
    /// cancelled: `id` itself (if it was queued) plus any job that had been
    /// coalesced onto `id`, since cancelling the coalescing target cannot
    /// leave a coalesced caller waiting forever on a job that no longer
    /// exists. Cancellation never has to wait behind other queued work: it
    /// only mutates this in-memory deque under the same lock every other
    /// operation uses, so it is always O(n) and immediate, never blocked on
    /// bulk job *execution* (which happens entirely outside the
    /// coordinator, after admission).
    pub fn cancel_queued(&mut self, id: JobId) -> Vec<JobId> {
        let before = self.waiting.len();
        self.waiting.retain(|request| request.id != id);
        let was_queued = self.waiting.len() != before;

        let coalesced_onto_id: Vec<JobId> = self
            .coalesced_onto
            .iter()
            .filter(|(_, target)| **target == id)
            .map(|(coalesced, _)| *coalesced)
            .collect();
        for coalesced in &coalesced_onto_id {
            self.coalesced_onto.remove(coalesced);
        }
        self.coalesced_onto.retain(|coalesced, _| *coalesced != id);

        let mut affected = Vec::new();
        if was_queued {
            affected.push(id);
        }
        affected.extend(coalesced_onto_id);
        affected
    }

    /// Scans the waiting queue in FIFO order but admits the *first job that
    /// can actually be granted*, not strictly the head. This is the fair
    /// wake-up mechanism: without it, a queue full of throughput jobs for
    /// resource X would permanently block a later latency job for resource
    /// Y that has a free slot, even though nothing prevents Y's job from
    /// running immediately. Scanning past blocked head-of-line jobs lets
    /// independent resources make progress under the shared global caps
    /// without starving each other.
    pub fn try_admit_next(&mut self) -> AdmitOutcome {
        let Some(index) = self
            .waiting
            .iter()
            .position(|request| self.occupancy.can_grant(request))
        else {
            return AdmitOutcome::NoneRunnable;
        };

        // `VecDeque::remove` preserves relative order of remaining items,
        // so jobs behind the admitted one keep their original FIFO position
        // for future admission attempts (no reordering beyond removing the
        // admitted entry).
        let request = self.waiting.remove(index).expect("index was just found");
        self.occupancy.grant(&request);
        let id = request.id;
        self.granted.insert(id, request);
        self.granted_refcount.insert(id, 1);
        AdmitOutcome::Granted(id)
    }

    /// Registers one more live holder for an already-granted `id`. Called
    /// once per coalesced caller resolved against `id`, whether resolved
    /// immediately (target was already granted when the coalescing
    /// submission arrived) or at the same moment `id` itself is granted
    /// (see `coalesced_onto_target`). A no-op if `id` is not currently
    /// granted (defensive; the dispatcher only calls this for ids it just
    /// confirmed are granted).
    pub fn add_granted_reference(&mut self, id: JobId) {
        if let Some(count) = self.granted_refcount.get_mut(&id) {
            *count += 1;
        }
    }

    /// Releases one holder's reference to a granted job. Every
    /// [`super::JobHandle`] (including one obtained by a coalesced
    /// submission, which is issued under the *target's* id, never its own)
    /// releases through this same path. Occupancy is only actually given
    /// back once every live holder (the target plus every caller coalesced
    /// onto it) has released — a coalesced holder dropping first must not
    /// pull the reservation out from under co-holders still using it.
    pub fn release_granted(&mut self, id: JobId) -> bool {
        self.coalesced_onto.remove(&id);
        let Some(count) = self.granted_refcount.get_mut(&id) else {
            return false;
        };
        *count -= 1;
        if *count > 0 {
            return true;
        }
        self.granted_refcount.remove(&id);
        match self.granted.remove(&id) {
            Some(request) => {
                self.occupancy.release(&request);
                true
            }
            None => false,
        }
    }

    /// Removes every queued and granted job unconditionally, releasing all
    /// occupancy. Used by shutdown so no reservation state survives past
    /// coordinator teardown.
    pub fn clear_all(&mut self) -> Vec<JobId> {
        let mut drained: Vec<JobId> = self.waiting.iter().map(|request| request.id).collect();
        drained.extend(self.granted.keys().copied());
        self.waiting.clear();
        self.granted.clear();
        self.granted_refcount.clear();
        self.occupancy = Occupancy::default();
        self.coalesced_onto.clear();
        drained
    }
}
