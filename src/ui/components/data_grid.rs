use egui::{Color32, RichText, Sense, Ui};
use egui_extras::{Column, TableBuilder};

use crate::models::DocumentRow;
use crate::theme::ThemePalette;

pub fn render_document_grid(
    ui: &mut Ui,
    documents: &[DocumentRow],
    selected_row: &mut Option<usize>,
    palette: &ThemePalette,
) {
    let available_height = ui.available_height().max(120.0);

    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .min_scrolled_height(100.0)
        .max_scroll_height(available_height)
        .column(Column::initial(85.0).at_least(50.0))
        .column(Column::initial(150.0).at_least(80.0))
        .column(Column::initial(200.0).at_least(120.0))
        .column(Column::initial(100.0).at_least(75.0))
        .column(Column::initial(100.0).at_least(75.0))
        .column(Column::remainder().at_least(130.0))
        .sense(Sense::click())
        .header(26.0, |mut header| {
            for col_title in ["_id", "name", "email", "status", "role", "created_at"] {
                header.col(|ui| {
                    ui.label(
                        RichText::new(col_title)
                            .strong()
                            .monospace()
                            .color(palette.accent_color),
                    );
                });
            }
        })
        .body(|body| {
            body.rows(24.0, documents.len(), |mut row| {
                let idx = row.index();
                let doc = &documents[idx];
                let is_selected = *selected_row == Some(idx);
                row.set_selected(is_selected);

                row.col(|ui| {
                    if ui
                        .label(RichText::new(&doc.id).monospace().color(palette.warning_color))
                        .clicked()
                    {
                        *selected_row = Some(idx);
                    }
                });
                row.col(|ui| {
                    if ui.label(RichText::new(&doc.name).strong()).clicked() {
                        *selected_row = Some(idx);
                    }
                });
                row.col(|ui| {
                    if ui
                        .label(RichText::new(&doc.email).monospace().size(12.0))
                        .clicked()
                    {
                        *selected_row = Some(idx);
                    }
                });
                row.col(|ui| {
                    let badge_color = match doc.status.as_str() {
                        "active" => palette.success_color,
                        "pending" => palette.warning_color,
                        _ => Color32::from_rgb(180, 100, 100),
                    };
                    if ui
                        .label(RichText::new(&doc.status).monospace().color(badge_color))
                        .clicked()
                    {
                        *selected_row = Some(idx);
                    }
                });
                row.col(|ui| {
                    if ui.label(RichText::new(&doc.role).monospace()).clicked() {
                        *selected_row = Some(idx);
                    }
                });
                row.col(|ui| {
                    if ui
                        .label(
                            RichText::new(&doc.created_at)
                                .monospace()
                                .size(11.5)
                                .color(palette.muted_text),
                        )
                        .clicked()
                    {
                        *selected_row = Some(idx);
                    }
                });

                if row.response().clicked() {
                    *selected_row = Some(idx);
                }
            });
        });
}
