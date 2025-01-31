use egui::Layout;
use nostrdb::{Ndb, ProfileRecord, Transaction};
use notedeck::ImageCache;
use tracing::{error, info};

use crate::profile::get_display_name;

use super::{
    profile::{get_profile_url, preview::one_line_display_name_widget},
    ProfilePic,
};

pub struct SearchResultsView<'a> {
    ndb: &'a Ndb,
    txn: &'a Transaction,
    img_cache: &'a mut ImageCache,
    results: &'a Vec<[u8; 32]>,
}

impl<'a> SearchResultsView<'a> {
    pub fn new(
        img_cache: &'a mut ImageCache,
        ndb: &'a Ndb,
        txn: &'a Transaction,
        results: &'a Vec<[u8; 32]>,
    ) -> Self {
        Self {
            ndb,
            txn,
            img_cache,
            results,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<usize> {
        let mut selection = None;
        ui.vertical(|ui| {
            info!("Showing {} results", self.results.len());
            for (i, res) in self.results.iter().enumerate() {
                let profile = match self.ndb.get_profile_by_pubkey(&self.txn, res) {
                    Ok(rec) => rec,
                    Err(e) => {
                        error!("Error fetching profile for pubkey {:?}: {e}", res);
                        return;
                    }
                };

                if ui.add(user_result(&profile, &mut self.img_cache)).clicked() {
                    selection = Some(i)
                }
            }
        });

        selection
    }
}

fn user_result<'a>(
    profile: &'a ProfileRecord<'_>,
    cache: &'a mut ImageCache,
) -> impl egui::Widget + use<'a> {
    |ui: &mut egui::Ui| -> egui::Response {
        ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add(ProfilePic::new(cache, get_profile_url(Some(profile))).size(48.0));
            ui.add(one_line_display_name_widget(
                ui.visuals(),
                get_display_name(Some(profile)),
                notedeck::NotedeckTextStyle::Body,
            ))
        })
        .inner
    }
}
