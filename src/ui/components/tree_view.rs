use std::fmt::Write as _;

use egui::{
    text::LayoutJob, CursorIcon, FontFamily, FontId, Frame, Margin, RichText, Rounding, Stroke,
    TextFormat, Ui,
};
use egui_phosphor::regular;

use crate::core::events::AppEvent;
use crate::core::state::AppState;
use crate::models::{ConnectionConfig, ConnectionStatus, WorkspaceTree};
use crate::theme::ThemePalette;
use crate::ui::components::buttons::primary_button;

fn is_system_database(name: &str) -> bool {
    name.eq_ignore_ascii_case("admin")
        || name.eq_ignore_ascii_case("local")
        || name.eq_ignore_ascii_case("config")
}

fn render_empty_state(ui: &mut Ui, palette: &ThemePalette, events: &mut Vec<AppEvent>) {
    ui.add_space(28.0);
    ui.vertical_centered(|ui| {
        Frame::none()
            .fill(palette.panel_bg)
            .inner_margin(Margin::same(12.0))
            .rounding(Rounding::same(12.0))
            .stroke(Stroke::new(1.0_f32, palette.border_color))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(regular::FOLDER_PLUS)
                        .size(24.0)
                        .color(palette.accent_color),
                );
            });
        ui.add_space(10.0);
        ui.label(
            RichText::new("No workspaces yet")
                .size(13.5)
                .strong()
                .color(palette.text_primary),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new("Organize your connections into customers and projects.")
                .size(11.0)
                .color(palette.muted_text),
        );
        ui.add_space(12.0);
        if primary_button(ui, "Manage Workspaces", palette.accent_color)
            .on_hover_cursor(CursorIcon::PointingHand)
            .clicked()
        {
            events.push(AppEvent::OpenManageWorkspacesModal);
        }
    });
}

fn render_admin_users(
    ui: &mut Ui,
    conn: &mut ConnectionConfig,
    conn_idx: usize,
    query: &str,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
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
                    && !user.username.to_lowercase().contains(query)
                    && !"users".contains(query)
                {
                    continue;
                }

                let is_protected = user.is_protected_admin();
                ui.horizontal(|ui| {
                    let label_text = if is_protected {
                        RichText::new(format!(
                            "  {} {} ({}) (Protected)",
                            regular::SHIELD_CHECK,
                            user.username,
                            user.db
                        ))
                        .color(palette.warning_color)
                        .strong()
                    } else {
                        RichText::new(format!(
                            "  {} {} ({})",
                            regular::USER,
                            user.username,
                            user.db
                        ))
                        .color(palette.text_primary)
                    };

                    let hover_text = format!(
                        "User: {}\nAuth DB: {}\nRoles: {}\n{}",
                        user.username,
                        user.db,
                        user.roles.len(),
                        if is_protected {
                            "Protected root admin user (click to inspect roles)"
                        } else {
                            "Click to manage user roles & password, or right-click for actions"
                        }
                    );

                    let user_resp = ui
                        .selectable_label(false, label_text)
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .on_hover_text(hover_text);

                    if user_resp.clicked() {
                        events.push(AppEvent::OpenUserTab {
                            connection_name: conn.name.clone(),
                            username: user.username.clone(),
                            user_db: user.db.clone(),
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
                                user_db: user.db.clone(),
                            });
                            ui.close_menu();
                        }
                        if ui
                            .button(format!("{} Duplicate User...", regular::COPY))
                            .clicked()
                        {
                            events.push(AppEvent::OpenDuplicateUserModal {
                                connection_index: conn_idx,
                                source_username: user.username.clone(),
                                source_user_db: user.db.clone(),
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
                                RichText::new(format!("{} Delete User...", regular::TRASH))
                                    .color(palette.error_color),
                            )
                            .clicked()
                        {
                            events.push(AppEvent::RequestDeleteUser {
                                connection_index: conn_idx,
                                username: user.username.clone(),
                                user_db: user.db.clone(),
                            });
                            ui.close_menu();
                        }
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(4.0);
                        if is_protected {
                            ui.label(
                                RichText::new(regular::LOCK_KEY)
                                    .size(11.0)
                                    .color(palette.warning_color),
                            );
                        } else {
                            ui.label(
                                RichText::new(format!("{} roles", user.roles.len()))
                                    .size(10.5)
                                    .color(palette.muted_text),
                            );
                        }
                    });
                });
            }
        });
    }
}

