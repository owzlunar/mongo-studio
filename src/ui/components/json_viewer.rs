use egui::{
    collapsing_header::CollapsingState, Color32, CursorIcon, Frame, Margin, RichText, Rounding,
    ScrollArea, Stroke, Ui,
};
use egui_phosphor::regular;
use serde_json::{Map, Value};

use crate::core::events::AppEvent;
use crate::models::DocumentRow;
use crate::theme::ThemePalette;
use crate::ui::components::data_grid::DocumentTarget;

const MAX_JSON_DEPTH: usize = 32;

#[derive(Clone, Copy)]
struct JsonSyntaxColors {
    key: Color32,
    string: Color32,
    number: Color32,
    boolean: Color32,
    null: Color32,
    punctuation: Color32,
    card_bg: Color32,
}

impl JsonSyntaxColors {
    fn from_theme(ui: &Ui, palette: &ThemePalette) -> Self {
        let is_dark = ui.visuals().dark_mode || palette.editor_bg.r() < 128;
        if is_dark {
            Self {
                key: Color32::from_rgb(156, 220, 254),
                string: Color32::from_rgb(206, 145, 120),
                number: Color32::from_rgb(181, 206, 168),
                boolean: Color32::from_rgb(86, 156, 214),
                null: palette.muted_text,
                punctuation: palette.muted_text,
                card_bg: palette.panel_bg,
            }
        } else {
            Self {
                key: Color32::from_rgb(4, 81, 165),
                string: Color32::from_rgb(163, 21, 21),
                number: Color32::from_rgb(9, 134, 88),
                boolean: Color32::from_rgb(0, 102, 204),
                null: palette.muted_text,
                punctuation: palette.muted_text,
                card_bg: palette.panel_bg,
            }
        }
    }
}

pub fn render_json_viewer(
    ui: &mut Ui,
    target: DocumentTarget<'_>,
    documents: &[DocumentRow],
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

    let syntax = JsonSyntaxColors::from_theme(ui, palette);

    ScrollArea::vertical()
        .id_source((
            "json_viewer_scroll",
            target.connection_name,
            target.database,
            target.collection,
        ))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (idx, doc) in documents.iter().enumerate() {
                let header_title = format!("Document #{}  •  _id: {}", idx + 1, doc.id);
                let card_id = ui.make_persistent_id((
                    "json_doc_card",
                    target.connection_name,
                    target.database,
                    target.collection,
                    idx,
                    &doc.id,
                ));
                let extracted_id = doc.extract_bson_id();

                Frame::none()
                    .fill(syntax.card_bg)
                    .inner_margin(Margin::same(8.0))
                    .rounding(Rounding::same(4.0))
                    .stroke(Stroke::new(1.0_f32, palette.border_color))
                    .show(ui, |ui| {
                        let collapsing =
                            CollapsingState::load_with_default_open(ui.ctx(), card_id, idx < 3);

                        collapsing
                            .show_header(ui, |ui| {
                                ui.label(
                                    RichText::new(&header_title)
                                        .monospace()
                                        .strong()
                                        .color(palette.text_primary),
                                );

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        match &extracted_id {
                                            Ok(id) => {
                                                if ui
                                                    .small_button(
                                                        RichText::new(format!(
                                                            "{} Delete",
                                                            regular::TRASH
                                                        ))
                                                        .color(palette.error_color),
                                                    )
                                                    .on_hover_cursor(CursorIcon::PointingHand)
                                                    .on_hover_text("Delete document")
                                                    .clicked()
                                                {
                                                    events.push(AppEvent::RequestDeleteDocument {
                                                        connection_name: target
                                                            .connection_name
                                                            .to_string(),
                                                        database: target.database.to_string(),
                                                        collection: target.collection.to_string(),
                                                        doc_id: id.clone(),
                                                        display_id: doc.id.clone(),
                                                    });
                                                }

                                                if ui
                                                    .small_button(format!(
                                                        "{} Edit",
                                                        regular::PENCIL_SIMPLE
                                                    ))
                                                    .on_hover_cursor(CursorIcon::PointingHand)
                                                    .on_hover_text("Edit document JSON")
                                                    .clicked()
                                                {
                                                    events.push(AppEvent::OpenEditDocumentModal {
                                                        connection_name: target
                                                            .connection_name
                                                            .to_string(),
                                                        database: target.database.to_string(),
                                                        collection: target.collection.to_string(),
                                                        original_id: id.clone(),
                                                        json_text: doc.to_pretty_json(),
                                                    });
                                                }
                                            }
                                            Err(err) => {
                                                ui.add_enabled(
                                                    false,
                                                    egui::Button::new(
                                                        RichText::new(format!(
                                                            "{} Delete",
                                                            regular::TRASH
                                                        ))
                                                        .color(palette.muted_text),
                                                    )
                                                    .small(),
                                                )
                                                .on_disabled_hover_text(format!(
                                                    "Cannot delete: {err}"
                                                ));

                                                ui.add_enabled(
                                                    false,
                                                    egui::Button::new(format!(
                                                        "{} Edit",
                                                        regular::PENCIL_SIMPLE
                                                    ))
                                                    .small(),
                                                )
                                                .on_disabled_hover_text(format!(
                                                    "Cannot edit: {err}"
                                                ));
                                            }
                                        }

                                        if ui
                                            .small_button(format!("{} Copy JSON", regular::COPY))
                                            .on_hover_cursor(CursorIcon::PointingHand)
                                            .on_hover_text("Copy pretty Extended JSON to clipboard")
                                            .clicked()
                                        {
                                            ui.ctx().copy_text(doc.to_pretty_json());
                                        }

                                        if !doc.id.is_empty()
                                            && ui
                                                .small_button(format!("{} Copy _id", regular::COPY))
                                                .on_hover_cursor(CursorIcon::PointingHand)
                                                .on_hover_text("Copy _id to clipboard")
                                                .clicked()
                                        {
                                            ui.ctx().copy_text(doc.id.clone());
                                        }
                                    },
                                );
                            })
                            .body(|ui| {
                                ui.add_space(2.0);
                                let json_val = doc.cached_or_parsed_json();
                                match json_val.as_ref() {
                                    Value::Object(map) => {
                                        render_json_object(ui, map, &syntax, (card_id, 0), 0);
                                    }
                                    other => {
                                        render_json_entry(
                                            ui,
                                            "$value",
                                            other,
                                            &syntax,
                                            (card_id, 0),
                                            0,
                                        );
                                    }
                                }
                            });
                    });

                ui.add_space(6.0);
            }
        });
}

