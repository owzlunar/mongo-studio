use std::borrow::Cow;

use egui::{
    Color32, ComboBox, CursorIcon, FontId, Frame, Margin, Pos2, Rect, RichText, Rounding,
    ScrollArea, Sense, Stroke, TextEdit, Ui, Vec2,
};
use egui_dock::TabViewer;
use egui_extras::{Column, TableBuilder};
use egui_phosphor::regular;

use crate::core::events::AppEvent;
use crate::database::{DocumentService, TransferFormat};
use crate::models::{
    CollectionInfo, ConnectionConfig, DocumentRow, MongoUser, SchemaField, UserRole,
};
use crate::query::AggregationStage;
use crate::theme::ThemePalette;
use crate::ui::components::buttons::{primary_button, secondary_button};
use crate::ui::components::data_grid::{render_document_grid, DocumentTarget};
use crate::ui::components::json_viewer::render_json_viewer;
use crate::ui::components::syntax_highlight::json_layouter;
use crate::workspace::tabs::{CollectionSubTab, CollectionTab, DocumentViewMode};

pub struct CollectionTabViewer<'a> {
    pub connections: &'a [ConnectionConfig],
    pub palette: &'a ThemePalette,
    pub events: &'a mut Vec<AppEvent>,
}

fn query_field_input(
    ui: &mut Ui,
    label: &str,
    value: &mut String,
    hint: &str,
    width: f32,
    is_valid: bool,
    error_color: Color32,
) -> bool {
    ui.label(
        RichText::new(label)
            .strong()
            .size(12.0)
            .color(ui.visuals().text_color()),
    );
    let mut edit = TextEdit::singleline(value)
        .font(FontId::monospace(12.0))
        .hint_text(hint)
        .margin(Margin::symmetric(6.0, 4.0))
        .min_size(Vec2::new(width, 24.0))
        .desired_width(width);

    if !is_valid {
        edit = edit.text_color(error_color);
    }

    let response = ui.add(edit);
    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
}

fn bson_type_color(ty: &str, palette: &ThemePalette) -> Color32 {
    let is_dark = palette.editor_bg.r() < 128;
    if is_dark {
        match ty {
            "String" => Color32::from_rgb(86, 156, 214),
            "Int32" | "Int64" | "Double" | "Decimal128" => Color32::from_rgb(78, 201, 176),
            "ObjectId" => Color32::from_rgb(206, 145, 120),
            "Boolean" => Color32::from_rgb(197, 134, 192),
            "DateTime" => Color32::from_rgb(220, 220, 170),
            "Document" => Color32::from_rgb(156, 220, 254),
            "Array" => Color32::from_rgb(181, 206, 168),
            "Null" => palette.muted_text,
            _ => palette.accent_color,
        }
    } else {
        match ty {
            "String" => Color32::from_rgb(4, 81, 165),
            "Int32" | "Int64" | "Double" | "Decimal128" => Color32::from_rgb(9, 134, 88),
            "ObjectId" => Color32::from_rgb(163, 21, 21),
            "Boolean" => Color32::from_rgb(129, 31, 144),
            "DateTime" => Color32::from_rgb(160, 90, 0),
            "Document" => Color32::from_rgb(0, 102, 180),
            "Array" => Color32::from_rgb(30, 120, 60),
            "Null" => palette.muted_text,
            _ => palette.accent_color,
        }
    }
}

fn render_type_distribution_bar(ui: &mut Ui, field: &SchemaField, palette: &ThemePalette) {
    let desired_width = ui.available_width().clamp(80.0, 220.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(desired_width, 14.0), Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, Rounding::same(3.0), palette.panel_bg);

        if field.type_breakdown.is_empty() {
            if field.frequency_pct > 0.0 {
                let color = bson_type_color(&field.bson_type, palette);
                painter.rect_filled(rect, Rounding::same(3.0), color);
            }
        } else {
            let total_pct: f32 = field
                .type_breakdown
                .iter()
                .map(|(_, pct)| (*pct).max(0.0))
                .sum();
            let norm = if total_pct > 0.0 { total_pct } else { 100.0 };
            let mut cursor_x = rect.left();
            let count = field.type_breakdown.len();

            for (idx, (ty, pct)) in field.type_breakdown.iter().enumerate() {
                let frac = ((*pct).max(0.0) / norm).clamp(0.0, 1.0);
                let next_x = if idx + 1 == count {
                    rect.right()
                } else {
                    (cursor_x + rect.width() * frac).min(rect.right())
                };

                if next_x > cursor_x {
                    let seg_rect = Rect::from_min_max(
                        Pos2::new(cursor_x, rect.top()),
                        Pos2::new(next_x, rect.bottom()),
                    );
                    let rounding = if count == 1 {
                        Rounding::same(3.0)
                    } else if idx == 0 {
                        Rounding {
                            nw: 3.0,
                            sw: 3.0,
                            ne: 0.0,
                            se: 0.0,
                        }
                    } else if idx + 1 == count {
                        Rounding {
                            nw: 0.0,
                            sw: 0.0,
                            ne: 3.0,
                            se: 3.0,
                        }
                    } else {
                        Rounding::ZERO
                    };
                    painter.rect_filled(seg_rect, rounding, bson_type_color(ty, palette));
                }
                cursor_x = next_x;
            }
        }

        painter.rect_stroke(
            rect,
            Rounding::same(3.0),
            Stroke::new(1.0_f32, palette.border_color),
        );
    }

    resp.on_hover_ui(|ui| {
        ui.label(
            RichText::new(&field.field_name)
                .monospace()
                .strong()
                .color(palette.text_primary),
        );
        ui.label(
            RichText::new(format!(
                "Present in {} doc(s) ({:.1}%)",
                field.doc_count, field.frequency_pct
            ))
            .size(11.5)
            .color(palette.muted_text),
        );
        ui.separator();
        if field.type_breakdown.is_empty() {
            ui.label(
                RichText::new(&field.bson_type)
                    .monospace()
                    .size(11.5)
                    .color(palette.text_primary),
            );
        } else {
            for (ty, pct) in &field.type_breakdown {
                ui.horizontal(|ui| {
                    let (swatch, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
                    ui.painter().rect_filled(
                        swatch,
                        Rounding::same(2.0),
                        bson_type_color(ty, palette),
                    );
                    ui.label(
                        RichText::new(format!("{ty}: {pct:.1}%"))
                            .monospace()
                            .size(11.5)
                            .color(palette.text_primary),
                    );
                });
            }
        }
    });
}

impl<'a> CollectionTabViewer<'a> {
    #[must_use]
    fn is_connection_connected(&self, connection_name: &str) -> bool {
        self.connections
            .iter()
            .find(|conn| conn.name == connection_name)
            .is_some_and(|conn| conn.status.is_connected())
    }

