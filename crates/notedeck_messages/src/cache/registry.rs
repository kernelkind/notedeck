use hashbrown::{hash_map::RawEntryMut, HashMap};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{BuildHasher, Hash, Hasher},
};

use crate::cache::ConversationId;

#[derive(Default)]
pub struct ConversationRegistry {
    next_id: ConversationId,
    conversation_ids: HashMap<ConversationIdentifier, ConversationId>,
}

impl ConversationRegistry {
    pub fn get(&self, id: ConversationIdentifierUnowned) -> Option<ConversationId> {
        let mut normalized = id;
        normalized.normalize();
        let hash = normalized.hash(self.conversation_ids.hasher());
        self.conversation_ids
            .raw_entry()
            .from_hash(hash, |existing| normalized.matches(existing))
            .map(|(_, v)| *v)
    }

    pub fn get_or_insert(&mut self, id: ConversationIdentifierUnowned) -> ConversationId {
        let mut normalized = id;
        normalized.normalize();
        let hash = normalized.hash(self.conversation_ids.hasher());

        match self
            .conversation_ids
            .raw_entry_mut()
            .from_hash(hash, |existing| normalized.matches(existing))
        {
            RawEntryMut::Occupied(entry) => *entry.get(),
            RawEntryMut::Vacant(entry) => {
                let owned = normalized.into_owned();
                let uid = self.next_id;
                entry.insert(owned, uid);
                self.next_id += uid;
                uid
            }
        }
    }

    pub fn insert(&mut self, id: ConversationIdentifier) -> ConversationId {
        let uid = self.next_id;
        self.conversation_ids.insert(id, uid);
        self.next_id += uid;

        uid
    }
}

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub enum ConversationIdentifier {
    Nip17(ConversationParticipants),
}

pub enum ConversationIdentifierUnowned<'a> {
    Nip17(ConversationParticipantsUnowned<'a>),
}

// Set of Pubkeys, sorted and deduplicated
#[derive(Hash, Eq, PartialEq, Debug, Clone)]
struct ConversationParticipants {
    participants: Vec<[u8; 32]>,
    hash: u64,
}

impl ConversationParticipants {
    pub fn new(mut items: Vec<[u8; 32]>) -> Self {
        items.sort();
        items.dedup();
        let hash = Self::hash_participants(&items);
        Self {
            participants: items,
            hash,
        }
    }

    fn hash_participants(items: &[[u8; 32]]) -> u64 {
        let mut hasher = DefaultHasher::new();
        items.hash(&mut hasher);
        hasher.finish()
    }
}

pub struct ConversationParticipantsUnowned<'a>(pub Vec<&'a [u8; 32]>);

impl<'a> ConversationParticipantsUnowned<'a> {
    fn normalize(&mut self) {
        self.0.sort_unstable();
        self.0.dedup();
    }

    fn hash_with<S: BuildHasher>(&self, build_hasher: &S) -> u64 {
        let mut hasher = build_hasher.build_hasher();
        self.0.hash(&mut hasher);
        hasher.finish()
    }

    fn matches(&self, owned: &ConversationParticipants) -> bool {
        self.hash_value() == owned.hash
    }

    fn hash_value(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.0.hash(&mut hasher);
        hasher.finish()
    }

    fn into_owned(self) -> ConversationParticipants {
        let owned = self.0.into_iter().map(|pk| *pk).collect();
        ConversationParticipants::new(owned)
    }
}

impl<'a> ConversationIdentifierUnowned<'a> {
    fn normalize(&mut self) {
        match self {
            Self::Nip17(participants) => participants.normalize(),
        }
    }

    fn hash<S: BuildHasher>(&self, build_hasher: &S) -> u64 {
        match self {
            Self::Nip17(participants) => participants.hash_with(build_hasher),
        }
    }

    fn matches(&self, owned: &ConversationIdentifier) -> bool {
        match (self, owned) {
            (Self::Nip17(left), ConversationIdentifier::Nip17(right)) => left.matches(right),
        }
    }

    fn into_owned(self) -> ConversationIdentifier {
        match self {
            Self::Nip17(participants) => ConversationIdentifier::Nip17(participants.into_owned()),
        }
    }
}