fn render_databases_and_collections(
    ui: &mut Ui,
    conn: &mut ConnectionConfig,
    conn_idx: usize,
    query: &str,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    for db in &mut conn.databases {
        let db_matches = query.is_empty()
            || db.name.to_lowercase().contains(query)
            || db
                .collections
                .iter()
                .any(|c| c.name.to_lowercase().contains(query));

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
                            RichText::new(format!("{} Drop Database...", regular::TRASH))
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
                        && !coll.name.to_lowercase().contains(query)
                        && !db.name.to_lowercase().contains(query)
                    {
                        continue;
                    }

                    ui.horizontal(|ui| {
                        let label = RichText::new(format!("  {} {}", regular::TABLE, coll.name))
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
                                .button(format!("{} Refresh Collection", regular::ARROWS_CLOCKWISE))
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
                                            "{} Truncate Data...",
                                            regular::BROOM
                                        ))
                                        .color(palette.warning_color),
                                    )
                                    .clicked()
                                {
                                    events.push(AppEvent::RequestTruncateCollection {
                                        connection_index: conn_idx,
                                        db_name: db.name.clone(),
                                        collection_name: coll.name.clone(),
                                    });
                                    ui.close_menu();
                                }
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

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(4.0);
                            if coll.loaded {
                                ui.label(
                                    RichText::new(format!("{}", coll.stats.document_count))
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
                        });
                    });
                }
            });
        }
    }
}

fn render_connection_row(
    ui: &mut Ui,
    conn: &mut ConnectionConfig,
    conn_idx: usize,
    is_active_tab: bool,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
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

    let maybe_env = conn.effective_environment();
    let is_dark = palette.is_dark();

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
    if let Some(env) = &maybe_env {
        let _ = write!(hover_details, "\nEnvironment: {}", env.label());
    }
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
            font_id: FontId::new(11.5, FontFamily::Proportional),
            color: palette.muted_text,
            ..Default::default()
        },
    );
    profile_job.append(
        &format!("{status_icon} "),
        0.0,
        TextFormat {
            font_id: FontId::new(12.5, FontFamily::Proportional),
            color: status_color,
            ..Default::default()
        },
    );
    profile_job.append(
        &display_name,
        0.0,
        TextFormat {
            font_id: FontId::new(12.5, FontFamily::Proportional),
            color: palette.text_primary,
            ..Default::default()
        },
    );
    if let Some(env) = &maybe_env {
        profile_job.append(
            &format!(" [{}]", env.label()),
            0.0,
            TextFormat {
                font_id: FontId::new(10.5, FontFamily::Monospace),
                color: env.color(is_dark),
                ..Default::default()
            },
        );
    }
    if conn.ssh_tunnel.enabled {
        profile_job.append(
            " [SSH]",
            0.0,
            TextFormat {
                font_id: FontId::new(10.5, FontFamily::Monospace),
                color: palette.accent_color,
                ..Default::default()
            },
        );
    }
    if conn.is_favorite {
        profile_job.append(
            " ⭐",
            0.0,
            TextFormat {
                font_id: FontId::new(10.5, FontFamily::Proportional),
                color: palette.warning_color,
                ..Default::default()
            },
        );
    }

    let bg_fill = if is_active_tab {
        if is_dark {
            egui::Color32::from_rgba_unmultiplied(30, 58, 102, 140)
        } else {
            egui::Color32::from_rgba_unmultiplied(219, 234, 254, 180)
        }
    } else {
        egui::Color32::TRANSPARENT
    };
    let border_stroke = if is_active_tab {
        if is_dark {
            Stroke::new(
                1.0_f32,
                egui::Color32::from_rgba_unmultiplied(59, 130, 246, 120),
            )
        } else {
            Stroke::new(
                1.0_f32,
                egui::Color32::from_rgba_unmultiplied(147, 197, 253, 180),
            )
        }
    } else {
        Stroke::NONE
    };

    Frame::none()
        .fill(bg_fill)
        .stroke(border_stroke)
        .rounding(Rounding::same(4.0))
        .inner_margin(Margin::symmetric(4.0, 2.0))
        .show(ui, |ui| {
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
                    let star_label = if conn.is_favorite {
                        format!("{} Unstar Favorite", regular::STAR)
                    } else {
                        format!("{} Star as Favorite", regular::STAR)
                    };
                    if ui.button(star_label).clicked() {
                        events.push(AppEvent::ToggleFavoriteConnection {
                            connection_id: conn.id,
                        });
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
        });
}

