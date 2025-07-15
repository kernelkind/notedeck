use egui_nav::ReturnType;
use enostr::{Filter, NoteId, RelayPool};
use hashbrown::HashMap;
use nostrdb::{Ndb, Subscription};
use notedeck::UnifiedSubscription;
use tracing::{error, info};
use uuid::Uuid;

use crate::timeline::ThreadSelection;

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

type RootNoteId = NoteId;

#[derive(Default)]
pub struct ThreadSubs {
    pub remotes: HashMap<RootNoteId, Remote>,
    scopes: HashMap<MetaId, Vec<Scope>>,
}

// column id
type MetaId = usize;

pub struct Remote {
    pub filter: Vec<Filter>,
    subid: String,
    dependers: usize,
}

impl std::fmt::Debug for Remote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Remote")
            .field("subid", &self.subid)
            .field("dependers", &self.dependers)
            .finish()
    }
}

struct Scope {
    pub root_id: NoteId,
    stack: Vec<Sub>,
}

pub struct Sub {
    pub selected_id: NoteId,
    pub sub: Subscription,
    pub filter: Vec<Filter>,
}

impl ThreadSubs {
    #[allow(clippy::too_many_arguments)]
    pub fn subscribe(
        &mut self,
        ndb: &mut Ndb,
        pool: &mut RelayPool,
        meta_id: usize,
        id: &ThreadSelection,
        local_sub_filter: Vec<Filter>,
        new_scope: bool,
        remote_sub_filter: impl FnOnce() -> Vec<Filter>,
    ) {
        let cur_scopes = self.scopes.entry(meta_id).or_default();

        let new_subs = if new_scope || cur_scopes.is_empty() {
            local_sub_new_scope(ndb, id, local_sub_filter, cur_scopes)
        } else {
            let cur_scope = cur_scopes.last_mut().expect("can't be empty");
            sub_current_scope(ndb, id, local_sub_filter, cur_scope)
        };

        let remote = match self.remotes.raw_entry_mut().from_key(&id.root_id.bytes()) {
            hashbrown::hash_map::RawEntryMut::Occupied(entry) => entry.into_mut(),
            hashbrown::hash_map::RawEntryMut::Vacant(entry) => {
                let filter = remote_sub_filter();
                let (_, res) = entry.insert(
                    NoteId::new(*id.root_id.bytes()),
                    Remote {
                        filter: filter.clone(),
                        subid: sub_remote(pool, filter, id),
                        dependers: 0,
                    },
                );

                res
            }
        };

        remote.dependers = remote.dependers.saturating_add_signed(new_subs);
        let num_dependers = remote.dependers;
        tracing::info!(
            "Sub stats: num remotes: {}, num locals: {}, num remote dependers: {:?}",
            self.remotes.len(),
            self.scopes.len(),
            num_dependers,
        );
    }

    pub fn unsubscribe(
        &mut self,
        ndb: &mut Ndb,
        pool: &mut RelayPool,
        meta_id: usize,
        id: &ThreadSelection,
        return_type: ReturnType,
    ) {
        let Some(scopes) = self.scopes.get_mut(&meta_id) else {
            return;
        };

        let Some(remote) = self.remotes.get_mut(&id.root_id.bytes()) else {
            tracing::error!("somehow we're unsubscribing but we don't have a remote");
            return;
        };

        match return_type {
            ReturnType::Drag => {
                if let Some(scope) = scopes.last_mut() {
                    let Some(cur_sub) = scope.stack.pop() else {
                        tracing::error!("expected a scope to be left");
                        return;
                    };

                    if scope.root_id.bytes() != id.root_id.bytes() {
                        tracing::error!("Somehow the current scope's root is not equal to the selected note's root. scope's root: {:?}, thread's root: {:?}", scope.root_id.hex(), id.root_id.bytes());
                    }

                    if ndb_unsub(ndb, cur_sub.sub, id) {
                        remote.dependers = remote.dependers.saturating_sub(1);
                    }

                    if scope.stack.is_empty() {
                        scopes.pop();
                    }
                }
            }
            ReturnType::Click => {
                let Some(scope) = scopes.pop() else {
                    tracing::error!("called unsubscribe but there aren't any scopes left");
                    return;
                };

                if scope.root_id.bytes() != id.root_id.bytes() {
                    tracing::error!("Somehow the current scope's root is not equal to the selected note's root. scope's root: {:?}, thread's root: {:?}", scope.root_id.hex(), id.root_id.bytes());
                }
                for sub in scope.stack {
                    if ndb_unsub(ndb, sub.sub, id) {
                        remote.dependers = remote.dependers.saturating_sub(1);
                    }
                }
            }
        }

        if scopes.is_empty() {
            self.scopes.remove(&meta_id);
        }

        let num_dependers = remote.dependers;

        if remote.dependers == 0 {
            let remote = self
                .remotes
                .remove(&id.root_id.bytes())
                .expect("code above should guarentee existence");
            tracing::info!("Remotely unsubscribed: {}", remote.subid);
            pool.unsubscribe(remote.subid);
        }

        tracing::info!(
            "unsub stats: num remotes: {}, num locals: {}, num remote dependers: {:?}",
            self.remotes.len(),
            self.scopes.len(),
            num_dependers,
        );
    }

