use egui::{Pos2, Rect, Response, Sense};

pub fn hover_expand(
    ui: &mut egui::Ui,
    id: egui::Id,
    size: f32,
    expand_size: f32,
    anim_speed: f32,
) -> (egui::Rect, f32, egui::Response) {
    // Allocate space for the profile picture with a fixed size
    let default_size = size + expand_size;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(default_size, default_size), egui::Sense::click());

    let val = ui
        .ctx()
        .animate_bool_with_time(id, response.hovered(), anim_speed);

    let size = size + val * expand_size;
    (rect, size, response)
}

pub fn hover_expand_small(ui: &mut egui::Ui, id: egui::Id) -> (egui::Rect, f32, egui::Response) {
    let size = 10.0;
    let expand_size = 5.0;
    let anim_speed = 0.05;

    hover_expand(ui, id, size, expand_size, anim_speed)
}

pub static ICON_EXPANSION_MULTIPLE: f32 = 1.2;
pub static ANIM_SPEED: f32 = 0.05;
pub struct AnimationHelper {
    rect: Rect,
    center: Pos2,
    response: Response,
    animation_progress: f32,
    expansion_multiple: f32,
}

impl AnimationHelper {
    pub fn new(
        ui: &mut egui::Ui,
        animation_name: impl std::hash::Hash,
        max_size: egui::Vec2,
    ) -> Self {
        let id = ui.id().with(animation_name);
        let (rect, response) = ui.allocate_exact_size(max_size, Sense::click());

        let animation_progress =
            ui.ctx()
                .animate_bool_with_time(id, response.hovered(), ANIM_SPEED);

        Self {
            rect,
            center: rect.center(),
            response,
            animation_progress,
            expansion_multiple: ICON_EXPANSION_MULTIPLE,
        }
    }

    pub fn new_from_rect(
        ui: &mut egui::Ui,
        animation_name: impl std::hash::Hash,
        animation_rect: egui::Rect,
    ) -> Self {
        let id = ui.id().with(animation_name);
        let response = ui.allocate_rect(animation_rect, Sense::click());

        let animation_progress =
            ui.ctx()
                .animate_bool_with_time(id, response.hovered(), ANIM_SPEED);

        Self {
            rect: animation_rect,
            center: animation_rect.center(),
            response,
            animation_progress,
            expansion_multiple: ICON_EXPANSION_MULTIPLE,
        }
    }

    pub fn scale_1d_pos(&self, min_object_size: f32) -> f32 {
        let max_object_size = min_object_size * self.expansion_multiple;

        if self.response.is_pointer_button_down_on() {
            min_object_size
        } else {
            min_object_size + ((max_object_size - min_object_size) * self.animation_progress)
        }
    }

    pub fn scale_radius(&self, min_diameter: f32) -> f32 {
        self.scale_1d_pos((min_diameter - 1.0) / 2.0)
    }

    pub fn get_animation_rect(&self) -> egui::Rect {
        self.rect
    }

    pub fn center(&self) -> Pos2 {
        self.rect.center()
    }

    pub fn take_animation_response(self) -> egui::Response {
        self.response
    }

    // Scale a minimum position from center to the current animation position
    pub fn scale_from_center(&self, x_min: f32, y_min: f32) -> Pos2 {
        Pos2::new(
            self.center.x + self.scale_1d_pos(x_min),
            self.center.y + self.scale_1d_pos(y_min),
        )
    }
}
