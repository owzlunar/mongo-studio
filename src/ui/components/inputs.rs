use egui::{Color32, FontId, RichText, TextEdit, Ui};

pub fn filter_input_row(
    ui: &mut Ui,
    label: &str,
    value: &mut String,
    hint: &str,
    is_valid: bool,
    error_color: Color32,
) -> bool {
    let mut submitted = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).strong().size(12.0));
        let mut edit = TextEdit::singleline(value)
            .font(FontId::monospace(12.5))
            .hint_text(hint)
            .desired_width(260.0);

        if !is_valid {
            edit = edit.text_color(error_color);
        }

        let response = ui.add(edit);
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            submitted = true;
        }
    });
    submitted
}
