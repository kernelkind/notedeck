use std::collections::HashMap;

use nostrdb::{Filter, FilterBuilder, Subscription};

use super::misc::{
    HashableFilter, NoteStream, NoteStreamInstance, NoteStreamInstanceId, NoteStreamInstanceState,
    SubscriptionId,
};

#[derive(Default)]
pub struct NoteStreamManager {
    hash_to_stream: HashMap<u64, NoteStream>,
    id_to_filter_hash: HashMap<NoteStreamInstanceId, u64>,
}

impl NoteStreamManager {
    pub(crate) fn find_new_ndb_subscriptions(&mut self) -> Vec<Vec<Filter>> {
        let mut filters = Vec::new();
        for (_, stream) in self.hash_to_stream.iter() {
            if !stream.has_subscription() && stream.is_active() {
                filters.push(stream.get_filter_id().as_vec());
            }
        }
        filters
    }

    /// Returns a list of subscription ids that should be deleted
    pub(crate) fn take_ndb_subscription_deletions(&mut self) -> Vec<SubscriptionId> {
        let mut deletions = Vec::new();
        for (_, stream) in self.hash_to_stream.iter_mut() {
            if !stream.is_active() && stream.has_subscription() {
                if let Some(sub_id) = stream.unsubscribe() {
                    deletions.push(sub_id);
                }
            }
        }
        deletions
    }

    pub(crate) fn save_subscription(
        &mut self,
        filters: Vec<Filter>,
        subscription: Subscription,
        remote_sub: String,
    ) {
        let hashable_filter = HashableFilter::new(filters);
        let hash_value = hashable_filter.compute_hash();
        if let Some(stream) = self.hash_to_stream.get_mut(&hash_value) {
            stream.add_subscription(SubscriptionId::new(subscription, remote_sub));
        }
    }

    pub(crate) fn get_active_filters_for_ids(&self) -> Vec<(NoteStreamInstanceId, Vec<Filter>)> {
        self.hash_to_stream
            .iter()
            .filter_map(|(_hash, stream)| {
                if stream.is_active() {
                    let stream_filters = stream.get_filter_id().as_vec();
                    Some(
                        stream
                            .get_instances()
                            .iter()
                            .filter_map(|(id, instance)| match instance.get_status() {
                                NoteStreamInstanceState::Reactivating => {
                                    if let Some(last_seen) = instance.get_last_seen() {
                                        let mut reactivating_filters = stream_filters.clone();
                                        reactivating_filters
                                            .push(FilterBuilder::new().since(last_seen).build());
                                        Some((id.clone(), reactivating_filters))
                                    } else {
                                        None
                                    }
                                }
                                NoteStreamInstanceState::Active => {
                                    Some((id.clone(), stream_filters.clone()))
                                }
                                NoteStreamInstanceState::Inactive => None,
                            })
                            .collect::<Vec<_>>(),
                    )
                } else {
                    None
                }
            })
            .flatten()
            .collect()
    }

    pub(crate) fn add_new(&mut self, id: NoteStreamInstanceId, hashable_filter: HashableFilter) {
        let hash_value = hashable_filter.compute_hash();
        let mut stream = NoteStream::new(hashable_filter);
        stream.add_instance(id.clone(), NoteStreamInstance::default());

        self.hash_to_stream.insert(hash_value, stream);

        self.id_to_filter_hash.insert(id.clone(), hash_value);
    }

    pub(crate) fn resume(&mut self, id: &NoteStreamInstanceId) {
        self.change_instance_active_state(id, true);
    }

    pub(crate) fn pause(&mut self, id: &NoteStreamInstanceId) {
        self.change_instance_active_state(id, false);
    }

    fn change_instance_active_state(&mut self, id: &NoteStreamInstanceId, activate: bool) {
        let filter_hash = if let Some(filter_id) = self.id_to_filter_hash.get(id) {
            filter_id
        } else {
            return;
        };

        if let Some(stream) = self.hash_to_stream.get_mut(filter_hash) {
            stream.modify_instance(id, |instance| {
                if activate {
                    instance.resume()
                } else {
                    instance.pause()
                }
            });
        }
    }

    pub(crate) fn update_last_seen(&mut self, id: &NoteStreamInstanceId, last_seen: u64) {
        if let Some(filter_hash) = self.id_to_filter_hash.get(id) {
            if let Some(stream) = self.hash_to_stream.get_mut(filter_hash) {
                stream.modify_instance(id, |instance| {
                    instance.set_last_seen(last_seen);
                });
            }
        }
    }

    pub(crate) fn get_note_stream_instance_mut(
        &mut self,
        id: &NoteStreamInstanceId,
    ) -> Option<&mut NoteStreamInstance> {
        if let Some(filter_hash) = self.id_to_filter_hash.get(id) {
            if let Some(stream) = self.hash_to_stream.get_mut(filter_hash) {
                return stream.get_instance_mut(id);
            }
        }
        None
    }
}

mod tests {
    use crate::{note_stream::misc::HashableFilter, test_data, thread::Thread};

    #[test]
    fn test_hashable() {
        let filter1 = Thread::filters(test_data::test_pubkey());
        let filter2 = Thread::filters(test_data::test_pubkey());

        let hashable_filter1 = HashableFilter::new(filter1);
        let hashable_filter2 = HashableFilter::new(filter2);

        assert_eq!(
            hashable_filter1.compute_hash(),
            hashable_filter2.compute_hash()
        );
    }
}
