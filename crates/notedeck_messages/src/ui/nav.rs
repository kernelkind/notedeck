use egui_nav::{NavResponse, RouteResponse};
use enostr::Pubkey;
use nostrdb::Ndb;
use notedeck::{AppContext, Images, Router, Settings};
use notedeck_ui::header::NavHeaderCore;

use crate::{
    cache::{ConversationCache, ConversationStates},
    route::Route,
    ui::messages::{ConversationListUi, MessagesAction},
    MessagesApp,
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
) -> NavResponse<Option<MessagesAction>> {
    egui_nav::Nav::new(router.routes())
        .navigating(router.navigating)
        .returning(router.returning)
        .animate_transitions(settings.animate_nav_transitions)
        .show_mut(ui, |ui, render_type, nav| match render_type {
            // TODO(kernelkind): impl
            egui_nav::NavUiType::Title => RouteResponse {
                response: None,
                can_take_drag_from: Vec::new(),
            },
            egui_nav::NavUiType::Body => {
                let Some(top) = nav.routes().last() else {
                    return RouteResponse {
                        response: None,
                        can_take_drag_from: Vec::new(),
                    };
                };

                render_nav_body(top, cache, states, ndb, selected_pubkey, ui, img_cache)
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
) -> RouteResponse<Option<MessagesAction>> {
    let response = match top {
        Route::ConvoList => {
            ConversationListUi::new(cache, states, ndb, img_cache).ui(ui, selected_pubkey)
        }
        Route::CreateConvo => todo!(),
    };

    RouteResponse {
        response,
        can_take_drag_from: vec![],
    }
}

pub struct NavTitle<'a> {
    routes: &'a [Route],
}

impl<'a> NavTitle<'a> {
    pub fn show(ui: &mut egui::Ui) {
        NavHeaderCore::show(ui, |ui| {});
    }
}

fn process_nav_response(
    app: &mut MessagesApp,
    ctx: &mut AppContext<'_>,
    ui: &mut egui::Ui,
    response: NavResponse<Option<MessagesAction>>,
) {
}
