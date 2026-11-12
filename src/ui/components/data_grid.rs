use std::borrow::Cow;

use egui::{Color32, CursorIcon, Label, RichText, ScrollArea, Sense, Ui};
use egui_extras::{Column, TableBuilder};
use egui_phosphor::regular;

use crate::core::events::AppEvent;
use crate::models::DocumentRow;
use crate::theme::ThemePalette;

#[derive(Debug, Clone, Copy)]
pub struct DocumentTarget<'a> {
    pub connection_name: &'a str,
    pub database: &'a str,
    pub collection: &'a str,
}

fn status_badge_color(value: &str, palette: &ThemePalette) -> Color32 {
    if value.eq_ignore_ascii_case("active")
        || value.eq_ignore_ascii_case("ok")
        || value.eq_ignore_ascii_case("paid")
        || value.eq_ignore_ascii_case("shipped")
        || value.eq_ignore_ascii_case("completed")
        || value.eq_ignore_ascii_case("true")
    {
        palette.success_color
    } else if value.eq_ignore_ascii_case("pending") || value.eq_ignore_ascii_case("warning") {
        palette.warning_color
    } else if value.eq_ignore_ascii_case("inactive")
        || value.eq_ignore_ascii_case("error")
        || value.eq_ignore_ascii_case("failed")
        || value.eq_ignore_ascii_case("out_of_stock")
        || value.eq_ignore_ascii_case("false")
    {
        palette.error_color
    } else {
        palette.text_primary
    }
}

pub fn render_document_grid(
    ui: &mut Ui,
    target: DocumentTarget<'_>,
    documents: &[DocumentRow],
    columns: &[String],
    selected_row: &mut Option<usize>,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    if documents.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("No documents match the current query.")
                    .color(palette.muted_text)
                    .size(13.0),
            );
        });
        return;
    }

    let effective_columns: Cow<'_, [String]> = if columns.is_empty() {
        Cow::Owned(DocumentRow::extract_columns(documents))
    } else {
        Cow::Borrowed(columns)
    };

    ScrollArea::both()
        .id_source((
            "doc_grid_scroll",
            target.connection_name,
            target.database,
            target.collection,
        ))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut table = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .vscroll(false)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .sense(Sense::click());

            for col in effective_columns.as_ref() {
                let col_spec = match col.as_str() {
                    "_id" => Column::initial(195.0).at_least(110.0),
                    "status" | "active" | "enabled" => Column::initial(110.0).at_least(80.0),
                    _ => Column::initial(165.0).at_least(90.0),
                };
                table = table.column(col_spec);
            }
            // Fixed Actions column
            table = table.column(Column::initial(138.0).at_least(128.0));

            table
                .header(26.0, |mut header| {
                    for col_title in effective_columns.as_ref() {
                        header.col(|ui| {
                            ui.label(
                                RichText::new(col_title)
                                    .strong()
                                    .monospace()
                                    .color(palette.accent_color),
                            );
                        });
                    }
                    header.col(|ui| {
                        ui.label(
                            RichText::new("Actions")
                                .strong()
                                .monospace()
                                .color(palette.accent_color),
                        );
                    });
                })
                .body(|body| {
                    body.rows(26.0, documents.len(), |mut row| {
                        let idx = row.index();
                        let doc = &documents[idx];
                        let is_selected = *selected_row == Some(idx);
                        row.set_selected(is_selected);

                        for col in effective_columns.as_ref() {
                            row.col(|ui| {
                                let cell_value = doc.cell_display_value(col);
                                let rich = if col == "_id" {
                                    RichText::new(cell_value)
                                        .monospace()
                                        .size(12.0)
                                        .color(palette.warning_color)
                                } else if col == "status" {
                                    let badge_color = status_badge_color(&cell_value, palette);
                                    RichText::new(cell_value)
                                        .monospace()
                                        .strong()
                                        .size(12.0)
                                        .color(badge_color)
                                } else {
                                    RichText::new(cell_value)
                                        .monospace()
                                        .size(12.0)
                                        .color(palette.text_primary)
                                };

                                if ui
                                    .add(Label::new(rich).truncate(true).sense(Sense::click()))
                                    .on_hover_cursor(CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    *selected_row = Some(idx);
                                }
                            });
                        }

                        row.col(|ui| {
                            let extracted_id = doc.extract_bson_id();
                            ui.horizontal(|ui| match &extracted_id {
                                Ok(id) => {
                                    if ui
                                        .small_button(format!("{} Edit", regular::PENCIL_SIMPLE))
                                        .on_hover_cursor(CursorIcon::PointingHand)
                                        .on_hover_text("Edit document JSON")
                                        .clicked()
                                    {
                                        *selected_row = Some(idx);
                                        events.push(AppEvent::OpenEditDocumentModal {
                                            connection_name: target.connection_name.to_string(),
                                            database: target.database.to_string(),
                                            collection: target.collection.to_string(),
                                            original_id: id.clone(),
                                            json_text: doc.to_pretty_json(),
                                        });
                                    }

                                    if ui
                                        .small_button(
                                            RichText::new(format!("{} Del", regular::TRASH))
                                                .color(palette.error_color),
                                        )
                                        .on_hover_cursor(CursorIcon::PointingHand)
                                        .on_hover_text("Delete document")
                                        .clicked()
                                    {
                                        *selected_row = Some(idx);
                                        events.push(AppEvent::RequestDeleteDocument {
                                            connection_name: target.connection_name.to_string(),
                                            database: target.database.to_string(),
                                            collection: target.collection.to_string(),
                                            doc_id: id.clone(),
                                            display_id: doc.id.clone(),
                                        });
                                    }
                                }
                                Err(err) => {
                                    ui.add_enabled(
                                        false,
                                        egui::Button::new(format!(
                                            "{} Edit",
                                            regular::PENCIL_SIMPLE
                                        ))
                                        .small(),
                                    )
                                    .on_disabled_hover_text(format!("Cannot edit: {err}"));

                                    ui.add_enabled(
                                        false,
                                        egui::Button::new(
                                            RichText::new(format!("{} Del", regular::TRASH))
                                                .color(palette.muted_text),
                                        )
                                        .small(),
                                    )
                                    .on_disabled_hover_text(format!("Cannot delete: {err}"));
                                }
                            });
                        });

                        if row
                            .response()
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .clicked()
                        {
                            *selected_row = Some(idx);
                        }
                    });
                });
        });
}
