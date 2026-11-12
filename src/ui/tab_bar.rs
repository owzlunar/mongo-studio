use egui::{Color32, ComboBox, CursorIcon, Frame, Margin, RichText, ScrollArea, Stroke, TextEdit, Ui, Vec2};
use egui_dock::TabViewer;
use egui_extras::{Column, TableBuilder};
use egui_phosphor::regular;

use crate::core::events::AppEvent;
use crate::database::DocumentService;
use crate::models::{CollectionInfo, ConnectionConfig, MongoUser, UserRole};
use crate::query::filter::QueryFilter;
use crate::theme::ThemePalette;
use crate::ui::components::buttons::{primary_button, secondary_button};
use crate::ui::components::data_grid::render_document_grid;
use crate::ui::components::inputs::filter_input_row;
use crate::ui::components::json_viewer::render_json_viewer;
use crate::workspace::tabs::{CollectionSubTab, CollectionTab, DocumentViewMode};

pub struct CollectionTabViewer<'a> {
    pub connections: &'a [ConnectionConfig],
    pub palette: &'a ThemePalette,
    pub events: &'a mut Vec<AppEvent>,
}

impl<'a> CollectionTabViewer<'a> {
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
            ui.label(RichText::new(regular::PLUGS_CONNECTED).strong());
            ui.label(RichText::new(conn_name).monospace().strong());
            ui.label(RichText::new(regular::CARET_RIGHT).color(self.palette.muted_text));
            ui.label(RichText::new(regular::USERS).strong());
            ui.label(RichText::new("Users").monospace().strong());
            ui.label(RichText::new(regular::CARET_RIGHT).color(self.palette.muted_text));
            ui.label(
                RichText::new(if is_protected {
                    regular::SHIELD_CHECK
                } else {
                    regular::USER
                })
                .strong(),
            );
            ui.label(RichText::new(username).monospace().strong());
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
                .rounding(egui::Rounding::same(4.0))
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
                        .size(13.5),
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
}

impl TabViewer for CollectionTabViewer<'_> {
    type Tab = CollectionTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        if tab.selected_user.is_some() {
            RichText::new(format!("{} User: {}", regular::USER_GEAR, tab.title()))
                .strong()
                .into()
        } else {
            RichText::new(format!("{} {}", regular::TABLE, tab.title()))
                .strong()
                .into()
        }
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        if tab.selected_user.is_some() {
            self.render_user_inspector(ui, tab);
            return;
        }

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
                if ui
                    .selectable_label(
                        selected,
                        RichText::new(format!("{} {}", subtab.icon(), subtab.label())).strong(),
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

                    if secondary_button(
                        ui,
                        &format!("{} Insert Doc", regular::PLUS),
                        self.palette.border_color,
                    )
                    .clicked()
                    {
                        self.events.push(AppEvent::InsertMockDocument {
                            database: tab.database.clone(),
                            collection: tab.collection.clone(),
                        });
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
                ui.label(
                    RichText::new(format!(
                        "{} Loading collection documents, indexes, and schema from MongoDB...",
                        regular::CIRCLE_HALF
                    ))
                    .color(self.palette.warning_color)
                    .strong(),
                );
            });
            return;
        }

        match tab.active_subtab {
            CollectionSubTab::Documents => {
                let parsed_filter = QueryFilter::parse(&tab.filter_query);
                let is_valid = parsed_filter.is_valid_json();

                ui.horizontal(|ui| {
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
                            .margin(Margin::symmetric(6.0, 4.0))
                            .min_size(Vec2::new(110.0, 24.0))
                            .desired_width(110.0),
                    );

                    if primary_button(
                        ui,
                        &format!("{} Find", regular::MAGNIFYING_GLASS),
                        self.palette.accent_color,
                    )
                    .clicked()
                        || enter_pressed
                    {
                        self.events.push(AppEvent::ExecuteFilterQuery {
                            database: tab.database.clone(),
                            collection: tab.collection.clone(),
                            filter: tab.filter_query.clone(),
                        });
                    }

                    if secondary_button(
                        ui,
                        &format!("{} Reset", regular::ARROW_COUNTER_CLOCKWISE),
                        self.palette.border_color,
                    )
                    .clicked()
                    {
                        tab.filter_query.clear();
                        tab.filter_query.push_str("{}");
                        self.events.push(AppEvent::ExecuteFilterQuery {
                            database: tab.database.clone(),
                            collection: tab.collection.clone(),
                            filter: "{}".to_string(),
                        });
                    }

                    if !is_valid {
                        ui.label(
                            RichText::new(format!("{} Invalid JSON filter", regular::WARNING))
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
                            "Showing {} of {} sampled documents (total in collection: {})",
                            filtered_docs.len(),
                            coll.documents.len(),
                            coll.stats.document_count
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
