use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use egui::{
    vec2, Align, Button, Color32, CornerRadius, Frame, Key, Layout, Margin, RichText, ScrollArea,
    Sense, TextEdit,
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

#[derive(Debug)]
pub enum MessagesAction {
    SendMessage {
        conversation_id: ConversationId,
        content: String,
    },
}

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

    pub fn ui(&mut self, ui: &mut egui::Ui, img_cache: &mut Images) -> Option<MessagesAction> {
        // Ensure we have a selected conversation when any exist so both panels stay in sync.
        let _ = self.active_conversation_id();
        let mut action = None;

        StripBuilder::new(ui)
            .size(Size::exact(300.0))
            .size(Size::remainder())
            .horizontal(|mut strip| {
                strip.cell(|ui| {
                    self.render_conversation_list_panel(ui, img_cache);
                });

                strip.cell(|ui| {
                    let panel_action = self.render_conversation_view_panel(ui, img_cache);
                    if action.is_none() {
                        action = panel_action;
                    }
                });
            });

        action
    }

    fn render_conversation_list_panel(&mut self, ui: &mut egui::Ui, img_cache: &mut Images) {
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
                                    let txn_ref = &txn;

                                    self.states.convos_list.ui_custom_layout(
                                        ui,
                                        num_convos,
                                        |ui, index| {
                                            let Some(id) =
                                                self.cache.get_id_by_index(index).copied()
                                            else {
                                                return 1;
                                            };

                                            let Some(summary) =
                                                self.cache.get_summary_by_index(index)
                                            else {
                                                return 1;
                                            };

                                            let title = conversation_title(
                                                summary.metadata,
                                                txn_ref,
                                                self.ndb,
                                                self.selected_pubkey,
                                            );

                                            let partner = direct_chat_partner(
                                                summary.metadata.participants.as_slice(),
                                                self.selected_pubkey,
                                            );
                                            let partner_profile = partner.and_then(|pk| {
                                                self.ndb
                                                    .get_profile_by_pubkey(txn_ref, pk.bytes())
                                                    .ok()
                                            });

                                            // tracing::info!(
                                            //     "click: {:?}, did click: {:?}",
                                            //     ui.ctx().input(|i| i.pointer.interact_pos()),
                                            //     ui.ctx().input(|i| i.pointer.any_click())
                                            // );
                                            let response = render_summary(
                                                ui,
                                                summary,
                                                Some(id) == active,
                                                &title,
                                                partner.is_some(),
                                                partner_profile.as_ref(),
                                                img_cache,
                                            );

                                            if response.clicked() {
                                                tracing::info!("CLICKED SUMMARY {id}");
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

    fn render_conversation_view_panel(
        &mut self,
        ui: &mut egui::Ui,
        img_cache: &mut Images,
    ) -> Option<MessagesAction> {
        let Some(conversation_id) = self.active_conversation_id() else {
            Frame::new()
                .fill(ui.visuals().panel_fill)
                .inner_margin(Margin::same(24))
                .show(ui, |ui| {
                    login_nsec_prompt(ui);
                });
            return None;
        };

        let Some(conversation) = self.cache.get(conversation_id) else {
            tracing::error!("don't have conversation for id {conversation_id}");
            return None;
        };

        let state = self.states.get_or_insert(conversation_id);
        let summary = ConversationSummary {
            metadata: &conversation.metadata,
            last_message: conversation.messages.latest(),
            unread_count: state.unread_count,
            total_messages: conversation.messages.len(),
        };
        let txn = Transaction::new(self.ndb).expect("txn");
        let title = conversation_title(summary.metadata, &txn, self.ndb, self.selected_pubkey);
        let meta_line = conversation_meta_line(&summary);
        let partner = direct_chat_partner(
            summary.metadata.participants.as_slice(),
            self.selected_pubkey,
        );
        let partner_profile =
            partner.and_then(|pk| self.ndb.get_profile_by_pubkey(&txn, pk.bytes()).ok());

        let outer_margin = Margin {
            left: 0,
            right: 0,
            top: 12,
            bottom: 0,
        };

        let mut action = None;
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
                            ScrollArea::vertical().show(ui, |ui| {
                                conversation_history(
                                    ui,
                                    conversation,
                                    state,
                                    self.ndb,
                                    &txn,
                                    img_cache,
                                    self.selected_pubkey,
                                );
                            });
                        });

                        strip.cell(|ui| {
                            let composer_action = conversation_composer(ui, state, conversation_id);
                            if action.is_none() {
                                action = composer_action;
                            }
                        });
                    });
            });

        action
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
    txn: &Transaction,
    img_cache: &mut Images,
    selected_pubkey: &Pubkey,
) {
    const GROUP_WINDOW_SECS: u64 = 5 * 60;
    Frame::new()
        .inner_margin(Margin::symmetric(16, 0))
        .show(ui, |ui| {
            let mut last_sender: Option<([u8; 32], u64)> = None;
            let mut last_day: Option<NaiveDate> = None;
            let current = *selected_pubkey.bytes();
            let today = Local::now().date_naive();
            let total = conversation.messages.len();
            state.list.ui_custom_layout(ui, total, |ui, index| {
                if index >= total {
                    return 0;
                }
                let latest_index = total - 1 - index;
                let noteref = conversation.messages.messages_ordered[latest_index];

                let Ok(note) = ndb.get_note_by_key(txn, noteref.key) else {
                    tracing::error!("Could not get key {:?}", noteref.key);
                    return 1;
                };

                let Some(chat_msg) = parse_chat_message(&note) else {
                    tracing::error!("Could not parse chat message for note {noteref:?}");
                    return 1;
                };

                let profile = ndb.get_profile_by_pubkey(txn, chat_msg.sender()).ok();
                let sender_bytes = *chat_msg.sender();
                let is_self = sender_bytes == current;
                let show_sender_name = !is_self
                    && match last_sender {
                        Some((prev_sender, prev_time)) if prev_sender == sender_bytes => {
                            let delta = noteref.created_at.saturating_sub(prev_time);
                            delta > GROUP_WINDOW_SECS
                        }
                        _ => true,
                    };
                last_sender = Some((sender_bytes, noteref.created_at));
                let sender_name = sender_label(profile.as_ref(), chat_msg.sender());
                let msg_dt = local_datetime(noteref.created_at);
                let msg_date = msg_dt.date_naive();
                if last_day.map(|d| d != msg_date).unwrap_or(true) {
                    let label = format_day_heading(msg_date, today);
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(label)
                                    .strong()
                                    .color(ui.visuals().weak_text_color()),
                            )
                            .wrap(),
                        );
                    });
                    ui.add_space(4.0);
                    last_day = Some(msg_date);
                }
                let timestamp_label = format_timestamp_label(&msg_dt);

                render_chat_message(
                    ui,
                    chat_msg,
                    img_cache,
                    profile.as_ref(),
                    is_self,
                    show_sender_name,
                    &sender_name,
                    &timestamp_label,
                );

                1
            });
        });
}

