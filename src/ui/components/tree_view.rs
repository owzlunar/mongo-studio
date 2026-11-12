use std::fmt::Write as _;

use egui::{text::LayoutJob, CursorIcon, FontFamily, FontId, RichText, TextFormat, Ui};
use egui_phosphor::regular;

use crate::core::events::AppEvent;
use crate::models::{ConnectionConfig, ConnectionStatus};
use crate::theme::ThemePalette;

fn is_system_database(name: &str) -> bool {
    name.eq_ignore_ascii_case("admin")
        || name.eq_ignore_ascii_case("local")
        || name.eq_ignore_ascii_case("config")
}

pub fn render_connection_tree(
    ui: &mut Ui,
    connections: &mut [ConnectionConfig],
    search_filter: &str,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    let query = search_filter.trim().to_lowercase();

    for (conn_idx, conn) in connections.iter_mut().enumerate() {
        if !conn.status.is_connected() {
            conn.expanded = false;
        }

        let (status_icon, status_color) = match conn.status {
            ConnectionStatus::Connected => (regular::PLUGS_CONNECTED, palette.success_color),
            ConnectionStatus::Connecting => (regular::CIRCLE_HALF, palette.warning_color),
            ConnectionStatus::Disconnected => (regular::PLUGS, palette.muted_text),
            ConnectionStatus::Error => (regular::WARNING_CIRCLE, palette.error_color),
        };

        let arrow = if conn.status.is_connected() && conn.expanded {
            regular::CARET_DOWN
        } else {
            regular::CARET_RIGHT
        };

        let mut hover_details = if let Some(err) = &conn.last_error {
            format!("{} ({})\nError: {err}", conn.status.label(), conn.host)
        } else if conn.status.is_connected() {
            format!(
                "{} ({}) — Click to expand/collapse, right-click for actions",
                conn.status.label(),
                conn.host
            )
        } else {
            format!(
                "{} ({}) — Click to connect & expand, right-click for actions",
                conn.status.label(),
                conn.host
            )
        };
        if conn.ssh_tunnel.enabled {
            let _ = write!(
                hover_details,
                "\nSSH Tunnel: {}@{} ({})",
                conn.ssh_tunnel.username,
                conn.ssh_tunnel.endpoint,
                conn.ssh_tunnel.auth_method.label()
            );
        }

        let display_name = ConnectionConfig::clamp_profile_name(&conn.name);

        let mut profile_job = LayoutJob::default();
        profile_job.append(
            &format!("{arrow} "),
            0.0,
            TextFormat {
                font_id: FontId::new(12.5, FontFamily::Proportional),
                color: palette.muted_text,
                ..Default::default()
            },
        );
        profile_job.append(
            &format!("{status_icon} "),
            0.0,
            TextFormat {
                font_id: FontId::new(13.5, FontFamily::Proportional),
                color: status_color,
                ..Default::default()
            },
        );
        profile_job.append(
            &display_name,
            0.0,
            TextFormat {
                font_id: FontId::new(13.0, FontFamily::Proportional),
                color: palette.text_primary,
                ..Default::default()
            },
        );
        if conn.ssh_tunnel.enabled {
            profile_job.append(
                " [SSH]",
                0.0,
                TextFormat {
                    font_id: FontId::new(11.0, FontFamily::Monospace),
                    color: palette.accent_color,
                    ..Default::default()
                },
            );
        }

        ui.horizontal(|ui| {
            let toggle_resp = ui
                .selectable_label(false, profile_job)
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text(&hover_details);

            if toggle_resp.clicked() {
                if conn.status.is_connected() {
                    conn.expanded = !conn.expanded;
                } else {
                    conn.expanded = false;
                    if conn.status != ConnectionStatus::Connecting {
                        events.push(AppEvent::ConnectProfile { index: conn_idx });
                    }
                }
            }

            toggle_resp.context_menu(|ui| {
                if ui
                    .button(format!("{} Connect / Reconnect", regular::PLUGS_CONNECTED))
                    .clicked()
                {
                    events.push(AppEvent::ConnectProfile { index: conn_idx });
                    ui.close_menu();
                }
                if ui
                    .button(format!("{} Edit Connection...", regular::PENCIL_SIMPLE))
                    .clicked()
                {
                    events.push(AppEvent::OpenEditConnectionModal { index: conn_idx });
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
                if conn.status.is_connected() {
                    ui.separator();
                    if ui
                        .button(format!("{} Add Database...", regular::DATABASE))
                        .clicked()
                    {
                        events.push(AppEvent::OpenCreateDatabaseModal {
                            connection_index: conn_idx,
                        });
                        ui.close_menu();
                    }
                    if conn.is_admin
                        && ui
                            .button(format!("{} Create User...", regular::USER_PLUS))
                            .clicked()
                    {
                        events.push(AppEvent::OpenCreateUserModal {
                            connection_index: conn_idx,
                        });
                        ui.close_menu();
                    }
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
        });

        if !conn.expanded || !conn.status.is_connected() {
            continue;
        }

        ui.indent(conn.id, |ui| {
            // Admin Users Management Node
            if conn.is_admin {
                let users_arrow = if conn.users_expanded {
                    regular::CARET_DOWN
                } else {
                    regular::CARET_RIGHT
                };

                ui.horizontal(|ui| {
                    let users_resp = ui
                        .selectable_label(
                            false,
                            RichText::new(format!(
                                "{users_arrow} {} Users ({})",
                                regular::USERS,
                                conn.users.len()
                            ))
                            .color(palette.accent_color)
                            .strong(),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .on_hover_text("Right-click to create a new MongoDB user");

                    if users_resp.clicked() {
                        conn.users_expanded = !conn.users_expanded;
                    }

                    users_resp.context_menu(|ui| {
                        if ui
                            .button(format!("{} Create User...", regular::USER_PLUS))
                            .clicked()
                        {
                            events.push(AppEvent::OpenCreateUserModal {
                                connection_index: conn_idx,
                            });
                            ui.close_menu();
                        }
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(4.0);
                        if ui
                            .small_button(RichText::new(regular::PLUS).size(11.0))
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .on_hover_text("Create User")
                            .clicked()
                        {
                            events.push(AppEvent::OpenCreateUserModal {
                                connection_index: conn_idx,
                            });
                        }
                    });
                });

                if conn.users_expanded {
                    ui.indent(("users_tree", conn.id), |ui| {
                        for user in &conn.users {
                            if !query.is_empty()
                                && !user.username.to_lowercase().contains(&query)
                                && !"users".contains(&query)
                            {
                                continue;
                            }

                            let is_protected = user.is_protected_admin();
                            ui.horizontal(|ui| {
                                let label_text = if is_protected {
                                    RichText::new(format!(
                                        "  {} {} (Protected)",
                                        regular::SHIELD_CHECK,
                                        user.username
                                    ))
                                    .color(palette.warning_color)
                                    .strong()
                                } else {
                                    RichText::new(format!(
                                        "  {} {}",
                                        regular::USER,
                                        user.username
                                    ))
                                    .color(palette.text_primary)
                                };

                                let user_resp = ui
                                    .selectable_label(false, label_text)
                                    .on_hover_cursor(CursorIcon::PointingHand)
                                    .on_hover_text(if is_protected {
                                        "Protected root admin user (click to inspect roles)"
                                    } else {
                                        "Click to manage user roles & password, or right-click for actions"
                                    });

                                if user_resp.clicked() {
                                    events.push(AppEvent::OpenUserTab {
                                        connection_name: conn.name.clone(),
                                        username: user.username.clone(),
                                    });
                                }

                                user_resp.context_menu(|ui| {
                                    if ui
                                        .button(format!(
                                            "{} Inspect / Manage Permissions",
                                            regular::USER_GEAR
                                        ))
                                        .clicked()
                                    {
                                        events.push(AppEvent::OpenUserTab {
                                            connection_name: conn.name.clone(),
                                            username: user.username.clone(),
                                        });
                                        ui.close_menu();
                                    }
                                    if ui
                                        .button(format!(
                                            "{} Duplicate User...",
                                            regular::COPY
                                        ))
                                        .clicked()
                                    {
                                        events.push(AppEvent::OpenDuplicateUserModal {
                                            connection_index: conn_idx,
                                            source_username: user.username.clone(),
                                        });
                                        ui.close_menu();
                                    }
                                    ui.separator();
                                    if is_protected {
                                        ui.label(
                                            RichText::new(format!(
                                                "{} System 'admin' cannot be deleted",
                                                regular::LOCK_KEY
                                            ))
                                            .size(11.5)
                                            .color(palette.muted_text),
                                        );
                                    } else if ui
                                        .button(
                                            RichText::new(format!(
                                                "{} Delete User...",
                                                regular::TRASH
                                            ))
                                            .color(palette.error_color),
                                        )
                                        .clicked()
                                    {
                                        events.push(AppEvent::RequestDeleteUser {
                                            connection_index: conn_idx,
                                            username: user.username.clone(),
                                        });
                                        ui.close_menu();
                                    }
                                });

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.add_space(4.0);
                                        if is_protected {
                                            ui.label(
                                                RichText::new(regular::LOCK_KEY)
                                                    .size(11.0)
                                                    .color(palette.warning_color),
                                            );
                                        } else {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} roles",
                                                    user.roles.len()
                                                ))
                                                .size(10.5)
                                                .color(palette.muted_text),
                                            );
                                        }
                                    },
                                );
                            });
                        }
                    });
                }
            }

            // Databases & Collections Tree
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
                let system_db = is_system_database(&db.name);

                ui.horizontal(|ui| {
                    let db_resp = ui
                        .selectable_label(
                            false,
                            RichText::new(format!("{db_arrow} {} {}", regular::DATABASE, db.name))
                                .color(palette.text_primary)
                                .strong(),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .on_hover_text("Click to expand/collapse, right-click for database actions");
                    if db_resp.clicked() {
                        db.expanded = !db.expanded;
                    }

                    db_resp.context_menu(|ui| {
                        if ui
                            .button(format!("{} Create Collection...", regular::PLUS))
                            .clicked()
                        {
                            events.push(AppEvent::OpenCreateCollectionModal {
                                connection_index: conn_idx,
                                db_name: db.name.clone(),
                            });
                            ui.close_menu();
                        }
                        if !system_db {
                            ui.separator();
                            if ui
                                .button(
                                    RichText::new(format!(
                                        "{} Drop Database...",
                                        regular::TRASH
                                    ))
                                    .color(palette.error_color),
                                )
                                .clicked()
                            {
                                events.push(AppEvent::RequestDropDatabase {
                                    connection_index: conn_idx,
                                    db_name: db.name.clone(),
                                });
                                ui.close_menu();
                            }
                        }
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(4.0);
                        if ui
                            .small_button(RichText::new(regular::PLUS).size(10.5))
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .on_hover_text("Create Collection")
                            .clicked()
                        {
                            events.push(AppEvent::OpenCreateCollectionModal {
                                connection_index: conn_idx,
                                db_name: db.name.clone(),
                            });
                        }
                        ui.label(
                            RichText::new(format!("{}", db.collections.len()))
                                .size(10.5)
                                .color(palette.muted_text),
                        )
                        .on_hover_text("Collections in database");
                    });
                });

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
                                ))
                                .color(palette.text_primary);
                                let resp = ui
                                    .selectable_label(false, label)
                                    .on_hover_cursor(CursorIcon::PointingHand)
                                    .on_hover_text(if coll.loaded {
                                        format!(
                                            "{}.{} ({} documents)",
                                            db.name, coll.name, coll.stats.document_count
                                        )
                                    } else {
                                        format!(
                                            "{}.{} (click to lazy-load collection data)",
                                            db.name, coll.name
                                        )
                                    });

                                if resp.clicked() {
                                    events.push(AppEvent::OpenCollection {
                                        connection_name: conn.name.clone(),
                                        database: db.name.clone(),
                                        collection: coll.name.clone(),
                                    });
                                }

                                resp.context_menu(|ui| {
                                    if ui
                                        .button(format!("{} Open Collection", regular::TABLE))
                                        .clicked()
                                    {
                                        events.push(AppEvent::OpenCollection {
                                            connection_name: conn.name.clone(),
                                            database: db.name.clone(),
                                            collection: coll.name.clone(),
                                        });
                                        ui.close_menu();
                                    }
                                    if ui
                                        .button(format!(
                                            "{} Refresh Collection",
                                            regular::ARROWS_CLOCKWISE
                                        ))
                                        .clicked()
                                    {
                                        events.push(AppEvent::RefreshCollection {
                                            connection_name: conn.name.clone(),
                                            database: db.name.clone(),
                                            collection: coll.name.clone(),
                                        });
                                        ui.close_menu();
                                    }
                                    if !coll.name.starts_with("system.") {
                                        ui.separator();
                                        if ui
                                            .button(
                                                RichText::new(format!(
                                                    "{} Drop Collection...",
                                                    regular::TRASH
                                                ))
                                                .color(palette.error_color),
                                            )
                                            .clicked()
                                        {
                                            events.push(AppEvent::RequestDropCollection {
                                                connection_index: conn_idx,
                                                db_name: db.name.clone(),
                                                collection_name: coll.name.clone(),
                                            });
                                            ui.close_menu();
                                        }
                                    }
                                });

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.add_space(4.0);
                                        if coll.loaded {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{}",
                                                    coll.stats.document_count
                                                ))
                                                .size(11.0)
                                                .color(palette.muted_text),
                                            );
                                        } else {
                                            ui.label(
                                                RichText::new(regular::CLOUD_ARROW_DOWN)
                                                    .size(11.0)
                                                    .color(palette.muted_text),
                                            )
                                            .on_hover_text(
                                                "Not loaded yet — click to load documents & indexes",
                                            );
                                        }
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
