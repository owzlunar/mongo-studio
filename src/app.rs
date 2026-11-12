use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Context, FontData, FontDefinitions, FontFamily, Key};

use crate::core::events::AppEvent;
use crate::core::state::AppState;
use crate::database::{
    connection::ConnectionClient, ConnectionManager, DatabaseService, UserService,
};
use crate::models::{
    CollectionInfo, ConnectionConfig, ConnectionStatus, DatabaseInfo, DocumentRow, MongoUser,
    UserRole,
};
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
                    result: Ok((conn.databases, conn.users, conn.is_admin)),
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

    fn spawn_load_collection_task(
        &self,
        ctx: &Context,
        connection_name: String,
        uri: String,
        database: String,
        collection: String,
    ) {
        let tx = self.bg_tx.clone();
        let ctx_clone = ctx.clone();
        self.runtime.spawn(async move {
            let result = ConnectionClient::introspect_single_collection(
                &uri,
                &database,
                &collection,
                DEFAULT_CONNECT_TIMEOUT,
            )
            .await
            .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::CollectionLoaded {
                connection_name,
                database,
                collection,
                result,
            });
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

                    let needs_load = self
                        .state
                        .connections
                        .iter()
                        .find(|c| c.name == connection_name)
                        .and_then(|c| {
                            c.databases
                                .iter()
                                .find(|db| db.name == database)
                                .and_then(|db| {
                                    db.collections.iter().find(|coll| coll.name == collection)
                                })
                                .map(|coll| (!coll.loaded, c.uri.clone()))
                        });

                    if let Some((true, uri)) = needs_load {
                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "Explorer",
                            format!("Lazy-loading collection {database}.{collection}..."),
                        );
                        self.spawn_load_collection_task(
                            ctx,
                            connection_name,
                            uri,
                            database,
                            collection,
                        );
                    } else {
                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "Workspace",
                            format!("Opened collection tab: {database}.{collection}"),
                        );
                    }
                }
                AppEvent::RefreshCollection {
                    connection_name,
                    database,
                    collection,
                } => {
                    if let Some(conn) = self
                        .state
                        .connections
                        .iter()
                        .find(|c| c.name == connection_name)
                    {
                        let uri = conn.uri.clone();
                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "Explorer",
                            format!("Refreshing collection {database}.{collection}..."),
                        );
                        self.spawn_load_collection_task(
                            ctx,
                            connection_name,
                            uri,
                            database,
                            collection,
                        );
                    }
                }
                AppEvent::CollectionLoaded {
                    connection_name,
                    database,
                    collection,
                    result,
                } => match result {
                    Ok(loaded_coll) => {
                        let doc_count = loaded_coll.stats.document_count;
                        if let Some(conn) = self
                            .state
                            .connections
                            .iter_mut()
                            .find(|c| c.name == connection_name)
                        {
                            if let Some(db) =
                                conn.databases.iter_mut().find(|d| d.name == database)
                            {
                                if let Some(slot) =
                                    db.collections.iter_mut().find(|c| c.name == collection)
                                {
                                    *slot = loaded_coll;
                                }
                            }
                        }
                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "Explorer",
                            format!(
                                "Loaded {database}.{collection} ({doc_count} total documents)"
                            ),
                        );
                    }
                    Err(err) => {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "Explorer",
                            format!("Failed to load {database}.{collection}: {err}"),
                        );
                    }
                },
                AppEvent::OpenUserTab {
                    connection_name,
                    username,
                } => {
                    self.state.open_or_focus_user(&connection_name, &username);
                    self.state.bottom_panel.push_log(
                        LogLevel::Info,
                        "UserManager",
                        format!("Opened user permissions inspector for '{username}'"),
                    );
                }
                AppEvent::OpenCreateDatabaseModal { connection_index } => {
                    self.state.create_db_conn_idx = connection_index;
                    self.state.new_db_name.clear();
                    self.state.new_db_collection = "items".to_string();
                    self.state.show_create_db_modal = true;
                }
                AppEvent::CreateDatabase {
                    connection_index,
                    db_name,
                    collection_name,
                } => {
                    let trimmed_db = db_name.trim().to_string();
                    let trimmed_coll = collection_name.trim().to_string();
                    if trimmed_db.is_empty() || trimmed_coll.is_empty() {
                        continue;
                    }

                    if let Some(conn) = self.state.connections.get_mut(connection_index) {
                        if let Some(existing_db) =
                            conn.databases.iter_mut().find(|d| d.name == trimmed_db)
                        {
                            existing_db.expanded = true;
                            if !existing_db
                                .collections
                                .iter()
                                .any(|c| c.name == trimmed_coll)
                            {
                                existing_db
                                    .collections
                                    .push(CollectionInfo::new_lazy_stub(&trimmed_db, &trimmed_coll));
                            }
                        } else {
                            let mut new_db = DatabaseInfo::new(&trimmed_db, true, [&trimmed_coll]);
                            if let Some(first_coll) = new_db.collections.first_mut() {
                                first_coll.documents.clear();
                                first_coll.stats.document_count = 0;
                                first_coll.loaded = true;
                            }
                            conn.databases.push(new_db);
                            conn.databases.sort_by(|a, b| a.name.cmp(&b.name));
                        }

                        let conn_name = conn.name.clone();
                        let uri = conn.uri.clone();
                        let _ = ConnectionManager::save_profiles(&self.state.connections);

                        self.state
                            .open_or_focus_collection(&conn_name, &trimmed_db, &trimmed_coll);

                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "DatabaseService",
                            format!(
                                "Created database '{trimmed_db}' with initial collection '{trimmed_coll}'"
                            ),
                        );

                        let tx = self.bg_tx.clone();
                        let ctx_clone = ctx.clone();
                        self.runtime.spawn(async move {
                            if DatabaseService::create_database(
                                &uri,
                                &trimmed_db,
                                &trimmed_coll,
                                DEFAULT_CONNECT_TIMEOUT,
                            )
                            .await
                            .is_ok()
                            {
                                if let Ok(loaded) = ConnectionClient::introspect_single_collection(
                                    &uri,
                                    &trimmed_db,
                                    &trimmed_coll,
                                    DEFAULT_CONNECT_TIMEOUT,
                                )
                                .await
                                {
                                    let _ = tx.send(AppEvent::CollectionLoaded {
                                        connection_name: conn_name,
                                        database: trimmed_db,
                                        collection: trimmed_coll,
                                        result: Ok(loaded),
                                    });
                                    ctx_clone.request_repaint();
                                }
                            }
                        });
                    }
                }
                AppEvent::OpenCreateUserModal { connection_index } => {
                    self.state.create_user_conn_idx = connection_index;
                    self.state.new_user_username.clear();
                    self.state.new_user_password.clear();
                    self.state.new_user_db = "admin".to_string();
                    self.state.new_user_role = "readWrite".to_string();
                    self.state.show_create_user_modal = true;
                }
                AppEvent::CreateUser {
                    connection_index,
                    username,
                    password,
                    initial_db,
                    initial_role,
                } => {
                    let trimmed = username.trim().to_string();
                    if trimmed.eq_ignore_ascii_case("admin") {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "UserManager",
                            "Refused to create or overwrite protected 'admin' user.",
                        );
                        continue;
                    }
                    if let Some(conn) = self.state.connections.get_mut(connection_index) {
                        let role = UserRole::new(&initial_role, &initial_db);
                        let new_user = MongoUser {
                            id: format!("admin.{trimmed}"),
                            username: trimmed.clone(),
                            db: "admin".to_string(),
                            roles: vec![role.clone()],
                        };
                        conn.users.retain(|u| u.username != trimmed);
                        conn.users.push(new_user);
                        conn.users_expanded = true;

                        let conn_name = conn.name.clone();
                        let uri = conn.uri.clone();
                        self.state.open_or_focus_user(&conn_name, &trimmed);

                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "UserManager",
                            format!(
                                "Created user '{trimmed}' with role '{initial_role}' on '{initial_db}'"
                            ),
                        );

                        let tx = self.bg_tx.clone();
                        let ctx_clone = ctx.clone();
                        self.runtime.spawn(async move {
                            let res = UserService::create_user(
                                &uri,
                                &trimmed,
                                &password,
                                &[role],
                                DEFAULT_CONNECT_TIMEOUT,
                            )
                            .await;
                            if res.is_ok() {
                                if let Ok(users) =
                                    UserService::list_users_uri(&uri, DEFAULT_CONNECT_TIMEOUT).await
                                {
                                    let _ = tx.send(AppEvent::UserOperationResult {
                                        connection_name: conn_name,
                                        message: format!(
                                            "Created MongoDB user '{trimmed}' on server"
                                        ),
                                        result: Ok(users),
                                    });
                                    ctx_clone.request_repaint();
                                }
                            }
                        });
                    }
                }
                AppEvent::GrantUserRole {
                    connection_name,
                    username,
                    db,
                    role,
                } => {
                    if username.trim().eq_ignore_ascii_case("admin") {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "UserManager",
                            "Cannot modify roles of protected 'admin' user.",
                        );
                        continue;
                    }
                    if let Some(conn) = self
                        .state
                        .connections
                        .iter_mut()
                        .find(|c| c.name == connection_name)
                    {
                        if let Some(user) = conn.users.iter_mut().find(|u| u.username == username) {
                            let new_role = UserRole::new(&role, &db);
                            if !user.roles.contains(&new_role) {
                                user.roles.push(new_role);
                            }
                            let updated_roles = user.roles.clone();
                            let user_db = user.db.clone();
                            let uri = conn.uri.clone();
                            let conn_name = conn.name.clone();
                            let uname = username.clone();

                            self.state.bottom_panel.push_log(
                                LogLevel::Success,
                                "UserManager",
                                format!("Granted role '{role}' on '{db}' to user '{username}'"),
                            );

                            let tx = self.bg_tx.clone();
                            let ctx_clone = ctx.clone();
                            self.runtime.spawn(async move {
                                if UserService::update_user_roles(
                                    &uri,
                                    &user_db,
                                    &uname,
                                    &updated_roles,
                                    DEFAULT_CONNECT_TIMEOUT,
                                )
                                .await
                                .is_ok()
                                {
                                    if let Ok(users) =
                                        UserService::list_users_uri(&uri, DEFAULT_CONNECT_TIMEOUT)
                                            .await
                                    {
                                        let _ = tx.send(AppEvent::UserOperationResult {
                                            connection_name: conn_name,
                                            message: format!(
                                                "Granted role '{role}' on '{db}' to '{uname}'"
                                            ),
                                            result: Ok(users),
                                        });
                                        ctx_clone.request_repaint();
                                    }
                                }
                            });
                        }
                    }
                }
                AppEvent::RevokeUserRole {
                    connection_name,
                    username,
                    db,
                    role,
                } => {
                    if username.trim().eq_ignore_ascii_case("admin") {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "UserManager",
                            "Cannot revoke roles from protected 'admin' user.",
                        );
                        continue;
                    }
                    if let Some(conn) = self
                        .state
                        .connections
                        .iter_mut()
                        .find(|c| c.name == connection_name)
                    {
                        if let Some(user) = conn.users.iter_mut().find(|u| u.username == username) {
                            user.roles.retain(|r| !(r.db == db && r.role == role));
                            let updated_roles = user.roles.clone();
                            let user_db = user.db.clone();
                            let uri = conn.uri.clone();
                            let conn_name = conn.name.clone();
                            let uname = username.clone();

                            self.state.bottom_panel.push_log(
                                LogLevel::Info,
                                "UserManager",
                                format!("Revoked role '{role}' on '{db}' from user '{username}'"),
                            );

                            let tx = self.bg_tx.clone();
                            let ctx_clone = ctx.clone();
                            self.runtime.spawn(async move {
                                if UserService::update_user_roles(
                                    &uri,
                                    &user_db,
                                    &uname,
                                    &updated_roles,
                                    DEFAULT_CONNECT_TIMEOUT,
                                )
                                .await
                                .is_ok()
                                {
                                    if let Ok(users) =
                                        UserService::list_users_uri(&uri, DEFAULT_CONNECT_TIMEOUT)
                                            .await
                                    {
                                        let _ = tx.send(AppEvent::UserOperationResult {
                                            connection_name: conn_name,
                                            message: format!(
                                                "Revoked role '{role}' on '{db}' from '{uname}'"
                                            ),
                                            result: Ok(users),
                                        });
                                        ctx_clone.request_repaint();
                                    }
                                }
                            });
                        }
                    }
                }
                AppEvent::ChangeUserPassword {
                    connection_name,
                    username,
                    new_password,
                } => {
                    if username.trim().eq_ignore_ascii_case("admin") {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "UserManager",
                            "Cannot change password of protected 'admin' user.",
                        );
                        continue;
                    }
                    if let Some(conn) = self
                        .state
                        .connections
                        .iter()
                        .find(|c| c.name == connection_name)
                    {
                        let uri = conn.uri.clone();
                        let user_db = conn
                            .users
                            .iter()
                            .find(|u| u.username == username)
                            .map_or_else(|| "admin".to_string(), |u| u.db.clone());
                        let uname = username.clone();
                        self.runtime.spawn(async move {
                            let _ = UserService::change_user_password(
                                &uri,
                                &user_db,
                                &uname,
                                &new_password,
                                DEFAULT_CONNECT_TIMEOUT,
                            )
                            .await;
                        });
                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "UserManager",
                            format!("Updated password for user '{username}'"),
                        );
                    }
                }
                AppEvent::RequestDeleteUser {
                    connection_index,
                    username,
                } => {
                    if username.trim().eq_ignore_ascii_case("admin") {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "UserManager",
                            "System 'admin' user is protected and cannot be deleted.",
                        );
                    } else {
                        self.state.pending_delete_user = Some((connection_index, username));
                    }
                }
                AppEvent::ConfirmDeleteUser {
                    connection_index,
                    username,
                } => {
                    if username.trim().eq_ignore_ascii_case("admin") {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "UserManager",
                            "System 'admin' user is protected and cannot be deleted.",
                        );
                        continue;
                    }
                    if let Some(conn) = self.state.connections.get_mut(connection_index) {
                        let user_db = conn
                            .users
                            .iter()
                            .find(|u| u.username == username)
                            .map_or_else(|| "admin".to_string(), |u| u.db.clone());
                        conn.users.retain(|u| u.username != username);

                        let uri = conn.uri.clone();
                        let uname = username.clone();
                        self.runtime.spawn(async move {
                            let _ = UserService::drop_user(
                                &uri,
                                &user_db,
                                &uname,
                                DEFAULT_CONNECT_TIMEOUT,
                            )
                            .await;
                        });

                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "UserManager",
                            format!("Deleted MongoDB user '{username}'"),
                        );
                    }
                }
                AppEvent::UserOperationResult {
                    connection_name,
                    message,
                    result,
                } => match result {
                    Ok(users) => {
                        if let Some(conn) = self
                            .state
                            .connections
                            .iter_mut()
                            .find(|c| c.name == connection_name)
                        {
                            conn.users = users;
                        }
                        self.state
                            .bottom_panel
                            .push_log(LogLevel::Success, "UserManager", message);
                    }
                    Err(err) => {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "UserManager",
                            format!("{message}: {err}"),
                        );
                    }
                },
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
                    self.state.editing_connection_idx = None;
                    self.state.show_new_connection_modal = true;
                    self.state.test_conn_status = None;
                    self.state.is_testing_connection = false;
                }
                AppEvent::OpenEditConnectionModal { index } => {
                    if let Some(conn) = self.state.connections.get(index) {
                        let (base_uri, uri_user, uri_pass) =
                            ConnectionConfig::split_uri_credentials(&conn.uri);
                        let effective_user = if conn.username.is_empty() {
                            uri_user
                        } else {
                            conn.username.clone()
                        };
                        let effective_pass = if conn.has_saved_password {
                            ConnectionManager::load_password_from_keyring(conn.id)
                                .unwrap_or(uri_pass)
                        } else {
                            uri_pass
                        };
                        self.state.editing_connection_idx = Some(index);
                        self.state.new_conn_name = conn.name.clone();
                        self.state.new_conn_uri = base_uri;
                        self.state.new_conn_username = effective_user;
                        self.state.new_conn_password = effective_pass;
                        self.state.new_conn_save_password = conn.has_saved_password;
                        self.state.test_conn_status = None;
                        self.state.is_testing_connection = false;
                        self.state.show_new_connection_modal = true;
                    }
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
                AppEvent::UpdateConnection { index, name, uri } => {
                    let clamped_name = ConnectionConfig::clamp_profile_name(&name);
                    let trimmed_uri = uri.trim();
                    if clamped_name.is_empty() || trimmed_uri.is_empty() {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "ConnectionManager",
                            "Connection name and URI must not be empty.",
                        );
                        continue;
                    }

                    if let Some(conn) = self.state.connections.get_mut(index) {
                        let (_base_uri, uri_user, uri_pass) =
                            ConnectionConfig::split_uri_credentials(trimmed_uri);
                        conn.name = clamped_name;
                        conn.uri = trimmed_uri.to_string();
                        conn.host = ConnectionConfig::extract_display_host(trimmed_uri);
                        conn.username = uri_user;
                        conn.requires_auth = !conn.username.is_empty() || !uri_pass.is_empty();
                        conn.status = ConnectionStatus::Connecting;
                        conn.expanded = false;
                        conn.last_error = None;

                        if self.state.new_conn_save_password
                            && !self.state.new_conn_password.is_empty()
                        {
                            let _ = ConnectionManager::save_password_to_keyring(
                                conn.id,
                                &self.state.new_conn_password,
                            );
                            conn.has_saved_password = true;
                        } else if !self.state.new_conn_save_password && conn.has_saved_password {
                            ConnectionManager::delete_password_from_keyring(conn.id);
                            conn.has_saved_password = false;
                        }

                        let conn_id = conn.id;
                        let conn_name = conn.name.clone();
                        let conn_uri = conn.uri.clone();
                        let redacted = ConnectionConfig::redact_uri(&conn_uri);
                        self.state.active_connection_idx = index;

                        let _ = ConnectionManager::save_profiles(&self.state.connections);

                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "ConnectionManager",
                            format!("Updated profile '{conn_name}' ({redacted}), reconnecting..."),
                        );

                        self.spawn_connect_task(ctx, conn_id, conn_name, conn_uri);
                    }
                }
                AppEvent::ConnectProfile { index } => {
                    if let Some(conn) = self.state.connections.get_mut(index) {
                        if conn.has_saved_password
                            && ConnectionConfig::split_uri_credentials(&conn.uri)
                                .2
                                .is_empty()
                        {
                            if let Some(saved_pw) =
                                ConnectionManager::load_password_from_keyring(conn.id)
                            {
                                let (base_uri, uri_user, _) =
                                    ConnectionConfig::split_uri_credentials(&conn.uri);
                                let user = if conn.username.is_empty() {
                                    uri_user
                                } else {
                                    conn.username.clone()
                                };
                                conn.uri =
                                    ConnectionConfig::normalize_uri(&base_uri, &user, &saved_pw);
                            }
                        }

                        if conn.needs_auth_prompt() {
                            let (base_uri, uri_user, _) =
                                ConnectionConfig::split_uri_credentials(&conn.uri);
                            let effective_user = if conn.username.is_empty() {
                                uri_user
                            } else {
                                conn.username.clone()
                            };
                            let conn_name = conn.name.clone();
                            self.state.editing_connection_idx = Some(index);
                            self.state.new_conn_name = conn_name.clone();
                            self.state.new_conn_uri = base_uri;
                            self.state.new_conn_username = effective_user;
                            self.state.new_conn_password.clear();
                            self.state.new_conn_save_password = false;
                            self.state.test_conn_status = None;
                            self.state.is_testing_connection = false;
                            self.state.show_new_connection_modal = true;

                            self.state.bottom_panel.push_log(
                                LogLevel::Info,
                                "ConnectionManager",
                                format!(
                                    "Authentication required for '{conn_name}' — please enter username and password."
                                ),
                            );
                            continue;
                        }

                        conn.status = ConnectionStatus::Connecting;
                        conn.expanded = false;
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
                        conn.expanded = false;
                        conn.last_error = None;
                        if !conn.has_saved_password {
                            conn.clear_session_password();
                        }
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
                            Ok((dbs, users, is_admin)) => {
                                let db_count = dbs.len();
                                let user_count = users.len();
                                conn.status = ConnectionStatus::Connected;
                                conn.databases = dbs;
                                conn.users = users;
                                conn.is_admin = is_admin;
                                conn.expanded = true;
                                conn.last_error = None;

                                let _ = ConnectionManager::save_profiles(&self.state.connections);
                                let latency_note = latency_ms
                                    .map(|ms| format!(" in {ms:.1} ms"))
                                    .unwrap_or_default();
                                let admin_note = if is_admin {
                                    format!(", {user_count} user(s) loaded")
                                } else {
                                    String::new()
                                };
                                self.state.bottom_panel.push_log(
                                    LogLevel::Success,
                                    "ConnectionManager",
                                    format!(
                                        "Connected to '{name}' ({redacted}){latency_note} — discovered {db_count} database(s){admin_note}"
                                    ),
                                );
                            }
                            Err(err) => {
                                conn.status = ConnectionStatus::Error;
                                conn.expanded = false;
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
