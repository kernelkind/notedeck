use std::fmt::Display;

use egui_nav::ReturnType;
use enostr::{Filter, NoteId, RelayPool};
use hashbrown::HashMap;
use nostrdb::{Ndb, Subscription};
use notedeck::UnifiedSubscription;
use uuid::Uuid;

use crate::timeline::ThreadSelection;

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

#[derive(Clone)]
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

impl std::fmt::Debug for TimelineSub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSub => write!(f, "NoSub"),
            Self::NeedsSub { new_dependers } => f
                .debug_struct("NeedsSub")
                .field("new_dependers", new_dependers)
                .finish(),
            Self::Single { filters: _, state } => {
                f.debug_struct("Single").field("state", state).finish()
            }
            Self::Multi {
                filters: _,
                state,
                dependers,
            } => f
                .debug_struct("Multi")
                .field("state", state)
                .field("dependers", dependers)
                .finish(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum SubState {
    RemoteOnly { id: String },
    NeedsRemote(Subscription),
    Unified(UnifiedSubscription),
}

impl TimelineSub {
    pub fn increment(&mut self) {
        let before = self.clone();
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

        tracing::info!("increment: {:?} -> {:?}", before, self);
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
        let before = self.clone();
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
                    dependers: *new_dependers,
                    filters: filters.to_vec(),
                };
            }
            _ => {}
        }

        tracing::info!("add_local: {:?} -> {:?}", before, self);
    }

    /// TODO(kernelkind): If the provided filter is different from what is present,
    /// we will unsubscribe and resubscribe with the new filter.
    pub fn subscribe_or_increment(
        &mut self,
        cur_filters: &[Filter],
        ndb: &Ndb,
        pool: &mut RelayPool,
    ) {
        let before = self.clone();
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
                    dependers: *new_dependers,
                };
            }
            TimelineSub::Single { filters: _, state } => {
                if let SubState::NeedsRemote(sub) = state {
                    let remote = sub_remote(pool, cur_filters.to_owned(), "Local only -> Unified");
                    let new_state = SubState::Unified(UnifiedSubscription {
                        local: *sub,
                        remote,
                    });
                    *self = TimelineSub::Single {
                        filters: cur_filters.to_owned(),
                        state: new_state,
                    };
                } else {
                    *self = TimelineSub::Multi {
                        filters: cur_filters.to_owned(),
                        state: state.clone(),
                        dependers: 2,
                    };
                }
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
                } else {
                    *dependers += 1;
                }
            }
        }
        tracing::info!("subscribe_or_increment: {:?} => {:?}", before, self);
    }

    pub fn unsubscribe_or_decrement(&mut self, ndb: &mut Ndb, pool: &mut RelayPool) {
        let before = self.clone();
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
        tracing::info!("unsubscribe_or_decrement: {:?} => {:?}", before, self);
    }

    pub fn needs_remote(&self) -> bool {
        match self {
            TimelineSub::NoSub => true,
            TimelineSub::NeedsSub { new_dependers: _ } => true,
            TimelineSub::Single { filters: _, state } => match state {
                SubState::RemoteOnly { id: _ } => false,
                SubState::NeedsRemote(_) => true,
                SubState::Unified(_) => false,
            },
            TimelineSub::Multi {
                filters: _,
                state,
                dependers: _,
            } => match state {
                SubState::RemoteOnly { id: _ } => false,
                SubState::NeedsRemote(_) => true,
                SubState::Unified(_) => false,
            },
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
