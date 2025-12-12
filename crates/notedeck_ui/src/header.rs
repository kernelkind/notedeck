use egui::{Layout, Sense, Stroke, UiBuilder};
use egui_extras::{Size, StripBuilder};

pub struct NavHeaderCore {}

impl NavHeaderCore {
    pub fn show(ui: &mut egui::Ui, header: impl FnOnce(&mut egui::Ui)) {
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
    }
}

pub fn chevron(
    ui: &mut egui::Ui,
    pad: f32,
    size: egui::Vec2,
    stroke: impl Into<Stroke>,
) -> egui::Response {
    let (r, painter) = ui.allocate_painter(size, egui::Sense::click());

    let min = r.rect.min;
    let max = r.rect.max;

    let apex = egui::Pos2::new(min.x + pad, min.y + size.y / 2.0);
    let top = egui::Pos2::new(max.x - pad, min.y + pad);
    let bottom = egui::Pos2::new(max.x - pad, max.y - pad);

    let stroke = stroke.into();
    painter.line_segment([apex, top], stroke);
    painter.line_segment([apex, bottom], stroke);

    r
}

/// Generic UI Widget to render widgets horizontally where each is aligned vertically
pub struct HorizontalHeader {
    height: f32,
}

impl HorizontalHeader {
    pub fn new(height: f32) -> Self {
        Self { height }
    }

    pub fn ui(
        self,
        ui: &mut egui::Ui,
        left_priority: i8, // lower the value, higher the priority
        center_priority: i8,
        right_priority: i8,
        left_to_right: &mut impl FnMut(&mut egui::Ui),
        centered: &mut impl FnMut(&mut egui::Ui),
        right_to_left: &mut impl FnMut(&mut egui::Ui),
    ) {
        let max_width = ui.available_width();

        let left_width = measure_width(ui, left_to_right);
        let center_width = measure_width(ui, centered);
        let right_width = measure_width(ui, right_to_left);

        let half_max = max_width / 2.0;
        let half_center = center_width / 2.0;
        let mut sizes = Vec::new();
        let left_spacing = half_max - left_width - half_center;

        let mut left_center = 0.0;

        if left_spacing > 0.0 {
            sizes.push(Size::exact(left_width));
            sizes.push(Size::exact(left_spacing));
        } else {
            // not enough room for left and center to both render cleanly
            if left_priority < center_priority {
                // left is prioritized more than center
                sizes.push(Size::exact(left_width));
                left_center = half_center + left_spacing; // left_spacing is negative
            } else {
                // left is less of a priority than center, so it gets the remaining area
                sizes.push(Size::remainder());
                left_center = half_center;
            }
        }

        let right_spacing = half_max - right_width - half_center;

        if right_spacing > 0.0 {
            sizes.push(Size::exact(left_center + half_center));
            sizes.push(Size::exact(right_spacing));
            sizes.push(Size::exact(right_width));
        } else {
            // not enough room for center and right to both render cleanly
            if center_priority < right_priority {
                // center is prioritized more than right
                sizes.push(Size::exact(left_center + half_center));
                sizes.push(Size::remainder());
            } else {
                sizes.push(Size::remainder());
                sizes.push(Size::exact(right_width));
            }
        }

        StripBuilder::new(ui)
            .cell_layout(Layout::left_to_right(egui::Align::Center))
            .size(Size::exact(self.height))
            .vertical(move |mut strip| {
                strip.strip(|mut builder| {
                    for size in sizes {
                        builder = builder.size(size);
                    }

                    builder.horizontal(|mut strip| {
                        strip.cell(left_to_right);
                        strip.empty();
                        strip.cell(centered);
                        strip.empty();
                        strip.cell(right_to_left);
                    });
                });
            });
    }
}

/// Taken from VirtualList::ui_custom_layout
fn measure_width(ui: &mut egui::Ui, render: &mut impl FnMut(&mut egui::Ui)) -> f32 {
    let mut measure_ui = ui.new_child(UiBuilder::new().max_rect(ui.max_rect()));
    measure_ui.set_invisible();

    let start_width = measure_ui.next_widget_position();
    measure_ui.scope_builder(UiBuilder::new().id_salt("measure"), |ui| {
        render(ui);
    });
    let end_width = measure_ui.next_widget_position();

    let added_width = end_width.x - start_width.x + ui.spacing().item_spacing.x;

    added_width
}