    pub fn get_local(&self, meta_id: usize) -> Option<&Sub> {
        self.scopes
            .get(&meta_id)
            .as_ref()
            .and_then(|s| s.last())
            .and_then(|s| s.stack.last())
    }
}

fn sub_current_scope(
    ndb: &mut Ndb,
    selection: &ThreadSelection,
    local_sub_filter: Vec<Filter>,
    cur_scope: &mut Scope,
) -> isize {
    let mut new_subs = 0;

    if selection.root_id.bytes() != cur_scope.root_id.bytes() {
        tracing::error!(
            "Somehow the current scope's root is not equal to the selected note's root"
        );
    }

    if let Some(sub) = ndb_sub(ndb, &local_sub_filter, selection) {
        cur_scope.stack.push(Sub {
            selected_id: NoteId::new(*selection.selected_or_root()),
            sub,
            filter: local_sub_filter,
        });
        new_subs += 1;
    }

    new_subs
}

fn ndb_sub(ndb: &Ndb, filter: &[Filter], id: impl std::fmt::Debug) -> Option<Subscription> {
    match ndb.subscribe(filter) {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::info!("Failed to get subscription for {:?}: {e}", id);
            None
        }
    }
}

fn ndb_unsub(ndb: &mut Ndb, sub: Subscription, id: impl std::fmt::Debug) -> bool {
    match ndb.unsubscribe(sub) {
        Ok(_) => true,
        Err(e) => {
            tracing::info!("Failed to unsub {:?}: {e}", id);
            false
        }
    }
}

fn sub_remote(pool: &mut RelayPool, filter: Vec<Filter>, id: impl std::fmt::Debug) -> String {
    let subid = Uuid::new_v4().to_string();

    tracing::info!("Remote subscribe for {:?}", id);

    pool.subscribe(subid.clone(), filter);

    subid
}

fn local_sub_new_scope(
    ndb: &mut Ndb,
    id: &ThreadSelection,
    local_sub_filter: Vec<Filter>,
    scopes: &mut Vec<Scope>,
) -> isize {
    let Some(sub) = ndb_sub(ndb, &local_sub_filter, id) else {
        return 0;
    };

    scopes.push(Scope {
        root_id: id.root_id.to_note_id(),
        stack: vec![Sub {
            selected_id: NoteId::new(*id.selected_or_root()),
            sub,
            filter: local_sub_filter,
        }],
    });

    1
}

#[derive(Debug)]
pub enum TimelineSub {
    NoSub,
    NeedsSub {
        new_dependers: usize,
    },
    Single {
        filters: Vec<Filter>,
        state: SubState,
    },
    Multi {
        filters: Vec<Filter>,
        state: SubState,
        dependers: usize,
    },
}

#[derive(Debug, Clone)]
pub enum SubState {
    RemoteOnly { id: String },
    NeedsRemote(Subscription),
    Unified(UnifiedSubscription),
}

impl TimelineSub {
    pub fn increment(&mut self) {
        match self {
            TimelineSub::NoSub => {
                *self = TimelineSub::NeedsSub { new_dependers: 1 };
            }
            TimelineSub::NeedsSub { new_dependers } => *new_dependers += 1,
            TimelineSub::Single { state, filters } => {
                *self = TimelineSub::Multi {
                    filters: filters.clone(),
                    state: state.clone(),
                    dependers: 2,
                }
            }
            TimelineSub::Multi {
                filters: _,
                state: _,
                dependers,
            } => *dependers += 1,
        }
    }

    pub fn decrement(&mut self) -> SubDecrementResponse {
        let mut resp = None;
        match self {
            TimelineSub::NoSub => {}
            TimelineSub::NeedsSub { new_dependers } => {
                *self = if *new_dependers > 1 {
                    TimelineSub::NeedsSub {
                        new_dependers: *new_dependers - 1,
                    }
                } else {
                    TimelineSub::NoSub
                };
            }
            TimelineSub::Single {
                state: unified_subscription,
                filters: _,
            } => {
                resp = Some(unified_subscription.clone());
                *self = TimelineSub::NoSub;
            }
            TimelineSub::Multi {
                state,
                dependers,
                filters,
            } => {
                if *dependers > 1 {
                    *dependers -= 1;
                } else {
                    *self = TimelineSub::Single {
                        state: state.clone(),
                        filters: filters.clone(),
                    }
                };
            }
        }

        SubDecrementResponse {
            need_unsubscribe: resp,
        }
    }

