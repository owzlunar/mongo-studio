use egui::{Context, FontData, FontDefinitions, FontFamily, Key};

use crate::core::events::AppEvent;
use crate::core::state::AppState;
use crate::database::ConnectionManager;
use crate::models::DocumentRow;
use crate::theme::{apply_theme, ThemePalette};
use crate::ui::layout::render_main_layout;
use crate::workspace::{LogLevel, WorkspacePersistence};

pub struct MongoStudioApp {
    pub state: AppState,
    pub palette: ThemePalette,
    event_queue: Vec<AppEvent>,
}

impl MongoStudioApp {
    #[must_use]
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        Self::setup_fonts(&cc.egui_ctx);

        let persisted = if let Some(storage) = cc.storage {
            eframe::get_value::<WorkspacePersistence>(storage, eframe::APP_KEY)
                .or_else(WorkspacePersistence::load_from_disk)
                .unwrap_or_default()
        } else {
            WorkspacePersistence::load_from_disk().unwrap_or_default()
        };

        let state = AppState::from_persistence(persisted);
        let palette = apply_theme(&cc.egui_ctx, state.theme_mode);

        Self {
            state,
            palette,
            event_queue: Vec::with_capacity(8),
        }
    }

    fn setup_fonts(ctx: &Context) {
        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert(
            "phosphor".to_owned(),
            FontData::from_owned(egui_phosphor::Variant::Regular.font_data().font.into_owned()),
        );
        if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
            family.push("phosphor".to_owned());
        }
        if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
            family.push("phosphor".to_owned());
        }
        ctx.set_fonts(fonts);
    }

    fn handle_shortcuts(ctx: &Context, events: &mut Vec<AppEvent>) {
        ctx.input(|input| {
            let cmd_or_ctrl = input.modifiers.command || input.modifiers.ctrl;
            if cmd_or_ctrl && input.key_pressed(Key::P) {
                events.push(AppEvent::ToggleCommandPalette);
            }
            if cmd_or_ctrl && input.key_pressed(Key::B) {
                events.push(AppEvent::ToggleSidebar);
            }
            if cmd_or_ctrl && input.key_pressed(Key::J) {
                events.push(AppEvent::ToggleBottomPanel);
            }
        });
    }

    pub fn process_events(&mut self, ctx: &Context, events: Vec<AppEvent>) {
        for event in events {
            match event {
                AppEvent::OpenCollection {
                    connection_name,
                    database,
                    collection,
                } => {
                    self.state
                        .open_or_focus_collection(&connection_name, &database, &collection);
                    self.state.bottom_panel.push_log(
                        LogLevel::Info,
                        "Workspace",
                        format!("Opened collection tab: {database}.{collection}"),
                    );
                }
                AppEvent::SelectActivity(view) => {
                    if self.state.active_activity == view {
                        self.state.sidebar_visible = !self.state.sidebar_visible;
                    } else {
                        self.state.active_activity = view;
                        self.state.sidebar_visible = true;
                    }
                }
                AppEvent::ToggleSidebar => {
                    self.state.sidebar_visible = !self.state.sidebar_visible;
                }
                AppEvent::ToggleBottomPanel => {
                    self.state.bottom_panel.visible = !self.state.bottom_panel.visible;
                }
                AppEvent::ToggleCommandPalette => {
                    self.state.command_palette_open = !self.state.command_palette_open;
                    if self.state.command_palette_open {
                        self.state.command_palette_query.clear();
                    }
                }
                AppEvent::SwitchTheme(mode) => {
                    self.state.theme_mode = mode;
                    self.palette = apply_theme(ctx, mode);
                    self.state.bottom_panel.push_log(
                        LogLevel::Info,
                        "Theme",
                        format!("Switched color theme to {mode:?}"),
                    );
                }
                AppEvent::OpenNewConnectionModal => {
                    self.state.show_new_connection_modal = true;
                }
                AppEvent::AddConnection { name, uri } => {
                    if let Ok(conn) = ConnectionManager::create_connection(&name, &uri) {
                        self.state.connections.push(conn);
                        self.state.active_connection_idx = self.state.connections.len() - 1;
                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "ConnectionManager",
                            format!("Added connection profile '{name}' ({uri})"),
                        );
                    }
                }
                AppEvent::RefreshConnections => {
                    self.state.bottom_panel.push_log(
                        LogLevel::Info,
                        "Explorer",
                        "Refreshed databases and collection metadata.",
                    );
                }
                AppEvent::ExecuteFilterQuery {
                    database,
                    collection,
                    filter,
                } => {
                    let query_cmd =
                        format!("db.getSiblingDB(\"{database}\").{collection}.find({filter})");
                    self.state.bottom_panel.query_history.push(query_cmd.clone());
                    self.state
                        .bottom_panel
                        .push_log(LogLevel::Success, "QueryRunner", query_cmd);
                }
                AppEvent::InsertMockDocument {
                    database,
                    collection,
                } => {
                    if let Some(coll) = self.state.find_collection_mut(&database, &collection) {
                        let next_id = coll.documents.len() + 1;
                        coll.documents.push(DocumentRow {
                            id: next_id.to_string(),
                            name: format!("User #{next_id}"),
                            email: format!("user{next_id}@example.com"),
                            status: "active".to_string(),
                            role: "member".to_string(),
                            created_at: "2026-11-11T15:00:00Z".to_string(),
                        });
                        coll.stats.document_count = coll.documents.len();
                    }
                    self.state.bottom_panel.push_log(
                        LogLevel::Success,
                        "DocumentService",
                        format!("Inserted new mock document into {database}.{collection}"),
                    );
                }
                AppEvent::ClearOutputLogs => {
                    self.state.bottom_panel.logs.clear();
                }
                AppEvent::ResetWorkspaceLayout => {
                    self.state = AppState::default();
                    self.palette = apply_theme(ctx, self.state.theme_mode);
                }
            }
        }
    }
}

impl eframe::App for MongoStudioApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let mut events = std::mem::take(&mut self.event_queue);
        events.clear();
        Self::handle_shortcuts(ctx, &mut events);
        render_main_layout(ctx, &mut self.state, &self.palette, &mut events);
        if !events.is_empty() {
            self.process_events(ctx, events);
        } else {
            self.event_queue = events;
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let persisted = self.state.to_persistence();
        eframe::set_value(storage, eframe::APP_KEY, &persisted);
        let _ = persisted.save_to_disk();
    }
}
