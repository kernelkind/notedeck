use egui::{Align, CornerRadius, CursorIcon, Layout, Response, RichText, Stroke};
use egui_nav::{NavResponse, RouteResponse};
use enostr::Pubkey;
use nostrdb::{Ndb, Transaction};
use notedeck::{ContactState, Images, NotedeckTextStyle, Router, Settings};
use notedeck_ui::{
    app_images,
    header::{chevron, NavHeaderCore},
    ProfilePic,
};

use crate::{
    cache::{Conversation, ConversationCache, ConversationStates},
    route::Route,
    ui::{
        create_convo::CreateConvoUi,
        messages::{
            conversation_title, direct_chat_partner, ConversationListUi, ConversationUi,
            MessagesAction,
        },
    },
};

pub fn render_nav(
    ui: &mut egui::Ui,
    router: &Router<Route>,
    settings: &Settings,
    cache: &ConversationCache,
    states: &mut ConversationStates,
    ndb: &Ndb,
    selected_pubkey: &Pubkey,
    img_cache: &mut Images,
    contacts: &ContactState,
) -> NavResponse<Option<MessagesAction>> {
    egui_nav::Nav::new(router.routes())
        .navigating(router.navigating)
        .returning(router.returning)
        .animate_transitions(settings.animate_nav_transitions)
        .show_mut(ui, |ui, render_type, nav| match render_type {
            egui_nav::NavUiType::Title => {
                let mut nav_title =
                    NavTitle::new(nav.routes(), cache, ndb, selected_pubkey, img_cache);
                RouteResponse {
                    response: nav_title.show(ui),
                    can_take_drag_from: Vec::new(),
                }
            }
            egui_nav::NavUiType::Body => {
                let Some(top) = nav.routes().last() else {
                    return RouteResponse {
                        response: None,
                        can_take_drag_from: Vec::new(),
                    };
                };

                render_nav_body(
                    top,
                    cache,
                    states,
                    ndb,
                    selected_pubkey,
                    ui,
                    img_cache,
                    contacts,
                )
            }
        })
}

fn render_nav_body(
    top: &Route,
    cache: &ConversationCache,
    states: &mut ConversationStates,
    ndb: &Ndb,
    selected_pubkey: &Pubkey,
    ui: &mut egui::Ui,
    img_cache: &mut Images,
    contacts: &ContactState,
) -> RouteResponse<Option<MessagesAction>> {
    let response = match top {
        Route::ConvoList => {
            ConversationListUi::new(cache, states, ndb, img_cache).ui(ui, selected_pubkey)
        }
        Route::CreateConvo => 's: {
            let Some(r) = CreateConvoUi::new(ndb, img_cache, contacts).ui(ui) else {
                break 's None;
            };

            Some(MessagesAction::Create {
                recipient: r.recipient,
            })
        }
        Route::Conversation => {
            ConversationUi::new(cache, states, ndb, img_cache).ui(ui, selected_pubkey)
        }
    };

    RouteResponse {
        response,
        can_take_drag_from: vec![],
    }
}

pub struct NavTitle<'a> {
    routes: &'a [Route],
    cache: &'a ConversationCache,
    ndb: &'a Ndb,
    selected_pubkey: &'a Pubkey,
    img_cache: &'a mut Images,
}

