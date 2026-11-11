use egui::{Color32, RichText, Ui};
use egui_dock::TabViewer;
use egui_extras::{Column, TableBuilder};

use crate::core::events::AppEvent;
use crate::database::DocumentService;
use crate::models::CollectionInfo;
use crate::query::filter::QueryFilter;
use crate::theme::ThemePalette;
use crate::ui::components::buttons::{primary_button, secondary_button};
use crate::ui::components::data_grid::render_document_grid;
use crate::ui::components::inputs::filter_input_row;
use crate::ui::components::json_viewer::render_json_viewer;
use crate::workspace::tabs::{CollectionSubTab, CollectionTab, DocumentViewMode};

pub struct CollectionTabViewer<'a> {
    pub collections: &'a [CollectionInfo],
    pub palette: &'a ThemePalette,
    pub events: &'a mut Vec<AppEvent>,
}

impl<'a> TabViewer for CollectionTabViewer<'a> {
    type Tab = CollectionTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        RichText::new(tab.title()).strong().into()
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        let maybe_coll = self
            .collections
            .iter()
            .find(|c| c.database == tab.database && c.name == tab.collection);

        // Top sub-navigation bar: Documents | Indexes | Schema | Aggregation
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{} ▸ {}", tab.database, tab.collection))
                    .monospace()
                    .color(self.palette.muted_text),
            );
            ui.separator();

            for subtab in [
                CollectionSubTab::Documents,
                CollectionSubTab::Indexes,
                CollectionSubTab::Schema,
                CollectionSubTab::Aggregation,
            ] {
                let selected = tab.active_subtab == subtab;
                if ui
                    .selectable_label(selected, RichText::new(subtab.label()).strong())
                    .clicked()
                {
                    tab.active_subtab = subtab;
                }
            }

            if tab.active_subtab == CollectionSubTab::Documents {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .selectable_label(
                            tab.view_mode == DocumentViewMode::JsonTree,
                            "{ } JSON View",
                        )
                        .clicked()
                    {
                        tab.view_mode = DocumentViewMode::JsonTree;
                    }
                    if ui
                        .selectable_label(tab.view_mode == DocumentViewMode::Table, "⊞ Table View")
                        .clicked()
                    {
                        tab.view_mode = DocumentViewMode::Table;
                    }

                    if secondary_button(ui, "+ Insert Doc", self.palette.border_color).clicked() {
                        self.events.push(AppEvent::InsertMockDocument {
                            database: tab.database.clone(),
                            collection: tab.collection.clone(),
                        });
                    }
                });
            }
        });

        ui.separator();

        let Some(coll) = maybe_coll else {
            ui.centered_and_justified(|ui| {
                ui.label(format!(
                    "Collection {}.{} not found.",
                    tab.database, tab.collection
                ));
            });
            return;
        };

        match tab.active_subtab {
            CollectionSubTab::Documents => {
                let parsed_filter = QueryFilter::parse(&tab.filter_query);
                let is_valid = parsed_filter.is_valid_json();

                ui.horizontal_wrapped(|ui| {
                    let enter_pressed = filter_input_row(
                        ui,
                        "Filter",
                        &mut tab.filter_query,
                        "{\"status\": \"active\"}",
                        is_valid,
                        self.palette.error_color,
                    );

                    ui.label(RichText::new("Sort").strong().size(12.0));
                    ui.add(
                        egui::TextEdit::singleline(&mut tab.sort_query)
                            .font(egui::FontId::monospace(12.0))
                            .desired_width(110.0),
                    );

                    if primary_button(ui, "Find", self.palette.accent_color).clicked()
                        || enter_pressed
                    {
                        self.events.push(AppEvent::ExecuteFilterQuery {
                            database: tab.database.clone(),
                            collection: tab.collection.clone(),
                            filter: tab.filter_query.clone(),
                        });
                    }

                    if secondary_button(ui, "Reset", self.palette.border_color).clicked() {
                        tab.filter_query = "{}".to_string();
                        self.events.push(AppEvent::ExecuteFilterQuery {
                            database: tab.database.clone(),
                            collection: tab.collection.clone(),
                            filter: "{}".to_string(),
                        });
                    }

                    if !is_valid {
                        ui.label(
                            RichText::new("⚠ Invalid JSON filter")
                                .color(self.palette.error_color)
                                .size(11.5),
                        );
                    }
                });

                ui.add_space(6.0);

                let filtered_docs =
                    DocumentService::filter_documents(&coll.documents, &tab.filter_query, tab.limit);

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "Showing {} of {} documents",
                            filtered_docs.len(),
                            coll.documents.len()
                        ))
                        .size(11.5)
                        .color(self.palette.muted_text),
                    );
                });

                ui.add_space(4.0);

                match tab.view_mode {
                    DocumentViewMode::Table => {
                        render_document_grid(
                            ui,
                            &filtered_docs,
                            &mut tab.selected_row,
                            self.palette,
                        );
                    }
                    DocumentViewMode::JsonTree => {
                        render_json_viewer(ui, &filtered_docs, self.palette);
                    }
                }
            }
            CollectionSubTab::Indexes => {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "Indexes on {}.{} ({} total)",
                        tab.database,
                        tab.collection,
                        coll.indexes.len()
                    ))
                    .strong(),
                );
                ui.add_space(6.0);

                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .column(Column::initial(160.0))
                    .column(Column::initial(220.0))
                    .column(Column::initial(100.0))
                    .column(Column::initial(80.0))
                    .column(Column::remainder())
                    .header(24.0, |mut header| {
                        for col_name in ["Index Name", "Key Specification", "Type", "Unique", "Size"]
                        {
                            header.col(|ui| {
                                ui.label(
                                    RichText::new(col_name)
                                        .strong()
                                        .color(self.palette.accent_color),
                                );
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(24.0, coll.indexes.len(), |mut row| {
                            let idx_info = &coll.indexes[row.index()];
                            row.col(|ui| {
                                ui.label(RichText::new(&idx_info.name).monospace().strong());
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(&idx_info.keys).monospace());
                            });
                            row.col(|ui| {
                                ui.label(&idx_info.index_type);
                            });
                            row.col(|ui| {
                                let badge = if idx_info.unique { "UNIQUE" } else { "—" };
                                ui.label(
                                    RichText::new(badge).color(if idx_info.unique {
                                        self.palette.success_color
                                    } else {
                                        self.palette.muted_text
                                    }),
                                );
                            });
                            row.col(|ui| {
                                ui.label(format!("{:.1} KB", idx_info.size_kb));
                            });
                        });
                    });
            }
            CollectionSubTab::Schema => {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "Schema Analysis — {}.{} (Sampled {} documents)",
                        tab.database,
                        tab.collection,
                        coll.documents.len()
                    ))
                    .strong(),
                );
                ui.add_space(6.0);

                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .column(Column::initial(140.0))
                    .column(Column::initial(150.0))
                    .column(Column::initial(180.0))
                    .column(Column::remainder())
                    .header(24.0, |mut header| {
                        for title in ["Field", "BSON Type", "Frequency", "Sample Value"] {
                            header.col(|ui| {
                                ui.label(
                                    RichText::new(title)
                                        .strong()
                                        .color(self.palette.accent_color),
                                );
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(24.0, coll.schema_fields.len(), |mut row| {
                            let field = &coll.schema_fields[row.index()];
                            row.col(|ui| {
                                ui.label(RichText::new(&field.field_name).monospace().strong());
                            });
                            row.col(|ui| {
                                ui.label(
                                    RichText::new(&field.bson_type)
                                        .monospace()
                                        .color(self.palette.success_color),
                                );
                            });
                            row.col(|ui| {
                                ui.add(
                                    egui::ProgressBar::new(field.frequency_pct / 100.0)
                                        .text(format!("{:.0}%", field.frequency_pct)),
                                );
                            });
                            row.col(|ui| {
                                ui.label(
                                    RichText::new(&field.sample_value)
                                        .monospace()
                                        .color(Color32::from_rgb(206, 145, 120)),
                                );
                            });
                        });
                    });
            }
            CollectionSubTab::Aggregation => {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "Aggregation Pipeline Builder — {}.{}",
                        tab.database, tab.collection
                    ))
                    .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "[\n  { \"$match\": { \"status\": \"active\" } },\n  { \"$sort\": { \"_id\": 1 } }\n]",
                    )
                    .monospace()
                    .color(self.palette.success_color),
                );
            }
        }
    }
}
