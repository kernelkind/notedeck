use egui::{Align, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea};
use egui_extras::{Size, StripBuilder};
use enostr::{NoteId, Pubkey};
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
    let title = summary
        .metadata
        .title
        .as_ref()
        .map(|t| t.title.clone())
        .unwrap_or_else(|| fallback_convo_title(&summary.metadata.participants));
    let meta_line = conversation_meta_line(&summary);
    let unread = summary.unread_count;

    Frame::new()
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).strong());

                    if !meta_line.is_empty() {
                        ui.label(RichText::new(meta_line).color(ui.visuals().weak_text_color()));
                    }
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if unread > 0 {
                        Frame::new()
                            .fill(ui.visuals().selection.bg_fill)
                            .stroke(ui.visuals().selection.stroke)
                            .corner_radius(CornerRadius::same(12))
                            .inner_margin(Margin::symmetric(8, 2))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(unread.to_string())
                                        .color(ui.visuals().selection.stroke.color)
                                        .strong(),
                                );
                            });
                    }
                });
            });
        })
        .response
}

pub fn render_chat_message(ui: &mut egui::Ui, chat_msg: Nip17ChatMessage) -> egui::Response {
    let sender = short_pubkey_from_bytes(chat_msg.sender());
    let recipients = format_recipients(chat_msg.recipients());
    let reply = chat_msg.reply_to().map(short_note_id_from_bytes);

    Frame::new()
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(sender).strong());
                    if let Some(subject) = chat_msg.subject() {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(subject)
                                .italics()
                                .color(ui.visuals().weak_text_color()),
                        );
                    }
                });

                if let Some(recipients) = recipients {
                    ui.label(
                        RichText::new(format!("To: {recipients}"))
                            .color(ui.visuals().weak_text_color()),
                    );
                }

                if let Some(reply_id) = reply {
                    ui.label(
                        RichText::new(format!("↩ {reply_id}"))
                            .color(ui.visuals().weak_text_color()),
                    );
                }

                ui.add_space(4.0);
                ui.label(chat_msg.message());
            });
        })
        .response
}

pub fn login_nsec_prompt(ui: &mut egui::Ui) {
    ui.centered_and_justified(|ui| {
        ui.vertical(|ui| {
            ui.heading("Add your private key");
            ui.label(
                "Messages are end-to-end encrypted. Add your nsec in Accounts to read and send chats.",
            );
        });
    });
}

fn fallback_convo_title(participants: &[Pubkey]) -> String {
    if participants.is_empty() {
        return "Conversation".to_string();
    }

    const MAX_SHOWN: usize = 3;
    let mut labels: Vec<String> = participants
        .iter()
        .take(MAX_SHOWN)
        .map(short_pubkey)
        .collect();

    if participants.len() > MAX_SHOWN {
        labels.push(format!("+{} more", participants.len() - MAX_SHOWN));
    }

    labels.join(", ")
}

fn conversation_meta_line(summary: &ConversationSummary<'_>) -> String {
    let mut parts = Vec::new();
    let participant_count = summary.metadata.participants.len();
    if participant_count > 0 {
        let plural = if participant_count == 1 { "" } else { "s" };
        parts.push(format!("{participant_count} participant{plural}"));
    }

    if summary.total_messages > 0 {
        let plural = if summary.total_messages == 1 { "" } else { "s" };
        parts.push(format!("{} message{plural}", summary.total_messages));
    } else {
        parts.push("No messages yet".to_string());
    }

    parts.join(" • ")
}

fn format_recipients(recipients: &[&[u8; 32]]) -> Option<String> {
    if recipients.is_empty() {
        return None;
    }

    Some(
        recipients
            .iter()
            .map(|pk| short_pubkey_from_bytes(pk))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

fn short_pubkey(pk: &Pubkey) -> String {
    short_hex(&pk.hex())
}

fn short_pubkey_from_bytes(bytes: &[u8; 32]) -> String {
    short_pubkey(&Pubkey::new(*bytes))
}

fn short_note_id_from_bytes(bytes: &[u8; 32]) -> String {
    short_hex(&NoteId::new(*bytes).hex())
}

fn short_hex(hex: &str) -> String {
    const START: usize = 8;
    const END: usize = 4;
    if hex.len() <= START + END {
        hex.to_owned()
    } else {
        format!("{}…{}", &hex[..START], &hex[hex.len() - END..])
    }
}
