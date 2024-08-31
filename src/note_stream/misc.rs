use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::{
    hash::DefaultHasher,
    sync::atomic::{AtomicU64, Ordering},
};

use enostr::Filter;

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct NoteStreamInstanceId {
    id: u64,
}

impl Default for NoteStreamInstanceId {
    fn default() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::SeqCst),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct HashableFilter {
    pub filters: HashSet<Filter>,
}

impl Hash for HashableFilter {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for filter in &self.filters {
            filter.hash(state);
        }
    }
}

impl HashableFilter {
    pub fn new(filters: Vec<Filter>) -> Self {
        Self {
            filters: filters.into_iter().collect(),
        }
    }

    pub fn compute_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }

    pub fn as_vec(&self) -> Vec<Filter> {
        self.filters.iter().cloned().collect()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum NoteStreamInstanceState {
    Active,
    Inactive,
    Reactivating, // transitioning from the inactive state to active
}

pub struct NoteStream {
    hashable_filter: HashableFilter,
    sub_id: Option<SubscriptionId>,
    active_instances: i32,
    instances: HashMap<NoteStreamInstanceId, NoteStreamInstance>,
}

impl NoteStream {
    pub fn new(hashable_filter: HashableFilter) -> Self {
        Self {
            hashable_filter,
            sub_id: None,
            active_instances: 0,
            instances: HashMap::new(),
        }
    }

    pub fn add_subscription(&mut self, sub_id: SubscriptionId) {
        self.sub_id = Some(sub_id);
    }

    pub fn has_subscription(&self) -> bool {
        self.sub_id.is_some()
    }

    pub fn unsubscribe(&mut self) -> Option<SubscriptionId> {
        self.sub_id.take()
    }

    pub fn add_instance(&mut self, id: NoteStreamInstanceId, instance: NoteStreamInstance) {
        match instance.status {
            NoteStreamInstanceState::Active => {
                self.active_instances += 1;
            }
            NoteStreamInstanceState::Reactivating => {
                self.active_instances += 1;
            }
            NoteStreamInstanceState::Inactive => {}
        }

        self.instances.insert(id, instance);
    }

    pub fn remove_instance(&mut self, instance: &NoteStreamInstanceId) {
        if let Some(instance) = self.instances.get(instance) {
            match instance.status {
                NoteStreamInstanceState::Active => {
                    self.active_instances -= 1;
                }
                NoteStreamInstanceState::Reactivating => {
                    self.active_instances -= 1;
                }
                NoteStreamInstanceState::Inactive => {}
            }
        }
        self.instances.remove(instance);
    }

    pub fn get_instance(&self, id: &NoteStreamInstanceId) -> Option<&NoteStreamInstance> {
        self.instances.get(id)
    }

    pub fn get_instance_mut(
        &mut self,
        id: &NoteStreamInstanceId,
    ) -> Option<&mut NoteStreamInstance> {
        self.instances.get_mut(id)
    }

    pub fn get_instances(&self) -> &HashMap<NoteStreamInstanceId, NoteStreamInstance> {
        &self.instances
    }

    pub fn modify_instance(
        &mut self,
        instance: &NoteStreamInstanceId,
        modify: impl FnOnce(&mut NoteStreamInstance),
    ) {
        if let Some(instance) = self.instances.get_mut(instance) {
            let active_state_before = instance.get_status().clone();
            modify(instance);
            if active_state_before != *instance.get_status() {
                match instance.status {
                    NoteStreamInstanceState::Active => self.active_instances += 1,
                    NoteStreamInstanceState::Inactive => self.active_instances -= 1,
                    NoteStreamInstanceState::Reactivating => {}
                }
            }
        }
    }

    pub fn get_filter_id(&self) -> &HashableFilter {
        &self.hashable_filter
    }

    /// Whether the stream is active ie, whether it desires to receive new notes
    pub fn is_active(&self) -> bool {
        self.active_instances != 0
    }
}

pub struct NoteStreamInstance {
    last_seen: Option<u64>, // timestamp of last note seen
    status: NoteStreamInstanceState,
}

impl NoteStreamInstance {
    pub fn pause(&mut self) {
        self.status = NoteStreamInstanceState::Inactive;
    }

    pub fn resume(&mut self) {
        if self.status == NoteStreamInstanceState::Inactive {
            self.status = NoteStreamInstanceState::Reactivating;
        }
    }

    pub fn set_last_seen(&mut self, last_seen: u64) {
        self.last_seen = Some(last_seen);
    }

    pub fn get_last_seen(&self) -> Option<u64> {
        self.last_seen
    }

    pub fn get_status(&self) -> &NoteStreamInstanceState {
        &self.status
    }

    pub fn set_status(&mut self, status: NoteStreamInstanceState) {
        self.status = status;
    }
}

impl Default for NoteStreamInstance {
    fn default() -> Self {
        Self {
            last_seen: None,
            status: NoteStreamInstanceState::Active,
        }
    }
}

#[derive(Clone)]
pub struct SubscriptionId {
    pub ndb_id: u64,
    pub remote_id: String,
}

impl SubscriptionId {
    pub fn new(ndb_id: u64, remote_id: String) -> Self {
        Self { ndb_id, remote_id }
    }
}
