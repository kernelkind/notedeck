use std::collections::{BTreeSet, HashSet};

use enostr::{Pubkey, RelayPool};
use nostrdb::{Filter, Ndb, Note, NoteKey, Subscription, Transaction};

pub struct Contacts {
    pub filter: Filter,
    pub(super) state: ContactState,
}

pub enum ContactState {
    Unreceived(UnreceivedState),
    Received {
        contacts: HashSet<Pubkey>,
        note_key: NoteKey,
    },
}

pub enum UnreceivedState {
    TallyEose(EoseTally),
    NoContacts,
}

#[derive(Default)]
pub struct EoseTally {
    pub(super) eose_from: BTreeSet<String>,
}

const EOSE_CONSENSUS_PERCENT: f32 = 0.6;

impl EoseTally {
    pub(super) fn reached_consensus(&mut self, pool: &RelayPool) -> bool {
        let num_pool_urls = pool.urls().len();

        let count_in_both = pool.urls().intersection(&self.eose_from).count();

        let percent_eose = count_in_both as f32 / num_pool_urls as f32;

        if percent_eose >= EOSE_CONSENSUS_PERCENT {
            tracing::info!(
                "Contacts: Got EOSEs from {}% of relays, which is geq than the minimum {}%.",
                percent_eose * 100.0,
                EOSE_CONSENSUS_PERCENT * 100.0,
            );
            return true;
        }

        false
    }
}

impl UnreceivedState {
    pub fn process(&mut self, pool: &RelayPool) {
        let UnreceivedState::TallyEose(eose_tally) = self else {
            return;
        };

        if eose_tally.reached_consensus(pool) {
            *self = UnreceivedState::NoContacts;
        }
    }
}

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum IsFollowing {
    /// We don't have the contact list, so we don't know
    Unknown,

    /// We are follow
    Yes,

    No,
}

impl Contacts {
    pub fn new(ndb: &Ndb, txn: &Transaction, pubkey: &[u8; 32]) -> Self {
        let filter = Filter::new().authors([pubkey]).kinds([3]).limit(1).build();

        let binding = ndb
            .query(txn, &[filter.clone()], 1)
            .expect("query user relays results");

        let res = binding.first();

        let state = match res {
            Some(res) => ContactState::Received {
                contacts: get_contacts_owned(&res.note),
                note_key: res.note_key,
            },
            None => ContactState::Unreceived(UnreceivedState::TallyEose(EoseTally::default())),
        };

        Self { filter, state }
    }

    pub fn is_following(&self, other: &Pubkey) -> IsFollowing {
        match &self.state {
            ContactState::Unreceived(unknown_state) => match unknown_state {
                UnreceivedState::TallyEose(_) => IsFollowing::Unknown,
                UnreceivedState::NoContacts => IsFollowing::No,
            },
            ContactState::Received {
                contacts,
                note_key: _,
            } => {
                if contacts.contains(other) {
                    IsFollowing::Yes
                } else {
                    IsFollowing::No
                }
            }
        }
    }

    pub(super) fn poll_for_updates(&mut self, ndb: &Ndb, txn: &Transaction, sub: Subscription) {
        let nks = ndb.poll_for_notes(sub, 1);

        let Some(key) = nks.first() else {
            return;
        };

        let note = match ndb.get_note_by_key(txn, *key) {
            Ok(note) => note,
            Err(e) => {
                tracing::error!("Could not find note at key {:?}: {e}", key);
                return;
            }
        };

        match &mut self.state {
            ContactState::Unreceived(_) => {
                self.state = ContactState::Received {
                    contacts: get_contacts_owned(&note),
                    note_key: *key,
                };
            }
            ContactState::Received { contacts, note_key } => {
                update_contacts(contacts, &note);
                *note_key = *key;
            }
        }
    }

    pub fn get_state(&self) -> &ContactState {
        &self.state
    }
}

fn get_contacts<'a>(note: &Note<'a>) -> HashSet<&'a [u8; 32]> {
    let mut contacts = HashSet::with_capacity(note.tags().count().into());

    for tag in note.tags() {
        if tag.count() < 2 {
            continue;
        }

        let Some("p") = tag.get_str(0) else {
            continue;
        };

        let Some(cur_id) = tag.get_id(1) else {
            continue;
        };

        contacts.insert(cur_id);
    }

    contacts
}

fn get_contacts_owned(note: &Note<'_>) -> HashSet<Pubkey> {
    get_contacts(note)
        .iter()
        .map(|p| Pubkey::new(**p))
        .collect()
}

fn update_contacts(cur: &mut HashSet<Pubkey>, new: &Note<'_>) {
    let new_contacts = get_contacts(new);

    cur.retain(|pk| new_contacts.contains(pk.bytes()));

    new_contacts.iter().for_each(|c| {
        if !cur.contains(*c) {
            cur.insert(Pubkey::new(**c));
        }
    });
}
