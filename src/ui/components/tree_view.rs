use egui::{Color32, CursorIcon, RichText, Ui};
use egui_phosphor::regular;

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

    for (conn_idx, conn) in connections.iter_mut().enumerate() {
        let status_dot_color = match conn.status {
            ConnectionStatus::Connected => palette.success_color,
            ConnectionStatus::Connecting => palette.warning_color,
            ConnectionStatus::Disconnected => palette.muted_text,
            ConnectionStatus::Error => palette.error_color,
        };

        let arrow = if conn.expanded {
            regular::CARET_DOWN
        } else {
            regular::CARET_RIGHT
        };

        let hover_details = if let Some(err) = &conn.last_error {
            format!("{} ({})\nError: {err}", conn.status.label(), conn.host)
        } else {
            format!(
                "{} ({}) — Right-click for connection actions",
                conn.status.label(),
                conn.host
            )
        };

        ui.horizontal(|ui| {
            let toggle_resp = ui
                .selectable_label(
                    false,
                    RichText::new(format!(
                        "{arrow} {} {}",
                        regular::PLUGS_CONNECTED,
                        conn.name
                    ))
                    .strong(),
                )
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text(&hover_details);

            if toggle_resp.clicked() {
                conn.expanded = !conn.expanded;
            }

            toggle_resp.context_menu(|ui| {
                if ui
                    .button(format!("{} Connect / Reconnect", regular::PLUGS_CONNECTED))
                    .clicked()
                {
                    events.push(AppEvent::ConnectProfile { index: conn_idx });
                    ui.close_menu();
                }
                if conn.status.is_connected()
                    && ui
                        .button(format!("{} Disconnect", regular::PLUGS))
                        .clicked()
                {
                    events.push(AppEvent::DisconnectProfile { index: conn_idx });
                    ui.close_menu();
                }
                ui.separator();
                if ui
                    .button(
                        RichText::new(format!("{} Remove Profile", regular::TRASH))
                            .color(palette.error_color),
                    )
                    .clicked()
                {
                    events.push(AppEvent::RemoveConnection { index: conn_idx });
                    ui.close_menu();
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(regular::RECORD)
                        .color(status_dot_color)
                        .size(11.0),
                )
                .on_hover_text(&hover_details);

                if !conn.status.is_connected()
                    && conn.status != ConnectionStatus::Connecting
                    && ui
                        .small_button(RichText::new(regular::PLUGS).size(11.0))
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .on_hover_text(format!("Connect to {}", conn.host))
                        .clicked()
                {
                    events.push(AppEvent::ConnectProfile { index: conn_idx });
                }
            });
        });

        if !conn.expanded {
            continue;
        }

        ui.indent(conn.id, |ui| {
            for db in &mut conn.databases {
                let db_matches = query.is_empty()
                    || db.name.to_lowercase().contains(&query)
                    || db
                        .collections
                        .iter()
                        .any(|c| c.name.to_lowercase().contains(&query));

                if !db_matches {
                    continue;
                }

                let db_arrow = if db.expanded {
                    regular::CARET_DOWN
                } else {
                    regular::CARET_RIGHT
                };
                let db_resp = ui
                    .selectable_label(
                        false,
                        RichText::new(format!("{db_arrow} {} {}", regular::DATABASE, db.name))
                            .color(Color32::from_rgb(210, 210, 210)),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand);
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
                                let label = RichText::new(format!(
                                    "  {} {}",
                                    regular::TABLE,
                                    coll.name
                                ));
                                let resp = ui
                                    .selectable_label(false, label)
                                    .on_hover_cursor(CursorIcon::PointingHand);
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
