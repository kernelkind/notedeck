pub mod picture;
pub mod preview;
pub mod profile_preview_controller;
pub mod profile_view;

use egui::{load::TexturePoll, Color32, RichText, Sense};
use nostrdb::ProfileRecord;
pub use picture::ProfilePic;
pub use preview::ProfilePreview;
pub use profile_preview_controller::ProfilePreviewOp;
pub use profile_view::ProfileView;

use crate::{app_style::NotedeckTextStyle, colors, images, DisplayName};

pub(crate) fn get_display_name<'a>(profile: Option<&'a ProfileRecord<'a>>) -> DisplayName<'a> {
    if let Some(name) = profile.and_then(|p| crate::profile::get_profile_name(p)) {
        name
    } else {
        DisplayName::One("??")
    }
}

pub(crate) fn get_profile_url<'a>(profile: Option<&'a ProfileRecord<'a>>) -> &'a str {
    if let Some(url) = profile.and_then(|pr| pr.record().profile().and_then(|p| p.picture())) {
        url
    } else {
        ProfilePic::no_pfp_url()
    }
}

pub(crate) fn display_name_widget<'a>(
    display_name: DisplayName<'a>,
    nip05: Option<&'a str>,
    add_placeholder_space: bool,
) -> impl egui::Widget + 'a {
    move |ui: &mut egui::Ui| match display_name {
        DisplayName::One(n) => {
            let name_response =
                ui.label(RichText::new(n).text_style(NotedeckTextStyle::Heading3.text_style()));
            if add_placeholder_space {
                ui.add_space(16.0);
            }
            name_response
        }

        DisplayName::Both {
            display_name,
            username,
        } => {
            ui.label(
                RichText::new(display_name).text_style(NotedeckTextStyle::Heading3.text_style()),
            );

            ui.horizontal(|ui| {
                let mut response = ui.label(
                    RichText::new(format!("@{}", username))
                        .size(12.0)
                        .color(colors::MID_GRAY),
                );

                if let Some(nip05) = nip05 {
                    response = ui.colored_label(Color32::from_rgb(0x00, 0x80, 0x80), nip05);
                }
                response
            })
            .inner
        }
    }
}

pub(crate) fn about_section_widget<'a>(profile: &'a ProfileRecord<'a>) -> impl egui::Widget + 'a {
    |ui: &mut egui::Ui| {
        if let Some(about) = profile.record().profile().and_then(|p| p.about()) {
            ui.label(about)
        } else {
            // need any Response so we dont need an Option
            ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover())
        }
    }
}

pub(crate) fn banner(ui: &mut egui::Ui, profile: &ProfileRecord<'_>) -> egui::Response {
    if let Some(texture) = banner_texture(ui, profile) {
        images::aspect_fill(
            ui,
            Sense::hover(),
            texture.id,
            texture.size.x / texture.size.y,
        )
    } else {
        // TODO: default banner texture
        ui.label("")
    }
}

fn get_nip5<'a>(profile: Option<&'a ProfileRecord>) -> Option<&'a str> {
    return profile.and_then(|profile| profile.record().profile().and_then(|p| p.nip05()));
}

fn banner_texture(
    ui: &mut egui::Ui,
    profile: &ProfileRecord<'_>,
) -> Option<egui::load::SizedTexture> {
    // TODO: cache banner
    let banner = profile.record().profile().and_then(|p| p.banner());

    if let Some(banner) = banner {
        let texture_load_res =
            egui::Image::new(banner).load_for_size(ui.ctx(), ui.available_size());
        if let Ok(texture_poll) = texture_load_res {
            match texture_poll {
                TexturePoll::Pending { .. } => {}
                TexturePoll::Ready { texture, .. } => return Some(texture),
            }
        }
    }

    None
}
