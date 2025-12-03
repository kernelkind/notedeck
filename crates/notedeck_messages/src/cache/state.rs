use crate::cache::ConversationId;
use egui::ahash::HashMap;
use egui_virtual_list::VirtualList;

/// Keep track of the UI state for conversations. Meant to be mutably accessed by UI
#[derive(Default)]
pub struct ConversationStates {
    cache: HashMap<ConversationId, ConversationState>,
    active: Option<ConversationId>,
}

pub struct ConversationState {
    list: VirtualList,
    unread_count: usize,
}
