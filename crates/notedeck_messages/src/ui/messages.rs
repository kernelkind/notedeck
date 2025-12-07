use egui::{
    Align, Button, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke, TextEdit,
};
use egui_extras::{Size, StripBuilder};
use enostr::{NoteId, Pubkey};
use nostrdb::{Ndb, Transaction};

use crate::cache::{
    parse_chat_message, ConversationCache, ConversationId, ConversationMetadata,
    ConversationStates, ConversationSummary, Nip17ChatMessage,
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
        // Ensure we have a selected conversation when any exist so both panels stay in sync.
        let _ = self.active_conversation_id();

        StripBuilder::new(ui)
            .size(Size::relative(0.33))
            .size(Size::remainder())
            .horizontal(|mut strip| {
                strip.cell(|ui| {
                    self.render_conversation_list_panel(ui);
                });

                strip.cell(|ui| {
                    self.render_conversation_view_panel(ui);
                });
            });
    }

    fn render_conversation_list_panel(&mut self, ui: &mut egui::Ui) {
        Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .inner_margin(Margin::symmetric(12, 10))
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .show(ui, |ui| {
                StripBuilder::new(ui)
                    .size(Size::exact(60.0))
                    .size(Size::remainder())
                    .vertical(|mut strip| {
                        strip.cell(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.heading("Chats");
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.add_enabled(false, Button::new("+ New"));
                                    });
                                });
                                ui.add_space(4.0);
                                ui.separator();
                            });
                        });

                        strip.cell(|ui| {
                            if self.cache.is_empty() {
                                ui.centered_and_justified(|ui| {
                                    ui.label("No conversations yet");
                                });
                                return;
                            }

                            ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    let num_convos = self.cache.len();
                                    let mut active = self.states.active;

                                    self.states.convos_list.ui_custom_layout(
                                        ui,
                                        num_convos,
                                        |ui, index| {
                                            let Some(id) =
                                                self.cache.get_id_by_index(index).copied()
                                            else {
                                                return 0;
                                            };

                                            let Some(summary) =
                                                self.cache.get_summary_by_index(index)
                                            else {
                                                return 0;
                                            };

                                            let response =
                                                render_summary(ui, summary, Some(id) == active);

                                            if response.clicked() {
                                                self.states.active = Some(id);
                                                active = Some(id);
                                            }

                                            1
                                        },
                                    );
                                });
                        });
                    });
            });
    }

    fn render_conversation_view_panel(&mut self, ui: &mut egui::Ui) {
        let Some(conversation_id) = self.active_conversation_id() else {
            Frame::new()
                .fill(ui.visuals().panel_fill)
                .inner_margin(Margin::same(24))
                .show(ui, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.heading("Select a conversation");
                        ui.label("Choose a chat from the left to start messaging.");
                    });
                });
            return;
        };

        let Some(conversation) = self.cache.get(conversation_id) else {
            tracing::error!("don't have conversation for id {conversation_id}");
            return;
        };

        let state = self.states.get_or_insert(conversation_id);
        let summary = ConversationSummary {
            metadata: &conversation.metadata,
            last_message: conversation.messages.latest(),
            unread_count: state.unread_count,
            total_messages: conversation.messages.len(),
        };
        let title = conversation_title(summary.metadata);
        let meta_line = conversation_meta_line(&summary);

        Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(Margin::symmetric(12, 10))
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .show(ui, |ui| {
                StripBuilder::new(ui)
                    .size(Size::exact(70.0))
                    .size(Size::remainder())
                    .size(Size::exact(80.0))
                    .vertical(|mut strip| {
                        strip.cell(|ui| {
                            ui.vertical(|ui| {
                                ui.heading(title);
                                if !meta_line.is_empty() {
                                    ui.label(
                                        RichText::new(meta_line)
                                            .color(ui.visuals().weak_text_color()),
                                    );
                                }
                            });
                            ui.add_space(4.0);
                            ui.separator();
                        });

                        strip.cell(|ui| {
                            state.list.ui_custom_layout(
                                ui,
                                conversation.messages.len(),
                                |ui, index| {
                                    let noteref = conversation.messages.messages_ordered[index];

                                    let txn = Transaction::new(self.ndb).expect("txn");
                                    let Ok(note) = self.ndb.get_note_by_key(&txn, noteref.key)
                                    else {
                                        return 1;
                                    };

                                    let Some(chat_msg) = parse_chat_message(&note) else {
                                        return 1;
                                    };

                                    render_chat_message(ui, chat_msg);

                                    1
                                },
                            );
                        });

                        strip.cell(|ui| {
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                let text_edit = TextEdit::singleline(&mut state.composer)
                                    .hint_text("Type a message")
                                    .desired_width(f32::INFINITY);
                                ui.add(text_edit);

                                let send =
                                    ui.add_enabled(!state.composer.is_empty(), Button::new("Send"));

                                if send.clicked() {
                                    state.composer.clear();
                                }
                            });
                        });
                    });
            });
    }

    fn active_conversation_id(&mut self) -> Option<ConversationId> {
        if let Some(id) = self.states.active {
            if self.cache.get(id).is_some() {
                return Some(id);
            }

            self.states.active = None;
        }

        let Some(first) = self.cache.first_convo_id() else {
            return None;
        };
        self.states.active = Some(first);
        Some(first)
    }
}

pub fn render_summary(
    ui: &mut egui::Ui,
    summary: ConversationSummary,
    selected: bool,
) -> egui::Response {
    let title = conversation_title(summary.metadata);
    let meta_line = conversation_meta_line(&summary);
    let unread = summary.unread_count;
    let visuals = ui.visuals();
    let fill = if selected {
        visuals.selection.bg_fill
    } else {
        visuals.extreme_bg_color
    };
    let stroke = if selected {
        visuals.selection.stroke
    } else {
        visuals.widgets.noninteractive.bg_stroke
    };

    Frame::new()
        .fill(fill)
        .stroke(stroke)
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

fn conversation_title(metadata: &ConversationMetadata) -> String {
    metadata
        .title
        .as_ref()
        .map(|t| t.title.clone())
        .unwrap_or_else(|| fallback_convo_title(&metadata.participants))
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
