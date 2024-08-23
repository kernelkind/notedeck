use egui::{Align, Button, Layout, Response};
use egui_extras::Size;
use enostr::Keypair;
use nostrdb::{ProfileRecord, Transaction};

use crate::{colors::PURPLE, imgcache::ImageCache, profile, ui::timeline::timeline_ui, Damus};

use super::{
    about_section_widget, banner, display_name_widget, get_display_name, get_nip5, get_profile_url,
    ProfilePic,
};

pub struct ProfileView<'a> {
    user_key: &'a Keypair,
    banner_height: Size,
    app: &'a mut Damus,
}

impl<'a> ProfileView<'a> {
    pub fn new(app: &'a mut Damus, user_key: &'a Keypair) -> Self {
        let banner_height = Size::exact(80.0);
        app.profiles
            .create_profile_state_if_nonexistent(&user_key.pubkey);
        ProfileView {
            banner_height,
            user_key,
            app,
        }
    }

    pub fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        ui.vertical(|ui| {
            if let Ok(txn) = Transaction::new(&self.app.ndb) {
                if let Some(profile) = self
                    .app
                    .ndb
                    .get_profile_by_pubkey(&txn, self.user_key.pubkey.bytes())
                    .ok()
                    .as_ref()
                {
                    ui.add_sized([ui.available_size().x, 80.0], |ui: &mut egui::Ui| {
                        banner(ui, profile)
                    });

                    crate::ui::padding(12.0, ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add(
                                ProfilePic::new(
                                    &mut self.app.img_cache,
                                    get_profile_url(Some(profile)),
                                )
                                .size(80.0),
                            );

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if self.user_key.secret_key.is_some() {
                                    ui.add(egui::Button::new("Edit Profile"));
                                } else {
                                    ui.add(egui::Button::new("Follow"));
                                }

                                ui.add(egui::Button::new("DM"));
                                ui.add(Button::new("Zap"));
                                ui.add(Button::new("More"));
                            });
                        });
                        ui.add(display_name_widget(
                            get_display_name(Some(profile)),
                            get_nip5(Some(profile)),
                            false,
                        ));
                        ui.add(about_section_widget(profile));

                        ui.add_space(8.0);

                        ui.horizontal(|ui| {
                            if let Some(website) = get_website(Some(profile)) {
                                ui.add(egui::Hyperlink::from_label_and_url(
                                    egui::RichText::new(website).color(PURPLE),
                                    website,
                                ));

                                ui.add_space(8.0);

                                if let Some(lud16) = get_lud16(Some(profile)) {
                                    ui.add(egui::Hyperlink::from_label_and_url(
                                        egui::RichText::new(lud16).color(PURPLE),
                                        lud16,
                                    ));
                                }
                            }
                        });

                        ui.add_space(8.0);

                        timeline_ui(
                            ui,
                            self.app,
                            false,
                            |app| {
                                &app.profiles
                                    .get_profile_state(&self.user_key.pubkey)
                                    .unwrap()
                                    .timeline
                            },
                            |app| {
                                &mut app
                                    .profiles
                                    .get_profile_state_mut(&self.user_key.pubkey)
                                    .unwrap()
                                    .timeline
                            },
                            |_| 0,
                        );
                    });
                }
            };
        })
        .response
    }
}

fn get_website<'a>(profile: Option<&'a ProfileRecord>) -> Option<&'a str> {
    return profile.and_then(|profile| profile.record().profile().and_then(|p| p.website()));
}

fn get_lud16<'a>(profile: Option<&'a ProfileRecord>) -> Option<&'a str> {
    return profile.and_then(|profile| profile.record().profile().and_then(|p| p.lud16()));
}

mod previews {
    use enostr::Pubkey;

    use crate::{
        app::update_damus,
        test_data::{self, test_profile_record},
        ui::{Preview, PreviewConfig, View},
    };

    use super::*;

    pub struct ProfileViewPreview<'a> {
        app: Damus,
        profile: ProfileRecord<'a>,
        first: bool,
        wills_key: Keypair,
    }

    impl<'a> ProfileViewPreview<'a> {
        pub fn new(is_mobile: bool) -> Self {
            let profile = test_profile_record();
            let app = test_data::test_app(is_mobile);
            let wills_key = Keypair::new(
                Pubkey::from_hex(
                    "32e1827635450ebb3c5a7d12c1f8e7b2b514439ac10a67eef3d9fd9c5c68e245",
                )
                .ok()
                .unwrap(),
                None,
            );
            ProfileViewPreview {
                app,
                profile,
                wills_key,
                first: true,
            }
        }
    }

    impl<'a> Default for ProfileViewPreview<'a> {
        fn default() -> Self {
            ProfileViewPreview::new(false)
        }
    }

    impl<'a> View for ProfileViewPreview<'a> {
        fn ui(&mut self, ui: &mut egui::Ui) {
            if self.first {
                self.first = false;
                self.app
                    .profiles
                    .create_profile_state_if_nonexistent(&self.wills_key.pubkey);
            }
            update_damus(&mut self.app, ui.ctx());
            ProfileView::new(&mut self.app, &self.wills_key).ui(ui);
        }
    }

    impl<'a> Preview for ProfileView<'a> {
        /// A preview of the profile preview :D
        type Prev = ProfileViewPreview<'a>;

        fn preview(cfg: PreviewConfig) -> Self::Prev {
            ProfileViewPreview::new(cfg.is_mobile)
        }
    }
}
