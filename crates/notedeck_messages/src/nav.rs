use enostr::Pubkey;
use nostrdb::Ndb;
use notedeck::{Router, Settings};
use notedeck_ui::header::NavHeaderCore;

use crate::cache::{ConversationCache, ConversationStates};

#[derive(Clone, Debug)]
pub enum Route {
    ConvoList,
    CreateConvo,
}

pub fn render_nav(
    ui: &mut egui::Ui,
    router: &Router<Route>,
    settings: &Settings,
    cache: &ConversationCache,
    states: &ConversationStates,
    ndb: &Ndb,
    selected_pubkey: &Pubkey,
) {
    egui_nav::Nav::new(router.routes())
        .navigating(router.navigating)
        .returning(router.returning)
        .animate_transitions(settings.animate_nav_transitions)
        .show_mut(ui, |ui, render_type, nav| {
            match render_type {
                egui_nav::NavUiType::Title => todo!(),
                egui_nav::NavUiType::Body => todo!(),
            }

        });
}

pub struct NavTitle<'a> {
    routes: &'a [Route]
}

impl<'a> NavTitle<'a> {
    pub fn show(ui: &mut egui::Ui) {
        NavHeaderCore::show(ui, |ui| {

        });

    }
}