use crate::colors::PINK;
use crate::ui::global_popup::FromApp;
use crate::{
    account_manager::AccountManager,
    app_style::NotedeckTextStyle,
    ui::{self, Preview, View},
};
use egui::{Align, Button, Frame, Image, Layout, RichText, ScrollArea, Vec2};

use super::global_popup::GlobalPopupType;
use super::profile::preview::SimpleProfilePreview;
use super::profile::{ProfilePreviewOp, SimpleProfilePreviewController};

pub struct AccountManagementView<'a> {
    account_manager: &'a mut AccountManager,
    simple_preview_controller: SimpleProfilePreviewController<'a>,
}

impl<'a> View for AccountManagementView<'a> {
    fn ui(&mut self, ui: &mut egui::Ui) {
        if ui::is_mobile() {
            self.show_mobile(ui);
        } else {
            self.show(ui);
        }
    }
}

impl<'a> AccountManagementView<'a> {
    pub fn new(
        account_manager: &'a mut AccountManager,
        simple_preview_controller: SimpleProfilePreviewController<'a>,
    ) -> Self {
        AccountManagementView {
            account_manager,
            simple_preview_controller,
        }
    }

    fn show(&mut self, ui: &mut egui::Ui) {
        Frame::none().outer_margin(24.0).show(ui, |ui| {
            ui.add(self.top_section_buttons_widget());
            ui.add_space(8.0);
            scroll_area().show(ui, |ui| {
                self.show_accounts(ui);
            });
        });
    }

    fn show_accounts(&mut self, ui: &mut egui::Ui) {
        let maybe_remove = self.simple_preview_controller.set_profile_previews(
            self.account_manager,
            ui,
            account_card_ui(),
        );

        self.maybe_remove_accounts(maybe_remove);
    }

    fn show_accounts_mobile(&mut self, ui: &mut egui::Ui) {
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_size_before_wrap().x, 32.0),
            Layout::top_down(egui::Align::Min),
            |ui| {
                // create all account 'cards' and get the indicies the user requested to remove
                let maybe_remove = self.simple_preview_controller.set_profile_previews(
                    self.account_manager,
                    ui,
                    account_card_ui(), // closure for creating an account 'card'
                );

                // remove all account indicies user requested
                self.maybe_remove_accounts(maybe_remove);
            },
        );
    }

    fn maybe_remove_accounts(&mut self, account_indices: Option<Vec<usize>>) {
        if let Some(to_remove) = account_indices {
            to_remove
                .iter()
                .for_each(|index| self.account_manager.remove_account(*index));
        }
    }

    fn show_mobile(&mut self, ui: &mut egui::Ui) -> egui::Response {
        egui::CentralPanel::default()
            .show(ui.ctx(), |ui| {
                ui.add(mobile_title());
                ui.add(self.top_section_buttons_widget());
                ui.add_space(8.0);
                scroll_area().show(ui, |ui| {
                    self.show_accounts_mobile(ui);
                });
            })
            .response
    }

    fn top_section_buttons_widget(&mut self) -> impl egui::Widget + '_ {
        |ui: &mut egui::Ui| {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_size_before_wrap().x, 32.0),
                    Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        if ui.add(add_account_button()).clicked() {
                            // TODO: route to AccountLoginView
                        }
                    },
                );

                // UNCOMMENT FOR LOGOUTALL BUTTON
                // ui.allocate_ui_with_layout(
                //     Vec2::new(ui.available_size_before_wrap().x, 32.0),
                //     Layout::right_to_left(egui::Align::Center),
                //     |ui| {
                //         if ui.add(logout_all_button()).clicked() {
                //             for index in (0..self.account_manager.num_accounts()).rev() {
                //                 self.account_manager.remove_account(index);
                //             }
                //         }
                //     },
                // );
            })
            .response
        }
    }
}

fn account_card_ui() -> fn(
    ui: &mut egui::Ui,
    preview: SimpleProfilePreview,
    width: f32,
    is_selected: bool,
) -> Option<ProfilePreviewOp> {
    |ui, preview, width, is_selected| {
        let mut op: Option<ProfilePreviewOp> = None;

        ui.add_sized(Vec2::new(width, 50.0), |ui: &mut egui::Ui| {
            Frame::none()
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(preview);

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if is_selected {
                                ui.add(selected_widget());
                            } else {
                                if ui
                                    .add(switch_button(ui.style().visuals.dark_mode))
                                    .clicked()
                                {
                                    op = Some(ProfilePreviewOp::SwitchTo);
                                }
                                if ui.add(sign_out_button(ui)).clicked() {
                                    op = Some(ProfilePreviewOp::RemoveAccount)
                                }
                            }
                        });
                    });
                })
                .response
        });
        ui.add_space(16.0);
        op
    }
}

impl<'a> FromApp<'a> for AccountManagementView<'a> {
    fn from_app(app: &'a mut crate::Damus) -> Self {
        AccountManagementView::new(
            &mut app.account_manager,
            SimpleProfilePreviewController::new(&app.ndb, &mut app.img_cache),
        )
    }
}

