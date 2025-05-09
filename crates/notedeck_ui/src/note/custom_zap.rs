pub struct CustomZapView {}

#[allow(clippy::new_without_default)]
impl CustomZapView {
    pub fn new() -> Self {
        Self {}
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<u64> {
        ui.label("hello from custom zap view");
        None
    }
}
