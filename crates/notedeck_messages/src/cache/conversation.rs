use std::cmp::Ordering;

use crate::cache::registry::{
    ConversationIdentifierUnowned, ConversationParticipantsUnowned, ConversationRegistry,
};

use super::message_store::MessageStore;
use enostr::Pubkey;
use hashbrown::HashMap;
use nostrdb::{Filter, FilterBuilder, Ndb, Note, QueryResult, Subscription, Transaction};
use notedeck::{note::event_tag, NoteRef};

pub type ConversationId = u32;

pub struct ConversationCache {
    registry: ConversationRegistry,
    conversations: HashMap<ConversationId, Conversation>,
    order: Vec<ConversationOrder>,
    pub initialized_convos: bool,
}

impl ConversationCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.conversations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.conversations.is_empty()
    }

    pub fn get(&self, id: ConversationId) -> Option<&Conversation> {
        self.conversations.get(&id)
    }

    pub fn get_id_by_index(&self, i: usize) -> Option<&ConversationId> {
        Some(&self.order.get(i)?.id)
    }

    pub fn get_summary_by_index(&self, i: usize) -> Option<ConversationSummary> {
        Some(self.conversations.get(self.get_id_by_index(i)?)?.summary())
    }

    /// A conversation is "closed" when the user navigates away from it. This is to close the ndb sub
    pub fn close_conversation(&mut self, ndb: &mut Ndb, id: ConversationId) {
        let Some(conversation) = self.conversations.get_mut(&id) else {
            return;
        };

        let ConversationActivity::Active(sub) = conversation.state else {
            return;
        };

        if let Err(e) = ndb.unsubscribe(sub) {
            tracing::error!("ndb could not unsub: {e:?}");
        }
    }

    /// A conversation is "opened" when the user navigates to the conversation
    #[profiling::function]
    pub fn open_conversation(&mut self, ndb: &Ndb, txn: &Transaction, id: ConversationId) {
        let Some(conversation) = self.conversations.get_mut(&id) else {
            return;
        };

        let pubkeys = conversation.metadata.participants.clone();
        let participants: Vec<&[u8; 32]> = pubkeys.iter().map(|p| p.bytes()).collect();

        let chatroom_filter = chatroom_filter(participants);
        let results = match ndb.query(txn, &chatroom_filter, 200) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("problem with chatroom filter ndb::query: {e:?}");
                return;
            }
        };

        let mut updated = false;
        for res in results {
            updated |= conversation.ingest_kind_14(res);
        }

        if updated {
            let latest = conversation.last_activity();
            refresh_order(&mut self.order, id, latest);
        }

        let sub = match ndb.subscribe(&chatroom_filter) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to ndb::subscribe to chatroom filter: {e:?}");
                return;
            }
        };

        conversation.state = ConversationActivity::Active(sub);
    }

    /// check for updates on an already opened conversation
    pub fn check_for_updates(&mut self, ndb: &Ndb, txn: &Transaction, id: ConversationId) {
        let Some(conversation) = self.conversations.get_mut(&id) else {
            return;
        };

        let ConversationActivity::Active(sub) = conversation.state else {
            tracing::warn!("attempting to check for updates on a stale conversation");
            return;
        };

        let notes = ndb.poll_for_notes(sub, 10);

        let mut updated = false;
        for key in notes {
            let Ok(note) = ndb.get_note_by_key(txn, key) else {
                continue;
            };

            updated |= conversation.messages.insert(NoteRef {
                key,
                created_at: note.created_at(),
            });
        }

        if updated {
            let latest = conversation.last_activity();
            refresh_order(&mut self.order, id, latest);
        }
    }

    pub fn init_conversations(&mut self, ndb: &Ndb, txn: &Transaction, cur_acc: &Pubkey) {
        let Some(results) = get_conversations(ndb, txn, cur_acc) else {
            tracing::warn!("Got no conversations from ndb");
            return;
        };

        tracing::trace!("Received {} conversations from ndb", results.len());

        for res in results {
            let participants = get_p_tags(&res.note);
            let id = self
                .registry
                .get_or_insert(ConversationIdentifierUnowned::Nip17(
                    ConversationParticipantsUnowned(participants.clone()),
                ));

            let conversation = self.conversations.entry(id).or_insert_with(|| {
                let participants: Vec<Pubkey> =
                    participants.into_iter().map(|p| Pubkey::new(*p)).collect();

                Conversation::new(participants)
            });

            tracing::trace!("ingesting into conversation: {:?}", res.note.json());
            if conversation.ingest_kind_14(res) {
                let latest = conversation.last_activity();
                refresh_order(&mut self.order, id, latest);
            }
        }
    }

    pub fn first_convo_id(&self) -> Option<ConversationId> {
        Some(self.order.first()?.id)
    }
}

