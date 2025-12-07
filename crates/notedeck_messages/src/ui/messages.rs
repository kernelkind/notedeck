use egui::{
    vec2, Align, Button, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, TextEdit,
};
use egui_extras::{Size, StripBuilder};
use enostr::{NoteId, Pubkey};
use nostrdb::{Ndb, ProfileRecord, Transaction};
use notedeck::{name::get_display_name, Images};
use notedeck_ui::ProfilePic;

use crate::cache::{
    parse_chat_message, Conversation, ConversationCache, ConversationId, ConversationMetadata,
    ConversationState, ConversationStates, ConversationSummary, Nip17ChatMessage,
};

pub struct MessagesUi<'a> {
    cache: &'a ConversationCache,
    states: &'a mut ConversationStates,
    ndb: &'a Ndb,
    selected_pubkey: &'a Pubkey,
}

impl<'a> MessagesUi<'a> {
    pub fn new(
        cache: &'a ConversationCache,
        states: &'a mut ConversationStates,
        ndb: &'a Ndb,
        selected_pubkey: &'a Pubkey,
    ) -> Self {
        Self {
            cache,
            states,
            ndb,
            selected_pubkey,
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, img_cache: &mut Images) {
        // Ensure we have a selected conversation when any exist so both panels stay in sync.
        let _ = self.active_conversation_id();

        StripBuilder::new(ui)
            .size(Size::exact(300.0))
            .size(Size::remainder())
            .horizontal(|mut strip| {
                strip.cell(|ui| {
                    self.render_conversation_list_panel(ui);
                });

                strip.cell(|ui| {
                    self.render_conversation_view_panel(ui, img_cache);
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
                    .size(Size::exact(32.0))
                    .size(Size::exact(60.0))
                    .size(Size::remainder())
                    .vertical(|mut strip| {
                        strip.empty();
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
                                    let txn = Transaction::new(self.ndb).expect("txn");

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

                                            let title = conversation_title(
                                                summary.metadata,
                                                &txn,
                                                self.ndb,
                                                self.selected_pubkey,
                                            );

                                            let response = render_summary(
                                                ui,
                                                summary,
                                                Some(id) == active,
                                                &title,
                                            );

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

    fn render_conversation_view_panel(&mut self, ui: &mut egui::Ui, img_cache: &mut Images) {
        let Some(conversation_id) = self.active_conversation_id() else {
            Frame::new()
                .fill(ui.visuals().panel_fill)
                .inner_margin(Margin::same(24))
                .show(ui, |ui| {
                    login_nsec_prompt(ui);
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
        let title = {
            let txn = Transaction::new(self.ndb).expect("txn");
            conversation_title(summary.metadata, &txn, self.ndb, self.selected_pubkey)
        };
        let meta_line = conversation_meta_line(&summary);

        let outer_margin = Margin {
            left: 0,
            right: 0,
            top: 12,
            bottom: 0,
        };

        Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(outer_margin)
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .show(ui, |ui| {
                StripBuilder::new(ui)
                    .size(Size::exact(70.0))
                    .size(Size::remainder())
                    .size(Size::exact(102.0))
                    .vertical(|mut strip| {
                        strip.cell(|ui| {
                            let partner = direct_chat_partner(
                                summary.metadata.participants.as_slice(),
                                self.selected_pubkey,
                            );
                            let txn = Transaction::new(self.ndb).expect("txn");
                            let partner_profile = partner.and_then(|pk| {
                                self.ndb.get_profile_by_pubkey(&txn, pk.bytes()).ok()
                            });
                            conversation_header(
                                ui,
                                &title,
                                &meta_line,
                                img_cache,
                                partner.is_some(),
                                partner_profile.as_ref(),
                            );
                        });

                        strip.cell(|ui| {
                            conversation_history(ui, conversation, state, self.ndb, img_cache);
                        });

                        strip.cell(|ui| {
                            conversation_composer(ui, state);
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

fn conversation_header(
    ui: &mut egui::Ui,
    title: &str,
    meta_line: &str,
    img_cache: &mut Images,
    show_partner_avatar: bool,
    partner_profile: Option<&ProfileRecord<'_>>,
) {
    Frame::new()
        .inner_margin(Margin::symmetric(16, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if show_partner_avatar {
                    ui.add_space(4.0);
                    let mut pic = ProfilePic::from_profile_or_default(img_cache, partner_profile)
                        .size(ProfilePic::medium_size() as f32);
                    ui.add(&mut pic);
                    ui.add_space(8.0);
                }

                ui.vertical(|ui| {
                    ui.heading(title);
                    if !meta_line.is_empty() {
                        ui.label(RichText::new(meta_line).color(ui.visuals().weak_text_color()));
                    }
                });
            })
        });
    ui.separator();
}

fn conversation_history(
    ui: &mut egui::Ui,
    conversation: &Conversation,
    state: &mut ConversationState,
    ndb: &Ndb,
    img_cache: &mut Images,
) {
    Frame::new()
        .inner_margin(Margin::symmetric(16, 0))
        .show(ui, |ui| {
            state
                .list
                .ui_custom_layout(ui, conversation.messages.len(), move |ui, index| {
                    let noteref = conversation.messages.messages_ordered[index];

                    let txn = Transaction::new(ndb).expect("txn");
                    let Ok(note) = ndb.get_note_by_key(&txn, noteref.key) else {
                        tracing::error!("Could not get key {:?}", noteref.key);
                        return 1;
                    };

                    let Some(chat_msg) = parse_chat_message(&note) else {
                        tracing::error!("Could not parse chat message for note {noteref:?}");
                        return 1;
                    };

                    let profile = ndb.get_profile_by_pubkey(&txn, chat_msg.sender()).ok();

                    render_chat_message(ui, chat_msg, img_cache, profile.as_ref());

                    1
                });
        });
}

fn conversation_composer(ui: &mut egui::Ui, state: &mut ConversationState) {
    {
        let rect = ui.available_rect_before_wrap();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::ZERO, ui.visuals().panel_fill);
    }
    let margin = Margin::symmetric(16, 4);
    Frame::new().inner_margin(margin).show(ui, |ui| {
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            let text_height = ui.spacing().item_spacing.y * 1.4;
            let size = vec2(ui.available_width(), text_height);
            // TODO(kernelkind): ideally this will be multiline, but the default multiline impl doesn't work the way
            // signal's multiline works... TBC

            let old = mut_visuals_corner_radius(ui, CornerRadius::same(16));

            let hint_text = RichText::new("Type a message")
                .color(ui.visuals().noninteractive().fg_stroke.color);
            let text_edit = TextEdit::singleline(&mut state.composer)
                .margin(Margin::symmetric(16, 8))
                .vertical_align(Align::Center)
                .hint_text(hint_text)
                .min_size(size);
            text_edit.show(ui);
            restore_widgets_corner_rad(ui, old);

            // tracing::info!("textedit actual size: {:?}", resp.rect.size());
        });
    });
}

/// An unfortunate hack to change the corner radius of a TextEdit...
/// returns old `CornerRadius`
fn mut_visuals_corner_radius(ui: &mut egui::Ui, rad: CornerRadius) -> WidgetsCornerRadius {
    let widgets = &ui.visuals().widgets;
    let old = WidgetsCornerRadius {
        active: widgets.active.corner_radius,
        hovered: widgets.hovered.corner_radius,
        inactive: widgets.inactive.corner_radius,
        noninteractive: widgets.noninteractive.corner_radius,
        open: widgets.open.corner_radius,
    };

    let widgets = &mut ui.visuals_mut().widgets;
    widgets.active.corner_radius = rad;
    widgets.hovered.corner_radius = rad;
    widgets.inactive.corner_radius = rad;
    widgets.noninteractive.corner_radius = rad;
    widgets.open.corner_radius = rad;

    old
}

fn restore_widgets_corner_rad(ui: &mut egui::Ui, old: WidgetsCornerRadius) {
    let widgets = &mut ui.visuals_mut().widgets;

    widgets.active.corner_radius = old.active;
    widgets.hovered.corner_radius = old.hovered;
    widgets.inactive.corner_radius = old.inactive;
    widgets.noninteractive.corner_radius = old.noninteractive;
    widgets.open.corner_radius = old.open;
}

struct WidgetsCornerRadius {
    active: CornerRadius,
    hovered: CornerRadius,
    inactive: CornerRadius,
    noninteractive: CornerRadius,
    open: CornerRadius,
}

pub fn render_summary(
    ui: &mut egui::Ui,
    summary: ConversationSummary,
    selected: bool,
    title: &str,
) -> egui::Response {
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
        .corner_radius(CornerRadius::same(12))
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

pub fn render_chat_message(
    ui: &mut egui::Ui,
    chat_msg: Nip17ChatMessage,
    img_cache: &mut Images,
    profile: Option<&ProfileRecord<'_>>,
) -> egui::Response {
    let sender = sender_label(profile, chat_msg.sender());
    let recipients = format_recipients(chat_msg.recipients());
    let reply = chat_msg.reply_to().map(short_note_id_from_bytes);

    Frame::new()
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    let mut pic = ProfilePic::from_profile_or_default(img_cache, profile)
                        .size(ProfilePic::medium_size() as f32);
                    ui.add(&mut pic);
                });
                ui.add_space(12.0);
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

fn fallback_convo_title(
    participants: &[Pubkey],
    txn: &Transaction,
    ndb: &Ndb,
    current: &Pubkey,
) -> String {
    if participants.is_empty() {
        return "Conversation".to_string();
    }

    let mut others: Vec<&Pubkey> = participants.iter().filter(|pk| *pk != current).collect();

    if let Some(partner) = direct_chat_partner(participants, current) {
        return participant_label(ndb, txn, partner);
    }

    if others.is_empty() {
        others = participants.iter().collect::<Vec<_>>();
    }

    let names: Vec<String> = others
        .iter()
        .map(|pk| participant_label(ndb, txn, pk))
        .collect();

    if names.is_empty() {
        return "Conversation".to_string();
    }

    names.join(", ")
}

fn conversation_title(
    metadata: &ConversationMetadata,
    txn: &Transaction,
    ndb: &Ndb,
    current: &Pubkey,
) -> String {
    metadata
        .title
        .as_ref()
        .map(|t| t.title.clone())
        .unwrap_or_else(|| fallback_convo_title(&metadata.participants, txn, ndb, current))
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

fn direct_chat_partner<'a>(participants: &'a [Pubkey], current: &Pubkey) -> Option<&'a Pubkey> {
    if participants.len() != 2 {
        return None;
    }

    participants.iter().find(|pk| *pk != current)
}

fn participant_label(ndb: &Ndb, txn: &Transaction, pk: &Pubkey) -> String {
    if let Ok(profile) = ndb.get_profile_by_pubkey(txn, pk.bytes()) {
        let name = get_display_name(Some(&profile)).name();
        if name != "??" {
            return name.to_owned();
        }
    }

    short_pubkey(pk)
}

fn sender_label(profile: Option<&ProfileRecord<'_>>, pubkey: &[u8; 32]) -> String {
    if let Some(profile) = profile {
        let display = get_display_name(Some(profile)).name();
        if display != "??" {
            return display.to_owned();
        }
    }

    short_pubkey_from_bytes(pubkey)
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