    pub fn get_local(&self) -> Option<Subscription> {
        match self {
            TimelineSub::NoSub => None,
            TimelineSub::NeedsSub { new_dependers: _ } => None,
            TimelineSub::Single { state, filters: _ }
            | TimelineSub::Multi {
                filters: _,
                state,
                dependers: _,
            } => match state {
                SubState::RemoteOnly { id: _ } => None,
                SubState::Unified(unified_subscription) => Some(unified_subscription.local),
                SubState::NeedsRemote(subscription) => Some(*subscription),
            },
        }
    }

    pub fn get_filters(&self) -> Option<&Vec<Filter>> {
        match self {
            TimelineSub::Single { state: _, filters }
            | TimelineSub::Multi {
                filters,
                state: _,
                dependers: _,
            } => Some(filters),
            TimelineSub::NoSub | TimelineSub::NeedsSub { new_dependers: _ } => None,
        }
    }

    pub fn add_local(&mut self, filters: &[Filter], local: Subscription) {
        match self {
            TimelineSub::NoSub => {
                *self = TimelineSub::Single {
                    state: SubState::NeedsRemote(local),
                    filters: filters.to_vec(),
                };
            }
            TimelineSub::NeedsSub { new_dependers } => {
                *self = TimelineSub::Multi {
                    state: SubState::NeedsRemote(local),
                    dependers: *new_dependers + 1,
                    filters: filters.to_vec(),
                };
            }
            _ => {}
        }
    }

    /// TODO(kernelkind): If the provided filter is different from what is present,
    /// we will unsubscribe and resubscribe with the new filter.
    pub fn subscribe_or_increment(
        &mut self,
        cur_filters: &[Filter],
        ndb: &Ndb,
        pool: &mut RelayPool,
    ) {
        match self {
            TimelineSub::NoSub => {
                let id = "SubState::NoSub";
                let remote = sub_remote(pool, cur_filters.to_owned(), id);
                let local = ndb_sub(ndb, cur_filters, id).expect("ndb sub");

                *self = TimelineSub::Single {
                    filters: cur_filters.to_owned(),
                    state: SubState::Unified(UnifiedSubscription { local, remote }),
                };
            }
            TimelineSub::NeedsSub { new_dependers } => {
                let id = "SubState::NoSub";
                let remote = sub_remote(pool, cur_filters.to_owned(), id);
                let local = ndb_sub(ndb, cur_filters, id).expect("ndb sub");

                *self = TimelineSub::Multi {
                    filters: cur_filters.to_owned(),
                    state: SubState::Unified(UnifiedSubscription { local, remote }),
                    dependers: *new_dependers + 1,
                };
            }
            TimelineSub::Single { filters: _, state } => {
                if let SubState::NeedsRemote(sub) = state {
                    let remote = sub_remote(pool, cur_filters.to_owned(), "Local only -> Unified");
                    *state = SubState::Unified(UnifiedSubscription {
                        local: *sub,
                        remote,
                    });
                }

                *self = TimelineSub::Multi {
                    filters: cur_filters.to_owned(),
                    state: state.clone(),
                    dependers: 2,
                };
            }
            TimelineSub::Multi {
                filters,
                state,
                dependers,
            } => {
                *filters = cur_filters.to_owned();

                if let SubState::NeedsRemote(sub) = state {
                    let remote = sub_remote(pool, cur_filters.to_owned(), "Local only -> Unified");
                    *state = SubState::Unified(UnifiedSubscription {
                        local: *sub,
                        remote,
                    });
                }

                *dependers += 1;
            }
        }
    }

    pub fn unsubscribe_or_decrement(&mut self, ndb: &mut Ndb, pool: &mut RelayPool) {
        match self {
            TimelineSub::NoSub => {}
            TimelineSub::NeedsSub { new_dependers } => {
                if *new_dependers > 1 {
                    *new_dependers -= 1;
                } else {
                    *self = TimelineSub::NoSub;
                }
            }
            TimelineSub::Single { filters: _, state } => {
                unsub(state, ndb, pool);
                *self = TimelineSub::NoSub;
            }
            TimelineSub::Multi {
                filters: _,
                state,
                dependers,
            } => {
                unsub(state, ndb, pool);
                if *dependers > 1 {
                    *dependers -= 1;
                } else {
                    unsub(state, ndb, pool);
                    *self = TimelineSub::NoSub;
                }
            }
        }
    }
}

fn unsub(sub_state: &SubState, ndb: &mut Ndb, pool: &mut RelayPool) {
    match sub_state {
        SubState::RemoteOnly { id } => {
            pool.unsubscribe(id.to_owned());
        }
        SubState::NeedsRemote(subscription) => {
            ndb_unsub(ndb, *subscription, "SubTypeState::NeedsRemote");
        }
        SubState::Unified(unified_subscription) => {
            ndb_unsub(ndb, unified_subscription.local, "SubTypeState::NeedsRemote");
            pool.unsubscribe(unified_subscription.remote.clone());
        }
    }
}

pub struct SubDecrementResponse {
    pub need_unsubscribe: Option<SubState>,
}
