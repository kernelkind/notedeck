use egui_extras::Size;
use nostrdb::ProfileRecord;

use crate::{images, imgcache::ImageCache};

use super::{
    about_section_widget, banner, display_name_widget, get_display_name, get_profile_url,
    ProfilePic,
};

pub struct ProfileView<'a, 'cache> {
    profile: &'a ProfileRecord<'a>,
    cache: &'cache mut ImageCache,
    banner_height: Size,
}

impl<'a, 'cache> ProfileView<'a, 'cache> {
    pub fn new(profile: &'a ProfileRecord<'a>, cache: &'cache mut ImageCache) -> Self {
        let banner_height = Size::exact(80.0);
        ProfileView {
            profile,
            cache,
            banner_height,
        }
    }

    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        ui.vertical(|ui| {
            ui.add_sized([ui.available_size().x, 80.0], |ui: &mut egui::Ui| {
                banner(ui, self.profile)
            });

            crate::ui::padding(12.0, ui, |ui| {
                ui.add(ProfilePic::new(self.cache, get_profile_url(Some(self.profile))).size(80.0));
                ui.add(display_name_widget(
                    get_display_name(Some(self.profile)),
                    false,
                ));
                ui.add(about_section_widget(self.profile));
            });
        })
        .response
    }
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
            ProfileView::new(&self.profile, &mut self.cache).ui(ui);
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
