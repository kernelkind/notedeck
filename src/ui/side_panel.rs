use egui::{Button, Color32, InnerResponse, Layout, Pos2, SidePanel, Stroke, Vec2, Widget};

use crate::{
    account_manager::AccountsRoute,
    colors,
    column::Column,
    route::{Route, Router},
    ui::{anim::hover_expand, profile_preview_controller},
    Damus,
};

use super::{ProfilePic, View};

pub struct DesktopSidePanel<'a> {
    app: &'a mut Damus,
}

impl<'a> View for DesktopSidePanel<'a> {
    fn ui(&mut self, ui: &mut egui::Ui) {
        self.show(ui);
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum SidePanelAction {
    Panel,
    Account,
    Settings,
    Columns,
    ComposeNote,
}

pub struct SidePanelResponse {
    pub response: egui::Response,
    pub action: SidePanelAction,
}

impl SidePanelResponse {
    fn new(action: SidePanelAction, response: egui::Response) -> Self {
        SidePanelResponse { action, response }
    }
}

impl<'a> DesktopSidePanel<'a> {
    pub fn new(app: &'a mut Damus) -> Self {
        DesktopSidePanel { app }
    }

    pub fn panel() -> SidePanel {
        egui::SidePanel::left("side_panel")
            .resizable(false)
            .exact_width(40.0)
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> SidePanelResponse {
        let dark_mode = ui.ctx().style().visuals.dark_mode;
        let spacing_amt = 16.0;

        let inner = ui
            .vertical(|ui| {
                let top_resp = ui
                    .with_layout(Layout::top_down(egui::Align::Center), |ui| {
                        let compose_resp = ui.add(compose_note_button());

                        if compose_resp.clicked() {
                            Some(InnerResponse::new(
                                SidePanelAction::ComposeNote,
                                compose_resp,
                            ))
                        } else {
                            None
                        }
                    })
                    .inner;

                let (pfp_resp, bottom_resp) = ui
                    .with_layout(Layout::bottom_up(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.y = spacing_amt;
                        let pfp_resp = self.pfp_button(ui);
                        let settings_resp = ui.add(settings_button(dark_mode));
                        let column_resp = ui.add(add_column_button(dark_mode));

                        let optional_inner = if pfp_resp.clicked() {
                            Some(egui::InnerResponse::new(
                                SidePanelAction::Account,
                                pfp_resp.clone(),
                            ))
                        } else if settings_resp.clicked() || settings_resp.hovered() {
                            Some(egui::InnerResponse::new(
                                SidePanelAction::Settings,
                                settings_resp,
                            ))
                        } else if column_resp.clicked() || column_resp.hovered() {
                            Some(egui::InnerResponse::new(
                                SidePanelAction::Columns,
                                column_resp,
                            ))
                        } else {
                            None
                        };

                        (pfp_resp, optional_inner)
                    })
                    .inner;

                if let Some(bottom_inner) = bottom_resp {
                    bottom_inner
                } else if let Some(top_inner) = top_resp {
                    top_inner
                } else {
                    egui::InnerResponse::new(SidePanelAction::Panel, pfp_resp)
                }
            })
            .inner;

        SidePanelResponse::new(inner.inner, inner.response)
    }

    fn pfp_button(&mut self, ui: &mut egui::Ui) -> egui::Response {
        if let Some(resp) =
            profile_preview_controller::show_with_selected_pfp(self.app, ui, show_pfp())
        {
            resp
        } else {
            add_button_to_ui(ui, no_account_pfp())
        }
    }

    pub fn perform_action(router: &mut Router<Route>, action: SidePanelAction) {
        match action {
            SidePanelAction::Panel => {} // TODO
            SidePanelAction::Account => {
                if router
                    .routes()
                    .iter()
                    .any(|&r| r == Route::Accounts(AccountsRoute::Accounts))
                {
                    // return if we are already routing to accounts
                    router.go_back();
                } else {
                    router.route_to(Route::accounts());
                }
            }
            SidePanelAction::Settings => {
                if router.routes().iter().any(|&r| r == Route::Relays) {
                    // return if we are already routing to accounts
                    router.go_back();
                } else {
                    router.route_to(Route::relays());
                }
            }
            SidePanelAction::Columns => (), // TODO
            SidePanelAction::ComposeNote => {
                if router.routes().iter().any(|&r| r == Route::ComposeNote) {
                    router.go_back();
                } else {
                    router.route_to(Route::ComposeNote);
                }
            }
        }
    }
}

fn show_pfp() -> fn(ui: &mut egui::Ui, pfp: ProfilePic) -> egui::Response {
    |ui, pfp| {
        let response = pfp.ui(ui);
        ui.allocate_rect(response.rect, egui::Sense::click())
    }
}

fn settings_button(dark_mode: bool) -> egui::Button<'static> {
    let _ = dark_mode;
    let img_data = egui::include_image!("../../assets/icons/settings_dark_4x.png");

    egui::Button::image(egui::Image::new(img_data).max_width(32.0)).frame(false)
}

fn add_button_to_ui(ui: &mut egui::Ui, button: Button) -> egui::Response {
    ui.add_sized(Vec2::new(32.0, 32.0), button)
}

fn no_account_pfp() -> Button<'static> {
    Button::new("A")
        .rounding(20.0)
        .min_size(Vec2::new(38.0, 38.0))
}

fn add_column_button(dark_mode: bool) -> egui::Button<'static> {
    let _ = dark_mode;
    let img_data = egui::include_image!("../../assets/icons/add_column_dark_4x.png");