fn conversation_composer(
    ui: &mut egui::Ui,
    state: &mut ConversationState,
    conversation_id: ConversationId,
) -> Option<MessagesAction> {
    {
        let rect = ui.available_rect_before_wrap();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::ZERO, ui.visuals().panel_fill);
    }
    let margin = Margin::symmetric(16, 4);
    let mut action = None;
    Frame::new().inner_margin(margin).show(ui, |ui| {
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            let text_height = ui.spacing().item_spacing.y * 1.4;
            let spacing = ui.spacing().item_spacing.x;
            let text_width = (ui.available_width() - spacing).max(0.0);
            let size = vec2(text_width, text_height);
            // TODO(kernelkind): ideally this will be multiline, but the default multiline impl doesn't work the way
            // signal's multiline works... TBC

            let old = mut_visuals_corner_radius(ui, CornerRadius::same(16));

            let hint_text = RichText::new("Type a message")
                .color(ui.visuals().noninteractive().fg_stroke.color);
            let text_edit = TextEdit::singleline(&mut state.composer)
                .margin(Margin::symmetric(16, 8))
                .vertical_align(Align::Center)
                .desired_width(text_width)
                .hint_text(hint_text)
                .min_size(size);
            let text_resp = ui.add(text_edit);
            restore_widgets_corner_rad(ui, old);

            let enter_to_send = text_resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if enter_to_send {
                if action.is_none() {
                    action = prepare_send_action(conversation_id, state);
                }
            }

            // let can_send = !state.composer.trim().is_empty();
            // let send_clicked = ui
            //     .add_enabled(
            //         can_send,
            //         Button::new("Send").min_size(vec2(button_width, text_height + 8.0)),
            //     )
            //     .clicked();
            // if send_clicked {
            //     if action.is_none() {
            //         action = prepare_send_action(conversation_id, state);
            //     }
            // }
        });
    });

    action
}

fn prepare_send_action(
    conversation_id: ConversationId,
    state: &mut ConversationState,
) -> Option<MessagesAction> {
    if state.composer.trim().is_empty() {
        return None;
    }

    let message = std::mem::take(&mut state.composer);
    Some(MessagesAction::SendMessage {
        conversation_id,
        content: message,
    })
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
    show_partner_avatar: bool,
    partner_profile: Option<&ProfileRecord<'_>>,
    img_cache: &mut Images,
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
                if show_partner_avatar {
                    let mut pic = ProfilePic::from_profile_or_default(img_cache, partner_profile)
                        .size(ProfilePic::medium_size() as f32);
                    ui.add(&mut pic);
                    ui.add_space(8.0);
                }

                ui.vertical(|ui| {
                    ui.add(egui::Label::new(RichText::new(title).strong()).selectable(false));

                    if !meta_line.is_empty() {
                        ui.add(
                            egui::Label::new(
                                RichText::new(meta_line).color(ui.visuals().weak_text_color()),
                            )
                            .selectable(false),
                        );
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
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(unread.to_string())
                                            .color(ui.visuals().selection.stroke.color)
                                            .strong(),
                                    )
                                    .selectable(false),
                                );
                            });
                    }
                });
            });
        })
        .response
        .interact(Sense::click())
}

