use std::{collections::HashMap, fmt::Display};

use enostr::{Filter, NoteId, RelayPool};
use nostrdb::{Ndb, Subscription};
use tracing::{error, info};
use uuid::Uuid;

#[derive(Debug)]
pub struct MultiSubscriber {
    pub filters: Vec<Filter>,
    pub local_subid: Option<Subscription>,
    pub remote_subid: Option<String>,
    local_subscribers: u32,
    remote_subscribers: u32,
}

impl MultiSubscriber {
    /// Create a MultiSubscriber with an initial local subscription.
    pub fn with_initial_local_sub(sub: Subscription, filters: Vec<Filter>) -> Self {
        let mut msub = MultiSubscriber::new(filters);
        msub.local_subid = Some(sub);
        msub.local_subscribers = 1;
        msub
    }

    pub fn new(filters: Vec<Filter>) -> Self {
        Self {
            filters,
            local_subid: None,
            remote_subid: None,
            local_subscribers: 0,
            remote_subscribers: 0,
        }
    }

    fn unsubscribe_remote(&mut self, ndb: &Ndb, pool: &mut RelayPool) {
        let remote_subid = if let Some(remote_subid) = &self.remote_subid {
            remote_subid
        } else {
            self.err_log(ndb, "unsubscribe_remote: nothing to unsubscribe from?");
            return;
        };

        pool.unsubscribe(remote_subid.clone());

        self.remote_subid = None;
    }

    /// Locally unsubscribe if we have one
    fn unsubscribe_local(&mut self, ndb: &mut Ndb) {
        let local_sub = if let Some(local_sub) = self.local_subid {
            local_sub
        } else {
            self.err_log(ndb, "unsubscribe_local: nothing to unsubscribe from?");
            return;
        };

        match ndb.unsubscribe(local_sub) {
            Err(e) => {
                self.err_log(ndb, &format!("Failed to unsubscribe: {e}"));
            }
            Ok(_) => {
                self.local_subid = None;
            }
        }
    }

    pub fn unsubscribe(&mut self, ndb: &mut Ndb, pool: &mut RelayPool) -> bool {
        if self.local_subscribers == 0 && self.remote_subscribers == 0 {
            self.err_log(
                ndb,
                "Called multi_subscriber unsubscribe when both sub counts are 0",
            );
            return false;
        }

        self.local_subscribers = self.local_subscribers.saturating_sub(1);
        self.remote_subscribers = self.remote_subscribers.saturating_sub(1);

        if self.local_subscribers == 0 && self.remote_subscribers == 0 {
            self.info_log(ndb, "Locally unsubscribing");
            self.unsubscribe_local(ndb);
            self.unsubscribe_remote(ndb, pool);
            self.local_subscribers = 0;
            self.remote_subscribers = 0;
            true
        } else {
            false
        }
    }

    fn info_log(&self, ndb: &Ndb, msg: &str) {
        info!(
            "{msg}. {}/{}/{} active ndb/local/remote subscriptions.",
            ndb.subscription_count(),
            self.local_subscribers,
            self.remote_subscribers,
        );
    }

    fn err_log(&self, ndb: &Ndb, msg: &str) {
        error!(
            "{msg}. {}/{}/{} active ndb/local/remote subscriptions.",
            ndb.subscription_count(),
            self.local_subscribers,
            self.remote_subscribers,
        );
    }

    pub fn subscribe(&mut self, ndb: &Ndb, pool: &mut RelayPool) {
        self.local_subscribers += 1;
        self.remote_subscribers += 1;

        if self.remote_subscribers == 1 {
            if self.remote_subid.is_some() {
                self.err_log(
                    ndb,
                    "Object is first subscriber, but it already had a subscription",
                );
                return;
            } else {
                let subid = Uuid::new_v4().to_string();
                pool.subscribe(subid.clone(), self.filters.clone());
                self.info_log(ndb, "First remote subscription");
                self.remote_subid = Some(subid);
            }
        }

        if self.local_subscribers == 1 {
            if self.local_subid.is_some() {
                self.err_log(ndb, "Should not have a local subscription already");
                return;
            }

            match ndb.subscribe(&self.filters) {
                Ok(sub) => {
                    self.info_log(ndb, "First local subscription");
                    self.local_subid = Some(sub);
                }

                Err(err) => {
                    error!("multi_subscriber: error subscribing locally: '{err}'")
                }
            }
        }
    }
}

pub struct LocalSub {
    pub sub: Subscription,
    pub sub_count: usize,
    pub filter: Vec<Filter>,
}

impl std::fmt::Debug for LocalSub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalSub")
            .field("sub", &self.sub)
            .field("sub_count", &self.sub_count)
            .finish()
    }
}

pub struct Remote {
    pub filter: Vec<Filter>,
    subid: String,
}

impl std::fmt::Debug for Remote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Remote")
            .field("subid", &self.subid)
            .finish()
    }
}

#[derive(PartialEq, Hash, Eq, Clone, Debug)]
pub enum SubscriberId {
    Thread(NoteId),
}

// /// Meant for managing one static remote subscription and one replaceable local sub which is subsumed by the remote
// #[derive(Default)]
// pub struct ReplaceableSub {
//     pub remote: Option<Remote>,
//     pub local_sub: Option<LocalSub>,
// }

