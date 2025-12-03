use std::collections::HashMap;

use crate::cache::ConversationId;

struct ConversationRegistry {
    next_id: ConversationId,
    conversation_ids: HashMap<ConversationIdentifier, ConversationId>,
}

impl ConversationRegistry {
    pub fn get(&self, id: ConversationIdentifierUnowned) -> Option<ConversationId> {
        unimplemented!()
    }

    pub fn get_or_insert(&mut self, id: ConversationIdentifierUnowned) -> ConversationId {
        unimplemented!()
    }

    pub fn insert(&mut self, id: ConversationIdentifier) -> ConversationId {
        let uid = self.next_id;
        self.conversation_ids.insert(id, uid);
        self.next_id += uid;

        uid
    }
}

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub struct ConversationGroup {
    hash: u64,
    identifier: ConversationIdentifier,
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
struct ConversationParticipants(Vec<[u8; 32]>);

impl ConversationParticipants {
    pub fn new(mut items: Vec<[u8; 32]>) -> Self {
        items.sort();
        items.dedup();
        Self(items)
    }
}

struct ConversationParticipantsUnowned<'a>(Vec<&'a [u8; 32]>);

// easily retrievable from Note<'a>
struct Nip17ChatMessage<'a> {
    receiver: &'a [u8; 32],
    sender: &'a [u8; 32],
    p_tags: Vec<&'a [u8; 32]>,
    subject: Option<&'a str>,
    reply_to: Option<&'a [u8; 32]>, // NoteId
    message: &'a str,
}
