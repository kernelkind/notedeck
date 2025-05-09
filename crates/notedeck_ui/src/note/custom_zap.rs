pub struct CustomZapView {}

#[allow(clippy::new_without_default)]
impl CustomZapView {
    pub fn new() -> Self {
        Self {}
    }

    pub fn ui(&mut self, _ui: &mut egui::Ui) -> Option<u64> {
        unimplemented!()
    }
}
