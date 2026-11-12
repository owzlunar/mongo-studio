use egui::{Color32, RichText, ScrollArea, Ui};

use crate::models::DocumentRow;
use crate::theme::ThemePalette;

pub fn render_json_viewer(ui: &mut Ui, documents: &[DocumentRow], palette: &ThemePalette) {
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (idx, doc) in documents.iter().enumerate() {
                let header_title = format!(
                    "Document #{}  •  _id: \"{}\"  ({})",
                    idx + 1,
                    doc.id,
                    doc.name
                );

                egui::CollapsingHeader::new(RichText::new(header_title).monospace().strong())
                    .default_open(idx < 3)
                    .show(ui, |ui| {
                        egui::Frame::none()
                            .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 45))
                            .inner_margin(egui::Margin::same(8.0))
                            .rounding(egui::Rounding::same(4.0))
                            .stroke(egui::Stroke::new(1.0_f32, palette.border_color))
                            .show(ui, |ui| {
                                render_key_value(ui, "_id", &doc.id, palette.warning_color);
                                render_key_value(ui, "name", &doc.name, palette.success_color);
                                render_key_value(ui, "email", &doc.email, palette.success_color);
                                render_key_value(ui, "status", &doc.status, palette.accent_color);
                                render_key_value(ui, "role", &doc.role, palette.success_color);
                                render_key_value(
                                    ui,
                                    "created_at",
                                    &doc.created_at,
                                    palette.muted_text,
                                );
                            });
                    });
                ui.add_space(4.0);
            }
        });
}

fn render_key_value(ui: &mut Ui, key: &str, value: &str, val_color: Color32) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("  \"{}\":", key))
                .monospace()
                .color(Color32::from_rgb(156, 220, 254)),
        );
        ui.label(
            RichText::new(format!("\"{}\"", value))
                .monospace()
                .color(val_color),
        );
    });
}
