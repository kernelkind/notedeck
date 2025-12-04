use egui::ScrollArea;
use egui_extras::{Size, StripBuilder};
use nostrdb::{Ndb, Transaction};

use crate::cache::{
    parse_chat_message, ConversationCache, ConversationStates, ConversationSummary,
    Nip17ChatMessage,
};

pub struct MessagesUi<'a> {
    cache: &'a ConversationCache,
    states: &'a mut ConversationStates,
    ndb: &'a Ndb,
}

impl<'a> MessagesUi<'a> {
    pub fn new(
        cache: &'a ConversationCache,
        states: &'a mut ConversationStates,
        ndb: &'a Ndb,
    ) -> Self {
        Self { cache, states, ndb }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        StripBuilder::new(ui)
            .size(Size::exact(200.0))
            .size(Size::remainder())
            .vertical(|mut strip| {
                strip.cell(|ui| {
                    ScrollArea::vertical().show(ui, |ui| {
                        let num_convos = self.cache.len();

                        self.states
                            .convos_list
                            .ui_custom_layout(ui, num_convos, |ui, index| {
                                let Some(summary) = self.cache.get_summary_by_index(index) else {
                                    return 0;
                                };
                                render_summary(ui, summary);

                                1
                            });
                    });
                });

                strip.cell(|ui| {
                    let id = self.states.active.unwrap_or({
                        let Some(id) = self.cache.first_convo_id() else {
                            return;
                        };
                        id
                    });

                    let Some(conversation) = self.cache.get(id) else {
                        return;
                    };

                    let state = self.states.get_or_insert(id);

                    state
                        .list
                        .ui_custom_layout(ui, conversation.messages.len(), |ui, index| {
                            let noteref = conversation.messages.messages_ordered[index];

                            let txn = Transaction::new(self.ndb).expect("txn");
                            let Ok(note) = self.ndb.get_note_by_key(&txn, noteref.key) else {
                                return 1;
                            };

                            let Some(chat_msg) = parse_chat_message(&note) else {
                                return 1;
                            };

                            render_chat_message(ui, chat_msg);

                            1
                        });
                });
            });
    }
}

pub fn render_summary(ui: &mut egui::Ui, summary: ConversationSummary) -> egui::Response {
    unimplemented!()
}

pub fn render_chat_message(ui: &mut egui::Ui, chat_msg: Nip17ChatMessage) -> egui::Response {
    unimplemented!()
}

pub fn login_nsec_prompt(ui: &mut egui::Ui) {
    unimplemented!()
}
