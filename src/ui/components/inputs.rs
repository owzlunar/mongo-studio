use egui::{Color32, FontId, Margin, RichText, TextEdit, Ui, Vec2};

pub fn filter_input_row(
    ui: &mut Ui,
    label: &str,
    value: &mut String,
    hint: &str,
    is_valid: bool,
    error_color: Color32,
) -> bool {
    ui.label(
        RichText::new(label)
            .strong()
            .size(12.0)
            .color(ui.visuals().text_color()),
    );
    let mut edit = TextEdit::singleline(value)
        .font(FontId::monospace(12.0))
        .hint_text(hint)
        .margin(Margin::symmetric(6.0, 4.0))
        .min_size(Vec2::new(260.0, 24.0))
        .desired_width(260.0);

    if !is_valid {
        edit = edit.text_color(error_color);
    }

    let response = ui.add(edit);
    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
}