impl<'a> NavTitle<'a> {
    pub fn new(
        routes: &'a [Route],
        cache: &'a ConversationCache,
        ndb: &'a Ndb,
        selected_pubkey: &'a Pubkey,
        img_cache: &'a mut Images,
    ) -> Self {
        Self {
            routes,
            cache,
            ndb,
            selected_pubkey,
            img_cache,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<MessagesAction> {
        ui.painter().rect(
            ui.available_rect_before_wrap(),
            CornerRadius::ZERO,
            ui.visuals().faint_bg_color,
            Stroke::NONE,
            egui::StrokeKind::Inside,
        );
        let mut action = None;
        NavHeaderCore::show(ui, |ui| {
            action = self.title_bar(ui);
        });
        action
    }

    fn title_bar(&mut self, ui: &mut egui::Ui) -> Option<MessagesAction> {
        if self.routes.is_empty() {
            return None;
        }

        let spacing = 8.0;
        ui.spacing_mut().item_spacing.x = spacing;

        let chev_width = 8.0;
        let back_resp = prev(self.routes).map(|_| {
            self.back_button(ui, egui::vec2(chev_width, 15.0))
                .on_hover_cursor(CursorIcon::PointingHand)
        });

        if back_resp.is_none() {
            ui.add_space(chev_width + spacing);
        }

        let top = self.routes.last().expect("routes can't be empty");
        let action = self.title(ui, top);

        action.or(back_resp.and_then(|resp| resp.clicked().then_some(MessagesAction::Back)))
    }

    fn back_button(&self, ui: &mut egui::Ui, chev_size: egui::Vec2) -> egui::Response {
        let color = ui.style().visuals.noninteractive().fg_stroke.color;
        let chev_resp = chevron(ui, 2.0, chev_size, egui::Stroke::new(2.0, color));

        chev_resp
    }

    fn route_label(&self, route: &Route) -> &'static str {
        match route {
            Route::ConvoList => "Messages",
            Route::CreateConvo => "New message",
            Route::Conversation => "Conversation",
        }
    }

    fn title(&mut self, ui: &mut egui::Ui, route: &Route) -> Option<MessagesAction> {
        match route {
            Route::ConvoList => chats_header(ui),
            Route::CreateConvo => {
                self.title_label(ui, "New chat");
                None
            }
            Route::Conversation => {
                self.conversation_title_section(ui);
                None
            }
        }
    }

    fn title_label(&mut self, ui: &mut egui::Ui, text: &str) -> egui::Response {
        ui.with_layout(Layout::top_down(egui::Align::Center), |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(text).text_style(NotedeckTextStyle::Heading.text_style()),
                )
                .selectable(false),
            )
        })
        .inner
    }

    fn conversation_title_section(&mut self, ui: &mut egui::Ui) {
        let Some(conversation_id) = self.cache.active else {
            self.title_label(ui, "Conversation");
            return;
        };

        let Some(conversation) = self.cache.get(conversation_id) else {
            self.title_label(ui, "Conversation");
            return;
        };

        let txn = Transaction::new(self.ndb).expect("txn");

        let title =
            conversation_title(&conversation.metadata, &txn, self.ndb, self.selected_pubkey);
        ui.horizontal(|ui| {
            self.conversation_pfp(ui, &txn, conversation);
            self.title_label(ui, title.as_ref());
        });
    }

    fn conversation_pfp(
        &mut self,
        ui: &mut egui::Ui,
        txn: &Transaction,
        conversation: &Conversation,
    ) -> Response {
        let participants = conversation.metadata.participants.as_slice();
        let current = self.selected_pubkey.bytes();
        let fallback = participants
            .iter()
            .find(|pk| pk.bytes() != current)
            .or_else(|| participants.first());
        let partner = direct_chat_partner(participants, self.selected_pubkey).or(fallback);

        let profile = partner.and_then(|pk| self.ndb.get_profile_by_pubkey(txn, pk.bytes()).ok());

        let mut pic = ProfilePic::from_profile_or_default(self.img_cache, profile.as_ref())
            .size(ProfilePic::medium_size() as f32);

        ui.add(&mut pic)
    }
}

fn prev<R>(xs: &[R]) -> Option<&R> {
    xs.get(xs.len().checked_sub(2)?)
}

fn chats_header(ui: &mut egui::Ui) -> Option<MessagesAction> {
    let mut action = None;
    ui.heading("Chats");
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let new_msg_icon = app_images::new_message_image();
        if ui
            .add(new_msg_icon)
            .on_hover_cursor(CursorIcon::PointingHand)
            .interact(egui::Sense::click())
            .clicked()
        {
            tracing::info!("CLICKED NEW MSG");
            action = Some(MessagesAction::Creating);
        }
    });

    action
}
