use std::sync::Arc;

use nostrdb::Note;

use super::{ConversationDescriptor, ConversationFilters, ConversationId, ConversationMetadata};

pub const NIP17_RUMOR_KIND: u64 = 14;
const CONVERSATION_TAG: char = 'd';
const CONVERSATION_TAG_STR: &str = "d";

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Nip17GroupId(Arc<str>);

impl Nip17GroupId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(Arc::from(id.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub struct Nip17ConversationDescriptor {
    pub group_id: Nip17GroupId,
    pub metadata: ConversationMetadata,
    pub page_size: usize,
}

impl Nip17ConversationDescriptor {
    pub fn new(group_id: Nip17GroupId) -> Self {
        Self {
            group_id,
            metadata: ConversationMetadata::default(),
            page_size: 256,
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

    pub fn into_descriptor(self) -> ConversationDescriptor {
        let filter = nostrdb::Filter::new()
            .kinds([NIP17_RUMOR_KIND])
            .tags([self.group_id.as_str()], CONVERSATION_TAG)
            .limit(self.page_size as u64)
            .build();

        ConversationDescriptor::new(
            ConversationId::from_nip17(self.group_id.as_str()),
            ConversationFilters::single_local(filter),
        )
        .with_metadata(self.metadata)
        .with_page_size(self.page_size)
    }
}

pub fn extract_group_id(note: &Note<'_>) -> Option<Nip17GroupId> {
    notedeck::note::event_tag(note, CONVERSATION_TAG_STR).map(Nip17GroupId::new)
}
