use egui::{Color32, RichText, Rounding, Stroke, Ui, Vec2};

pub fn primary_button(ui: &mut Ui, label: &str, accent: Color32) -> egui::Response {
    let btn = egui::Button::new(RichText::new(label).color(Color32::WHITE).strong())
        .fill(accent)
        .rounding(Rounding::same(3.0))
        .min_size(Vec2::new(72.0, 24.0));
    ui.add(btn)
}

pub fn secondary_button(ui: &mut Ui, label: &str, border: Color32) -> egui::Response {
    let btn = egui::Button::new(RichText::new(label))
        .stroke(Stroke::new(1.0, border))
        .rounding(Rounding::same(3.0))
        .min_size(Vec2::new(64.0, 24.0));
    ui.add(btn)
}