fn render_json_object(
    ui: &mut Ui,
    map: &Map<String, Value>,
    syntax: &JsonSyntaxColors,
    parent_salt: (egui::Id, usize),
    depth: usize,
) {
    if map.is_empty() {
        ui.label(RichText::new("{}").monospace().color(syntax.punctuation));
        return;
    }

    for (key, val) in map {
        render_json_entry(ui, key, val, syntax, parent_salt, depth);
    }
}

fn render_json_entry(
    ui: &mut Ui,
    key_label: &str,
    value: &Value,
    syntax: &JsonSyntaxColors,
    parent_salt: (egui::Id, usize),
    depth: usize,
) {
    if depth >= MAX_JSON_DEPTH {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("\"{key_label}\":"))
                    .monospace()
                    .color(syntax.key),
            );
            ui.label(
                RichText::new(value.to_string())
                    .monospace()
                    .color(syntax.punctuation),
            );
        });
        return;
    }

    match value {
        Value::Object(map) if !map.is_empty() => {
            let header = format!("\"{key_label}\": {{{} fields}}", map.len());
            egui::CollapsingHeader::new(RichText::new(header).monospace().color(syntax.key))
                .id_source((parent_salt, key_label, depth))
                .default_open(depth == 0 && map.len() <= 4)
                .show(ui, |ui| {
                    for (child_idx, (child_key, child_val)) in map.iter().enumerate() {
                        render_json_entry(
                            ui,
                            child_key,
                            child_val,
                            syntax,
                            (parent_salt.0, child_idx),
                            depth + 1,
                        );
                    }
                });
        }
        Value::Array(arr) if !arr.is_empty() => {
            let header = format!("\"{key_label}\": [{} items]", arr.len());
            egui::CollapsingHeader::new(RichText::new(header).monospace().color(syntax.key))
                .id_source((parent_salt, key_label, depth))
                .default_open(depth == 0 && arr.len() <= 4)
                .show(ui, |ui| {
                    for (item_idx, item_val) in arr.iter().enumerate() {
                        let idx_label = format!("[{item_idx}]");
                        render_json_entry(
                            ui,
                            &idx_label,
                            item_val,
                            syntax,
                            (parent_salt.0, item_idx),
                            depth + 1,
                        );
                    }
                });
        }
        Value::Object(_) => {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("\"{key_label}\":"))
                        .monospace()
                        .color(syntax.key),
                );
                ui.label(RichText::new("{}").monospace().color(syntax.punctuation));
            });
        }
        Value::Array(_) => {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("\"{key_label}\":"))
                        .monospace()
                        .color(syntax.key),
                );
                ui.label(RichText::new("[]").monospace().color(syntax.punctuation));
            });
        }
        Value::String(s) => {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("\"{key_label}\":"))
                        .monospace()
                        .color(syntax.key),
                );
                ui.label(
                    RichText::new(format!("\"{s}\""))
                        .monospace()
                        .color(syntax.string),
                );
            });
        }
        Value::Number(n) => {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("\"{key_label}\":"))
                        .monospace()
                        .color(syntax.key),
                );
                ui.label(
                    RichText::new(n.to_string())
                        .monospace()
                        .color(syntax.number),
                );
            });
        }
        Value::Bool(b) => {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("\"{key_label}\":"))
                        .monospace()
                        .color(syntax.key),
                );
                ui.label(
                    RichText::new(if *b { "true" } else { "false" })
                        .monospace()
                        .color(syntax.boolean),
                );
            });
        }
        Value::Null => {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("\"{key_label}\":"))
                        .monospace()
                        .color(syntax.key),
                );
                ui.label(RichText::new("null").monospace().color(syntax.null));
            });
        }
    }
}