fn mobile_title() -> impl egui::Widget {
    |ui: &mut egui::Ui| {
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(GlobalPopupType::AccountManagement.title())
                    .text_style(NotedeckTextStyle::Heading2.text_style())
                    .strong(),
            );
        })
        .response
    }
}

fn scroll_area() -> ScrollArea {
    egui::ScrollArea::vertical()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .auto_shrink([false; 2])
}

fn add_account_button() -> Button<'static> {
    let img_data = egui::include_image!("../../assets/icons/add_account_icon_4x.png");
    let img = Image::new(img_data).fit_to_exact_size(Vec2::new(48.0, 48.0));
    Button::image_and_text(
        img,
        RichText::new(" Add account")
            .size(16.0)
            // TODO: this color should not be hard coded. Find some way to add it to the visuals
            .color(PINK),
    )
    .frame(false)
}

fn sign_out_button(ui: &egui::Ui) -> egui::Button<'static> {
    let img_data = egui::include_image!("../../assets/icons/signout_icon_4x.png");
    let img = Image::new(img_data).fit_to_exact_size(Vec2::new(16.0, 16.0));

    egui::Button::image_and_text(
        img,
        RichText::new("Sign out").color(ui.visuals().noninteractive().fg_stroke.color),
    )
    .frame(false)
}

fn switch_button(dark_mode: bool) -> egui::Button<'static> {
    let _ = dark_mode;

    egui::Button::new("Switch").min_size(Vec2::new(76.0, 32.0))
}

fn selected_widget() -> impl egui::Widget {
    |ui: &mut egui::Ui| {
        Frame::none()
            .show(ui, |ui| {
                ui.label(RichText::new("Selected").size(13.0).color(PINK));
                let img_data = egui::include_image!("../../assets/icons/select_icon_3x.png");
                let img = Image::new(img_data).max_size(Vec2::new(16.0, 16.0));
                ui.add(img);
            })
            .response
    }
}

// fn logout_all_button() -> egui::Button<'static> {
//     egui::Button::new("Logout all")
// }

// PREVIEWS

mod preview {
    use nostrdb::{Config, Ndb};
    use ui::account_switcher::AccountSelectionWidget;

    use super::*;
    use crate::imgcache::ImageCache;
    use crate::key_storage::KeyStorage;
    use crate::relay_generation::RelayGenerator;
    use crate::test_data;
    use std::path::Path;

    pub struct AccountManagementPreview {
        account_manager: AccountManager,
        ndb: Ndb,
        img_cache: ImageCache,
    }

    fn get_ndb_and_img_cache() -> (Ndb, ImageCache) {
        let mut config = Config::new();
        config.set_ingester_threads(2);

        let db_dir = Path::new(".");
        let path = db_dir.to_str().unwrap();
        let ndb = Ndb::new(path, &config).expect("ndb");
        let imgcache_dir = db_dir.join("cache/img");
        let img_cache = ImageCache::new(imgcache_dir);
        (ndb, img_cache)
    }

    impl AccountManagementPreview {
        fn new() -> Self {
            let mut account_manager =
                AccountManager::new(None, KeyStorage::None, RelayGenerator::Constant, || {});
            let accounts = test_data::get_test_accounts();
            accounts
                .into_iter()
                .for_each(|acc| account_manager.add_account(acc.key, || {}));
            let (ndb, img_cache) = get_ndb_and_img_cache();

            AccountManagementPreview {
                account_manager,
                ndb,
                img_cache,
            }
        }
    }

    impl View for AccountManagementPreview {
        fn ui(&mut self, ui: &mut egui::Ui) {
            ui.add_space(24.0);
            AccountManagementView::new(
                &mut self.account_manager,
                SimpleProfilePreviewController::new(&self.ndb, &mut self.img_cache),
            )
            .ui(ui);
        }
    }

    impl<'a> Preview for AccountManagementView<'a> {
        type Prev = AccountManagementPreview;

        fn preview() -> Self::Prev {
            AccountManagementPreview::new()
        }
    }

    pub struct AccountSelectionPreview {
        account_manager: AccountManager,
        ndb: Ndb,
        img_cache: ImageCache,
    }

    impl AccountSelectionPreview {
        fn new() -> Self {
            let mut account_manager =
                AccountManager::new(None, KeyStorage::None, RelayGenerator::Constant, || {});
            let accounts = test_data::get_test_accounts();
            accounts
                .into_iter()
                .for_each(|acc| account_manager.add_account(acc.key, || {}));
            let (ndb, img_cache) = get_ndb_and_img_cache();
            AccountSelectionPreview {
                account_manager,
                ndb,
                img_cache,
            }
        }
    }

    impl View for AccountSelectionPreview {
        fn ui(&mut self, ui: &mut egui::Ui) {
            AccountSelectionWidget::new(
                &mut self.account_manager,
                SimpleProfilePreviewController::new(&self.ndb, &mut self.img_cache),
            )
            .ui(ui);
        }
    }

    impl<'a> Preview for AccountSelectionWidget<'a> {
        type Prev = AccountSelectionPreview;

        fn preview() -> Self::Prev {
            AccountSelectionPreview::new()
        }
    }
}