    #[must_use]
    fn find_collection(
        &self,
        connection_name: &str,
        database: &str,
        collection: &str,
    ) -> Option<&'a CollectionInfo> {
        self.connections
            .iter()
            .filter(|conn| conn.name == connection_name)
            .flat_map(|conn| &conn.databases)
            .filter(|db| db.name == database)
            .flat_map(|db| &db.collections)
            .find(|c| c.name == collection)
            .or_else(|| {
                self.connections
                    .iter()
                    .flat_map(|conn| &conn.databases)
                    .filter(|db| db.name == database)
                    .flat_map(|db| &db.collections)
                    .find(|c| c.name == collection)
            })
    }

    #[must_use]
    fn find_user_and_conn(
        &self,
        connection_name: &str,
        username: &str,
    ) -> Option<(usize, &'a ConnectionConfig, &'a MongoUser)> {
        for (idx, conn) in self.connections.iter().enumerate() {
            if conn.name == connection_name {
                if let Some(u) = conn.users.iter().find(|u| u.username == username) {
                    return Some((idx, conn, u));
                }
            }
        }
        for (idx, conn) in self.connections.iter().enumerate() {
            if let Some(u) = conn.users.iter().find(|u| u.username == username) {
                return Some((idx, conn, u));
            }
        }
        None
    }

    fn emit_live_query(&mut self, tab: &CollectionTab) {
        self.events.push(AppEvent::ExecuteLiveQuery {
            connection_name: tab.connection_name.clone(),
            database: tab.database.clone(),
            collection: tab.collection.clone(),
            filter: tab.filter_query.clone(),
            projection: tab.projection_query.clone(),
            sort: tab.sort_query.clone(),
            skip: tab.skip,
            limit: tab.limit,
        });
    }

    fn render_user_inspector(&mut self, ui: &mut Ui, tab: &mut CollectionTab) {
        let Some((conn_idx, conn, user)) = tab
            .selected_user
            .as_deref()
            .and_then(|username| self.find_user_and_conn(&tab.connection_name, username))
        else {
            let username = tab.selected_user.as_deref().unwrap_or("");
            ui.centered_and_justified(|ui| {
                ui.label(format!(
                    "User '{}' not found on connection '{}'.",
                    username, tab.connection_name
                ));
            });
            return;
        };

        let is_protected = user.is_protected_admin();
        let conn_name = conn.name.as_str();
        let username = user.username.as_str();
        let server_roles = &user.roles;

        if tab.draft_roles.is_none() {
            tab.draft_roles = Some(server_roles.clone());
        }
        if tab.role_db_input.is_empty() {
            tab.role_db_input.push_str("admin");
        }
        if tab.role_name_input.is_empty() {
            tab.role_name_input.push_str("readWrite");
        }

        if tab.cached_db_conn != conn_name
            || tab.cached_db_count != conn.databases.len()
            || tab.cached_db_options.is_empty()
        {
            tab.cached_db_options.clear();
            tab.cached_db_options
                .extend(conn.databases.iter().map(|d| d.name.clone()));
            if !tab.cached_db_options.iter().any(|d| d == "admin") {
                tab.cached_db_options.insert(0, "admin".to_string());
            }
            tab.cached_db_conn.clone_from(&conn.name);
            tab.cached_db_count = conn.databases.len();
        }

        // Top Bar: Breadcrumb + Status + [Change Password] & [Delete User]
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(regular::PLUGS_CONNECTED)
                    .strong()
                    .color(self.palette.text_primary),
            );
            ui.label(
                RichText::new(conn_name)
                    .monospace()
                    .strong()
                    .color(self.palette.text_primary),
            );
            ui.label(RichText::new(regular::CARET_RIGHT).color(self.palette.muted_text));
            ui.label(
                RichText::new(regular::USERS)
                    .strong()
                    .color(self.palette.text_primary),
            );
            ui.label(
                RichText::new("Users")
                    .monospace()
                    .strong()
                    .color(self.palette.text_primary),
            );
            ui.label(RichText::new(regular::CARET_RIGHT).color(self.palette.muted_text));
            ui.label(
                RichText::new(if is_protected {
                    regular::SHIELD_CHECK
                } else {
                    regular::USER
                })
                .strong()
                .color(self.palette.text_primary),
            );
            ui.label(
                RichText::new(username)
                    .monospace()
                    .strong()
                    .color(self.palette.text_primary),
            );
            ui.separator();

            if is_protected {
                ui.label(
                    RichText::new(regular::LOCK_KEY)
                        .color(self.palette.warning_color)
                        .size(12.0),
                );
                ui.label(
                    RichText::new("Protected System Admin (Read-Only)")
                        .color(self.palette.warning_color)
                        .strong()
                        .size(12.0),
                );
            } else {
                ui.label(
                    RichText::new(regular::CHECK_CIRCLE)
                        .color(self.palette.success_color)
                        .size(12.0),
                );
                ui.label(
                    RichText::new("Editable User Account")
                        .color(self.palette.success_color)
                        .size(12.0),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !is_protected
                    && secondary_button(
                        ui,
                        &format!("{} Delete User", regular::TRASH),
                        self.palette.error_color,
                    )
                    .clicked()
                {
                    self.events.push(AppEvent::RequestDeleteUser {
                        connection_index: conn_idx,
                        username: username.to_string(),
                    });
                }

                if !is_protected
                    && secondary_button(
                        ui,
                        &format!("{} Change Password...", regular::KEY),
                        self.palette.accent_color,
                    )
                    .clicked()
                {
                    self.events.push(AppEvent::OpenChangePasswordModal {
                        connection_name: conn_name.to_string(),
                        username: username.to_string(),
                    });
                }

                if secondary_button(
                    ui,
                    &format!("{} Duplicate User...", regular::COPY),
                    self.palette.border_color,
                )
                .on_hover_text("Create a new user copying all database roles from this user")
                .clicked()
                {
                    self.events.push(AppEvent::OpenDuplicateUserModal {
                        connection_index: conn_idx,
                        source_username: username.to_string(),
                    });
                }
            });
        });

        ui.separator();
        ui.add_space(4.0);

        if is_protected {
            Frame::none()
                .fill(Color32::from_rgba_unmultiplied(220, 160, 40, 28))
                .stroke(Stroke::new(1.0_f32, self.palette.warning_color))
                .rounding(Rounding::same(4.0))
                .inner_margin(Margin::same(10.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(regular::SHIELD_CHECK)
                                .size(16.0)
                                .color(self.palette.warning_color),
                        );
                        ui.label(
                            RichText::new(
                                "System 'admin' account is protected. Roles and credentials are read-only and this account cannot be deleted.",
                            )
                            .color(self.palette.warning_color)
                            .strong(),
                        );
                    });
                });
            ui.add_space(8.0);
        }

        // Bottom Bar (only for editable users): Cancel & Apply buttons pinned to bottom-right
        if !is_protected {
            let footer_frame = Frame::none()
                .fill(self.palette.panel_bg)
                .inner_margin(Margin::symmetric(12.0, 8.0))
                .stroke(Stroke::new(1.0_f32, self.palette.border_color));

            egui::TopBottomPanel::bottom(egui::Id::new(("user_inspector_footer", username)))
                .resizable(false)
                .exact_height(44.0)
                .frame(footer_frame)
                .show_inside(ui, |ui| {
                    let has_changes = tab
                        .draft_roles
                        .as_ref()
                        .is_some_and(|draft| draft != server_roles);

                    ui.horizontal(|ui| {
                        if has_changes {
                            ui.label(
                                RichText::new(regular::WARNING_CIRCLE)
                                    .color(self.palette.warning_color)
                                    .size(12.0),
                            );
                            ui.label(
                                RichText::new(
                                    "Unsaved role changes — click Apply to save to MongoDB",
                                )
                                .color(self.palette.warning_color)
                                .strong()
                                .size(12.0),
                            );
                        } else {
                            ui.label(
                                RichText::new(format!(
                                    "{} Database roles are in sync with server ({} assigned)",
                                    regular::CHECK_CIRCLE,
                                    server_roles.len()
                                ))
                                .color(self.palette.muted_text)
                                .size(12.0),
                            );
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_enabled_ui(has_changes, |ui| {
                                if primary_button(
                                    ui,
                                    &format!("{} Apply", regular::CHECK),
                                    self.palette.accent_color,
                                )
                                .clicked()
                                {
                                    if let Some(draft) = &tab.draft_roles {
                                        self.events.push(AppEvent::SaveUserRoles {
                                            connection_name: conn_name.to_string(),
                                            username: username.to_string(),
                                            roles: draft.clone(),
                                        });
                                    }
                                }

                                if secondary_button(
                                    ui,
                                    &format!("{} Cancel", regular::ARROW_COUNTER_CLOCKWISE),
                                    self.palette.border_color,
                                )
                                .clicked()
                                {
                                    tab.draft_roles = Some(server_roles.clone());
                                }
                            });
                        });
                    });
                });
        }

        // Main Full-Area Dynamic Table (Row 0 = Add New Role, Rows 1..N = Existing/Draft Roles)
        let central_frame = Frame::none()
            .fill(self.palette.editor_bg)
            .inner_margin(Margin::symmetric(4.0, 2.0));

        egui::CentralPanel::default()
            .frame(central_frame)
            .show_inside(ui, |ui| {
                let draft_len = tab.draft_roles.as_ref().map_or(0, Vec::len);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} Database Roles & Permissions ({draft_len})",
                            regular::KEY
                        ))
                        .strong()
                        .size(13.5)
                        .color(self.palette.text_primary),
                    );
                    if !is_protected {
                        ui.label(
                            RichText::new(
                                "— Use the first row to add a database role, or edit existing roles inline.",
                            )
                            .size(11.5)
                            .color(self.palette.muted_text),
                        );
                    }
                });
                ui.add_space(6.0);

                let avail_height = ui.available_height().max(140.0);

                let mut add_clicked = false;
                let mut remove_idx: Option<usize> = None;

                TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .vscroll(true)
                    .min_scrolled_height(120.0)
                    .max_scroll_height(avail_height)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .column(Column::initial(220.0).at_least(150.0))
                    .column(Column::initial(195.0).at_least(140.0))
                    .column(Column::remainder().at_least(200.0))
                    .column(Column::initial(110.0).at_least(90.0))
                    .header(26.0, |mut header| {
                        for col_name in [
                            "Database Name",
                            "Role Name",
                            "Privileges Description",
                            "Action",
                        ] {
                            header.col(|ui| {
                                ui.label(
                                    RichText::new(col_name)
                                        .strong()
                                        .color(self.palette.accent_color),
                                );
                            });
                        }
                    })
                    .body(|mut body| {
                        // Row 0: Dynamic Add New Database Role Row (for editable users)
                        if !is_protected {
                            body.row(32.0, |mut row| {
                                row.col(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(regular::DATABASE)
                                                .color(self.palette.muted_text),
                                        );
                                        ComboBox::from_id_source(("add_row_db_combo", username))
                                            .selected_text(
                                                RichText::new(&tab.role_db_input).monospace(),
                                            )
                                            .width(180.0)
                                            .height(280.0)
                                            .show_ui(ui, |ui| {
                                                ui.add(
                                                    TextEdit::singleline(&mut tab.role_db_filter)
                                                        .hint_text("Search database...")
                                                        .margin(Margin::symmetric(6.0, 4.0))
                                                        .desired_width(175.0),
                                                );
                                                ui.separator();
                                                let trimmed_filter = tab.role_db_filter.trim();
                                                let filter_q = if trimmed_filter.is_empty() {
                                                    None
                                                } else {
                                                    Some(trimmed_filter.to_lowercase())
                                                };
                                                ScrollArea::vertical().max_height(210.0).show(
                                                    ui,
                                                    |ui| {
                                                        for db_opt in &tab.cached_db_options {
                                                            if let Some(ref q) = filter_q {
                                                                if !db_opt
                                                                    .to_lowercase()
                                                                    .contains(q)
                                                                {
                                                                    continue;
                                                                }
                                                            }
                                                            if ui
                                                                .selectable_label(
                                                                    tab.role_db_input == *db_opt,
                                                                    db_opt,
                                                                )
                                                                .clicked()
                                                            {
                                                                tab.role_db_input
                                                                    .clone_from(db_opt);
                                                            }
                                                        }
                                                    },
                                                );
                                            });
                                    });
                                });

                                row.col(|ui| {
                                    ComboBox::from_id_source(("add_row_role_combo", username))
                                        .selected_text(
                                            RichText::new(&tab.role_name_input)
                                                .monospace()
                                                .color(self.palette.success_color),
                                        )
                                        .width(175.0)
                                        .show_ui(ui, |ui| {
                                            for &role_opt in UserRole::COMMON_ROLES {
                                                if ui
                                                    .selectable_label(
                                                        tab.role_name_input == role_opt,
                                                        role_opt,
                                                    )
                                                    .clicked()
                                                {
                                                    tab.role_name_input.clear();
                                                    tab.role_name_input.push_str(role_opt);
                                                }
                                            }
                                        });
                                });

                                row.col(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new("New:")
                                                .size(11.5)
                                                .strong()
                                                .color(self.palette.accent_color),
                                        );
                                        ui.label(
                                            RichText::new(UserRole::role_description(
                                                &tab.role_name_input,
                                            ))
                                            .size(11.5)
                                            .color(self.palette.muted_text),
                                        );
                                    });
                                });

                                row.col(|ui| {
                                    if primary_button(
                                        ui,
                                        &format!("{} Add", regular::PLUS),
                                        self.palette.accent_color,
                                    )
                                    .on_hover_text("Add this database role to the table")
                                    .clicked()
                                    {
                                        add_clicked = true;
                                    }
                                });
                            });
                        }

                        // Rows 1..N: Assigned / Staged Database Roles
                        if let Some(draft_roles) = tab.draft_roles.as_mut() {
                            for (idx, role_item) in draft_roles.iter_mut().enumerate() {
                                let is_modified = !server_roles.contains(role_item);
                                body.row(28.0, |mut row| {
                                    row.col(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(regular::DATABASE)
                                                    .color(self.palette.muted_text),
                                            );
                                            ui.label(
                                                RichText::new(&role_item.db)
                                                    .monospace()
                                                    .strong()
                                                    .color(self.palette.text_primary),
                                            );
                                            if is_modified {
                                                ui.label(
                                                    RichText::new("●")
                                                        .size(10.0)
                                                        .color(self.palette.warning_color),
                                                )
                                                .on_hover_text("Modified (pending Apply)");
                                            }
                                        });
                                    });

                                    row.col(|ui| {
                                        if is_protected {
                                            ui.label(
                                                RichText::new(&role_item.role)
                                                    .monospace()
                                                    .color(self.palette.success_color),
                                            );
                                        } else {
                                            ComboBox::from_id_source((
                                                "edit_row_role_combo",
                                                username,
                                                idx,
                                                &role_item.db,
                                            ))
                                            .selected_text(
                                                RichText::new(&role_item.role)
                                                    .monospace()
                                                    .color(self.palette.success_color),
                                            )
                                            .width(175.0)
                                            .show_ui(ui, |ui| {
                                                for &role_opt in UserRole::COMMON_ROLES {
                                                    if ui
                                                        .selectable_label(
                                                            role_item.role == role_opt,
                                                            role_opt,
                                                        )
                                                        .clicked()
                                                    {
                                                        role_item.role.clear();
                                                        role_item.role.push_str(role_opt);
                                                    }
                                                }
                                            });
                                        }
                                    });

                                    row.col(|ui| {
                                        ui.label(
                                            RichText::new(role_item.description())
                                                .size(11.5)
                                                .color(self.palette.muted_text),
                                        );
                                    });

                                    row.col(|ui| {
                                        if is_protected {
                                            ui.label(
                                                RichText::new("Locked")
                                                    .size(11.0)
                                                    .color(self.palette.muted_text),
                                            );
                                        } else if ui
                                            .small_button(
                                                RichText::new("Remove")
                                                    .color(self.palette.error_color),
                                            )
                                            .on_hover_cursor(CursorIcon::PointingHand)
                                            .on_hover_text(
                                                "Remove this database role (click Apply to save)",
                                            )
                                            .clicked()
                                        {
                                            remove_idx = Some(idx);
                                        }
                                    });
                                });
                            }
                        }
                    });

                if add_clicked {
                    let candidate = UserRole::new(&tab.role_name_input, &tab.role_db_input);
                    if let Some(draft_roles) = tab.draft_roles.as_mut() {
                        if let Some(existing_same_db) =
                            draft_roles.iter_mut().find(|r| r.db == candidate.db)
                        {
                            existing_same_db.role = candidate.role;
                        } else {
                            draft_roles.insert(0, candidate);
                        }
                    }
                }

                if let Some(idx) = remove_idx {
                    if let Some(draft_roles) = tab.draft_roles.as_mut() {
                        if idx < draft_roles.len() {
                            draft_roles.remove(idx);
                        }
                    }
                }
            });
    }

    fn render_indexes_subtab(&mut self, ui: &mut Ui, tab: &CollectionTab, coll: &CollectionInfo) {
        let is_system_coll = tab.collection.starts_with("system.");

        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "Indexes on {}.{} ({} total)",
                    tab.database,
                    tab.collection,
                    coll.indexes.len()
                ))
                .strong()
                .color(self.palette.text_primary),
            );
            ui.label(
                RichText::new(
                    "(_id_ primary index is protected; system.* collections are read-only)",
                )
                .size(11.5)
                .color(self.palette.muted_text),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(!is_system_coll, |ui| {
                    let resp = primary_button(
                        ui,
                        &format!("{} Create Index...", regular::PLUS),
                        self.palette.accent_color,
                    );
                    let resp = if is_system_coll {
                        resp.on_disabled_hover_text(
                            "Cannot create indexes on protected system.* collections",
                        )
                    } else {
                        resp.on_hover_text("Create a new index on this collection")
                    };
                    if resp.clicked() {
                        self.events.push(AppEvent::OpenCreateIndexModal {
                            connection_name: tab.connection_name.clone(),
                            database: tab.database.clone(),
                            collection: tab.collection.clone(),
                        });
                    }
                });
            });
        });
        ui.add_space(6.0);

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .column(Column::initial(170.0).at_least(110.0))
            .column(Column::initial(220.0).at_least(130.0))
            .column(Column::initial(95.0).at_least(70.0))
            .column(Column::initial(75.0).at_least(60.0))
            .column(Column::initial(85.0).at_least(65.0))
            .column(Column::remainder().at_least(90.0))
            .header(24.0, |mut header| {
                for col_name in [
                    "Index Name",
                    "Key Specification",
                    "Type",
                    "Unique",
                    "Size",
                    "Action",
                ] {
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
                body.rows(26.0, coll.indexes.len(), |mut row| {
                    let idx_info = &coll.indexes[row.index()];
                    row.col(|ui| {
                        ui.label(
                            RichText::new(&idx_info.name)
                                .monospace()
                                .strong()
                                .color(self.palette.text_primary),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(&idx_info.keys)
                                .monospace()
                                .color(self.palette.text_primary),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(&idx_info.index_type).color(self.palette.text_primary),
                        );
                    });
                    row.col(|ui| {
                        let badge = if idx_info.unique { "UNIQUE" } else { "—" };
                        ui.label(RichText::new(badge).color(if idx_info.unique {
                            self.palette.success_color
                        } else {
                            self.palette.muted_text
                        }));
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(format!("{:.1} KB", idx_info.size_kb))
                                .color(self.palette.text_primary),
                        );
                    });
                    row.col(|ui| {
                        let is_primary = idx_info.name == "_id_";
                        let can_drop = !is_primary && !is_system_coll;
                        ui.add_enabled_ui(can_drop, |ui| {
                            let btn = egui::Button::new(
                                RichText::new(format!("{} Drop", regular::TRASH)).color(
                                    if can_drop {
                                        self.palette.error_color
                                    } else {
                                        self.palette.muted_text
                                    },
                                ),
                            )
                            .small();

                            let resp = ui
                                .add(btn)
                                .on_hover_cursor(CursorIcon::PointingHand)
                                .on_disabled_hover_text(if is_primary {
                                    "Primary '_id_' index cannot be dropped"
                                } else {
                                    "Cannot drop indexes on protected system.* collections"
                                });

                            if can_drop
                                && resp.on_hover_text("Drop this index from MongoDB").clicked()
                            {
                                self.events.push(AppEvent::RequestDropIndex {
                                    connection_name: tab.connection_name.clone(),
                                    database: tab.database.clone(),
                                    collection: tab.collection.clone(),
                                    index_name: idx_info.name.clone(),
                                });
                            }
                        });
                    });
                });
            });
    }

    fn render_schema_subtab(
        &mut self,
        ui: &mut Ui,
        tab: &mut CollectionTab,
        coll: &CollectionInfo,
    ) {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "Schema Analysis — {}.{} ({} fields)",
                    tab.database,
                    tab.collection,
                    coll.schema_fields.len()
                ))
                .strong()
                .color(self.palette.text_primary),
            );
            ui.separator();
            ui.label(
                RichText::new("Sample Size")
                    .size(12.0)
                    .strong()
                    .color(self.palette.text_primary),
            );
            ComboBox::from_id_source((
                "schema_sample_size_combo",
                &tab.connection_name,
                &tab.database,
                &tab.collection,
            ))
            .selected_text(RichText::new(tab.schema_sample_size.to_string()).monospace())
            .width(78.0)
            .show_ui(ui, |ui| {
                for sample_opt in [50_usize, 100, 250, 500, 1000] {
                    ui.selectable_value(
                        &mut tab.schema_sample_size,
                        sample_opt,
                        sample_opt.to_string(),
                    );
                }
            });

            ui.add_enabled_ui(!tab.schema_is_analyzing, |ui| {
                if primary_button(
                    ui,
                    &format!("{} Re-analyze Schema", regular::ARROWS_CLOCKWISE),
                    self.palette.accent_color,
                )
                .on_hover_text("Sample documents and recompute field types and frequency")
                .clicked()
                {
                    tab.schema_is_analyzing = true;
                    self.events.push(AppEvent::AnalyzeCollectionSchema {
                        connection_name: tab.connection_name.clone(),
                        database: tab.database.clone(),
                        collection: tab.collection.clone(),
                        sample_size: tab.schema_sample_size,
                    });
                }
            });

            if tab.schema_is_analyzing {
                ui.spinner();
                ui.label(
                    RichText::new("Analyzing schema...")
                        .size(11.5)
                        .color(self.palette.warning_color),
                );
            }
        });
        ui.add_space(6.0);

        if coll.schema_fields.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new(
                        "No schema fields discovered yet. Click 'Re-analyze Schema' to sample documents.",
                    )
                    .color(self.palette.muted_text),
                );
            });
            return;
        }

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .column(Column::initial(175.0).at_least(110.0))
            .column(Column::initial(145.0).at_least(100.0))
            .column(Column::initial(140.0).at_least(100.0))
            .column(Column::initial(180.0).at_least(110.0))
            .column(Column::remainder().at_least(120.0))
            .header(24.0, |mut header| {
                for title in [
                    "Field Path",
                    "BSON Type(s)",
                    "Frequency",
                    "Type Distribution",
                    "Sample Value",
                ] {
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
                        ui.label(
                            RichText::new(&field.field_name)
                                .monospace()
                                .strong()
                                .color(self.palette.text_primary),
                        );
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
                                .text(format!("{:.0}% ({})", field.frequency_pct, field.doc_count)),
                        );
                    });
                    row.col(|ui| {
                        render_type_distribution_bar(ui, field, self.palette);
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(&field.sample_value)
                                .monospace()
                                .color(self.palette.warning_color),
                        );
                    });
                });
            });
    }

    fn render_aggregation_subtab(&mut self, ui: &mut Ui, tab: &mut CollectionTab) {
        ui.add_space(4.0);

        let builder_validation = AggregationStage::build_pipeline(&tab.agg_stages);
        let raw_validation = AggregationStage::parse_raw_pipeline(&tab.agg_raw_json);
        let pipeline_valid = if tab.agg_raw_mode {
            raw_validation.is_ok()
        } else {
            builder_validation.is_ok()
        };

        // Toolbar
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "Aggregation Pipeline — {}.{}",
                    tab.database, tab.collection
                ))
                .strong()
                .color(self.palette.text_primary),
            );
            ui.separator();

            if ui
                .selectable_label(!tab.agg_raw_mode, "Builder")
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
            {
                tab.agg_raw_mode = false;
            }
            if ui
                .selectable_label(tab.agg_raw_mode, "Raw JSON")
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
            {
                tab.agg_raw_mode = true;
            }

            if !tab.agg_raw_mode {
                if secondary_button(
                    ui,
                    &format!("{} Add Stage", regular::PLUS),
                    self.palette.border_color,
                )
                .on_hover_text("Append a new aggregation pipeline stage")
                .clicked()
                {
                    tab.agg_stages.push(AggregationStage::new(
                        "$match",
                        AggregationStage::default_expression("$match"),
                        true,
                    ));
                }

                if secondary_button(
                    ui,
                    &format!("{} To Raw JSON", regular::BRACKETS_CURLY),
                    self.palette.border_color,
                )
                .on_hover_text("Serialize enabled stages into Raw JSON Pipeline mode")
                .clicked()
                {
                    match AggregationStage::stages_to_pretty_json(&tab.agg_stages) {
                        Ok(pretty) => {
                            tab.agg_raw_json = pretty;
                            tab.agg_raw_mode = true;
                            tab.agg_error = None;
                        }
                        Err(err) => {
                            tab.agg_error = Some(err.to_string());
                        }
                    }
                }
            }

            if secondary_button(
                ui,
                &format!("{} Copy Pipeline JSON", regular::COPY),
                self.palette.border_color,
            )
            .on_hover_text("Copy pipeline JSON array to clipboard")
            .clicked()
            {
                let json_to_copy = if tab.agg_raw_mode {
                    tab.agg_raw_json.clone()
                } else {
                    AggregationStage::stages_to_pretty_json(&tab.agg_stages)
                        .unwrap_or_else(|_| tab.agg_raw_json.clone())
                };
                ui.ctx().copy_text(json_to_copy);
            }

            ui.add_enabled_ui(pipeline_valid && !tab.agg_is_running, |ui| {
                if primary_button(
                    ui,
                    &format!("{} Run Pipeline", regular::LIGHTNING),
                    self.palette.accent_color,
                )
                .on_hover_text("Execute aggregation pipeline against collection")
                .clicked()
                {
                    tab.agg_error = None;
                    let (raw_json, stages) = if tab.agg_raw_mode {
                        (tab.agg_raw_json.clone(), Vec::new())
                    } else {
                        (String::new(), tab.agg_stages.clone())
                    };
                    self.events.push(AppEvent::RunAggregationPipeline {
                        connection_name: tab.connection_name.clone(),
                        database: tab.database.clone(),
                        collection: tab.collection.clone(),
                        raw_json,
                        stages,
                    });
                }
            });

            if tab.agg_is_running {
                ui.spinner();
                ui.label(
                    RichText::new("Running aggregation...")
                        .size(11.5)
                        .color(self.palette.warning_color),
                );
            } else if let (Some(docs), Some(ms)) = (&tab.agg_results, tab.agg_elapsed_ms) {
                let stage_count = if tab.agg_raw_mode {
                    raw_validation.as_ref().map_or(0, Vec::len)
                } else {
                    tab.agg_stages.iter().filter(|s| s.enabled).count()
                };
                ui.label(
                    RichText::new(format!(
                        "{} doc(s) • {} stage(s) • {ms:.1} ms",
                        docs.len(),
                        stage_count
                    ))
                    .monospace()
                    .size(11.5)
                    .color(self.palette.success_color),
                );
            }
        });

        ui.add_space(6.0);

        // Editor Section (Builder or Raw JSON)
        if tab.agg_raw_mode {
            ScrollArea::vertical()
                .id_source((
                    "agg_raw_scroll",
                    &tab.connection_name,
                    &tab.database,
                    &tab.collection,
                ))
                .max_height(210.0)
                .show(ui, |ui| {
                    let mut layouter = json_layouter(12.0);
                    ui.add(
                        TextEdit::multiline(&mut tab.agg_raw_json)
                            .font(FontId::monospace(12.0))
                            .layouter(&mut layouter)
                            .desired_width(f32::INFINITY)
                            .desired_rows(9)
                            .margin(Margin::same(8.0)),
                    );
                });

            ui.add_space(4.0);
            match &raw_validation {
                Ok(docs) => {
                    ui.label(
                        RichText::new(format!(
                            "{} Valid JSON pipeline ({} stage(s))",
                            regular::CHECK_CIRCLE,
                            docs.len()
                        ))
                        .size(11.5)
                        .color(self.palette.success_color),
                    );
                }
                Err(err) => {
                    ui.label(
                        RichText::new(format!("{} {err}", regular::WARNING_CIRCLE))
                            .size(11.5)
                            .color(self.palette.error_color),
                    );
                }
            }
        } else {
            let mut move_up: Option<usize> = None;
            let mut move_down: Option<usize> = None;
            let mut remove_idx: Option<usize> = None;
            let stage_len = tab.agg_stages.len();

            ScrollArea::vertical()
                .id_source((
                    "agg_builder_scroll",
                    &tab.connection_name,
                    &tab.database,
                    &tab.collection,
                ))
                .max_height(245.0)
                .show(ui, |ui| {
                    if tab.agg_stages.is_empty() {
                        ui.label(
                            RichText::new("No stages configured. Click '+ Add Stage' to begin.")
                                .color(self.palette.muted_text),
                        );
                    }

                    for (idx, stage) in tab.agg_stages.iter_mut().enumerate() {
                        Frame::none()
                            .fill(self.palette.panel_bg)
                            .inner_margin(Margin::same(8.0))
                            .rounding(Rounding::same(4.0))
                            .stroke(Stroke::new(1.0_f32, self.palette.border_color))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut stage.enabled, "");
                                    ui.label(
                                        RichText::new(format!("Stage #{}", idx + 1))
                                            .strong()
                                            .size(12.0)
                                            .color(self.palette.accent_color),
                                    );

                                    let mut selected_op = stage.operator.clone();
                                    ComboBox::from_id_source((
                                        "agg_stage_op",
                                        &tab.connection_name,
                                        &tab.database,
                                        &tab.collection,
                                        idx,
                                    ))
                                    .selected_text(
                                        RichText::new(&selected_op)
                                            .monospace()
                                            .strong()
                                            .color(self.palette.text_primary),
                                    )
                                    .width(120.0)
                                    .show_ui(ui, |ui| {
                                        for &op in AggregationStage::SUPPORTED_OPERATORS {
                                            ui.selectable_value(
                                                &mut selected_op,
                                                op.to_string(),
                                                op,
                                            );
                                        }
                                    });
                                    if selected_op != stage.operator {
                                        stage.expression =
                                            AggregationStage::default_expression(&selected_op)
                                                .to_string();
                                        stage.operator = selected_op;
                                    }

                                    match stage.to_pipeline_doc() {
                                        Ok(_) if stage.enabled => {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} Valid",
                                                    regular::CHECK_CIRCLE
                                                ))
                                                .size(11.0)
                                                .color(self.palette.success_color),
                                            );
                                        }
                                        Ok(_) => {
                                            ui.label(
                                                RichText::new("Disabled")
                                                    .size(11.0)
                                                    .color(self.palette.muted_text),
                                            );
                                        }
                                        Err(err) => {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} {err}",
                                                    regular::WARNING_CIRCLE
                                                ))
                                                .size(11.0)
                                                .color(self.palette.error_color),
                                            );
                                        }
                                    }

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .add_enabled(
                                                    stage_len > 1,
                                                    egui::Button::new(
                                                        RichText::new(regular::TRASH).color(
                                                            if stage_len > 1 {
                                                                self.palette.error_color
                                                            } else {
                                                                self.palette.muted_text
                                                            },
                                                        ),
                                                    )
                                                    .small(),
                                                )
                                                .on_hover_cursor(CursorIcon::PointingHand)
                                                .on_hover_text("Remove stage")
                                                .clicked()
                                            {
                                                remove_idx = Some(idx);
                                            }

                                            if ui
                                                .add_enabled(
                                                    idx + 1 < stage_len,
                                                    egui::Button::new(regular::CARET_DOWN).small(),
                                                )
                                                .on_hover_cursor(CursorIcon::PointingHand)
                                                .on_hover_text("Move stage down")
                                                .clicked()
                                            {
                                                move_down = Some(idx);
                                            }

                                            if ui
                                                .add_enabled(
                                                    idx > 0,
                                                    egui::Button::new(regular::CARET_UP).small(),
                                                )
                                                .on_hover_cursor(CursorIcon::PointingHand)
                                                .on_hover_text("Move stage up")
                                                .clicked()
                                            {
                                                move_up = Some(idx);
                                            }
                                        },
                                    );
                                });

                                ui.add_space(4.0);
                                let mut layouter = json_layouter(12.0);
                                ui.add(
                                    TextEdit::multiline(&mut stage.expression)
                                        .font(FontId::monospace(12.0))
                                        .layouter(&mut layouter)
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(3)
                                        .margin(Margin::same(6.0)),
                                );
                            });
                        ui.add_space(4.0);
                    }
                });

            if let Some(idx) = move_up {
                if idx > 0 && idx < tab.agg_stages.len() {
                    tab.agg_stages.swap(idx, idx - 1);
                }
            } else if let Some(idx) = move_down {
                if idx + 1 < tab.agg_stages.len() {
                    tab.agg_stages.swap(idx, idx + 1);
                }
            } else if let Some(idx) = remove_idx {
                if tab.agg_stages.len() > 1 && idx < tab.agg_stages.len() {
                    tab.agg_stages.remove(idx);
                }
            }

            ui.add_space(2.0);
            match &builder_validation {
                Ok(docs) => {
                    ui.label(
                        RichText::new(format!(
                            "{} Pipeline ready ({} enabled stage(s))",
                            regular::CHECK_CIRCLE,
                            docs.len()
                        ))
                        .size(11.5)
                        .color(self.palette.success_color),
                    );
                }
                Err(err) => {
                    ui.label(
                        RichText::new(format!("{} {err}", regular::WARNING_CIRCLE))
                            .size(11.5)
                            .color(self.palette.error_color),
                    );
                }
            }
        }

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        // Results Pane
        if let Some(ref err) = tab.agg_error {
            Frame::none()
                .fill(Color32::from_rgba_unmultiplied(220, 60, 60, 28))
                .stroke(Stroke::new(1.0_f32, self.palette.error_color))
                .rounding(Rounding::same(4.0))
                .inner_margin(Margin::same(8.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(regular::WARNING_CIRCLE)
                                .color(self.palette.error_color)
                                .size(14.0),
                        );
                        ui.label(
                            RichText::new(err)
                                .monospace()
                                .color(self.palette.error_color)
                                .size(12.0),
                        );
                    });
                });
            ui.add_space(6.0);
        }

        if let Some(ref results) = tab.agg_results {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Pipeline Output ({} document(s))", results.len()))
                        .strong()
                        .size(12.5),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .selectable_label(
                            tab.view_mode == DocumentViewMode::JsonTree,
                            format!("{} JSON View", regular::BRACKETS_CURLY),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .clicked()
                    {
                        tab.view_mode = DocumentViewMode::JsonTree;
                    }
                    if ui
                        .selectable_label(
                            tab.view_mode == DocumentViewMode::Table,
                            format!("{} Table View", regular::TABLE),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .clicked()
                    {
                        tab.view_mode = DocumentViewMode::Table;
                    }
                });
            });
            ui.add_space(4.0);

            let target = DocumentTarget {
                connection_name: &tab.connection_name,
                database: &tab.database,
                collection: &tab.collection,
            };

            match tab.view_mode {
                DocumentViewMode::Table => {
                    let mut agg_selected = None;
                    render_document_grid(
                        ui,
                        target,
                        results,
                        &tab.agg_cached_columns,
                        &mut agg_selected,
                        self.palette,
                        self.events,
                    );
                }
                DocumentViewMode::JsonTree => {
                    render_json_viewer(ui, target, results, self.palette, self.events);
                }
            }
        } else if tab.agg_error.is_none() {
            ui.label(
                RichText::new("Configure pipeline stages above and click 'Run Pipeline' to inspect output documents.")
                    .color(self.palette.muted_text)
                    .size(12.0),
            );
        }
    }
}

