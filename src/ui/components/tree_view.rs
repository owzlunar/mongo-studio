use egui::{Color32, RichText, Ui};

use crate::core::events::AppEvent;
use crate::models::{ConnectionConfig, ConnectionStatus};
use crate::theme::ThemePalette;

pub fn render_connection_tree(
    ui: &mut Ui,
    connections: &mut [ConnectionConfig],
    search_filter: &str,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    let query = search_filter.trim().to_lowercase();

    for conn in connections.iter_mut() {
        let status_dot_color = match conn.status {
            ConnectionStatus::Connected => palette.success_color,
            ConnectionStatus::Connecting => palette.warning_color,
            ConnectionStatus::Disconnected => palette.muted_text,
            ConnectionStatus::Error => palette.error_color,
        };

        let arrow = if conn.expanded { "▼" } else { "▶" };

        ui.horizontal(|ui| {
            let toggle_resp = ui.selectable_label(
                false,
                RichText::new(format!("{} {}", arrow, conn.name)).strong(),
            );
            if toggle_resp.clicked() {
                conn.expanded = !conn.expanded;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new("●")
                        .color(status_dot_color)
                        .size(11.0),
                )
                .on_hover_text(format!("{} ({})", conn.status.label(), conn.host));
            });
        });

        if !conn.expanded {
            continue;
        }

        ui.indent(conn.id, |ui| {
            for db in conn.databases.iter_mut() {
                let db_matches = query.is_empty()
                    || db.name.to_lowercase().contains(&query)
                    || db
                        .collections
                        .iter()
                        .any(|c| c.name.to_lowercase().contains(&query));

                if !db_matches {
                    continue;
                }

                let db_arrow = if db.expanded { "▼" } else { "▶" };
                let db_resp = ui.selectable_label(
                    false,
                    RichText::new(format!("{} {}", db_arrow, db.name))
                        .color(Color32::from_rgb(210, 210, 210)),
                );
                if db_resp.clicked() {
                    db.expanded = !db.expanded;
                }

                if db.expanded {
                    ui.indent(&db.name, |ui| {
                        for coll in &db.collections {
                            if !query.is_empty()
                                && !coll.name.to_lowercase().contains(&query)
                                && !db.name.to_lowercase().contains(&query)
                            {
                                continue;
                            }

                            ui.horizontal(|ui| {
                                let label = RichText::new(format!("  {}", coll.name));
                                let resp = ui.selectable_label(false, label);
                                if resp.clicked() {
                                    events.push(AppEvent::OpenCollection {
                                        connection_name: conn.name.clone(),
                                        database: db.name.clone(),
                                        collection: coll.name.clone(),
                                    });
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(format!("{}", coll.documents.len()))
                                                .size(11.0)
                                                .color(palette.muted_text),
                                        );
                                    },
                                );
                            });
                        }
                    });
                }
            }
        });

        ui.add_space(4.0);
    }
}
