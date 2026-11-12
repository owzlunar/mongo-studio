use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Context, FontData, FontDefinitions, FontFamily, Key};

use crate::core::events::AppEvent;
use crate::core::state::AppState;
use crate::database::{
    connection::ConnectionClient, ConnectionManager, DatabaseService, UserService,
};
use crate::models::{ConnectionConfig, ConnectionStatus, DatabaseInfo, DocumentRow, UserRole};
use crate::theme::{apply_theme, ThemePalette};
use crate::ui::layout::render_main_layout;
use crate::workspace::{LogLevel, WorkspacePersistence};

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

struct RoleUpdateTask {
    connection_name: String,
    uri: String,
    user_db: String,
    username: String,
    roles: Vec<UserRole>,
    success_message: String,
    failure_prefix: String,
}

pub struct MongoStudioApp {
    pub state: AppState,
    pub palette: ThemePalette,
    event_queue: Vec<AppEvent>,
    runtime: Arc<tokio::runtime::Runtime>,
    bg_tx: Sender<AppEvent>,
    bg_rx: Receiver<AppEvent>,
    pending_open_user: Option<(String, String)>,
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
            pending_open_user: None,
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

    fn spawn_save_profiles_task(&self, ctx: &Context) {
        let connections = self.state.connections.clone();
        let tx = self.bg_tx.clone();
        let ctx_clone = ctx.clone();
        self.runtime.spawn(async move {
            if let Err(err) = ConnectionManager::save_profiles_async(connections).await {
                // INTENTIONAL: receiver is dropped only on application shutdown.
                let _ = tx.send(AppEvent::BackgroundTaskError {
                    source: "ConnectionManager".to_string(),
                    message: format!("Failed to save connection profiles: {err}"),
                });
                ctx_clone.request_repaint();
            }
        });
    }

    fn spawn_keyring_save_task(&self, ctx: &Context, conn_id: uuid::Uuid, password: String) {
        let tx = self.bg_tx.clone();
        let ctx_clone = ctx.clone();
        self.runtime.spawn(async move {
            if let Err(err) =
                ConnectionManager::save_password_to_keyring_async(conn_id, password).await
            {
                // INTENTIONAL: receiver is dropped only on application shutdown.
                let _ = tx.send(AppEvent::BackgroundTaskError {
                    source: "Keyring".to_string(),
                    message: format!("Failed to save password to OS Keychain: {err}"),
                });
                ctx_clone.request_repaint();
            }
        });
    }

    fn spawn_keyring_load_task(&self, ctx: &Context, index: usize, open_edit_modal: bool) {
        let Some(conn) = self.state.connections.get(index) else {
            return;
        };
        let conn_id = conn.id;
        let tx = self.bg_tx.clone();
        let ctx_clone = ctx.clone();
        self.runtime.spawn(async move {
            let result = ConnectionManager::load_password_from_keyring_async(conn_id)
                .await
                .map_err(|err| err.to_string());
            // INTENTIONAL: receiver is dropped only on application shutdown.
            let _ = tx.send(AppEvent::KeyringPasswordLoaded {
                index,
                open_edit_modal,
                result,
            });
            ctx_clone.request_repaint();
        });
    }

    fn spawn_keyring_delete_task(&self, ctx: &Context, conn_id: uuid::Uuid) {
        let tx = self.bg_tx.clone();
        let ctx_clone = ctx.clone();
        self.runtime.spawn(async move {
            if let Err(err) = ConnectionManager::delete_password_from_keyring_async(conn_id).await {
                // INTENTIONAL: receiver is dropped only on application shutdown.
                let _ = tx.send(AppEvent::BackgroundTaskError {
                    source: "Keyring".to_string(),
                    message: format!("Failed to delete password from OS Keychain: {err}"),
                });
                ctx_clone.request_repaint();
            }
        });
    }

