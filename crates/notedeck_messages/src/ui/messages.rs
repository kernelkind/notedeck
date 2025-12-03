use crate::cache::{ConversationCache, ConversationStates};

pub struct MessagesUi<'a> {
    cache: &'a ConversationCache,
    states: &'a mut ConversationStates,
}

impl<'a> MessagesUi<'a> {
    pub fn new(cache: &'a ConversationCache, states: &'a mut ConversationStates) -> Self {
        Self { cache, states }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {}
}

pub fn login_nsec_prompt(ui: &mut egui::Ui) {
    unimplemented!()
}