fn refresh_order(order: &mut Vec<ConversationOrder>, id: ConversationId, latest: u64) {
    if let Some(pos) = order.iter().position(|entry| entry.id == id) {
        order.remove(pos);
    }

    let entry = ConversationOrder { id, latest };
    let idx = match order.binary_search(&entry) {
        Ok(idx) | Err(idx) => idx,
    };
    order.insert(idx, entry);
}

fn get_p_tags<'a>(note: &Note<'a>) -> Vec<&'a [u8; 32]> {
    let mut items = Vec::new();
    for tag in note.tags() {
        if tag.count() < 2 {
            continue;
        }

        if tag.get_str(0) != Some("p") {
            continue;
        }

        let Some(item) = tag.get_id(1) else {
            continue;
        };

        items.push(item);
    }

    items
}

#[derive(Clone, Copy, Debug)]
struct ConversationOrder {
    id: ConversationId,
    latest: u64,
}

// Equality is *only by id*.
// This allows BTreeSet::take/remove using only the id (latest ignored).
impl PartialEq for ConversationOrder {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for ConversationOrder {}

// Ordering is by:
//   1. latest DESC (newest first)
//   2. id ASC      (stable tie-breaker)
// This determines where it sits inside the BTreeSet.
impl PartialOrd for ConversationOrder {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ConversationOrder {
    fn cmp(&self, other: &Self) -> Ordering {
        // newer first
        match other.latest.cmp(&self.latest) {
            Ordering::Equal => self.id.cmp(&other.id),
            non_eq => non_eq,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ConversationMetadata {
    pub title: Option<TitleMetadata>,
    pub participants: Vec<Pubkey>,
}

#[derive(Clone, Debug)]
pub struct TitleMetadata {
    pub title: String,
    pub last_modified: u64,
}

impl ConversationMetadata {
    pub fn new(participants: Vec<Pubkey>) -> Self {
        Self {
            title: None,
            participants,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConversationSummary<'a> {
    pub metadata: &'a ConversationMetadata,
    pub last_message: Option<&'a NoteRef>,
    pub unread_count: usize,
    pub total_messages: usize,
}

pub struct Conversation {
    pub messages: MessageStore,
    pub state: ConversationActivity,
    pub metadata: ConversationMetadata,
}

pub enum ConversationActivity {
    Active(Subscription),
    Stale,
}

impl Conversation {
    pub fn new(participants: Vec<Pubkey>) -> Self {
        Self {
            messages: MessageStore::default(),
            metadata: ConversationMetadata::new(participants),
            state: ConversationActivity::Stale,
        }
    }

    fn summary<'a>(&'a self) -> ConversationSummary<'a> {
        ConversationSummary {
            metadata: &self.metadata,
            last_message: self.messages.latest(),
            unread_count: 0, // TODO: fix
            total_messages: self.messages.len(),
        }
    }

    fn last_activity(&self) -> u64 {
        self.messages.newest_timestamp().unwrap_or(0)
    }

    pub fn ingest_kind_14(&mut self, kind_14_res: QueryResult) -> bool {
        if kind_14_res.note.kind() != 14 {
            tracing::error!("tried to ingest a non-kind 14 note...");
            return false;
        }

        let res = kind_14_res;

        if let Some(title) = event_tag(&res.note, "subject") {
            let created = res.note.created_at();

            if self
                .metadata
                .title
                .as_ref()
                .map_or(true, |cur| created > cur.last_modified)
            {
                self.metadata.title = Some(TitleMetadata {
                    title: title.to_string(),
                    last_modified: created,
                });
            }
        }

        self.messages.insert(NoteRef {
            key: res.note_key,
            created_at: res.note.created_at(),
        })
    }
}

impl Default for ConversationCache {
    fn default() -> Self {
        Self {
            registry: ConversationRegistry::default(),
            conversations: HashMap::new(),
            order: Vec::new(),
            initialized_convos: false,
        }
    }
}

fn get_conversations<'a>(
    ndb: &Ndb,
    txn: &'a Transaction,
    cur_acc: &Pubkey,
) -> Option<Vec<QueryResult<'a>>> {
    match ndb.query(txn, &conversation_filter(cur_acc), 300) {
        Ok(r) => Some(r),
        Err(e) => {
            tracing::error!("error fetching kind 14 messages: {e}");
            None
        }
    }
}

fn conversation_filter(cur_acc: &Pubkey) -> Vec<Filter> {
    // vec![
    //     FilterBuilder::new()
    //         .authors([cur_acc.bytes()])
    //         .kinds([14])
    //         .build(),
    //     FilterBuilder::new()
    //         .kinds([14])
    //         .pubkey([cur_acc.bytes()])
    //         .build(),
    // ]
    vec![FilterBuilder::new()
        .kinds([14])
        .pubkey([cur_acc.bytes()])
        .build()]
}

fn chatroom_filter(participants: Vec<&[u8; 32]>) -> Vec<Filter> {
    let num_participants = participants.len();
    vec![FilterBuilder::new()
        .kinds([14])
        .pubkey(participants)
        .custom(move |note| {
            let mut p_tags = 0;
            for tag in note.tags() {
                if tag.get_str(0) != Some("p") {
                    continue;
                }
                p_tags += 1;

                if p_tags > num_participants {
                    return false;
                }
            }
            if p_tags != num_participants {
                return false;
            }

            true
        })
        .build()]
}

// easily retrievable from Note<'a>
pub struct Nip17ChatMessage<'a> {
    sender: &'a [u8; 32],
    p_tags: Vec<&'a [u8; 32]>,
    subject: Option<&'a str>,
    reply_to: Option<&'a [u8; 32]>, // NoteId
    message: &'a str,
}

impl<'a> Nip17ChatMessage<'a> {
    pub fn sender(&self) -> &'a [u8; 32] {
        self.sender
    }

