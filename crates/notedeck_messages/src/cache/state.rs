use std::collections::HashMap;

use crate::cache::ConversationId;
use egui_virtual_list::VirtualList;

/// Keep track of the UI state for conversations. Meant to be mutably accessed by UI
#[derive(Default)]
pub struct ConversationStates {
    cache: HashMap<ConversationId, ConversationState>,
    pub convos_list: VirtualList,
}

impl ConversationStates {
    pub fn new() -> Self {
        let mut convos_list = VirtualList::new();
        convos_list.hide_on_resize(None);
        Self {
            cache: Default::default(),
            convos_list,
        }
    }
    pub fn get_or_insert(&mut self, id: ConversationId) -> &mut ConversationState {
        self.cache
            .entry(id)
            .or_insert_with(|| ConversationState::default())
    }
}

#[derive(Default)]
pub struct ConversationState {
    pub list: VirtualList,
    pub unread_count: usize,
    pub composer: String,
}