// impl ReplaceableSub {
//     pub fn subscribe(
//         &mut self,
//         ndb: &mut Ndb,
//         pool: &mut RelayPool,
//         id: &SubscriberId,
//         local_sub_filter: Vec<Filter>,
//         remote_sub_filter: impl FnOnce() -> Vec<Filter>,
//     ) {
//         if self.remote.is_none() {
//             let subid = Uuid::new_v4().to_string();

//             let filter = remote_sub_filter();
//             let remote = Remote {
//                 filter: filter.clone(),
//                 subid: subid.clone(),
//             };

//             self.remote = Some(remote);
//             tracing::info!("Remote subscribe for {:?}", id);
//             pool.subscribe(subid, filter);
//         }

//         if let Some(local_sub) = &mut self.local_sub {
//             if local_sub.id == *id {
//                 local_sub.sub_count += 1;
//                 return;
//             }
//             match ndb.unsubscribe(local_sub.sub) {
//                 Ok(_) => tracing::info!("Unsubscribed from previous local sub: {:?}", local_sub.id),
//                 Err(e) => tracing::info!(
//                     "Failed to unsub from previous local sub {:?}: {e}",
//                     local_sub.id
//                 ),
//             };
//         }

//         if let Ok(sub) = ndb.subscribe(&local_sub_filter) {
//             tracing::info!("Local subscribe for {:?}", id);
//             self.local_sub = Some(LocalSub {
//                 id: id.clone(),
//                 sub,
//                 filter: local_sub_filter,
//             });
//         } else {
//             tracing::error!("Failed to ndb subscribe");
//         }
//     }

//     pub fn unsubscribe(&mut self, ndb: &mut Ndb, pool: &mut RelayPool) {
//         if let Some(sub) = &self.local_sub {
//             tracing::info!("Unsubscribing from local subscription for: {:?}", sub.id);
//             let res = ndb.unsubscribe(sub.sub);

//             if let Err(e) = res {
//                 tracing::error!("Failed to unsub ndb: {e}");
//             }
//         } else {
//             tracing::error!("Failed to local unsub",);
//         }

//         self.local_sub = None;

//         let Some(remote) = &self.remote else {
//             return;
//         };

//         tracing::info!("Unsubscribed remote for: {:?}", remote.subid);
//         pool.unsubscribe(remote.subid.clone());

//         self.remote = None;
//     }
// }

/// For managing one remote subscription & multiple local subscriptions which are subsumed by the remote

#[derive(Default, Debug)]

pub struct MultiSubscriber2 {
    pub remote: Option<Remote>,
    local_subs: HashMap<SubscriberId, LocalSub>,
}

impl MultiSubscriber2 {
    pub fn subscribe(
        &mut self,
        ndb: &Ndb,
        pool: &mut RelayPool,
        id: &SubscriberId,
        local_sub_filter: Vec<Filter>,
        remote_sub_filter: impl FnOnce() -> Vec<Filter>,
    ) {
        if let Some(local_sub) = self.local_subs.get_mut(id) {
            local_sub.sub_count += 1;
            tracing::info!(
                "ALREADY HAVE LOCAL SUB FOR ID: {:?}. New Count: {}",
                id,
                local_sub.sub_count
            )
        } else {
            if let Ok(sub) = ndb.subscribe(&local_sub_filter) {
                tracing::info!("Local subscribe for {:?}", id);

                self.local_subs.insert(
                    id.clone(),
                    LocalSub {
                        sub,
                        sub_count: 1,
                        filter: local_sub_filter,
                    },
                );
            }
        }

        if self.remote.is_none() {
            let subid = Uuid::new_v4().to_string();

            let filter = remote_sub_filter();

            let remote = Remote {
                filter: filter.clone(),

                subid: subid.clone(),
            };

            self.remote = Some(remote);

            tracing::info!("Remote subscribe for {:?}", id);

            pool.subscribe(subid, filter);
        };
    }

    pub fn unsubscribe(&mut self, ndb: &mut Ndb, pool: &mut RelayPool, id: &SubscriberId) -> bool {
        if let Some(local_sub) = self.local_subs.get_mut(id) {
            local_sub.sub_count -= 1;
            if local_sub.sub_count > 0 {
                tracing::info!(
                    "Still have {} local subscribers. Not remote or local unsubscribing",
                    local_sub.sub_count
                );
                return false;
            }
        };

        let local_sub = self.local_subs.remove(id);

        if let Some(sub) = local_sub {
            tracing::info!("Unsubscribing from local subscription for: {:?}", id);

            let res = ndb.unsubscribe(sub.sub);

            if let Err(e) = res {
                tracing::error!("Failed to unsub ndb: {e}");
            }
        } else {
            tracing::error!(
                "Failed to local unsub. Did not find {:?} in local subscriptions",
                id
            );
        }

        if !self.local_subs.is_empty() {
            return false;
        }

        let Some(remote) = &self.remote else {
            tracing::error!("Somehow we don't have a remote subscription but we did have a local");

            return false;
        };

        tracing::info!("Unsubscribed remote for: {:?}", id);

        pool.unsubscribe(remote.subid.clone());

        self.remote = None;

        true
    }

    pub fn get_local(&self, id: &SubscriberId) -> Option<&LocalSub> {
        self.local_subs.get(id)
    }
}
