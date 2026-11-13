use egui::{Color32, CursorIcon, RichText, Rounding, Stroke, Ui, Vec2};

pub const PRIMARY_BUTTON_MIN_SIZE: Vec2 = Vec2::new(72.0, 24.0);
pub const SECONDARY_BUTTON_MIN_SIZE: Vec2 = Vec2::new(64.0, 24.0);
pub const BUTTON_ROUNDING_RADIUS: f32 = 3.0;
pub const BUTTON_FONT_SIZE: f32 = 12.0;

pub fn primary_button(ui: &mut Ui, label: &str, accent: Color32) -> egui::Response {
    let btn = egui::Button::new(
        RichText::new(label)
            .color(Color32::WHITE)
            .strong()
            .size(BUTTON_FONT_SIZE),
    )
    .fill(accent)
    .rounding(Rounding::same(BUTTON_ROUNDING_RADIUS))
    .min_size(PRIMARY_BUTTON_MIN_SIZE);
    ui.add(btn).on_hover_cursor(CursorIcon::PointingHand)
}

pub fn secondary_button(ui: &mut Ui, label: &str, border: Color32) -> egui::Response {
    let btn = egui::Button::new(
        RichText::new(label)
            .size(BUTTON_FONT_SIZE)
            .color(ui.visuals().text_color()),
    )
    .stroke(Stroke::new(1.0_f32, border))
    .rounding(Rounding::same(BUTTON_ROUNDING_RADIUS))
    .min_size(SECONDARY_BUTTON_MIN_SIZE);
    ui.add(btn).on_hover_cursor(CursorIcon::PointingHand)
}
