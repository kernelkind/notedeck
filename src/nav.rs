use crate::{
    account_manager::render_accounts_route,
    app_style::{desktop_font_size, NotedeckTextStyle},
    fonts::NamedFontFamily,
    relay_pool_manager::RelayPoolManager,
    route::Route,
    thread::thread_unsubscribe,
    timeline::route::{render_timeline_route, TimelineRoute, TimelineRouteResponse},
    ui::{
        self,
        anim::{AnimationHelper, ICON_EXPANSION_MULTIPLE},
        note::PostAction,
        RelayView, View,
    },
    Damus,
};

use egui::{Layout, RichText};
use egui_nav::{Nav, NavAction};

pub fn render_nav(col: usize, app: &mut Damus, ui: &mut egui::Ui) {
    let col_id = app.columns.get_column_id_at_index(col);
    // TODO(jb55): clean up this router_mut mess by using Router<R> in egui-nav directly
    let nav_response = Nav::new(app.columns().column(col).router().routes().clone())
        .navigating(app.columns_mut().column_mut(col).router_mut().navigating)
        .returning(app.columns_mut().column_mut(col).router_mut().returning)
        .title(title_bar)
        .title_height(48.0)
        .show_mut(col_id, ui, |ui, nav| {
            let column = app.columns.column_mut(col);
            match nav.top() {
                Route::Timeline(tlr) => render_timeline_route(
                    &app.ndb,
                    &mut app.columns,
                    &mut app.pool,
                    &mut app.drafts,
                    &mut app.img_cache,
                    &mut app.note_cache,
                    &mut app.threads,
                    &mut app.accounts,
                    *tlr,
                    col,
                    app.textmode,
                    ui,
                ),
                Route::Accounts(amr) => {
                    render_accounts_route(
                        ui,
                        &app.ndb,
                        col,
                        &mut app.columns,
                        &mut app.img_cache,
                        &mut app.accounts,
                        &mut app.view_state.login,
                        *amr,
                    );
                    None
                }
                Route::Relays => {
                    let manager = RelayPoolManager::new(app.pool_mut());
                    RelayView::new(manager).ui(ui);
                    None
                }
                Route::ComposeNote => {
                    let kp = app.accounts.selected_or_first_nsec()?;
                    let draft = app.drafts.compose_mut();

                    let txn = nostrdb::Transaction::new(&app.ndb).expect("txn");
                    let post_response = ui::PostView::new(
                        &app.ndb,
                        draft,
                        crate::draft::DraftSource::Compose,
                        &mut app.img_cache,
                        &mut app.note_cache,
                        kp,
                    )
                    .ui(&txn, ui);

                    if let Some(action) = post_response.action {
                        PostAction::execute(kp, &action, &mut app.pool, draft, |np, seckey| {
                            np.to_note(seckey)
                        });
                        column.router_mut().go_back();
                    }

                    None
                }
            }
        });

    if let Some(reply_response) = nav_response.inner {
        // start returning when we're finished posting
        match reply_response {
            TimelineRouteResponse::Post(resp) => {
                if let Some(action) = resp.action {
                    match action {
                        PostAction::Post(_) => {
                            app.columns_mut().column_mut(col).router_mut().returning = true;
                        }
                    }
                }
            }
        }
    }

    if let Some(NavAction::Returned) = nav_response.action {
        let r = app.columns_mut().column_mut(col).router_mut().pop();
        if let Some(Route::Timeline(TimelineRoute::Thread(id))) = r {
            thread_unsubscribe(
                &app.ndb,
                &mut app.threads,
                &mut app.pool,
                &mut app.note_cache,
                id.bytes(),
            );
        }
    } else if let Some(NavAction::Navigated) = nav_response.action {
        app.columns_mut().column_mut(col).router_mut().navigating = false;
    } else if let Some(NavAction::Deleting) = nav_response.action {
        app.columns_mut().request_deletion_at_index(col);
    }
}

fn title_bar(
    painter: &egui::Painter,
    allocated_response: egui::Response,
    title_name: String,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
            ui.vertical(|ui| {
                ui.add(title(title_name, title_height));
            })
        });

        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(delete_column_button(allocated_response))
        })
        .inner
    })
    .inner
}

static ICON_WIDTH: f32 = 32.0;
fn delete_column_button(resp: egui::Response) -> impl egui::Widget {
    move |ui: &mut egui::Ui| -> egui::Response {
        let img_size = 16.0;
        let max_size = ICON_WIDTH * ICON_EXPANSION_MULTIPLE;

        let img_data = egui::include_image!("../assets/icons/column_delete_icon_4x.png");
        let img = egui::Image::new(img_data).max_width(img_size);

        let helper =
            AnimationHelper::new(ui, "delete-column-button", egui::vec2(max_size, max_size));

        let cur_img_size = helper.scale_1d_pos(img_size);

        if resp.hovered() {
            img.paint_at(
                ui,
                helper
                    .get_animation_rect()
                    .shrink((max_size - cur_img_size) / 2.0),
            );
        }
        helper.take_animation_response()
    }
}

fn title(title_name: String, max_size: f32) -> impl egui::Widget {
    move |ui: &mut egui::Ui| -> egui::Response {
        let title = RichText::new(title_name)
            .family(egui::FontFamily::Name(
                NamedFontFamily::Bold.as_str().into(),
            ))
            .size(desktop_font_size(&NotedeckTextStyle::Body)); // TODO: replace with generic function after merge of add_columns

        let title_height: f32 = {
            let mut height = 0.0;
            ui.fonts(|f| height = title.font_height(f, ui.style()));
            height
        };

        let padding = (max_size - title_height) / 2.0;

        ui.add_space(padding);
        let resp = ui
            .horizontal(|ui| {
                ui.add_space(16.0);
                ui.add(egui::Label::new(title).selectable(false))
            })
            .inner;
        ui.add_space(padding);

        resp
    }
}