impl TabViewer for CollectionTabViewer<'_> {
    type Tab = CollectionTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        if tab.selected_user.is_some() {
            RichText::new(format!("{} User: {}", regular::USER_GEAR, tab.title()))
                .strong()
                .color(self.palette.text_primary)
                .into()
        } else {
            RichText::new(format!("{} {}", regular::TABLE, tab.title()))
                .strong()
                .color(self.palette.text_primary)
                .into()
        }
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        if tab.selected_user.is_some() {
            self.render_user_inspector(ui, tab);
            return;
        }

        let is_connected = self.is_connection_connected(&tab.connection_name);
        let maybe_coll = self.find_collection(&tab.connection_name, &tab.database, &tab.collection);

        // Top sub-navigation bar: Documents | Indexes | Schema | Aggregation
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} {} {} {} {}",
                    regular::DATABASE,
                    tab.database,
                    regular::CARET_RIGHT,
                    regular::TABLE,
                    tab.collection
                ))
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
                let subtab_color = if selected {
                    self.palette.accent_color
                } else {
                    self.palette.text_primary
                };
                if ui
                    .selectable_label(
                        selected,
                        RichText::new(format!("{} {}", subtab.icon(), subtab.label()))
                            .strong()
                            .color(subtab_color),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .clicked()
                {
                    tab.active_subtab = subtab;
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if secondary_button(
                    ui,
                    &format!("{} Refresh", regular::ARROWS_CLOCKWISE),
                    self.palette.border_color,
                )
                .clicked()
                {
                    self.events.push(AppEvent::RefreshCollection {
                        connection_name: tab.connection_name.clone(),
                        database: tab.database.clone(),
                        collection: tab.collection.clone(),
                    });
                }

                if tab.active_subtab == CollectionSubTab::Documents {
                    if ui
                        .selectable_label(
                            tab.view_mode == DocumentViewMode::JsonTree,
                            format!("{} JSON View", regular::BRACKETS_CURLY),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .clicked()
                    {
                        tab.view_mode = DocumentViewMode::JsonTree;
                    }
                    if ui
                        .selectable_label(
                            tab.view_mode == DocumentViewMode::Table,
                            format!("{} Table View", regular::TABLE),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .clicked()
                    {
                        tab.view_mode = DocumentViewMode::Table;
                    }
                }
            });
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

        if !coll.loaded {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new(
                        "Loading collection documents, indexes, and schema from MongoDB...",
                    )
                    .color(self.palette.warning_color)
                    .strong(),
                );
            });
            return;
        }

        match tab.active_subtab {
            CollectionSubTab::Documents => {
                let filter_valid = DocumentRow::parse_filter_doc(&tab.filter_query).is_ok();
                let proj_valid =
                    DocumentRow::parse_bson_doc_allow_empty(&tab.projection_query).is_ok();
                let sort_valid = DocumentRow::parse_bson_doc_allow_empty(&tab.sort_query).is_ok();
                let all_valid = filter_valid && proj_valid && sort_valid;

                let mut trigger_query = false;

                ui.horizontal_wrapped(|ui| {
                    if query_field_input(
                        ui,
                        "Filter",
                        &mut tab.filter_query,
                        "{}",
                        210.0,
                        filter_valid,
                        self.palette.error_color,
                    ) {
                        trigger_query = true;
                    }

                    if query_field_input(
                        ui,
                        "Projection",
                        &mut tab.projection_query,
                        "{}",
                        140.0,
                        proj_valid,
                        self.palette.error_color,
                    ) {
                        trigger_query = true;
                    }

                    if query_field_input(
                        ui,
                        "Sort",
                        &mut tab.sort_query,
                        "{\"_id\": 1}",
                        140.0,
                        sort_valid,
                        self.palette.error_color,
                    ) {
                        trigger_query = true;
                    }
                });

                ui.add_space(4.0);

                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("Limit")
                            .strong()
                            .size(12.0)
                            .color(self.palette.text_primary),
                    );
                    ComboBox::from_id_source((
                        "doc_limit_combo",
                        &tab.connection_name,
                        &tab.database,
                        &tab.collection,
                    ))
                    .selected_text(RichText::new(tab.limit.to_string()).monospace())
                    .width(68.0)
                    .show_ui(ui, |ui| {
                        for limit_opt in [20_usize, 50, 100, 250] {
                            if ui
                                .selectable_label(tab.limit == limit_opt, limit_opt.to_string())
                                .clicked()
                                && tab.limit != limit_opt
                            {
                                tab.limit = limit_opt;
                                tab.skip = 0;
                                trigger_query = true;
                            }
                        }
                    });

                    ui.add_enabled_ui(all_valid && !tab.is_loading, |ui| {
                        if primary_button(
                            ui,
                            &format!("{} Find", regular::MAGNIFYING_GLASS),
                            self.palette.accent_color,
                        )
                        .clicked()
                        {
                            trigger_query = true;
                        }
                    });

                    ui.add_enabled_ui(!tab.is_loading, |ui| {
                        if secondary_button(
                            ui,
                            &format!("{} Reset", regular::ARROW_COUNTER_CLOCKWISE),
                            self.palette.border_color,
                        )
                        .clicked()
                        {
                            tab.filter_query.clear();
                            tab.filter_query.push_str("{}");
                            tab.projection_query.clear();
                            tab.projection_query.push_str("{}");
                            tab.sort_query.clear();
                            tab.sort_query.push_str("{\"_id\": 1}");
                            tab.skip = 0;
                            tab.limit = 20;
                            self.emit_live_query(tab);
                        }
                    });

                    if secondary_button(
                        ui,
                        &format!("{} Insert Doc", regular::PLUS),
                        self.palette.border_color,
                    )
                    .clicked()
                    {
                        self.events.push(AppEvent::OpenInsertDocumentModal {
                            connection_name: tab.connection_name.clone(),
                            database: tab.database.clone(),
                            collection: tab.collection.clone(),
                        });
                    }

                    if secondary_button(
                        ui,
                        &format!("{} Save Query", regular::FLOPPY_DISK),
                        self.palette.border_color,
                    )
                    .on_hover_text("Save current filter, projection, sort, and limit")
                    .clicked()
                    {
                        self.events.push(AppEvent::OpenSaveQueryModal {
                            connection_name: tab.connection_name.clone(),
                            database: tab.database.clone(),
                            collection: tab.collection.clone(),
                            filter: tab.filter_query.clone(),
                            projection: tab.projection_query.clone(),
                            sort: tab.sort_query.clone(),
                            limit: tab.limit,
                        });
                    }

                    ui.menu_button(
                        RichText::new(format!("{} Export", regular::CLOUD_ARROW_DOWN)).size(12.0),
                        |ui| {
                            for fmt in [
                                TransferFormat::Json,
                                TransferFormat::JsonLines,
                                TransferFormat::Csv,
                            ] {
                                if ui
                                    .button(fmt.label())
                                    .on_hover_cursor(CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    ui.close_menu();
                                    if let Some(path) = rfd::FileDialog::new()
                                        .set_file_name(format!(
                                            "{}.{}.{}",
                                            tab.database,
                                            tab.collection,
                                            fmt.extension()
                                        ))
                                        .add_filter(fmt.label(), &[fmt.extension()])
                                        .save_file()
                                    {
                                        self.events.push(AppEvent::ExportCollectionData {
                                            connection_name: tab.connection_name.clone(),
                                            database: tab.database.clone(),
                                            collection: tab.collection.clone(),
                                            filter: tab.filter_query.clone(),
                                            format: fmt,
                                            path,
                                        });
                                    }
                                }
                            }
                        },
                    )
                    .response
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text("Export collection documents using current filter");

                    let is_system_coll = tab.collection.starts_with("system.");
                    ui.add_enabled_ui(!is_system_coll, |ui| {
                        let resp = ui
                            .menu_button(
                                RichText::new(format!("{} Import", regular::FILES)).size(12.0),
                                |ui| {
                                    for fmt in [
                                        TransferFormat::Json,
                                        TransferFormat::JsonLines,
                                        TransferFormat::Csv,
                                    ] {
                                        if ui
                                            .button(fmt.label())
                                            .on_hover_cursor(CursorIcon::PointingHand)
                                            .clicked()
                                        {
                                            ui.close_menu();
                                            if let Some(path) = rfd::FileDialog::new()
                                                .add_filter(fmt.label(), &[fmt.extension()])
                                                .pick_file()
                                            {
                                                self.events.push(AppEvent::ImportCollectionData {
                                                    connection_name: tab.connection_name.clone(),
                                                    database: tab.database.clone(),
                                                    collection: tab.collection.clone(),
                                                    format: fmt,
                                                    path,
                                                });
                                            }
                                        }
                                    }
                                },
                            )
                            .response
                            .on_hover_cursor(CursorIcon::PointingHand);

                        if is_system_coll {
                            resp.on_disabled_hover_text(
                                "Cannot import into protected system.* collections",
                            );
                        } else {
                            resp.on_hover_text("Import documents from JSON, JSONL, or CSV");
                        }
                    });

                    if tab.is_loading {
                        ui.spinner();
                        ui.label(
                            RichText::new("Querying MongoDB...")
                                .size(11.5)
                                .color(self.palette.warning_color),
                        );
                    } else if !all_valid {
                        ui.label(
                            RichText::new(format!(
                                "{} Invalid JSON in query parameters",
                                regular::WARNING
                            ))
                            .color(self.palette.error_color)
                            .size(11.5),
                        );
                    }
                });

                if trigger_query && all_valid && !tab.is_loading {
                    tab.skip = 0;
                    self.emit_live_query(tab);
                }

                ui.add_space(6.0);

                // In live mode `coll.documents` already holds the server-paginated page;
                // in disconnected/mock mode we filter & slice in memory so `Reset` keeps all mock rows.
                let (displayed_docs, total): (Cow<'_, [DocumentRow]>, usize) = if is_connected {
                    let total = tab.total_matched.unwrap_or(coll.stats.document_count);
                    (Cow::Borrowed(coll.documents.as_slice()), total)
                } else {
                    let filtered = DocumentService::filter_documents(
                        &coll.documents,
                        &tab.filter_query,
                        usize::MAX,
                    );
                    let total = filtered.len();
                    let effective_limit = tab.limit.max(1);
                    let page_docs: Vec<DocumentRow> = filtered
                        .into_iter()
                        .skip(tab.skip)
                        .take(effective_limit)
                        .collect();
                    (Cow::Owned(page_docs), total)
                };

                tab.ensure_cached_columns(displayed_docs.as_ref());

                let displayed_len = displayed_docs.len();
                let limit = tab.limit.max(1);
                let has_prev = tab.skip > 0 && !tab.is_loading;
                let has_next = !tab.is_loading && (tab.skip + displayed_len) < total;

                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(has_prev, egui::Button::new("< Prev").small())
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .clicked()
                    {
                        tab.skip = tab.skip.saturating_sub(limit);
                        self.emit_live_query(tab);
                    }

                    let page_summary = if displayed_len == 0 {
                        match tab.last_query_ms {
                            Some(ms) => format!("No documents  •  {ms:.1} ms"),
                            None => "No documents".to_string(),
                        }
                    } else {
                        let start_item = tab.skip + 1;
                        let end_item = tab.skip + displayed_len;
                        let current_page = (tab.skip / limit) + 1;
                        let total_pages = total.max(end_item).div_ceil(limit).max(1);
                        match tab.last_query_ms {
                            Some(ms) => format!(
                                "Page {current_page} of {total_pages} • Showing {start_item}-{end_item} of {total} • {ms:.1} ms"
                            ),
                            None => format!(
                                "Page {current_page} of {total_pages} • Showing {start_item}-{end_item} of {total}"
                            ),
                        }
                    };

                    ui.label(
                        RichText::new(page_summary)
                            .size(11.5)
                            .monospace()
                            .color(self.palette.muted_text),
                    );

                    if ui
                        .add_enabled(has_next, egui::Button::new("Next >").small())
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .clicked()
                    {
                        tab.skip = tab.skip.saturating_add(limit);
                        self.emit_live_query(tab);
                    }
                });

                ui.add_space(4.0);

                let target = DocumentTarget {
                    connection_name: &tab.connection_name,
                    database: &tab.database,
                    collection: &tab.collection,
                };

                match tab.view_mode {
                    DocumentViewMode::Table => {
                        render_document_grid(
                            ui,
                            target,
                            displayed_docs.as_ref(),
                            &tab.cached_columns,
                            &mut tab.selected_row,
                            self.palette,
                            self.events,
                        );
                    }
                    DocumentViewMode::JsonTree => {
                        render_json_viewer(
                            ui,
                            target,
                            displayed_docs.as_ref(),
                            self.palette,
                            self.events,
                        );
                    }
                }
            }
            CollectionSubTab::Indexes => {
                self.render_indexes_subtab(ui, tab, coll);
            }
            CollectionSubTab::Schema => {
                self.render_schema_subtab(ui, tab, coll);
            }
            CollectionSubTab::Aggregation => {
                self.render_aggregation_subtab(ui, tab);
            }
        }
    }
}
