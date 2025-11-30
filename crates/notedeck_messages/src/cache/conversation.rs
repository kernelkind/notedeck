use std::{cmp::Ordering, sync::Arc};

use crate::cache::message_store::MessageStore;
use egui_virtual_list::VirtualList;
use enostr::Pubkey;
use hashbrown::{hash_map::Entry, HashMap};
use nostrdb::{Filter, Ndb, NoteKey, QueryResult, Transaction};
use notedeck::NoteRef;
use tracing::{error, info, warn};

const DEFAULT_PAGE_SIZE: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ConversationId(Arc<str>);

impl ConversationId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(Arc::from(id.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn from_nip17(identifier: &str) -> Self {
        Self::new(format!("nip17:{identifier}"))
    }
}

impl PartialOrd for ConversationId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ConversationId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl std::fmt::Display for ConversationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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
pub struct ConversationDescriptor {
    pub id: ConversationId,
    pub filters: ConversationFilters,
    pub metadata: ConversationMetadata,
    pub page_size: usize,
}

impl ConversationDescriptor {
    pub fn new(id: ConversationId, filters: ConversationFilters) -> Self {
        Self {
            id,
            filters,
            metadata: ConversationMetadata::default(),
            page_size: DEFAULT_PAGE_SIZE,
        }
    }

    pub fn with_metadata(mut self, metadata: ConversationMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn with_page_size(mut self, page_size: usize) -> Self {
        self.page_size = page_size.max(1);
        self
    }
}

#[derive(Clone, Debug)]
pub struct ConversationSummary {
    pub id: ConversationId,
    pub metadata: ConversationMetadata,
    pub last_message: Option<NoteRef>,
    pub unread_count: usize,
    pub total_messages: usize,
}

#[derive(Clone, Debug)]
pub struct ConversationHydration {
    pub id: ConversationId,
    pub inserted: Vec<NoteKey>,
    pub summary: ConversationSummary,
    pub was_fresh: bool,
}

#[derive(Clone, Debug)]
pub struct ConversationUpdate {
    pub id: ConversationId,
    pub inserted: Vec<NoteKey>,
    pub summary: ConversationSummary,
}

pub struct ConversationNode {
    pub messages: MessageStore,
    pub list: VirtualList,
    pub set_scroll_offset: Option<f32>,
    pub metadata: ConversationMetadata,
    pub unread_count: usize,
    filters: ConversationFilters,
    page_size: usize,
}

impl ConversationNode {
    fn from_descriptor(descriptor: ConversationDescriptor) -> Self {
        ConversationNode {
            messages: MessageStore::new(),
            list: VirtualList::new(),
            set_scroll_offset: None,
            metadata: descriptor.metadata,
            unread_count: 0,
            filters: descriptor.filters,
            page_size: descriptor.page_size,
        }
    }

    fn summary(&self, id: &ConversationId) -> ConversationSummary {
        ConversationSummary {
            id: id.clone(),
            metadata: self.metadata.clone(),
            last_message: self.messages.latest().copied(),
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

    fn page_limit(&self) -> i32 {
        self.page_size
            .try_into()
            .unwrap_or(i32::try_from(DEFAULT_PAGE_SIZE).unwrap())
    }

    fn filters(&self) -> &[Filter] {
        &self.filters.local
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ConversationOrdering {
    id: ConversationId,
    last_activity: u64,
}

impl Ord for ConversationOrdering {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.last_activity.cmp(&other.last_activity) {
            Ordering::Equal => self.id.cmp(&other.id),
            Ordering::Greater => Ordering::Less,
            Ordering::Less => Ordering::Greater,
        }
    }
}

impl PartialOrd for ConversationOrdering {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Default)]
pub struct ConversationCache {
    nodes: HashMap<ConversationId, ConversationNode>,
    ordering: std::collections::BTreeSet<ConversationOrdering>,
    ordering_handles: HashMap<ConversationId, ConversationOrdering>,
}

impl ConversationCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn summaries_by_activity(&self) -> Vec<ConversationSummary> {
        self.ordering
            .iter()
            .filter_map(|order| {
                self.nodes
                    .get(&order.id)
                    .map(|node| node.summary(&order.id))
            })
            .collect()
    }

    pub fn node(&self, id: &ConversationId) -> Option<&ConversationNode> {
        self.nodes.get(id)
    }

    pub fn node_mut(&mut self, id: &ConversationId) -> Option<&mut ConversationNode> {
        self.nodes.get_mut(id)
    }

    pub fn mark_seen(&mut self, id: &ConversationId) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.unread_count = 0;
        }
    }

    #[profiling::function]
    pub fn open_conversation(
        &mut self,
        ndb: &Ndb,
        txn: &Transaction,
        descriptor: ConversationDescriptor,
    ) -> Option<ConversationHydration> {
        let id = descriptor.id.clone();
        let (inserted, summary, last_activity, was_fresh) = {
            let (node, was_fresh) = match self.nodes.entry(id.clone()) {
                Entry::Occupied(entry) => (entry.into_mut(), false),
                Entry::Vacant(entry) => {
                    let node = ConversationNode::from_descriptor(descriptor);
                    (entry.insert(node), true)
                }
            };

            if node.filters.local.is_empty() {
                warn!("Conversation {} has no local filters", id);
                return None;
            }

            let limit = node.page_limit();
            let filters = node.filters().to_vec();
            let query_res = ndb.query(txn, &filters, limit);
            let refs = match query_res {
                Ok(results) => refs_from_query(results),
                Err(err) => {
                    error!("Failed to hydrate conversation {}: {err}", id);
                    return None;
                }
            };

            let inserted = node.ingest_refs(refs);
            let summary = node.summary(&id);
            let last_activity = node.last_activity();
            (inserted, summary, last_activity, was_fresh)
        };

        self.refresh_ordering(&id, last_activity);

        if inserted.is_empty() && !was_fresh {
            return None;
        }

        info!(
            "Conversation {} hydrated with {} notes (fresh={was_fresh})",
            id,
            inserted.len()
        );

        Some(ConversationHydration {
            id: id.clone(),
            inserted,
            summary,
            was_fresh,
        })
    }

    pub fn ingest_refs<I>(&mut self, id: &ConversationId, notes: I) -> Option<ConversationUpdate>
    where
        I: IntoIterator<Item = NoteRef>,
    {
        let (inserted, summary, last_activity) = {
            let node = self.nodes.get_mut(id)?;
            let inserted = node.ingest_refs(notes);
            if inserted.is_empty() {
                return None;
            }
            let summary = node.summary(id);
            let last_activity = node.last_activity();
            (inserted, summary, last_activity)
        };

        self.refresh_ordering(id, last_activity);

        Some(ConversationUpdate {
            id: id.clone(),
            inserted,
            summary,
        })
    }

    fn refresh_ordering(&mut self, id: &ConversationId, last_activity: u64) {
        if let Some(handle) = self.ordering_handles.remove(id) {
            self.ordering.remove(&handle);
        }

        let ordering = ConversationOrdering {
            id: id.clone(),
            last_activity,
        };
        self.ordering.insert(ordering.clone());
        self.ordering_handles.insert(id.clone(), ordering);
    }
}

fn refs_from_query(results: Vec<QueryResult<'_>>) -> Vec<NoteRef> {
    results
        .into_iter()
        .map(NoteRef::from_query_result)
        .collect()
}