    egui::Button::image(egui::Image::new(img_data).max_width(32.0)).frame(false)
}

fn compose_note_button() -> impl Widget {
    |ui: &mut egui::Ui| -> egui::Response {
        let id = ui.id().with("note-compose-button");

        let expansion_multiple = 1.2;

        let max_size = 40.0;
        let min_size = max_size / expansion_multiple;
        let expansion_increase = max_size - min_size;

        let anim_speed = 0.05;
        let (rect, cur_size, resp) = hover_expand(ui, id, min_size, expansion_increase, anim_speed);
        let animation_progress = (cur_size - min_size) / expansion_increase; // 0.0 to 1.0 where 0 is min_size and 1 is max_size

        let painter = ui.painter_at(rect);

        let rect_center = rect.center();
        let min_radius = (min_size - 1.0) / 2.0;
        let cur_radius = (cur_size - 1.0) / 2.0;

        let max_plus_sign_size = 14.0;
        let min_plus_sign_size = max_plus_sign_size / expansion_multiple;
        let cur_plus_sign_size = min_plus_sign_size + (expansion_increase * animation_progress);
        let half_min_plus_sign_size = min_plus_sign_size / 2.0;
        let half_cur_plus_sign_size = cur_plus_sign_size / 2.0;

        let max_line_width = 2.8;
        let min_line_width = max_line_width / expansion_multiple;
        let cur_line_width =
            min_line_width + ((max_line_width - min_line_width) * animation_progress);

        let cur_edge_circle_radius = (cur_line_width - 1.0) / 2.0;
        let min_edge_circle_radius = (min_line_width - 1.0) / 2.0;

        let (
            use_background_radius,
            use_line_width,
            use_edge_circle_radius,
            use_half_plus_sign_size,
        ) = if resp.is_pointer_button_down_on() {
            (
                min_radius,
                min_line_width,
                min_edge_circle_radius,
                half_min_plus_sign_size,
            )
        } else {
            (
                cur_radius,
                cur_line_width,
                cur_edge_circle_radius,
                half_cur_plus_sign_size,
            )
        };

        let north_edge = Pos2::new(rect_center.x, rect_center.y + use_half_plus_sign_size);
        let south_edge = Pos2::new(rect_center.x, rect_center.y - use_half_plus_sign_size);

        let west_edge = Pos2::new(rect_center.x + use_half_plus_sign_size, rect_center.y);
        let east_edge = Pos2::new(rect_center.x - use_half_plus_sign_size, rect_center.y);

        painter.circle_filled(rect_center, use_background_radius, colors::PINK);
        painter.line_segment(
            [north_edge, south_edge],
            Stroke::new(use_line_width, Color32::WHITE),
        );
        painter.line_segment(
            [west_edge, east_edge],
            Stroke::new(use_line_width, Color32::WHITE),
        );
        painter.circle_filled(north_edge, use_edge_circle_radius, Color32::WHITE);
        painter.circle_filled(south_edge, use_edge_circle_radius, Color32::WHITE);
        painter.circle_filled(west_edge, use_edge_circle_radius, Color32::WHITE);
        painter.circle_filled(east_edge, use_edge_circle_radius, Color32::WHITE);

        resp
    }
}

mod preview {

    use egui_extras::{Size, StripBuilder};

    use crate::{
        test_data,
        ui::{Preview, PreviewConfig},
    };

    use super::*;

    pub struct DesktopSidePanelPreview {
        app: Damus,
    }

    impl DesktopSidePanelPreview {
        fn new() -> Self {
            let mut app = test_data::test_app();
            app.columns
                .columns_mut()
                .push(Column::new(vec![Route::accounts()]));
            DesktopSidePanelPreview { app }
        }
    }

    impl View for DesktopSidePanelPreview {
        fn ui(&mut self, ui: &mut egui::Ui) {
            StripBuilder::new(ui)
                .size(Size::exact(40.0))
                .sizes(Size::remainder(), 0)
                .clip(true)
                .horizontal(|mut strip| {
                    strip.cell(|ui| {
                        let mut panel = DesktopSidePanel::new(&mut self.app);
                        let response = panel.show(ui);

                        DesktopSidePanel::perform_action(
                            self.app.columns.columns_mut()[0].router_mut(),
                            response.action,
                        );
                    });
                });
        }
    }

    impl<'a> Preview for DesktopSidePanel<'a> {
        type Prev = DesktopSidePanelPreview;

        fn preview(_cfg: PreviewConfig) -> Self::Prev {
            DesktopSidePanelPreview::new()
        }
    }
}
