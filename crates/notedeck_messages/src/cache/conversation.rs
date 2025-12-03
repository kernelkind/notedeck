use std::cmp::Ordering;

use crate::cache::ConversationIdentifier;

use super::message_store::MessageStore;
use enostr::Pubkey;
use hashbrown::{hash_map::Entry, HashMap};
use nostrdb::{Filter, FilterBuilder, Ndb, NoteKey, QueryResult, Transaction};
use notedeck::NoteRef;
use tracing::{error, warn};

const DEFAULT_PAGE_SIZE: usize = 256;

pub type ConversationId = u32;
pub struct ConversationCache {
    conversation_ids: HashMap<ConversationIdentifier, ConversationId>,
    conversations: HashMap<ConversationId, Conversation>,
    order: Vec<ConversationOrder>,
}

#[derive(Clone, Copy, Debug)]
struct ConversationOrder {
    id: ConversationId,
    latest: u64,
}

impl ConversationOrder {
    /// Use for removal in BTreeSet
    pub fn only_id(id: ConversationId) -> Self {
        Self { id, latest: 0 }
    }
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
    pub title: Option<String>,
    pub picture_url: Option<String>,
    pub participants: Vec<Pubkey>,
}

#[derive(Clone, Debug, Default)]
pub struct ConversationFilters {
    pub local: Vec<Filter>,
    pub remote: Vec<Filter>,
}

impl ConversationFilters {
    pub fn single_local(filter: Filter) -> Self {
        Self {
            local: vec![filter],
            remote: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.local.is_empty() && self.remote.is_empty()
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
    pub set_scroll_offset: Option<f32>,
    pub metadata: ConversationMetadata,
    pub unread_count: usize,
    filters: ConversationFilters,
}

impl Conversation {
    fn summary<'a>(&'a self) -> ConversationSummary<'a> {
        ConversationSummary {
            metadata: &self.metadata,
            last_message: self.messages.latest(),
            unread_count: self.unread_count,
            total_messages: self.messages.len(),
        }
    }

    fn last_activity(&self) -> u64 {
        self.messages.newest_timestamp().unwrap_or(0)
    }

    fn ingest_refs<I>(&mut self, notes: I) -> Vec<NoteKey>
    where
        I: IntoIterator<Item = NoteRef>,
    {
        let inserted = self.messages.extend(notes);
        if inserted.is_empty() {
            return Vec::new();
        }

        self.unread_count += inserted.len();
        inserted.into_iter().map(|r| r.key).collect()
    }

    fn filters(&self) -> &[Filter] {
        &self.filters.local
    }
}

impl Default for ConversationCache {
    fn default() -> Self {
        Self {
            conversation_ids: HashMap::new(),
            conversations: HashMap::new(),
            order: Vec::new(),
        }
    }
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

    #[profiling::function]
    pub fn open_conversation(&mut self, ndb: &Ndb, txn: &Transaction, id: ConversationId) {}
}

fn refs_from_query(results: Vec<QueryResult<'_>>) -> Vec<NoteRef> {
    results
        .into_iter()
        .map(NoteRef::from_query_result)
        .collect()
}

fn get_conversations(ndb: &Ndb, txn: &Transaction, cur_acc: &Pubkey) {
    let res = ndb.query(txn, &conversation_filter(cur_acc), 300);
}

fn conversation_filter(cur_acc: &Pubkey) -> Vec<Filter> {
    vec![
        FilterBuilder::new()
            .authors([cur_acc.bytes()])
            .kinds([14])
            .build(),
        FilterBuilder::new()
            .kinds([14])
            .pubkey([cur_acc.bytes()])
            .build(),
    ]
}