    pub fn recipients(&self) -> &[&'a [u8; 32]] {
        &self.p_tags
    }

    pub fn subject(&self) -> Option<&'a str> {
        self.subject
    }

    pub fn reply_to(&self) -> Option<&'a [u8; 32]> {
        self.reply_to
    }

    pub fn message(&self) -> &'a str {
        self.message
    }
}

pub fn parse_chat_message<'a>(note: &Note<'a>) -> Option<Nip17ChatMessage<'a>> {
    if note.kind() != 14 {
        return None;
    }

    let mut p_tags = Vec::new();
    let mut subject = None;
    let mut reply_to = None;

    for tag in note.tags() {
        if tag.count() < 2 {
            continue;
        }
        let Some(first) = tag.get_str(0) else {
            continue;
        };

        if first == "p" {
            if let Some(id) = tag.get_id(1) {
                p_tags.push(id);
            }
        } else if first == "subject" {
            subject = tag.get_str(1);
        } else if first == "e" {
            reply_to = tag.get_id(1);
        }
    }

    Some(Nip17ChatMessage {
        sender: note.pubkey(),
        p_tags,
        subject,
        reply_to,
        message: note.content(),
    })
}

#[cfg(test)]
mod tests {
    use nostrdb::{Config, Ndb};

    #[test]
    fn test_giftwrap() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        let map_size = 1024usize * 1024usize * 1024usize * 1024usize;
        let config = Config::new().set_ingester_threads(2).set_mapsize(map_size);

        let mut ndb = Ndb::new(&path.to_string_lossy(), &config).unwrap();
    }
}