fn render_connection_node(
    ui: &mut Ui,
    state: &mut AppState,
    conn_id: uuid::Uuid,
    query: &str,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    let Some(conn_idx) = state.connections.iter().position(|c| c.id == conn_id) else {
        return;
    };

    let conn_name = state.connections[conn_idx].name.clone();
    let is_active_tab = state
        .dock_state
        .iter_all_tabs()
        .any(|(_, t)| t.connection_name == conn_name);

    let conn = &mut state.connections[conn_idx];
    render_connection_row(ui, conn, conn_idx, is_active_tab, palette, events);

    if conn.expanded && conn.status.is_connected() {
        ui.indent(conn.id, |ui| {
            if conn.is_admin {
                render_admin_users(ui, conn, conn_idx, query, palette, events);
            }
            render_databases_and_collections(ui, conn, conn_idx, query, palette, events);
        });
    }
}

pub fn render_connection_tree(
    ui: &mut Ui,
    state: &mut AppState,
    palette: &ThemePalette,
    events: &mut Vec<AppEvent>,
) {
    let tree = WorkspaceTree::build_with_custom(
        &state.connections,
        &state.sidebar_search,
        state.active_workspace_filter.as_deref(),
        state.quick_access_filter,
        &state.recent_connections,
        &state.custom_workspaces,
        &state.custom_folders,
    );

    if tree.is_empty() {
        render_empty_state(ui, palette, events);
        return;
    }

    let query = state.sidebar_search.trim().to_lowercase();

    // 1. Workspaces & Projects Hierarchy
    for ws in &tree.workspaces {
        let ws_id = ui.make_persistent_id(("workspace_expanded", &ws.name));
        let ws_expanded = !state.collapsed_workspaces.contains(&ws.name);

        let ws_arrow = if ws_expanded {
            regular::CARET_DOWN
        } else {
            regular::CARET_RIGHT
        };

        ui.horizontal(|ui| {
            let header_label =
                RichText::new(format!("{ws_arrow} {} {}", regular::BUILDINGS, ws.name))
                    .size(12.5)
                    .strong()
                    .color(palette.text_primary);
            let resp = ui
                .selectable_label(false, header_label)
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text(format!(
                    "Workspace: {}\nClick to expand/collapse, right-click for actions",
                    ws.name
                ));
            if resp.clicked() {
                if ws_expanded {
                    state.collapsed_workspaces.insert(ws.name.clone());
                } else {
                    state.collapsed_workspaces.remove(&ws.name);
                }
            }

            resp.context_menu(|ui| {
                if ui
                    .button(format!("{} Add Connection...", regular::PLUS))
                    .clicked()
                {
                    events.push(AppEvent::OpenNewConnectionModalForTarget {
                        workspace: Some(ws.name.clone()),
                        folder: None,
                    });
                    ui.close_menu();
                }
                if ui
                    .button(format!("{} New Folder...", regular::FOLDER_PLUS))
                    .clicked()
                {
                    state.quick_new_folder_dialog = Some(ws.name.clone());
                    state.quick_new_folder_input.clear();
                    ui.close_menu();
                }
                ui.separator();
                if ui
                    .button(format!("{} Rename Workspace...", regular::PENCIL_SIMPLE))
                    .clicked()
                {
                    state.rename_workspace_dialog = Some(ws.name.clone());
                    state.rename_workspace_input = ws.name.clone();
                    ui.close_menu();
                }
                if ui
                    .button(
                        RichText::new(format!("{} Delete Workspace", regular::TRASH))
                            .color(palette.error_color),
                    )
                    .clicked()
                {
                    state.delete_workspace_dialog = Some(ws.name.clone());
                    ui.close_menu();
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(2.0);
                let add_btn = ui
                    .small_button(RichText::new(regular::PLUS).size(10.5))
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text("Add Connection to this Workspace");
                if add_btn.clicked() {
                    events.push(AppEvent::OpenNewConnectionModalForTarget {
                        workspace: Some(ws.name.clone()),
                        folder: None,
                    });
                }
                ui.label(
                    RichText::new(format!("({})", ws.total_connections()))
                        .size(10.5)
                        .color(palette.muted_text),
                );
            });
        });

        if ws_expanded {
            ui.indent(ws_id, |ui| {
                // Folders inside workspace
                for folder in &ws.folders {
                    let folder_id =
                        ui.make_persistent_id(("folder_expanded", &ws.name, &folder.name));
                    let folder_key = (ws.name.clone(), folder.name.clone());
                    let folder_expanded = !state.collapsed_folders.contains(&folder_key);
                    let folder_arrow = if folder_expanded {
                        regular::CARET_DOWN
                    } else {
                        regular::CARET_RIGHT
                    };

                    ui.horizontal(|ui| {
                        let header_label = RichText::new(format!(
                            "{folder_arrow} {} {}",
                            regular::FOLDER,
                            folder.name
                        ))
                        .size(12.0)
                        .strong()
                        .color(palette.text_primary);
                        let resp = ui
                            .selectable_label(false, header_label)
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .on_hover_text(format!(
                                "Folder: {}\nClick to expand/collapse, right-click for actions",
                                folder.name
                            ));
                        if resp.clicked() {
                            if folder_expanded {
                                state.collapsed_folders.insert(folder_key.clone());
                            } else {
                                state.collapsed_folders.remove(&folder_key);
                            }
                        }

                        resp.context_menu(|ui| {
                            if ui
                                .button(format!("{} Add Connection...", regular::PLUS))
                                .clicked()
                            {
                                events.push(AppEvent::OpenNewConnectionModalForTarget {
                                    workspace: Some(ws.name.clone()),
                                    folder: Some(folder.name.clone()),
                                });
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui
                                .button(format!("{} Rename Folder...", regular::PENCIL_SIMPLE))
                                .clicked()
                            {
                                state.rename_folder_dialog =
                                    Some((ws.name.clone(), folder.name.clone()));
                                state.rename_folder_input = folder.name.clone();
                                ui.close_menu();
                            }
                            if ui
                                .button(
                                    RichText::new(format!("{} Delete Folder", regular::TRASH))
                                        .color(palette.error_color),
                                )
                                .clicked()
                            {
                                state.delete_folder_dialog =
                                    Some((ws.name.clone(), folder.name.clone()));
                                ui.close_menu();
                            }
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(2.0);
                            let add_btn = ui
                                .small_button(RichText::new(regular::PLUS).size(10.5))
                                .on_hover_cursor(CursorIcon::PointingHand)
                                .on_hover_text("Add Connection to this Folder");
                            if add_btn.clicked() {
                                events.push(AppEvent::OpenNewConnectionModalForTarget {
                                    workspace: Some(ws.name.clone()),
                                    folder: Some(folder.name.clone()),
                                });
                            }
                            ui.label(
                                RichText::new(format!("({})", folder.connection_ids.len()))
                                    .size(10.5)
                                    .color(palette.muted_text),
                            );
                        });
                    });

                    if folder_expanded {
                        ui.indent(folder_id, |ui| {
                            for &conn_id in &folder.connection_ids {
                                render_connection_node(ui, state, conn_id, &query, palette, events);
                            }
                        });
                    }
                }

                // Standalone connections in workspace (no folder)
                for &conn_id in &ws.standalone_connections {
                    render_connection_node(ui, state, conn_id, &query, palette, events);
                }
            });
        }
        ui.add_space(2.0);
    }

    // 2. Unassigned Connections Group
    if !tree.unassigned_connections.is_empty() {
        let unassigned_id = ui.make_persistent_id("unassigned_connections_group");
        let unassigned_expanded = !state.collapsed_unassigned;
        let arrow = if unassigned_expanded {
            regular::CARET_DOWN
        } else {
            regular::CARET_RIGHT
        };

        ui.horizontal(|ui| {
            let header_label = RichText::new(format!(
                "{arrow} {} Other Connections",
                regular::FOLDER_DASHED
            ))
            .size(12.0)
            .color(palette.muted_text);
            let resp = ui
                .selectable_label(false, header_label)
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text("Connections not yet assigned to a workspace");
            if resp.clicked() {
                state.collapsed_unassigned = !state.collapsed_unassigned;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(2.0);
                ui.label(
                    RichText::new(format!("({})", tree.unassigned_connections.len()))
                        .size(10.5)
                        .color(palette.muted_text),
                );
            });
        });

        if unassigned_expanded {
            ui.indent(unassigned_id, |ui| {
                for &conn_id in &tree.unassigned_connections {
                    render_connection_node(ui, state, conn_id, &query, palette, events);
                }
            });
        }
    }
}