pub fn render_chat_message(
    ui: &mut egui::Ui,
    chat_msg: Nip17ChatMessage,
    img_cache: &mut Images,
    profile: Option<&ProfileRecord<'_>>,
    is_self: bool,
    show_sender_name: bool,
    sender_name: &str,
    timestamp_label: &str,
) -> egui::Response {
    let reply = chat_msg.reply_to().map(short_note_id_from_bytes);
    let message = chat_msg.message();
    let bubble_fill = if is_self {
        ui.visuals().selection.bg_fill
    } else {
        ui.visuals().extreme_bg_color
    };
    let text_color = ui.visuals().text_color();
    let secondary_color = ui.visuals().weak_text_color();
    let reply_ref = reply.as_deref();

    if is_self {
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            your_chat_bubble(
                ui,
                bubble_fill,
                message,
                timestamp_label,
                text_color,
                secondary_color,
            )
        })
        .inner
    } else {
        let avatar_size = ProfilePic::medium_size() as f32;
        ui.horizontal(|ui| {
            if show_sender_name {
                let mut pic =
                    ProfilePic::from_profile_or_default(img_cache, profile).size(avatar_size);
                ui.add(&mut pic);
            } else {
                ui.allocate_space(vec2(avatar_size, avatar_size));
            }
            ui.add_space(8.0);
            let inner = ui.vertical(|ui| {
                ui.add_space(4.0);
                other_chat_bubble(
                    ui,
                    bubble_fill,
                    show_sender_name.then_some(sender_name),
                    reply_ref,
                    message,
                    timestamp_label,
                    text_color,
                    secondary_color,
                )
            });
            inner.inner
        })
        .inner
    }
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

fn other_chat_bubble(
    ui: &mut egui::Ui,
    bubble_fill: Color32,
    sender_name: Option<&str>,
    reply_ref: Option<&str>,
    message: &str,
    timestamp_label: &str,
    text_color: egui::Color32,
    secondary_color: egui::Color32,
) -> egui::Response {
    Frame::new()
        .fill(bubble_fill)
        .corner_radius(CornerRadius::same(18))
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(360.0));
            other_chat_bubble_contents(
                ui,
                sender_name,
                reply_ref,
                message,
                timestamp_label,
                text_color,
                secondary_color,
            );
        })
        .response
}

fn your_chat_bubble(
    ui: &mut egui::Ui,
    bubble_fill: Color32,
    message: &str,
    timestamp_label: &str,
    text_color: egui::Color32,
    secondary_color: egui::Color32,
) -> egui::Response {
    Frame::new()
        .fill(bubble_fill)
        .corner_radius(CornerRadius::same(18))
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(360.0));
            your_chat_bubble_contents(ui, message, timestamp_label, text_color, secondary_color);
        })
        .response
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

fn local_datetime(timestamp: u64) -> DateTime<Local> {
    DateTime::<Utc>::from_timestamp(timestamp as i64, 0)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap())
        .with_timezone(&Local)
}

fn format_day_heading(date: NaiveDate, today: NaiveDate) -> String {
    if date == today {
        "Today".to_string()
    } else if date == today - Duration::days(1) {
        "Yesterday".to_string()
    } else {
        date.format("%A, %B %-d, %Y").to_string()
    }
}

fn format_timestamp_label(dt: &DateTime<Local>) -> String {
    dt.format("%-I:%M %p").to_string()
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

fn other_chat_bubble_contents(
    ui: &mut egui::Ui,
    sender_name: Option<&str>,
    reply_to: Option<&str>,
    message: &str,
    timestamp_label: &str,
    text_color: egui::Color32,
    secondary_color: egui::Color32,
) {
    ui.vertical(|ui| {
        if let Some(name) = sender_name {
            ui.label(RichText::new(name).strong().color(secondary_color));
            ui.add_space(2.0);
        }

        if let Some(reply_id) = reply_to {
            ui.label(RichText::new(format!("↩ {reply_id}")).color(secondary_color));
            ui.add_space(4.0);
        } else {
            ui.add_space(2.0);
        }

        let msg_resp = ui.label(RichText::new(message).color(text_color));

        ui.add_space(4.0);
        let desired_size = {
            let mut rect = ui.available_rect_before_wrap();
            rect.set_width(msg_resp.rect.width());
            rect.size()
        };
        ui.allocate_ui_with_layout(desired_size, Layout::right_to_left(Align::BOTTOM), |ui| {
            ui.label(
                RichText::new(timestamp_label)
                    .small()
                    .color(secondary_color),
            );
        });
    });
}

fn your_chat_bubble_contents(
    ui: &mut egui::Ui,
    message: &str,
    timestamp_label: &str,
    text_color: egui::Color32,
    secondary_color: egui::Color32,
) {
    ui.with_layout(Layout::top_down(Align::Max), |ui| {
        ui.label(RichText::new(message).color(text_color));

        ui.label(
            RichText::new(timestamp_label)
                .small()
                .color(secondary_color),
        );
    });
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
