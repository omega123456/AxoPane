use std::collections::{BTreeSet, HashMap, VecDeque};

use super::{JobClass, JobId};

pub const MAX_LATENCY_SLOTS: usize = 4;

pub const MAX_CPU_SLOTS: usize = 2;

pub const MAX_QUEUED_JOBS: usize = 256;

pub const MAX_SLOTS_PER_RESOURCE_PER_CLASS: usize = 1;

pub fn canonicalize_resource_keys(keys: impl IntoIterator<Item = String>) -> Vec<String> {
    let set: BTreeSet<String> = keys.into_iter().collect();
    set.into_iter().collect()
}

#[derive(Debug, Clone)]
pub struct JobRequest {
    pub id: JobId,
    pub classes: BTreeSet<JobClass>,
    pub resource_keys: Vec<String>,
    pub coalesce_key: Option<String>,
}

#[derive(Debug, Default)]
struct Occupancy {
    global: HashMap<JobClass, usize>,
    per_resource: HashMap<(String, JobClass), usize>,
}

impl Occupancy {
    fn global_cap(class: JobClass) -> usize {
        match class {
            JobClass::Latency => MAX_LATENCY_SLOTS,
            JobClass::Throughput => super::max_throughput_slots(),
            JobClass::Cpu => MAX_CPU_SLOTS,
        }
    }

    fn can_grant(&self, request: &JobRequest) -> bool {
        request
            .classes
            .iter()
            .all(|&class| self.class_has_room(class, request))
    }

    fn class_has_room(&self, class: JobClass, request: &JobRequest) -> bool {
        let current_global = *self.global.get(&class).unwrap_or(&0);
        let global_cap = Self::global_cap(class);
        if current_global + 1 > global_cap {
            return false;
        }
        limited_resource_keys(class, request).all(|key| {
            let current_per_resource = *self.per_resource.get(&(key.clone(), class)).unwrap_or(&0);
            current_per_resource < MAX_SLOTS_PER_RESOURCE_PER_CLASS
        })
    }

    fn grant(&mut self, request: &JobRequest) {
        for &class in &request.classes {
            self.grant_class(class, request);
        }
    }

    fn grant_class(&mut self, class: JobClass, request: &JobRequest) {
        *self.global.entry(class).or_insert(0) += 1;
        for key in limited_resource_keys(class, request) {
            *self.per_resource.entry((key.clone(), class)).or_insert(0) += 1;
        }
    }

    fn release(&mut self, request: &JobRequest) {
        for &class in &request.classes {
            self.release_class(class, request);
        }
    }

    fn release_class(&mut self, class: JobClass, request: &JobRequest) {
        saturating_decrement(self.global.get_mut(&class));
        for key in limited_resource_keys(class, request) {
            saturating_decrement(self.per_resource.get_mut(&(key.clone(), class)));
        }
    }
}

fn limited_resource_keys(class: JobClass, request: &JobRequest) -> impl Iterator<Item = &String> {
    let resource_limited = matches!(class, JobClass::Latency | JobClass::Throughput);
    request
        .resource_keys
        .iter()
        .filter(move |_| resource_limited)
}

fn saturating_decrement(count: Option<&mut usize>) {
    if let Some(count) = count {
        *count = count.saturating_sub(1);
    }
}

pub enum AdmitOutcome {
    NoneRunnable,
    Granted(JobId),
}

#[derive(Default)]
pub struct SchedulerState {
    waiting: VecDeque<JobRequest>,
    granted: HashMap<JobId, JobRequest>,
    granted_refcount: HashMap<JobId, usize>,
    occupancy: Occupancy,
    coalesced_onto: HashMap<JobId, JobId>,
}

impl SchedulerState {
    pub fn queued_len(&self) -> usize {
        self.waiting.len()
    }

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

    pub fn record_coalesced(&mut self, coalesced_id: JobId, target_id: JobId) {
        self.coalesced_onto.insert(coalesced_id, target_id);
    }

    pub fn is_granted(&self, id: JobId) -> bool {
        self.granted.contains_key(&id)
    }

    pub fn coalesced_onto_target(&self, target_id: JobId) -> Vec<JobId> {
        self.coalesced_onto
            .iter()
            .filter(|(_, target)| **target == target_id)
            .map(|(coalesced, _)| *coalesced)
            .collect()
    }

    pub fn enqueue(&mut self, request: JobRequest) -> bool {
        if self.waiting.len() >= MAX_QUEUED_JOBS {
            return false;
        }
        self.waiting.push_back(request);
        true
    }

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

    pub fn try_admit_next(&mut self) -> AdmitOutcome {
        let Some(index) = self
            .waiting
            .iter()
            .position(|request| self.occupancy.can_grant(request))
        else {
            return AdmitOutcome::NoneRunnable;
        };

        let request = self.waiting.remove(index).expect("index was just found");
        self.occupancy.grant(&request);
        let id = request.id;
        self.granted.insert(id, request);
        self.granted_refcount.insert(id, 1);
        AdmitOutcome::Granted(id)
    }

    pub fn add_granted_reference(&mut self, id: JobId) {
        if let Some(count) = self.granted_refcount.get_mut(&id) {
            *count += 1;
        }
    }

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
