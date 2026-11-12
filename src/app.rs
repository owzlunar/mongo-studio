use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Context, FontData, FontDefinitions, FontFamily, Key};

use crate::core::events::AppEvent;
use crate::core::state::AppState;
use crate::database::{connection::ConnectionClient, ConnectionManager};
use crate::models::{ConnectionConfig, ConnectionStatus, DocumentRow};
use crate::theme::{apply_theme, ThemePalette};
use crate::ui::layout::render_main_layout;
use crate::workspace::{LogLevel, WorkspacePersistence};

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub struct MongoStudioApp {
    pub state: AppState,
    pub palette: ThemePalette,
    event_queue: Vec<AppEvent>,
    runtime: Arc<tokio::runtime::Runtime>,
    bg_tx: Sender<AppEvent>,
    bg_rx: Receiver<AppEvent>,
}

impl MongoStudioApp {
    /// Creates a new [`MongoStudioApp`] instance, restoring persisted workspace state and
    /// initializing the background Tokio runtime for live MongoDB connections.
    ///
    /// # Panics
    ///
    /// Panics if the multi-threaded Tokio runtime cannot be initialized by the OS.
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

        let mut state = AppState::from_persistence(persisted);
        if let Some(saved_profiles) = ConnectionManager::load_profiles() {
            state.connections = saved_profiles;
        }
        let palette = apply_theme(&cc.egui_ctx, state.theme_mode);

        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("failed to initialize Tokio runtime for Mongo Studio"),
        );
        let (bg_tx, bg_rx) = mpsc::channel();

        Self {
            state,
            palette,
            event_queue: Vec::with_capacity(8),
            runtime,
            bg_tx,
            bg_rx,
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

    fn spawn_connect_task(
        &self,
        ctx: &Context,
        conn_id: uuid::Uuid,
        name: String,
        uri: String,
    ) {
        let tx = self.bg_tx.clone();
        let ctx_clone = ctx.clone();
        self.runtime.spawn(async move {
            let start = Instant::now();
            let outcome =
                ConnectionClient::connect_and_introspect(&name, &uri, DEFAULT_CONNECT_TIMEOUT)
                    .await;
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            let event = match outcome {
                Ok(conn) => AppEvent::ConnectionResult {
                    conn_id,
                    name,
                    uri,
                    result: Ok(conn.databases),
                    latency_ms: Some(elapsed_ms),
                },
                Err(err) => AppEvent::ConnectionResult {
                    conn_id,
                    name,
                    uri,
                    result: Err(err.to_string()),
                    latency_ms: None,
                },
            };
            let _ = tx.send(event);
            ctx_clone.request_repaint();
        });
    }

    fn spawn_ping_task(&self, ctx: &Context, uri: String) {
        let tx = self.bg_tx.clone();
        let ctx_clone = ctx.clone();
        self.runtime.spawn(async move {
            let result = ConnectionClient::ping_uri(&uri, DEFAULT_CONNECT_TIMEOUT)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::TestConnectionResult { uri, result });
            ctx_clone.request_repaint();
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
                    self.state.test_conn_status = None;
                    self.state.is_testing_connection = false;
                }
                AppEvent::TestConnection { name: _, uri } => {
                    self.state.is_testing_connection = true;
                    self.state.test_conn_status = None;
                    let redacted = ConnectionConfig::redact_uri(&uri);
                    self.state.bottom_panel.push_log(
                        LogLevel::Info,
                        "ConnectionClient",
                        format!("Pinging MongoDB server at '{redacted}'..."),
                    );
                    self.spawn_ping_task(ctx, uri);
                }
                AppEvent::TestConnectionResult { uri, result } => {
                    self.state.is_testing_connection = false;
                    let redacted = ConnectionConfig::redact_uri(&uri);
                    match result {
                        Ok(latency_ms) => {
                            let msg = format!("Connected ({latency_ms:.1} ms)");
                            self.state.test_conn_status = Some(Ok(msg));
                            self.state.bottom_panel.push_log(
                                LogLevel::Success,
                                "ConnectionClient",
                                format!("Ping to '{redacted}' succeeded in {latency_ms:.1} ms"),
                            );
                        }
                        Err(err) => {
                            self.state.test_conn_status = Some(Err(err.clone()));
                            self.state.bottom_panel.push_log(
                                LogLevel::Error,
                                "ConnectionClient",
                                format!("Ping to '{redacted}' failed: {err}"),
                            );
                        }
                    }
                }
                AppEvent::AddConnection { name, uri } => {
                    match ConnectionConfig::new_profile(&name, &uri) {
                        Ok(mut conn) => {
                            if self.state.new_conn_save_password
                                && !self.state.new_conn_password.is_empty()
                            {
                                let _ = ConnectionManager::save_password_to_keyring(
                                    conn.id,
                                    &self.state.new_conn_password,
                                );
                                conn.has_saved_password = true;
                            }

                            let conn_id = conn.id;
                            let conn_name = conn.name.clone();
                            let conn_uri = conn.uri.clone();
                            let redacted = ConnectionConfig::redact_uri(&conn_uri);

                            self.state.connections.push(conn);
                            self.state.active_connection_idx = self.state.connections.len() - 1;
                            let _ = ConnectionManager::save_profiles(&self.state.connections);

                            self.state.bottom_panel.push_log(
                                LogLevel::Info,
                                "ConnectionManager",
                                format!("Connecting to '{conn_name}' ({redacted})..."),
                            );

                            self.spawn_connect_task(ctx, conn_id, conn_name, conn_uri);
                        }
                        Err(err) => {
                            self.state.bottom_panel.push_log(
                                LogLevel::Error,
                                "ConnectionManager",
                                format!("Invalid connection config: {err}"),
                            );
                        }
                    }
                }
                AppEvent::ConnectProfile { index } => {
                    if let Some(conn) = self.state.connections.get_mut(index) {
                        conn.status = ConnectionStatus::Connecting;
                        conn.last_error = None;
                        let conn_id = conn.id;
                        let conn_name = conn.name.clone();
                        let conn_uri = conn.uri.clone();
                        let redacted = ConnectionConfig::redact_uri(&conn_uri);
                        self.state.active_connection_idx = index;

                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "ConnectionManager",
                            format!("Connecting profile '{conn_name}' ({redacted})..."),
                        );
                        self.spawn_connect_task(ctx, conn_id, conn_name, conn_uri);
                    }
                }
                AppEvent::DisconnectProfile { index } => {
                    if let Some(conn) = self.state.connections.get_mut(index) {
                        conn.status = ConnectionStatus::Disconnected;
                        conn.last_error = None;
                        let name = conn.name.clone();
                        let _ = ConnectionManager::save_profiles(&self.state.connections);
                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "ConnectionManager",
                            format!("Disconnected profile '{name}'"),
                        );
                    }
                }
                AppEvent::RemoveConnection { index } => {
                    if index < self.state.connections.len() {
                        let removed = self.state.connections.remove(index);
                        ConnectionManager::delete_password_from_keyring(removed.id);
                        if self.state.active_connection_idx >= self.state.connections.len()
                            && !self.state.connections.is_empty()
                        {
                            self.state.active_connection_idx = self.state.connections.len() - 1;
                        }
                        let _ = ConnectionManager::save_profiles(&self.state.connections);
                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "ConnectionManager",
                            format!("Removed connection profile '{}'", removed.name),
                        );
                    }
                }
                AppEvent::ConnectionResult {
                    conn_id,
                    name,
                    uri,
                    result,
                    latency_ms,
                } => {
                    let redacted = ConnectionConfig::redact_uri(&uri);
                    if let Some(conn) = self
                        .state
                        .connections
                        .iter_mut()
                        .find(|c| c.id == conn_id || (c.name == name && c.uri == uri))
                    {
                        match result {
                            Ok(dbs) => {
                                let db_count = dbs.len();
                                let first_db_coll = dbs.iter().find_map(|db| {
                                    db.collections
                                        .first()
                                        .map(|coll| (db.name.clone(), coll.name.clone()))
                                });

                                conn.status = ConnectionStatus::Connected;
                                conn.databases = dbs;
                                conn.expanded = true;
                                conn.last_error = None;

                                if let Some((db_name, coll_name)) = first_db_coll {
                                    self.state.open_or_focus_collection(
                                        &name,
                                        &db_name,
                                        &coll_name,
                                    );
                                }

                                let _ = ConnectionManager::save_profiles(&self.state.connections);
                                let latency_note = latency_ms
                                    .map(|ms| format!(" in {ms:.1} ms"))
                                    .unwrap_or_default();
                                self.state.bottom_panel.push_log(
                                    LogLevel::Success,
                                    "ConnectionManager",
                                    format!(
                                        "Connected to '{name}' ({redacted}){latency_note} — discovered {db_count} database(s)"
                                    ),
                                );
                            }
                            Err(err) => {
                                conn.status = ConnectionStatus::Error;
                                conn.last_error = Some(err.clone());
                                let _ = ConnectionManager::save_profiles(&self.state.connections);
                                self.state.bottom_panel.push_log(
                                    LogLevel::Error,
                                    "ConnectionManager",
                                    format!("Failed to connect to '{name}' ({redacted}): {err}"),
                                );
                            }
                        }
                    }
                }
                AppEvent::RefreshConnections => {
                    let active_idx = self.state.active_connection_idx;
                    if let Some(conn) = self.state.connections.get_mut(active_idx) {
                        conn.status = ConnectionStatus::Connecting;
                        conn.last_error = None;
                        let conn_id = conn.id;
                        let conn_name = conn.name.clone();
                        let conn_uri = conn.uri.clone();
                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "Explorer",
                            format!("Refreshing databases and collections for '{conn_name}'..."),
                        );
                        self.spawn_connect_task(ctx, conn_id, conn_name, conn_uri);
                    }
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
                            raw_json: None,
                        });
                        coll.stats.document_count = coll.documents.len();
                    }
                    self.state.bottom_panel.push_log(
                        LogLevel::Success,
                        "DocumentService",
                        format!("Inserted new document into {database}.{collection}"),
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

        while let Ok(bg_event) = self.bg_rx.try_recv() {
            events.push(bg_event);
        }

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
        let _ = ConnectionManager::save_profiles(&self.state.connections);
    }
}
