use egui::{Sense, UiBuilder};

pub struct NavHeaderCore {}

impl NavHeaderCore {
    pub fn show(ui: &mut egui::Ui, header: impl FnOnce(&mut egui::Ui)) {
        crate::padding(8.0, ui, |ui| {
            let mut rect = ui.available_rect_before_wrap();
            rect.set_height(48.0);

            let mut child_ui = ui.new_child(
                UiBuilder::new()
                    .max_rect(rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );

            let interact_rect = child_ui.interact(rect, child_ui.id().with("drag"), Sense::drag());
            if interact_rect.drag_started_by(egui::PointerButton::Primary) {
                child_ui
                    .ctx()
                    .send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }

            header(&mut child_ui);

            ui.advance_cursor_after_rect(rect);
        });
    }
}
