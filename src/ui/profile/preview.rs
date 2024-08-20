use crate::imgcache::ImageCache;
use crate::ui::ProfilePic;
use egui::{Frame, Widget};
use egui_extras::Size;
use nostrdb::ProfileRecord;

use super::{
    about_section_widget, banner, display_name_widget, get_display_name, get_nip5, get_profile_url,
};

pub struct ProfilePreview<'a, 'cache> {
    profile: &'a ProfileRecord<'a>,
    cache: &'cache mut ImageCache,
    banner_height: Size,
}

impl<'a, 'cache> ProfilePreview<'a, 'cache> {
    pub fn new(profile: &'a ProfileRecord<'a>, cache: &'cache mut ImageCache) -> Self {
        let banner_height = Size::exact(80.0);
        ProfilePreview {
            profile,
            cache,
            banner_height,
        }
    }

    pub fn banner_height(&mut self, size: Size) {
        self.banner_height = size;
    }

    fn body(self, ui: &mut egui::Ui) {
        crate::ui::padding(12.0, ui, |ui| {
            ui.add(ProfilePic::new(self.cache, get_profile_url(Some(self.profile))).size(80.0));
            ui.add(display_name_widget(
                get_display_name(Some(self.profile)),
                get_nip5(Some(self.profile)),
                false,
            ));
            ui.add(about_section_widget(self.profile));
        });
    }
}

impl<'a, 'cache> egui::Widget for ProfilePreview<'a, 'cache> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        ui.vertical(|ui| {
            ui.add_sized([ui.available_size().x, 80.0], |ui: &mut egui::Ui| {
                banner(ui, self.profile)
            });

            self.body(ui);
        })
        .response
    }
}

pub struct SimpleProfilePreview<'a, 'cache> {
    profile: Option<&'a ProfileRecord<'a>>,
    cache: &'cache mut ImageCache,
}

impl<'a, 'cache> SimpleProfilePreview<'a, 'cache> {
    pub fn new(profile: Option<&'a ProfileRecord<'a>>, cache: &'cache mut ImageCache) -> Self {
        SimpleProfilePreview { profile, cache }
    }
}

impl<'a, 'cache> egui::Widget for SimpleProfilePreview<'a, 'cache> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        Frame::none()
            .show(ui, |ui| {
                ui.add(ProfilePic::new(self.cache, get_profile_url(self.profile)).size(48.0));
                ui.vertical(|ui| {
                    ui.add(display_name_widget(
                        get_display_name(self.profile),
                        get_nip5(self.profile),
                        true,
                    ));
                });
            })
            .response
    }
}

mod previews {
    use super::*;
    use crate::test_data::test_profile_record;
    use crate::ui::{Preview, PreviewConfig, View};

    pub struct ProfilePreviewPreview<'a> {
        profile: ProfileRecord<'a>,
        cache: ImageCache,
    }

    impl<'a> ProfilePreviewPreview<'a> {
        pub fn new() -> Self {
            let profile = test_profile_record();
            let cache = ImageCache::new(ImageCache::rel_datadir().into());
            ProfilePreviewPreview { profile, cache }
        }
    }

    impl<'a> Default for ProfilePreviewPreview<'a> {
        fn default() -> Self {
            ProfilePreviewPreview::new()
        }
    }

    impl<'a> View for ProfilePreviewPreview<'a> {
        fn ui(&mut self, ui: &mut egui::Ui) {
            ProfilePreview::new(&self.profile, &mut self.cache).ui(ui);
        }
    }

    impl<'a, 'cache> Preview for ProfilePreview<'a, 'cache> {
        /// A preview of the profile preview :D
        type Prev = ProfilePreviewPreview<'a>;

        fn preview(_cfg: PreviewConfig) -> Self::Prev {
            ProfilePreviewPreview::new()
        }
    }
}
