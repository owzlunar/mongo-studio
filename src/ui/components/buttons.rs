use egui::{Color32, CursorIcon, RichText, Rounding, Stroke, Ui, Vec2};

pub fn primary_button(ui: &mut Ui, label: &str, accent: Color32) -> egui::Response {
    let btn = egui::Button::new(
        RichText::new(label)
            .color(Color32::WHITE)
            .strong()
            .size(12.0),
    )
    .fill(accent)
    .rounding(Rounding::same(3.0))
    .min_size(Vec2::new(72.0, 24.0));
    ui.add(btn).on_hover_cursor(CursorIcon::PointingHand)
}

pub fn secondary_button(ui: &mut Ui, label: &str, border: Color32) -> egui::Response {
    let btn = egui::Button::new(RichText::new(label).size(12.0))
        .stroke(Stroke::new(1.0_f32, border))
        .rounding(Rounding::same(3.0))
        .min_size(Vec2::new(64.0, 24.0));
    ui.add(btn).on_hover_cursor(CursorIcon::PointingHand)
}
