use egui::{Color32, ComboBox, CursorIcon, Margin, RichText, TextEdit, Ui, Vec2};
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

    fn render_user_inspector(&mut self, ui: &mut Ui, tab: &mut CollectionTab, username: &str) {
        let Some((conn_idx, conn, user)) =
            self.find_user_and_conn(&tab.connection_name, username)
        else {
            ui.centered_and_justified(|ui| {
                ui.label(format!(
                    "User '{}' not found on connection '{}'.",
                    username, tab.connection_name
                ));
            });
            return;
        };

        let is_protected = user.is_protected_admin();

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} {} {} {} Users {} {} {}",
                    regular::PLUGS_CONNECTED,
                    conn.name,
                    regular::CARET_RIGHT,
                    regular::USERS,
                    regular::CARET_RIGHT,
                    if is_protected {
                        regular::SHIELD_CHECK
                    } else {
                        regular::USER
                    },
                    user.username
                ))
                .monospace()
                .strong(),
            );
            ui.separator();

            if is_protected {
                ui.label(
                    RichText::new(format!(
                        "{} Protected System Admin (Read-Only)",
                        regular::LOCK_KEY
                    ))
                    .color(self.palette.warning_color)
                    .strong()
                    .size(12.0),
                );
            } else {
                ui.label(
                    RichText::new(format!("{} Editable User Account", regular::CHECK_CIRCLE))
                        .color(self.palette.success_color)
                        .size(12.0),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if secondary_button(
                        ui,
                        &format!("{} Delete User", regular::TRASH),
                        self.palette.error_color,
                    )
                    .clicked()
                    {
                        self.events.push(AppEvent::RequestDeleteUser {
                            connection_index: conn_idx,
                            username: user.username.clone(),
                        });
                    }
                });
            }
        });

        ui.separator();
        ui.add_space(6.0);

        if is_protected {
            egui::Frame::none()
                .fill(Color32::from_rgba_unmultiplied(220, 160, 40, 28))
                .stroke(egui::Stroke::new(1.0_f32, self.palette.warning_color))
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
            ui.add_space(10.0);
        }

        ui.label(
            RichText::new(format!(
                "{} Assigned Database Roles & Permissions ({})",
                regular::KEY,
                user.roles.len()
            ))
            .strong()
            .size(13.5),
        );
        ui.add_space(6.0);

        let roles_snapshot = user.roles.clone();
        let conn_name_owned = conn.name.clone();
        let username_owned = user.username.clone();

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .column(Column::initial(150.0))
            .column(Column::initial(160.0))
            .column(Column::remainder())
            .column(Column::initial(100.0))
            .header(24.0, |mut header| {
                for col_name in ["Database", "Role Name", "Privileges Description", "Action"] {
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
                body.rows(26.0, roles_snapshot.len(), |mut row| {
                    let role_item = &roles_snapshot[row.index()];
                    row.col(|ui| {
                        ui.label(
                            RichText::new(format!("{} {}", regular::DATABASE, role_item.db))
                                .monospace()
                                .strong(),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(&role_item.role)
                                .monospace()
                                .color(self.palette.success_color),
                        );
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
                                RichText::new(format!("{} Revoke", regular::TRASH))
                                    .color(self.palette.error_color),
                            )
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .on_hover_text("Revoke this database role from user")
                            .clicked()
                        {
                            self.events.push(AppEvent::RevokeUserRole {
                                connection_name: conn_name_owned.clone(),
                                username: username_owned.clone(),
                                db: role_item.db.clone(),
                                role: role_item.role.clone(),
                            });
                        }
                    });
                });
            });

        if is_protected {
            return;
        }

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);

        // Grant Database Role section
        ui.label(
            RichText::new(format!(
                "{} Grant Database Access & Role",
                regular::PLUS_CIRCLE
            ))
            .strong()
            .size(13.0),
        );
        ui.add_space(4.0);

        let mut db_options: Vec<String> = conn.databases.iter().map(|d| d.name.clone()).collect();
        if !db_options.iter().any(|d| d == "admin") {
            db_options.insert(0, "admin".to_string());
        }
        if tab.role_db_input.is_empty() {
            tab.role_db_input = "admin".to_string();
        }
        if tab.role_name_input.is_empty() {
            tab.role_name_input = "readWrite".to_string();
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new("Database:").size(12.0));
            ComboBox::from_id_source(("grant_db_combo", &user.username))
                .selected_text(RichText::new(&tab.role_db_input).monospace())
                .width(160.0)
                .show_ui(ui, |ui| {
                    for db_opt in &db_options {
                        ui.selectable_value(&mut tab.role_db_input, db_opt.clone(), db_opt);
                    }
                });

            ui.add_space(8.0);
            ui.label(RichText::new("Role:").size(12.0));
            ComboBox::from_id_source(("grant_role_combo", &user.username))
                .selected_text(RichText::new(&tab.role_name_input).monospace())
                .width(170.0)
                .show_ui(ui, |ui| {
                    for &role_opt in UserRole::COMMON_ROLES {
                        ui.selectable_value(
                            &mut tab.role_name_input,
                            role_opt.to_string(),
                            role_opt,
                        );
                    }
                });

            ui.add_space(8.0);
            if primary_button(
                ui,
                &format!("{} Grant Role", regular::PLUS),
                self.palette.accent_color,
            )
            .clicked()
            {
                self.events.push(AppEvent::GrantUserRole {
                    connection_name: conn.name.clone(),
                    username: user.username.clone(),
                    db: tab.role_db_input.clone(),
                    role: tab.role_name_input.clone(),
                });
            }
        });

        ui.add_space(14.0);
        ui.separator();
        ui.add_space(8.0);

        // Change Password section
        ui.label(
            RichText::new(format!("{} Change User Password", regular::KEY))
                .strong()
                .size(13.0),
        );
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.add(
                TextEdit::singleline(&mut tab.password_input)
                    .password(!tab.show_password)
                    .hint_text("Enter new password...")
                    .margin(Margin::symmetric(6.0, 4.0))
                    .desired_width(220.0),
            );
            ui.checkbox(&mut tab.show_password, RichText::new("Show").size(11.5));

            if primary_button(
                ui,
                &format!("{} Update Password", regular::CHECK),
                self.palette.accent_color,
            )
            .clicked()
                && !tab.password_input.trim().is_empty()
            {
                self.events.push(AppEvent::ChangeUserPassword {
                    connection_name: conn.name.clone(),
                    username: user.username.clone(),
                    new_password: tab.password_input.clone(),
                });
                tab.password_input.clear();
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
        if let Some(username) = tab.selected_user.clone() {
            self.render_user_inspector(ui, tab, &username);
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
