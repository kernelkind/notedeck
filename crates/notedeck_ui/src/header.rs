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
        left_aligned: &mut impl FnMut(&mut egui::Ui),
        centered: &mut impl FnMut(&mut egui::Ui),
        right_aligned: &mut impl FnMut(&mut egui::Ui),
    ) {
        let item_spacing = 6.0 * ui.spacing().item_spacing.x;
        let max_width = ui.available_width() - item_spacing;

        let left_width = measure_width(ui, left_aligned);
        let center_width = measure_width(ui, centered);
        let right_width = measure_width(ui, right_aligned);

        let half_max = max_width / 2.0;
        let half_center = center_width / 2.0;
        let left_spacing = half_max - left_width - half_center;
        let right_spacing = half_max - right_width - half_center;

        let mut left_center = half_center;
        let left_cell = if left_spacing > 0.0 || left_priority < center_priority {
            Size::exact(left_width)
        } else {
            Size::remainder()
        };
        let mut left_gap = Size::exact(left_spacing.max(0.0));

        if left_spacing <= 0.0 {
            left_gap = Size::exact(0.0);
            if left_priority < center_priority {
                left_center = (half_center + left_spacing).max(0.0);
            }
        }

        let mut center_cell = Size::exact((left_center + half_center).max(0.0));
        let mut right_gap = Size::exact(right_spacing.max(0.0));
        let mut right_cell = Size::exact(right_width);

        if right_spacing <= 0.0 {
            right_gap = Size::exact(0.0);
            if center_priority < right_priority {
                right_cell = Size::remainder();
            } else {
                center_cell = Size::remainder();
            }
        }

        let sizes = [left_cell, left_gap, center_cell, right_gap, right_cell];

        StripBuilder::new(ui)
            .cell_layout(Layout::left_to_right(egui::Align::Center))
            .size(Size::exact(self.height))
            .vertical(move |mut strip| {
                strip.strip(|mut builder| {
                    for size in sizes {
                        builder = builder.size(size);
                    }

                    builder.horizontal(|mut strip| {
                        strip.cell(left_aligned);
                        strip.empty();
                        strip.cell(centered);
                        strip.empty();
                        strip.cell(right_aligned);
                    });
                });
            });
    }
}

/// Inspired by VirtualList::ui_custom_layout
fn measure_width(ui: &mut egui::Ui, render: &mut impl FnMut(&mut egui::Ui)) -> f32 {
    let mut measure_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(ui.max_rect())
            .layout(Layout::left_to_right(egui::Align::Min)),
    );
    measure_ui.set_invisible();

    let start_width = measure_ui.next_widget_position();
    measure_ui.scope_builder(UiBuilder::new().id_salt("measure"), |ui| {
        render(ui);
    });
    let end_width = measure_ui.next_widget_position();

    (end_width.x - start_width.x + ui.spacing().item_spacing.x).max(0.0)
}
