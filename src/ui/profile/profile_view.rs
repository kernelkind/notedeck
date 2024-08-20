use egui::{Align, Button, Layout};
use egui_extras::Size;
use enostr::{FullKeypair, Keypair};
use nostrdb::ProfileRecord;

use crate::{colors::PURPLE, images, imgcache::ImageCache};

use super::{
    about_section_widget, banner, display_name_widget, get_display_name, get_nip5, get_profile_url,
    ProfilePic,
};

pub struct ProfileView<'a, 'cache> {
    user_key: &'a Keypair,
    profile: &'a ProfileRecord<'a>,
    cache: &'cache mut ImageCache,
    banner_height: Size,
}

impl<'a, 'cache> ProfileView<'a, 'cache> {
    pub fn new(
        user_key: &'a Keypair,
        profile: &'a ProfileRecord<'a>,
        cache: &'cache mut ImageCache,
    ) -> Self {
        let banner_height = Size::exact(80.0);
        ProfileView {
            profile,
            cache,
            banner_height,
            user_key,
        }
    }

    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        ui.vertical(|ui| {
            ui.add_sized([ui.available_size().x, 80.0], |ui: &mut egui::Ui| {
                banner(ui, self.profile)
            });

            crate::ui::padding(12.0, ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        ProfilePic::new(self.cache, get_profile_url(Some(self.profile))).size(80.0),
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
                    get_display_name(Some(self.profile)),
                    get_nip5(Some(self.profile)),
                    false,
                ));
                ui.add(about_section_widget(self.profile));

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    if let Some(website) = get_website(Some(self.profile)) {
                        ui.add(egui::Hyperlink::from_label_and_url(
                            egui::RichText::new(website).color(PURPLE),
                            website,
                        ));

                        ui.add_space(8.0);

                        if let Some(lud16) = get_lud16(Some(self.profile)) {
                            ui.add(egui::Hyperlink::from_label_and_url(
                                egui::RichText::new(lud16).color(PURPLE),
                                lud16,
                            ));
                        }
                    }
                });
            });
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
    use crate::{
        test_data::test_profile_record,
        ui::{Preview, PreviewConfig, View},
    };

    use super::*;

    pub struct ProfileViewPreview<'a> {
        profile: ProfileRecord<'a>,
        cache: ImageCache,
    }

    impl<'a> ProfileViewPreview<'a> {
        pub fn new() -> Self {
            let profile = test_profile_record();
            let cache = ImageCache::new(ImageCache::rel_datadir().into());
            ProfileViewPreview { profile, cache }
        }
    }

    impl<'a> Default for ProfileViewPreview<'a> {
        fn default() -> Self {
            ProfileViewPreview::new()
        }
    }

    impl<'a> View for ProfileViewPreview<'a> {
        fn ui(&mut self, ui: &mut egui::Ui) {
            ProfileView::new(
                &FullKeypair::generate().to_keypair(),
                &self.profile,
                &mut self.cache,
            )
            .ui(ui);
        }
    }

    impl<'a, 'cache> Preview for ProfileView<'a, 'cache> {
        /// A preview of the profile preview :D
        type Prev = ProfileViewPreview<'a>;

        fn preview(_cfg: PreviewConfig) -> Self::Prev {
            ProfileViewPreview::new()
        }
    }
}