    fn open_auth_prompt_for_profile(&mut self, index: usize) {
        let Some(conn) = self.state.connections.get_mut(index) else {
            return;
        };
        conn.status = ConnectionStatus::Disconnected;
        let (base_uri, uri_user, _) = ConnectionConfig::split_uri_credentials(&conn.uri);
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
        self.state.new_conn_save_password = conn.has_saved_password;
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
    }

    fn connect_profile_now(&mut self, ctx: &Context, index: usize) {
        let Some(conn) = self.state.connections.get_mut(index) else {
            return;
        };

        if conn.needs_auth_prompt() {
            self.open_auth_prompt_for_profile(index);
            return;
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
            // INTENTIONAL: receiver is dropped only on application shutdown.
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
            // INTENTIONAL: receiver is dropped only on application shutdown.
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
            // INTENTIONAL: receiver is dropped only on application shutdown.
            let _ = tx.send(AppEvent::CollectionLoaded {
                connection_name,
                database,
                collection,
                result,
            });
            ctx_clone.request_repaint();
        });
    }

    fn spawn_update_user_roles_task(&self, ctx: &Context, task: RoleUpdateTask) {
        let tx = self.bg_tx.clone();
        let ctx_clone = ctx.clone();
        self.runtime.spawn(async move {
            let outcome = UserService::update_user_roles_and_list(
                &task.uri,
                &task.user_db,
                &task.username,
                &task.roles,
                DEFAULT_CONNECT_TIMEOUT,
            )
            .await;

            let (message, result) = match outcome {
                Ok(users) => (task.success_message, Ok(users)),
                Err(err) => (task.failure_prefix, Err(err.to_string())),
            };
            // INTENTIONAL: receiver is dropped only on application shutdown.
            let _ = tx.send(AppEvent::UserOperationResult {
                connection_name: task.connection_name,
                message,
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
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "DatabaseService",
                            "Database name and initial collection name must not be empty.",
                        );
                        continue;
                    }

                    if let Some(conn) = self.state.connections.get(connection_index) {
                        let conn_name = conn.name.clone();
                        let uri = conn.uri.clone();

                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "DatabaseService",
                            format!(
                                "Creating database '{trimmed_db}' with initial collection '{trimmed_coll}'..."
                            ),
                        );

                        let tx = self.bg_tx.clone();
                        let ctx_clone = ctx.clone();
                        self.runtime.spawn(async move {
                            let result = DatabaseService::create_database_and_introspect(
                                &uri,
                                &trimmed_db,
                                &trimmed_coll,
                                DEFAULT_CONNECT_TIMEOUT,
                            )
                            .await
                            .map_err(|err| err.to_string());

                            // INTENTIONAL: receiver is dropped only on application shutdown.
                            let _ = tx.send(AppEvent::DatabaseCreated {
                                connection_name: conn_name,
                                db_name: trimmed_db,
                                collection_name: trimmed_coll,
                                result,
                            });
                            ctx_clone.request_repaint();
                        });
                    }
                }
                AppEvent::DatabaseCreated {
                    connection_name,
                    db_name,
                    collection_name,
                    result,
                } => match result {
                    Ok(coll) => {
                        if let Some(conn) = self
                            .state
                            .connections
                            .iter_mut()
                            .find(|c| c.name == connection_name)
                        {
                            if let Some(existing_db) =
                                conn.databases.iter_mut().find(|d| d.name == db_name)
                            {
                                existing_db.expanded = true;
                                if let Some(slot) = existing_db
                                    .collections
                                    .iter_mut()
                                    .find(|c| c.name == collection_name)
                                {
                                    *slot = coll;
                                } else {
                                    existing_db.collections.push(coll);
                                }
                            } else {
                                let mut new_db =
                                    DatabaseInfo::new(&db_name, true, [&collection_name]);
                                if let Some(first_coll) = new_db.collections.first_mut() {
                                    *first_coll = coll;
                                }
                                conn.databases.push(new_db);
                                conn.databases.sort_by(|a, b| a.name.cmp(&b.name));
                            }
                        }

                        self.state.open_or_focus_collection(
                            &connection_name,
                            &db_name,
                            &collection_name,
                        );
                        self.spawn_save_profiles_task(ctx);

                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "DatabaseService",
                            format!(
                                "Created database '{db_name}' with initial collection '{collection_name}'"
                            ),
                        );
                    }
                    Err(err) => {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "DatabaseService",
                            format!(
                                "Failed to create database '{db_name}' ({collection_name}): {err}"
                            ),
                        );
                    }
                },
                AppEvent::OpenCreateUserModal { connection_index } => {
                    self.state.create_user_conn_idx = connection_index;
                    self.state.duplicate_source_user = None;
                    self.state.duplicate_source_roles.clear();
                    self.state.new_user_username.clear();
                    self.state.new_user_password.clear();
                    self.state.new_user_confirm_password.clear();
                    self.state.new_user_show_password = false;
                    self.state.new_user_db = "admin".to_string();
                    self.state.new_user_role = "readWrite".to_string();
                    self.state.show_create_user_modal = true;
                }
                AppEvent::OpenDuplicateUserModal {
                    connection_index,
                    source_username,
                } => {
                    let cloned_roles = self
                        .state
                        .connections
                        .get(connection_index)
                        .and_then(|c| c.users.iter().find(|u| u.username == source_username))
                        .map_or_else(
                            || vec![UserRole::new("readWrite", "admin")],
                            |u| u.roles.clone(),
                        );

                    self.state.create_user_conn_idx = connection_index;
                    self.state.duplicate_source_user = Some(source_username.clone());
                    self.state.duplicate_source_roles = cloned_roles;
                    self.state.new_user_username = format!("{source_username}_copy");
                    self.state.new_user_password.clear();
                    self.state.new_user_confirm_password.clear();
                    self.state.new_user_show_password = false;
                    self.state.show_create_user_modal = true;
                }
                AppEvent::CreateUser {
                    connection_index,
                    username,
                    password,
                    roles,
                } => {
                    let trimmed = username.trim().to_string();
                    if trimmed.is_empty() {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "UserManager",
                            "Username must not be empty.",
                        );
                        continue;
                    }
                    if trimmed.eq_ignore_ascii_case("admin") {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "UserManager",
                            "Refused to create or overwrite protected 'admin' user.",
                        );
                        continue;
                    }
                    if let Some(conn) = self.state.connections.get(connection_index) {
                        let effective_roles = if roles.is_empty() {
                            vec![UserRole::new("read", "admin")]
                        } else {
                            roles
                        };
                        let role_count = effective_roles.len();
                        let conn_name = conn.name.clone();
                        let uri = conn.uri.clone();

                        self.pending_open_user = Some((conn_name.clone(), trimmed.clone()));
                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "UserManager",
                            format!(
                                "Creating MongoDB user '{trimmed}' with {role_count} assigned database role(s)..."
                            ),
                        );

                        let tx = self.bg_tx.clone();
                        let ctx_clone = ctx.clone();
                        self.runtime.spawn(async move {
                            let res = UserService::create_user_and_list(
                                &uri,
                                &trimmed,
                                &password,
                                &effective_roles,
                                DEFAULT_CONNECT_TIMEOUT,
                            )
                            .await;

                            let (message, result) = match res {
                                Ok(users) => (
                                    format!(
                                        "Created MongoDB user '{trimmed}' with {role_count} assigned database role(s)"
                                    ),
                                    Ok(users),
                                ),
                                Err(err) => (
                                    format!("Failed to create MongoDB user '{trimmed}'"),
                                    Err(err.to_string()),
                                ),
                            };
                            // INTENTIONAL: receiver is dropped only on application shutdown.
                            let _ = tx.send(AppEvent::UserOperationResult {
                                connection_name: conn_name,
                                message,
                                result,
                            });
                            ctx_clone.request_repaint();
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
                    let task = self
                        .state
                        .connections
                        .iter()
                        .find(|c| c.name == connection_name)
                        .and_then(|conn| {
                            conn.users
                                .iter()
                                .find(|u| u.username == username)
                                .map(|user| {
                                    let mut updated_roles = user.roles.clone();
                                    let new_role = UserRole::new(&role, &db);
                                    if !updated_roles.contains(&new_role) {
                                        updated_roles.push(new_role);
                                    }
                                    RoleUpdateTask {
                                        connection_name: conn.name.clone(),
                                        uri: conn.uri.clone(),
                                        user_db: user.db.clone(),
                                        username: username.clone(),
                                        roles: updated_roles,
                                        success_message: format!(
                                            "Granted role '{role}' on '{db}' to '{username}'"
                                        ),
                                        failure_prefix: format!(
                                            "Failed to grant role '{role}' on '{db}' to '{username}'"
                                        ),
                                    }
                                })
                        });
                    if let Some(task) = task {
                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "UserManager",
                            format!("Granting role '{role}' on '{db}' to user '{username}'..."),
                        );
                        self.spawn_update_user_roles_task(ctx, task);
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
                    let task = self
                        .state
                        .connections
                        .iter()
                        .find(|c| c.name == connection_name)
                        .and_then(|conn| {
                            conn.users
                                .iter()
                                .find(|u| u.username == username)
                                .map(|user| {
                                    let mut updated_roles = user.roles.clone();
                                    updated_roles.retain(|r| !(r.db == db && r.role == role));
                                    RoleUpdateTask {
                                        connection_name: conn.name.clone(),
                                        uri: conn.uri.clone(),
                                        user_db: user.db.clone(),
                                        username: username.clone(),
                                        roles: updated_roles,
                                        success_message: format!(
                                            "Revoked role '{role}' on '{db}' from '{username}'"
                                        ),
                                        failure_prefix: format!(
                                            "Failed to revoke role '{role}' on '{db}' from '{username}'"
                                        ),
                                    }
                                })
                        });
                    if let Some(task) = task {
                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "UserManager",
                            format!("Revoking role '{role}' on '{db}' from user '{username}'..."),
                        );
                        self.spawn_update_user_roles_task(ctx, task);
                    }
                }
                AppEvent::SaveUserRoles {
                    connection_name,
                    username,
                    roles,
                } => {
                    if username.trim().eq_ignore_ascii_case("admin") {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "UserManager",
                            "Cannot modify roles of protected 'admin' user.",
                        );
                        continue;
                    }
                    let role_count = roles.len();
                    let task = self
                        .state
                        .connections
                        .iter()
                        .find(|c| c.name == connection_name)
                        .and_then(|conn| {
                            conn.users
                                .iter()
                                .find(|u| u.username == username)
                                .map(|user| RoleUpdateTask {
                                    connection_name: conn.name.clone(),
                                    uri: conn.uri.clone(),
                                    user_db: user.db.clone(),
                                    username: username.clone(),
                                    roles,
                                    success_message: format!(
                                        "Saved {role_count} role(s) for '{username}' on server"
                                    ),
                                    failure_prefix: format!(
                                        "Failed to save roles for '{username}'"
                                    ),
                                })
                        });
                    if let Some(task) = task {
                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "UserManager",
                            format!("Saving {role_count} database role(s) for user '{username}'..."),
                        );
                        self.spawn_update_user_roles_task(ctx, task);
                    }
                }
                AppEvent::OpenChangePasswordModal {
                    connection_name,
                    username,
                } => {
                    if username.trim().eq_ignore_ascii_case("admin") {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "UserManager",
                            "Cannot change password of protected 'admin' user.",
                        );
                        continue;
                    }
                    self.state.change_pwd_conn_name = connection_name;
                    self.state.change_pwd_username = username;
                    self.state.change_pwd_new.clear();
                    self.state.change_pwd_confirm.clear();
                    self.state.change_pwd_show = false;
                    self.state.show_change_password_modal = true;
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
                        let conn_name = conn.name.clone();
                        let uri = conn.uri.clone();
                        let user_db = conn
                            .users
                            .iter()
                            .find(|u| u.username == username)
                            .map_or_else(|| "admin".to_string(), |u| u.db.clone());
                        let uname = username.clone();

                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "UserManager",
                            format!("Updating password for user '{username}'..."),
                        );

                        let tx = self.bg_tx.clone();
                        let ctx_clone = ctx.clone();
                        self.runtime.spawn(async move {
                            let result = UserService::change_user_password(
                                &uri,
                                &user_db,
                                &uname,
                                &new_password,
                                DEFAULT_CONNECT_TIMEOUT,
                            )
                            .await
                            .map_err(|err| err.to_string());

                            // INTENTIONAL: receiver is dropped only on application shutdown.
                            let _ = tx.send(AppEvent::UserPasswordChanged {
                                connection_name: conn_name,
                                username: uname,
                                result,
                            });
                            ctx_clone.request_repaint();
                        });
                    }
                }
                AppEvent::UserPasswordChanged {
                    connection_name,
                    username,
                    result,
                } => match result {
                    Ok(()) => {
                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "UserManager",
                            format!("Updated password for user '{username}' on '{connection_name}'"),
                        );
                    }
                    Err(err) => {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "UserManager",
                            format!(
                                "Failed to update password for user '{username}' on '{connection_name}': {err}"
                            ),
                        );
                    }
                },
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
                    if let Some(conn) = self.state.connections.get(connection_index) {
                        let user_db = conn
                            .users
                            .iter()
                            .find(|u| u.username == username)
                            .map_or_else(|| "admin".to_string(), |u| u.db.clone());
                        let conn_name = conn.name.clone();
                        let uri = conn.uri.clone();
                        let uname = username.clone();

                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "UserManager",
                            format!("Deleting MongoDB user '{username}'..."),
                        );

                        let tx = self.bg_tx.clone();
                        let ctx_clone = ctx.clone();
                        self.runtime.spawn(async move {
                            let res = UserService::drop_user_and_list(
                                &uri,
                                &user_db,
                                &uname,
                                DEFAULT_CONNECT_TIMEOUT,
                            )
                            .await;

                            let (message, result) = match res {
                                Ok(users) => (
                                    format!("Deleted MongoDB user '{uname}'"),
                                    Ok(users),
                                ),
                                Err(err) => (
                                    format!("Failed to delete MongoDB user '{uname}'"),
                                    Err(err.to_string()),
                                ),
                            };
                            // INTENTIONAL: receiver is dropped only on application shutdown.
                            let _ = tx.send(AppEvent::UserOperationResult {
                                connection_name: conn_name,
                                message,
                                result,
                            });
                            ctx_clone.request_repaint();
                        });
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
                            conn.users_expanded = true;
                        }
                        if self.pending_open_user.as_ref().is_some_and(|(c, u)| {
                            *c == connection_name
                                && self
                                    .state
                                    .connections
                                    .iter()
                                    .find(|conn| conn.name == connection_name)
                                    .is_some_and(|conn| {
                                        conn.users.iter().any(|usr| usr.username == *u)
                                    })
                        }) {
                            if let Some((c, u)) = self.pending_open_user.take() {
                                self.state.open_or_focus_user(&c, &u);
                            }
                        }
                        self.state
                            .bottom_panel
                            .push_log(LogLevel::Success, "UserManager", message);
                    }
                    Err(err) => {
                        if self
                            .pending_open_user
                            .as_ref()
                            .is_some_and(|(c, _)| *c == connection_name)
                        {
                            self.pending_open_user = None;
                        }
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "UserManager",
                            format!("{message}: {err}"),
                        );
                    }
                },
                AppEvent::BackgroundTaskError { source, message } => {
                    self.state
                        .bottom_panel
                        .push_log(LogLevel::Error, &source, message);
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
                        let needs_keyring_load = conn.has_saved_password && uri_pass.is_empty();
                        self.state.editing_connection_idx = Some(index);
                        self.state.new_conn_name = conn.name.clone();
                        self.state.new_conn_uri = base_uri;
                        self.state.new_conn_username = effective_user;
                        self.state.new_conn_password = uri_pass;
                        self.state.new_conn_save_password = conn.has_saved_password;
                        self.state.test_conn_status = None;
                        self.state.is_testing_connection = false;
                        self.state.show_new_connection_modal = true;

                        if needs_keyring_load {
                            self.spawn_keyring_load_task(ctx, index, true);
                        }
                    }
                }
                AppEvent::KeyringPasswordLoaded {
                    index,
                    open_edit_modal,
                    result,
                } => {
                    if open_edit_modal {
                        match result {
                            Ok(Some(pw)) => {
                                if self.state.editing_connection_idx == Some(index) {
                                    self.state.new_conn_password = pw;
                                }
                            }
                            Ok(None) => {}
                            Err(err) => {
                                self.state.bottom_panel.push_log(
                                    LogLevel::Error,
                                    "Keyring",
                                    format!("Failed to load saved password from OS Keychain: {err}"),
                                );
                            }
                        }
                    } else {
                        match result {
                            Ok(Some(pw)) => {
                                if let Some(conn) = self.state.connections.get_mut(index) {
                                    let (base_uri, uri_user, _) =
                                        ConnectionConfig::split_uri_credentials(&conn.uri);
                                    let user = if conn.username.is_empty() {
                                        uri_user
                                    } else {
                                        conn.username.clone()
                                    };
                                    conn.uri =
                                        ConnectionConfig::normalize_uri(&base_uri, &user, &pw);
                                }
                                self.connect_profile_now(ctx, index);
                            }
                            Ok(None) => {
                                let mut auth_missing = false;
                                if let Some(conn) = self.state.connections.get_mut(index) {
                                    if conn.requires_auth {
                                        conn.has_saved_password = false;
                                        conn.status = ConnectionStatus::Disconnected;
                                        auth_missing = true;
                                    }
                                }
                                if auth_missing {
                                    self.spawn_save_profiles_task(ctx);
                                }
                                self.connect_profile_now(ctx, index);
                            }
                            Err(err) => {
                                self.state.bottom_panel.push_log(
                                    LogLevel::Error,
                                    "Keyring",
                                    format!("Failed to load saved password from OS Keychain: {err}"),
                                );
                                let needs_prompt =
                                    self.state.connections.get_mut(index).is_some_and(|conn| {
                                        let uri_pass =
                                            ConnectionConfig::split_uri_credentials(&conn.uri).2;
                                        if conn.requires_auth && uri_pass.is_empty() {
                                            conn.status = ConnectionStatus::Disconnected;
                                            true
                                        } else {
                                            false
                                        }
                                    });
                                if needs_prompt {
                                    self.open_auth_prompt_for_profile(index);
                                } else {
                                    self.connect_profile_now(ctx, index);
                                }
                            }
                        }
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
                            let password_to_save = if self.state.new_conn_save_password
                                && !self.state.new_conn_password.is_empty()
                            {
                                conn.has_saved_password = true;
                                Some(self.state.new_conn_password.clone())
                            } else {
                                None
                            };

                            let conn_id = conn.id;
                            let conn_name = conn.name.clone();
                            let conn_uri = conn.uri.clone();
                            let redacted = ConnectionConfig::redact_uri(&conn_uri);

                            self.state.connections.push(conn);
                            self.state.active_connection_idx = self.state.connections.len() - 1;

                            if let Some(password) = password_to_save {
                                self.spawn_keyring_save_task(ctx, conn_id, password);
                            }
                            self.spawn_save_profiles_task(ctx);

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

                    let update_outcome = self.state.connections.get_mut(index).map(|conn| {
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

                        let keyring_op = if self.state.new_conn_save_password
                            && !self.state.new_conn_password.is_empty()
                        {
                            conn.has_saved_password = true;
                            Some(Some(self.state.new_conn_password.clone()))
                        } else if !self.state.new_conn_save_password && conn.has_saved_password {
                            conn.has_saved_password = false;
                            Some(None)
                        } else {
                            None
                        };

                        (conn.id, conn.name.clone(), conn.uri.clone(), keyring_op)
                    });

                    if let Some((conn_id, conn_name, conn_uri, keyring_op)) = update_outcome {
                        let redacted = ConnectionConfig::redact_uri(&conn_uri);
                        self.state.active_connection_idx = index;

                        match keyring_op {
                            Some(Some(password)) => {
                                self.spawn_keyring_save_task(ctx, conn_id, password);
                            }
                            Some(None) => {
                                self.spawn_keyring_delete_task(ctx, conn_id);
                            }
                            None => {}
                        }
                        self.spawn_save_profiles_task(ctx);

                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "ConnectionManager",
                            format!("Updated profile '{conn_name}' ({redacted}), reconnecting..."),
                        );

                        self.spawn_connect_task(ctx, conn_id, conn_name, conn_uri);
                    }
                }
                AppEvent::ConnectProfile { index } => {
                    let needs_keyring = self.state.connections.get(index).is_some_and(|conn| {
                        conn.has_saved_password
                            && ConnectionConfig::split_uri_credentials(&conn.uri)
                                .2
                                .is_empty()
                    });

                    if needs_keyring {
                        if let Some(conn) = self.state.connections.get_mut(index) {
                            conn.status = ConnectionStatus::Connecting;
                            conn.expanded = false;
                            conn.last_error = None;
                        }
                        self.spawn_keyring_load_task(ctx, index, false);
                    } else {
                        self.connect_profile_now(ctx, index);
                    }
                }
                AppEvent::DisconnectProfile { index } => {
                    let disconnected_name = self.state.connections.get_mut(index).map(|conn| {
                        conn.status = ConnectionStatus::Disconnected;
                        conn.expanded = false;
                        conn.last_error = None;
                        if !conn.has_saved_password {
                            conn.clear_session_password();
                        }
                        conn.name.clone()
                    });
                    if let Some(name) = disconnected_name {
                        self.spawn_save_profiles_task(ctx);
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
                        if self.state.active_connection_idx >= self.state.connections.len()
                            && !self.state.connections.is_empty()
                        {
                            self.state.active_connection_idx = self.state.connections.len() - 1;
                        }
                        self.spawn_keyring_delete_task(ctx, removed.id);
                        self.spawn_save_profiles_task(ctx);
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
                    let matched = self
                        .state
                        .connections
                        .iter_mut()
                        .find(|c| c.id == conn_id || (c.name == name && c.uri == uri));

                    if let Some(conn) = matched {
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

                                let latency_note = latency_ms
                                    .map(|ms| format!(" in {ms:.1} ms"))
                                    .unwrap_or_default();
                                let admin_note = if is_admin {
                                    format!(", {user_count} user(s) loaded")
                                } else {
                                    String::new()
                                };
                                self.spawn_save_profiles_task(ctx);
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
                                self.spawn_save_profiles_task(ctx);
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
                    self.pending_open_user = None;
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
        if let Err(err) = persisted.save_to_disk() {
            tracing::warn!(error = %err, "failed to save workspace state on shutdown");
        }
        if let Err(err) = ConnectionManager::save_profiles(&self.state.connections) {
            tracing::warn!(error = %err, "failed to save connection profiles on shutdown");
        }
    }
}
