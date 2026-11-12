use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bson::oid::ObjectId;
use bson::{Bson, Document};
use egui::{Context, FontData, FontDefinitions, FontFamily, Key};
use uuid::Uuid;

use crate::core::events::AppEvent;
use crate::core::state::{AppState, PendingDeleteDocument, PendingDropIndex};
use crate::database::{
    connection::ConnectionClient, CollectionService, ConnectionManager, DatabaseService,
    DocumentQueryParams, DocumentService, ExportStats, IndexService, SchemaAnalyzer, ShellService,
    SshTunnelRegistry, TransferService, UserService,
};
use crate::models::{
    ConnectionConfig, ConnectionStatus, DatabaseInfo, DocumentRow, IndexInfo, QueryStore,
    SshAuthMethod, SshTunnelConfig, UserRole, MAX_QUERY_HISTORY,
};
use crate::query::AggregationStage;
use crate::theme::{apply_theme, ThemePalette};
use crate::ui::layout::render_main_layout;
use crate::workspace::{
    BottomPanelTab, CollectionSubTab, CollectionTab, LogLevel, WorkspacePersistence,
};

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

struct RoleUpdateTask {
    target: ConnectionTarget,
    user_db: String,
    username: String,
    roles: Vec<UserRole>,
    success_message: String,
    failure_prefix: String,
}

#[derive(Clone)]
struct ConnectionTarget {
    id: Uuid,
    name: String,
    uri: String,
    username: String,
    has_saved_password: bool,
    is_connected: bool,
    ssh_tunnel: SshTunnelConfig,
    ssh_secret: String,
}

fn is_system_database_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed.eq_ignore_ascii_case("admin")
        || trimmed.eq_ignore_ascii_case("local")
        || trimmed.eq_ignore_ascii_case("config")
}

fn parse_history_query(command: &str) -> Option<(Option<String>, String, String)> {
    let trimmed = command.trim().trim_end_matches(';').trim();
    let mut rest = trimmed.strip_prefix("db.")?;
    let mut parsed_db: Option<String> = None;

    if let Some(after_sib) = rest.strip_prefix("getSiblingDB(") {
        let (db_arg, after_paren) = after_sib.split_once(')')?;
        let clean_db = db_arg.trim().trim_matches('"').trim_matches('\'').trim();
        if clean_db.is_empty() {
            return None;
        }
        parsed_db = Some(clean_db.to_string());
        rest = after_paren.trim_start().strip_prefix('.')?;
    }

    let (coll_part, args_part) = rest.split_once(".find(")?;
    let coll = coll_part.trim();
    if coll.is_empty() {
        return None;
    }

    let raw_filter = args_part.trim_end().strip_suffix(')')?.trim();
    let filter = if raw_filter.is_empty() {
        "{}".to_string()
    } else {
        raw_filter.to_string()
    };

    Some((parsed_db, coll.to_string(), filter))
}

#[allow(clippy::too_many_arguments)]
async fn resolve_effective_uri(
    ssh_registry: &SshTunnelRegistry,
    conn_id: Uuid,
    raw_uri: String,
    username: String,
    has_saved_password: bool,
    ssh_tunnel: SshTunnelConfig,
    ssh_secret: String,
    timeout: Duration,
) -> Result<String, String> {
    let (base_uri, uri_user, uri_pass) = ConnectionConfig::split_uri_credentials(&raw_uri);
    let mongo_uri = if has_saved_password && uri_pass.is_empty() {
        let pw = ConnectionManager::load_password_from_keyring_async(conn_id)
            .await
            .map_err(|err| format!("Failed to load saved password from OS Keychain: {err}"))?;
        if let Some(pw) = pw {
            let effective_user = if username.is_empty() {
                uri_user
            } else {
                username
            };
            ConnectionConfig::normalize_uri(&base_uri, &effective_user, &pw)
        } else {
            raw_uri
        }
    } else {
        raw_uri
    };

    if ssh_tunnel.enabled {
        let effective_ssh_secret = if ssh_secret.is_empty() && ssh_tunnel.has_saved_secret {
            ConnectionManager::load_ssh_secret_from_keyring_async(conn_id)
                .await
                .map_err(|err| format!("Failed to load saved SSH secret from OS Keychain: {err}"))?
                .unwrap_or_default()
        } else {
            ssh_secret
        };

        ssh_registry
            .ensure_tunnel_and_rewrite_uri_async(
                conn_id,
                mongo_uri,
                ssh_tunnel,
                effective_ssh_secret,
                timeout,
            )
            .await
            .map_err(|err| err.to_string())
    } else {
        Ok(mongo_uri)
    }
}

async fn resolve_target_uri(
    ssh_registry: &SshTunnelRegistry,
    target: &ConnectionTarget,
) -> Result<String, String> {
    resolve_effective_uri(
        ssh_registry,
        target.id,
        target.uri.clone(),
        target.username.clone(),
        target.has_saved_password,
        target.ssh_tunnel.clone(),
        target.ssh_secret.clone(),
        DEFAULT_CONNECT_TIMEOUT,
    )
    .await
}

pub struct MongoStudioApp {
    pub state: AppState,
    pub palette: ThemePalette,
    event_queue: Vec<AppEvent>,
    runtime: Arc<tokio::runtime::Runtime>,
    bg_tx: Sender<AppEvent>,
    bg_rx: Receiver<AppEvent>,
    pending_open_user: Option<(String, String)>,
    ssh_registry: SshTunnelRegistry,
    ssh_session_secrets: HashMap<Uuid, String>,
}

impl Drop for MongoStudioApp {
    fn drop(&mut self) {
        self.ssh_registry.close_all();
    }
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
            ssh_registry: SshTunnelRegistry::new(),
            ssh_session_secrets: HashMap::new(),
        }
    }

    fn setup_fonts(ctx: &Context) {
        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert(
            "phosphor".to_owned(),
            FontData::from_owned(
                egui_phosphor::Variant::Regular
                    .font_data()
                    .font
                    .into_owned(),
            ),
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

    fn connection_target_from_config(&self, conn: &ConnectionConfig) -> ConnectionTarget {
        ConnectionTarget {
            id: conn.id,
            name: conn.name.clone(),
            uri: conn.uri.clone(),
            username: conn.username.clone(),
            has_saved_password: conn.has_saved_password,
            is_connected: conn.status.is_connected(),
            ssh_tunnel: conn.ssh_tunnel.clone(),
            ssh_secret: self
                .ssh_session_secrets
                .get(&conn.id)
                .cloned()
                .unwrap_or_default(),
        }
    }

    fn connection_target_by_index(&self, index: usize) -> Option<ConnectionTarget> {
        self.state
            .connections
            .get(index)
            .map(|c| self.connection_target_from_config(c))
    }

    fn connection_target_by_name(&self, connection_name: &str) -> Option<ConnectionTarget> {
        self.state
            .connections
            .iter()
            .find(|c| c.name == connection_name)
            .map(|c| self.connection_target_from_config(c))
    }

    fn tab_query_params(
        &self,
        connection_name: &str,
        database: &str,
        collection: &str,
    ) -> Option<(String, String, String, usize, usize)> {
        for (_, node) in self.state.dock_state.iter_all_nodes() {
            if let Some(tabs) = node.tabs() {
                if let Some(tab) = tabs.iter().find(|t| {
                    t.selected_user.is_none()
                        && t.connection_name == connection_name
                        && t.database == database
                        && t.collection == collection
                }) {
                    return Some((
                        tab.filter_query.clone(),
                        tab.projection_query.clone(),
                        tab.sort_query.clone(),
                        tab.skip,
                        tab.limit,
                    ));
                }
            }
        }
        None
    }

    fn set_tab_loading(
        &mut self,
        connection_name: &str,
        database: &str,
        collection: &str,
        loading: bool,
    ) {
        for (_, tab) in self.state.dock_state.iter_all_tabs_mut() {
            if tab.selected_user.is_none()
                && tab.connection_name == connection_name
                && tab.database == database
                && tab.collection == collection
            {
                tab.is_loading = loading;
            }
        }
    }

    fn set_tab_agg_running(
        &mut self,
        connection_name: &str,
        database: &str,
        collection: &str,
        running: bool,
    ) {
        for (_, tab) in self.state.dock_state.iter_all_tabs_mut() {
            if tab.selected_user.is_none()
                && tab.connection_name == connection_name
                && tab.database == database
                && tab.collection == collection
            {
                tab.agg_is_running = running;
                if running {
                    tab.agg_error = None;
                }
            }
        }
    }

    fn set_tab_schema_analyzing(
        &mut self,
        connection_name: &str,
        database: &str,
        collection: &str,
        analyzing: bool,
    ) {
        for (_, tab) in self.state.dock_state.iter_all_tabs_mut() {
            if tab.selected_user.is_none()
                && tab.connection_name == connection_name
                && tab.database == database
                && tab.collection == collection
            {
                tab.schema_is_analyzing = analyzing;
            }
        }
    }

    fn refresh_tab_columns_from_collection(
        &mut self,
        connection_name: &str,
        database: &str,
        collection: &str,
        total_count: Option<usize>,
    ) {
        let docs_and_count = self
            .state
            .find_collection_by_conn(connection_name, database, collection)
            .map(|c| (c.documents.clone(), c.stats.document_count));

        if let Some((docs, doc_count)) = docs_and_count {
            for (_, tab) in self.state.dock_state.iter_all_tabs_mut() {
                if tab.selected_user.is_none()
                    && tab.connection_name == connection_name
                    && tab.database == database
                    && tab.collection == collection
                {
                    tab.refresh_cached_columns(&docs);
                    tab.total_matched = Some(total_count.unwrap_or(doc_count));
                }
            }
        }
    }

    fn push_query_history(&mut self, ctx: &Context, query_cmd: String) {
        if self.state.bottom_panel.query_history.last() == Some(&query_cmd) {
            return;
        }
        self.state.bottom_panel.query_history.push(query_cmd);
        if self.state.bottom_panel.query_history.len() > MAX_QUERY_HISTORY {
            let excess = self.state.bottom_panel.query_history.len() - MAX_QUERY_HISTORY;
            self.state.bottom_panel.query_history.drain(0..excess);
        }
        self.spawn_save_query_store_task(ctx);
    }

    fn dispatch_live_query(
        &mut self,
        ctx: &Context,
        connection_name: String,
        database: String,
        collection: String,
        mut params: DocumentQueryParams,
    ) {
        params.limit = params.limit.clamp(1, 500);
        let query_cmd = format!(
            "db.getSiblingDB(\"{database}\").{collection}.find({})",
            params.filter
        );
        self.push_query_history(ctx, query_cmd.clone());

        let target = self.connection_target_by_name(&connection_name);
        if let Some(conn) = target.filter(|c| c.is_connected) {
            self.set_tab_loading(&connection_name, &database, &collection, true);

            let ssh_registry = self.ssh_registry.clone();
            let tx = self.bg_tx.clone();
            let ctx_clone = ctx.clone();
            self.runtime.spawn(async move {
                let result = match resolve_target_uri(&ssh_registry, &conn).await {
                    Ok(effective_uri) => DocumentService::query_documents(
                        &effective_uri,
                        &database,
                        &collection,
                        &params,
                        DEFAULT_CONNECT_TIMEOUT,
                    )
                    .await
                    .map_err(|err| err.to_string()),
                    Err(err) => Err(err),
                };

                // INTENTIONAL: receiver is dropped only on application shutdown.
                let _ = tx.send(AppEvent::QueryPageLoaded {
                    connection_name,
                    database,
                    collection,
                    skip: params.skip,
                    limit: params.limit,
                    result,
                });
                ctx_clone.request_repaint();
            });
            return;
        }

        // Disconnected / mock fallback: filter and paginate in-memory without mutating source rows.
        let effective_limit = params.limit;
        let skip = params.skip;
        let fallback_page = self
            .state
            .find_collection_by_conn(&connection_name, &database, &collection)
            .map(|coll| {
                let filtered =
                    DocumentService::filter_documents(&coll.documents, &params.filter, usize::MAX);
                let total_count = filtered.len();
                let page_docs: Vec<DocumentRow> = filtered
                    .into_iter()
                    .skip(skip)
                    .take(effective_limit)
                    .collect();
                (total_count, page_docs)
            });

        if let Some((total_count, page_docs)) = fallback_page {
            let returned = page_docs.len();
            for (_, tab) in self.state.dock_state.iter_all_tabs_mut() {
                if tab.selected_user.is_none()
                    && tab.connection_name == connection_name
                    && tab.database == database
                    && tab.collection == collection
                {
                    tab.is_loading = false;
                    tab.skip = skip;
                    tab.limit = effective_limit;
                    tab.total_matched = Some(total_count);
                    tab.last_query_ms = Some(0.1);
                    tab.refresh_cached_columns(&page_docs);
                    tab.selected_row = if returned == 0 { None } else { Some(0) };
                }
            }
            self.state.bottom_panel.push_log(
                LogLevel::Success,
                "QueryRunner",
                format!(
                    "{query_cmd} -> {returned} doc(s) (in-memory, total matched: {total_count})"
                ),
            );
        } else {
            self.set_tab_loading(&connection_name, &database, &collection, false);
            self.state.bottom_panel.push_log(
                LogLevel::Error,
                "QueryRunner",
                format!("Collection {database}.{collection} not found for connection '{connection_name}'."),
            );
        }
    }

    fn requery_open_collection(
        &mut self,
        ctx: &Context,
        connection_name: &str,
        database: &str,
        collection: &str,
    ) {
        let (filter, projection, sort, skip, limit) = self
            .tab_query_params(connection_name, database, collection)
            .unwrap_or_else(|| {
                (
                    "{}".to_string(),
                    "{}".to_string(),
                    "{\"_id\": 1}".to_string(),
                    0,
                    20,
                )
            });
        self.dispatch_live_query(
            ctx,
            connection_name.to_string(),
            database.to_string(),
            collection.to_string(),
            DocumentQueryParams {
                filter,
                projection,
                sort,
                skip,
                limit,
            },
        );
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

    fn spawn_save_query_store_task(&self, ctx: &Context) {
        let mut store = QueryStore {
            saved_queries: self.state.saved_queries.clone(),
            query_history: self.state.bottom_panel.query_history.clone(),
        };
        store.cap_history();
        let tx = self.bg_tx.clone();
        let ctx_clone = ctx.clone();
        self.runtime.spawn(async move {
            if let Err(err) = store.save_async().await {
                // INTENTIONAL: receiver is dropped only on application shutdown.
                let _ = tx.send(AppEvent::BackgroundTaskError {
                    source: "QueryStore".to_string(),
                    message: format!("Failed to persist saved queries: {err}"),
                });
                ctx_clone.request_repaint();
            }
        });
    }

    fn spawn_keyring_save_task(&self, ctx: &Context, conn_id: Uuid, password: String) {
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

    fn spawn_keyring_delete_task(&self, ctx: &Context, conn_id: Uuid) {
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

    fn spawn_ssh_keyring_save_task(&self, ctx: &Context, conn_id: Uuid, secret: String) {
        let tx = self.bg_tx.clone();
        let ctx_clone = ctx.clone();
        self.runtime.spawn(async move {
            if let Err(err) =
                ConnectionManager::save_ssh_secret_to_keyring_async(conn_id, secret).await
            {
                // INTENTIONAL: receiver is dropped only on application shutdown.
                let _ = tx.send(AppEvent::BackgroundTaskError {
                    source: "Keyring".to_string(),
                    message: format!("Failed to save SSH secret to OS Keychain: {err}"),
                });
                ctx_clone.request_repaint();
            }
        });
    }

    fn spawn_ssh_keyring_load_task(&self, ctx: &Context, index: usize, open_edit_modal: bool) {
        let Some(conn) = self.state.connections.get(index) else {
            return;
        };
        let conn_id = conn.id;
        let tx = self.bg_tx.clone();
        let ctx_clone = ctx.clone();
        self.runtime.spawn(async move {
            let result = ConnectionManager::load_ssh_secret_from_keyring_async(conn_id)
                .await
                .map_err(|err| err.to_string());
            // INTENTIONAL: receiver is dropped only on application shutdown.
            let _ = tx.send(AppEvent::SshSecretLoaded {
                index,
                open_edit_modal,
                result,
            });
            ctx_clone.request_repaint();
        });
    }

    fn spawn_ssh_keyring_delete_task(&self, ctx: &Context, conn_id: Uuid) {
        let tx = self.bg_tx.clone();
        let ctx_clone = ctx.clone();
        self.runtime.spawn(async move {
            if let Err(err) = ConnectionManager::delete_ssh_secret_from_keyring_async(conn_id).await
            {
                // INTENTIONAL: receiver is dropped only on application shutdown.
                let _ = tx.send(AppEvent::BackgroundTaskError {
                    source: "Keyring".to_string(),
                    message: format!("Failed to delete SSH secret from OS Keychain: {err}"),
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
        let conn_snapshot = conn.clone();
        let conn_id = conn_snapshot.id;
        let conn_name = conn_snapshot.name.clone();

        self.state.populate_connection_modal(index, &conn_snapshot);
        if let Some(secret) = self.ssh_session_secrets.get(&conn_id) {
            self.state.new_conn_ssh_secret = secret.clone();
        }

        self.state.bottom_panel.push_log(
            LogLevel::Info,
            "ConnectionManager",
            format!("Authentication required for '{conn_name}' — please complete credentials."),
        );
    }

    fn connect_profile_now(&mut self, ctx: &Context, index: usize) {
        let Some(conn) = self.state.connections.get(index) else {
            return;
        };
        let ssh_secret = self
            .ssh_session_secrets
            .get(&conn.id)
            .map(String::as_str)
            .unwrap_or("");
        if conn.needs_auth_prompt() || conn.ssh_tunnel.needs_secret_prompt(ssh_secret) {
            self.open_auth_prompt_for_profile(index);
            return;
        }

        if let Some(conn_mut) = self.state.connections.get_mut(index) {
            conn_mut.status = ConnectionStatus::Connecting;
            conn_mut.expanded = false;
            conn_mut.last_error = None;
        }
        self.state.active_connection_idx = index;

        let Some(target) = self.connection_target_by_index(index) else {
            return;
        };
        let redacted = ConnectionConfig::redact_uri(&target.uri);
        if target.ssh_tunnel.enabled {
            self.state.bottom_panel.push_log(
                LogLevel::Info,
                "ConnectionManager",
                format!(
                    "Connecting profile '{}' ({redacted}) via SSH tunnel ({})...",
                    target.name, target.ssh_tunnel.endpoint
                ),
            );
        } else {
            self.state.bottom_panel.push_log(
                LogLevel::Info,
                "ConnectionManager",
                format!("Connecting profile '{}' ({redacted})...", target.name),
            );
        }
        self.spawn_connect_task(ctx, target);
    }

    fn spawn_connect_task(&self, ctx: &Context, target: ConnectionTarget) {
        let ssh_registry = self.ssh_registry.clone();
        let tx = self.bg_tx.clone();
        let ctx_clone = ctx.clone();
        self.runtime.spawn(async move {
            let start = Instant::now();
            let outcome = match resolve_target_uri(&ssh_registry, &target).await {
                Ok(effective_uri) => ConnectionClient::connect_and_introspect(
                    &target.name,
                    &effective_uri,
                    DEFAULT_CONNECT_TIMEOUT,
                )
                .await
                .map_err(|err| err.to_string()),
                Err(err) => Err(err),
            };
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            let event = match outcome {
                Ok(conn) => AppEvent::ConnectionResult {
                    conn_id: target.id,
                    name: target.name,
                    uri: target.uri,
                    result: Ok((conn.databases, conn.users, conn.is_admin)),
                    latency_ms: Some(elapsed_ms),
                },
                Err(err) => AppEvent::ConnectionResult {
                    conn_id: target.id,
                    name: target.name,
                    uri: target.uri,
                    result: Err(err),
                    latency_ms: None,
                },
            };
            // INTENTIONAL: receiver is dropped only on application shutdown.
            let _ = tx.send(event);
            ctx_clone.request_repaint();
        });
    }

    fn spawn_ping_task(
        &self,
        ctx: &Context,
        uri: String,
        ssh_tunnel: SshTunnelConfig,
        ssh_secret: String,
        editing_conn: Option<(Uuid, String, bool, bool)>,
    ) {
        let cached_ssh_secret = editing_conn
            .as_ref()
            .and_then(|(id, _, _, _)| self.ssh_session_secrets.get(id).cloned())
            .unwrap_or_default();
        let tx = self.bg_tx.clone();
        let ctx_clone = ctx.clone();
        self.runtime.spawn(async move {
            let (base_uri, uri_user, uri_pass) = ConnectionConfig::split_uri_credentials(&uri);
            let effective_mongo_uri = if uri_pass.is_empty() {
                if let Some((conn_id, ref mongo_user, true, _)) = editing_conn {
                    match ConnectionManager::load_password_from_keyring_async(conn_id).await {
                        Ok(Some(pw)) => {
                            let effective_user = if mongo_user.is_empty() {
                                uri_user
                            } else {
                                mongo_user.clone()
                            };
                            ConnectionConfig::normalize_uri(&base_uri, &effective_user, &pw)
                        }
                        Ok(None) => uri.clone(),
                        Err(err) => {
                            let _ = tx.send(AppEvent::TestConnectionResult {
                                uri,
                                result: Err(format!(
                                    "Failed to load saved password from OS Keychain: {err}"
                                )),
                            });
                            ctx_clone.request_repaint();
                            return;
                        }
                    }
                } else {
                    uri.clone()
                }
            } else {
                uri.clone()
            };

            let result = if ssh_tunnel.enabled {
                let effective_secret = if !ssh_secret.is_empty() {
                    ssh_secret
                } else if !cached_ssh_secret.is_empty() {
                    cached_ssh_secret
                } else if ssh_tunnel.has_saved_secret {
                    if let Some((conn_id, _, _, true)) = editing_conn {
                        match ConnectionManager::load_ssh_secret_from_keyring_async(conn_id).await {
                            Ok(secret) => secret.unwrap_or_default(),
                            Err(err) => {
                                let _ = tx.send(AppEvent::TestConnectionResult {
                                    uri,
                                    result: Err(format!(
                                        "Failed to load saved SSH secret from OS Keychain: {err}"
                                    )),
                                });
                                ctx_clone.request_repaint();
                                return;
                            }
                        }
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                SshTunnelRegistry::ping_through_ssh_async(
                    effective_mongo_uri,
                    ssh_tunnel,
                    effective_secret,
                    DEFAULT_CONNECT_TIMEOUT,
                )
                .await
                .map_err(|e| e.to_string())
            } else {
                ConnectionClient::ping_uri(&effective_mongo_uri, DEFAULT_CONNECT_TIMEOUT)
                    .await
                    .map_err(|e| e.to_string())
            };
            // INTENTIONAL: receiver is dropped only on application shutdown.
            let _ = tx.send(AppEvent::TestConnectionResult { uri, result });
            ctx_clone.request_repaint();
        });
    }

    fn spawn_load_collection_task(
        &self,
        ctx: &Context,
        target: ConnectionTarget,
        database: String,
        collection: String,
    ) {
        let ssh_registry = self.ssh_registry.clone();
        let tx = self.bg_tx.clone();
        let ctx_clone = ctx.clone();
        self.runtime.spawn(async move {
            let result = match resolve_target_uri(&ssh_registry, &target).await {
                Ok(effective_uri) => ConnectionClient::introspect_single_collection(
                    &effective_uri,
                    &database,
                    &collection,
                    DEFAULT_CONNECT_TIMEOUT,
                )
                .await
                .map_err(|e| e.to_string()),
                Err(err) => Err(err),
            };
            // INTENTIONAL: receiver is dropped only on application shutdown.
            let _ = tx.send(AppEvent::CollectionLoaded {
                connection_name: target.name,
                database,
                collection,
                result,
            });
            ctx_clone.request_repaint();
        });
    }

    fn spawn_update_user_roles_task(&self, ctx: &Context, task: RoleUpdateTask) {
        let ssh_registry = self.ssh_registry.clone();
        let tx = self.bg_tx.clone();
        let ctx_clone = ctx.clone();
        self.runtime.spawn(async move {
            let outcome = match resolve_target_uri(&ssh_registry, &task.target).await {
                Ok(effective_uri) => UserService::update_user_roles_and_list(
                    &effective_uri,
                    &task.user_db,
                    &task.username,
                    &task.roles,
                    DEFAULT_CONNECT_TIMEOUT,
                )
                .await
                .map_err(|err| err.to_string()),
                Err(err) => Err(err),
            };

            let (message, result) = match outcome {
                Ok(users) => (task.success_message, Ok(users)),
                Err(err) => (task.failure_prefix, Err(err)),
            };
            // INTENTIONAL: receiver is dropped only on application shutdown.
            let _ = tx.send(AppEvent::UserOperationResult {
                connection_name: task.target.name,
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
                    self.refresh_tab_columns_from_collection(
                        &connection_name,
                        &database,
                        &collection,
                        None,
                    );

                    let conn_target = self.connection_target_by_name(&connection_name);
                    let needs_load = self
                        .state
                        .find_collection_by_conn(&connection_name, &database, &collection)
                        .is_some_and(|coll| !coll.loaded);

                    let is_connected = conn_target.as_ref().is_some_and(|t| t.is_connected);

                    if needs_load {
                        if let Some(target) = conn_target {
                            if target.is_connected {
                                self.set_tab_loading(
                                    &connection_name,
                                    &database,
                                    &collection,
                                    true,
                                );
                                self.state.bottom_panel.push_log(
                                    LogLevel::Info,
                                    "Explorer",
                                    format!("Lazy-loading collection {database}.{collection}..."),
                                );
                                self.spawn_load_collection_task(ctx, target, database, collection);
                                continue;
                            }
                        }
                    }

                    if is_connected {
                        self.requery_open_collection(ctx, &connection_name, &database, &collection);
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
                    self.refresh_tab_columns_from_collection(
                        &connection_name,
                        &database,
                        &collection,
                        None,
                    );
                    if let Some(target) = self.connection_target_by_name(&connection_name) {
                        if target.is_connected {
                            self.set_tab_loading(&connection_name, &database, &collection, true);
                            self.state.bottom_panel.push_log(
                                LogLevel::Info,
                                "Explorer",
                                format!("Refreshing collection {database}.{collection}..."),
                            );
                            self.spawn_load_collection_task(ctx, target, database, collection);
                        } else {
                            self.requery_open_collection(
                                ctx,
                                &connection_name,
                                &database,
                                &collection,
                            );
                        }
                    } else {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "Explorer",
                            format!("Connection '{connection_name}' not found."),
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
                            if let Some(db) = conn.databases.iter_mut().find(|d| d.name == database)
                            {
                                if let Some(slot) =
                                    db.collections.iter_mut().find(|c| c.name == collection)
                                {
                                    *slot = loaded_coll;
                                }
                            }
                        }
                        self.refresh_tab_columns_from_collection(
                            &connection_name,
                            &database,
                            &collection,
                            Some(doc_count),
                        );
                        self.set_tab_loading(&connection_name, &database, &collection, false);
                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "Explorer",
                            format!("Loaded {database}.{collection} ({doc_count} total documents)"),
                        );

                        if self
                            .tab_query_params(&connection_name, &database, &collection)
                            .is_some()
                            && self
                                .connection_target_by_name(&connection_name)
                                .is_some_and(|t| t.is_connected)
                        {
                            self.requery_open_collection(
                                ctx,
                                &connection_name,
                                &database,
                                &collection,
                            );
                        }
                    }
                    Err(err) => {
                        self.set_tab_loading(&connection_name, &database, &collection, false);
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

                    let Some(target) = self.connection_target_by_index(connection_index) else {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "DatabaseService",
                            "Connection profile not found.",
                        );
                        continue;
                    };

                    self.state.bottom_panel.push_log(
                        LogLevel::Info,
                        "DatabaseService",
                        format!(
                            "Creating database '{trimmed_db}' with initial collection '{trimmed_coll}'..."
                        ),
                    );

                    let ssh_registry = self.ssh_registry.clone();
                    let tx = self.bg_tx.clone();
                    let ctx_clone = ctx.clone();
                    self.runtime.spawn(async move {
                        let result = match resolve_target_uri(&ssh_registry, &target).await {
                            Ok(effective_uri) => DatabaseService::create_database_and_introspect(
                                &effective_uri,
                                &trimmed_db,
                                &trimmed_coll,
                                DEFAULT_CONNECT_TIMEOUT,
                            )
                            .await
                            .map_err(|err| err.to_string()),
                            Err(err) => Err(err),
                        };

                        // INTENTIONAL: receiver is dropped only on application shutdown.
                        let _ = tx.send(AppEvent::DatabaseCreated {
                            connection_name: target.name,
                            db_name: trimmed_db,
                            collection_name: trimmed_coll,
                            result,
                        });
                        ctx_clone.request_repaint();
                    });
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
                                    existing_db.collections.sort_by(|a, b| a.name.cmp(&b.name));
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
                        self.refresh_tab_columns_from_collection(
                            &connection_name,
                            &db_name,
                            &collection_name,
                            None,
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
                AppEvent::OpenCreateCollectionModal {
                    connection_index,
                    db_name,
                } => {
                    self.state.create_coll_conn_idx = connection_index;
                    self.state.create_coll_db_name = db_name;
                    self.state.new_coll_name.clear();
                    self.state.show_create_coll_modal = true;
                }
                AppEvent::CreateCollection {
                    connection_index,
                    db_name,
                    collection_name,
                } => {
                    let trimmed_db = db_name.trim().to_string();
                    let trimmed_coll = collection_name.trim().to_string();
                    if trimmed_db.is_empty() || trimmed_coll.is_empty() {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "CollectionService",
                            "Database name and collection name must not be empty.",
                        );
                        continue;
                    }
                    if trimmed_coll.starts_with("system.") {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "CollectionService",
                            format!("Cannot create protected system collection '{trimmed_coll}'."),
                        );
                        continue;
                    }

                    let Some(target) = self.connection_target_by_index(connection_index) else {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "CollectionService",
                            "Connection profile not found.",
                        );
                        continue;
                    };

                    self.state.bottom_panel.push_log(
                        LogLevel::Info,
                        "CollectionService",
                        format!("Creating collection '{trimmed_db}.{trimmed_coll}'..."),
                    );

                    let ssh_registry = self.ssh_registry.clone();
                    let tx = self.bg_tx.clone();
                    let ctx_clone = ctx.clone();
                    self.runtime.spawn(async move {
                        let result = match resolve_target_uri(&ssh_registry, &target).await {
                            Ok(effective_uri) => CollectionService::create_collection(
                                &effective_uri,
                                &trimmed_db,
                                &trimmed_coll,
                                DEFAULT_CONNECT_TIMEOUT,
                            )
                            .await
                            .map_err(|err| err.to_string()),
                            Err(err) => Err(err),
                        };

                        // INTENTIONAL: receiver is dropped only on application shutdown.
                        let _ = tx.send(AppEvent::CollectionCreated {
                            connection_name: target.name,
                            db_name: trimmed_db,
                            result,
                        });
                        ctx_clone.request_repaint();
                    });
                }
                AppEvent::CollectionCreated {
                    connection_name,
                    db_name,
                    result,
                } => match result {
                    Ok(coll) => {
                        let coll_name = coll.name.clone();
                        let doc_count = coll.stats.document_count;
                        if let Some(conn) = self
                            .state
                            .connections
                            .iter_mut()
                            .find(|c| c.name == connection_name)
                        {
                            if let Some(db) = conn.databases.iter_mut().find(|d| d.name == db_name)
                            {
                                db.expanded = true;
                                if let Some(slot) =
                                    db.collections.iter_mut().find(|c| c.name == coll_name)
                                {
                                    *slot = coll;
                                } else {
                                    db.collections.push(coll);
                                    db.collections.sort_by(|a, b| a.name.cmp(&b.name));
                                }
                            }
                        }

                        self.state
                            .open_or_focus_collection(&connection_name, &db_name, &coll_name);
                        self.refresh_tab_columns_from_collection(
                            &connection_name,
                            &db_name,
                            &coll_name,
                            Some(doc_count),
                        );
                        self.spawn_save_profiles_task(ctx);

                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "CollectionService",
                            format!("Created collection '{db_name}.{coll_name}'"),
                        );
                    }
                    Err(err) => {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "CollectionService",
                            format!("Failed to create collection in '{db_name}': {err}"),
                        );
                    }
                },
                AppEvent::RequestDropCollection {
                    connection_index,
                    db_name,
                    collection_name,
                } => {
                    if collection_name.trim().starts_with("system.") {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "CollectionService",
                            format!("Cannot drop protected system collection '{collection_name}'."),
                        );
                    } else {
                        self.state.pending_drop_coll =
                            Some((connection_index, db_name, collection_name));
                    }
                }
                AppEvent::ConfirmDropCollection {
                    connection_index,
                    db_name,
                    collection_name,
                } => {
                    if collection_name.trim().starts_with("system.") {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "CollectionService",
                            format!("Cannot drop protected system collection '{collection_name}'."),
                        );
                        continue;
                    }

                    let Some(target) = self.connection_target_by_index(connection_index) else {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "CollectionService",
                            "Connection profile not found.",
                        );
                        continue;
                    };

                    self.state.bottom_panel.push_log(
                        LogLevel::Info,
                        "CollectionService",
                        format!("Dropping collection '{db_name}.{collection_name}'..."),
                    );

                    let ssh_registry = self.ssh_registry.clone();
                    let tx = self.bg_tx.clone();
                    let ctx_clone = ctx.clone();
                    self.runtime.spawn(async move {
                        let result = match resolve_target_uri(&ssh_registry, &target).await {
                            Ok(effective_uri) => CollectionService::drop_collection(
                                &effective_uri,
                                &db_name,
                                &collection_name,
                                DEFAULT_CONNECT_TIMEOUT,
                            )
                            .await
                            .map_err(|err| err.to_string()),
                            Err(err) => Err(err),
                        };

                        // INTENTIONAL: receiver is dropped only on application shutdown.
                        let _ = tx.send(AppEvent::CollectionDropped {
                            connection_name: target.name,
                            db_name,
                            collection_name,
                            result,
                        });
                        ctx_clone.request_repaint();
                    });
                }
                AppEvent::CollectionDropped {
                    connection_name,
                    db_name,
                    collection_name,
                    result,
                } => match result {
                    Ok(()) => {
                        if let Some(conn) = self
                            .state
                            .connections
                            .iter_mut()
                            .find(|c| c.name == connection_name)
                        {
                            if let Some(db) = conn.databases.iter_mut().find(|d| d.name == db_name)
                            {
                                db.collections.retain(|c| c.name != collection_name);
                            }
                        }

                        let target_tab =
                            CollectionTab::new(&connection_name, &db_name, &collection_name);
                        if let Some(loc) = self.state.dock_state.find_tab(&target_tab) {
                            self.state.dock_state.remove_tab(loc);
                        }

                        self.spawn_save_profiles_task(ctx);
                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "CollectionService",
                            format!("Dropped collection '{db_name}.{collection_name}'"),
                        );
                    }
                    Err(err) => {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "CollectionService",
                            format!(
                                "Failed to drop collection '{db_name}.{collection_name}': {err}"
                            ),
                        );
                    }
                },
                AppEvent::RequestDropDatabase {
                    connection_index,
                    db_name,
                } => {
                    if is_system_database_name(&db_name) {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "DatabaseService",
                            format!("Cannot drop protected system database '{db_name}'."),
                        );
                    } else {
                        self.state.pending_drop_db = Some((connection_index, db_name));
                    }
                }
                AppEvent::ConfirmDropDatabase {
                    connection_index,
                    db_name,
                } => {
                    if is_system_database_name(&db_name) {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "DatabaseService",
                            format!("Cannot drop protected system database '{db_name}'."),
                        );
                        continue;
                    }

                    let Some(target) = self.connection_target_by_index(connection_index) else {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "DatabaseService",
                            "Connection profile not found.",
                        );
                        continue;
                    };

                    self.state.bottom_panel.push_log(
                        LogLevel::Info,
                        "DatabaseService",
                        format!("Dropping database '{db_name}'..."),
                    );

                    let ssh_registry = self.ssh_registry.clone();
                    let tx = self.bg_tx.clone();
                    let ctx_clone = ctx.clone();
                    self.runtime.spawn(async move {
                        let result = match resolve_target_uri(&ssh_registry, &target).await {
                            Ok(effective_uri) => DatabaseService::drop_database(
                                &effective_uri,
                                &db_name,
                                DEFAULT_CONNECT_TIMEOUT,
                            )
                            .await
                            .map_err(|err| err.to_string()),
                            Err(err) => Err(err),
                        };

                        // INTENTIONAL: receiver is dropped only on application shutdown.
                        let _ = tx.send(AppEvent::DatabaseDropped {
                            connection_name: target.name,
                            db_name,
                            result,
                        });
                        ctx_clone.request_repaint();
                    });
                }
                AppEvent::DatabaseDropped {
                    connection_name,
                    db_name,
                    result,
                } => match result {
                    Ok(()) => {
                        if let Some(conn) = self
                            .state
                            .connections
                            .iter_mut()
                            .find(|c| c.name == connection_name)
                        {
                            conn.databases.retain(|d| d.name != db_name);
                        }

                        let tabs_to_close: Vec<CollectionTab> = self
                            .state
                            .dock_state
                            .iter_all_nodes()
                            .filter_map(|(_, node)| node.tabs())
                            .flatten()
                            .filter(|t| {
                                t.selected_user.is_none()
                                    && t.connection_name == connection_name
                                    && t.database == db_name
                            })
                            .cloned()
                            .collect();
                        for tab_key in tabs_to_close {
                            if let Some(loc) = self.state.dock_state.find_tab(&tab_key) {
                                self.state.dock_state.remove_tab(loc);
                            }
                        }

                        self.spawn_save_profiles_task(ctx);
                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "DatabaseService",
                            format!("Dropped database '{db_name}'"),
                        );
                    }
                    Err(err) => {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "DatabaseService",
                            format!("Failed to drop database '{db_name}': {err}"),
                        );
                    }
                },
                AppEvent::ExecuteLiveQuery {
                    connection_name,
                    database,
                    collection,
                    filter,
                    projection,
                    sort,
                    skip,
                    limit,
                } => {
                    self.dispatch_live_query(
                        ctx,
                        connection_name,
                        database,
                        collection,
                        DocumentQueryParams {
                            filter,
                            projection,
                            sort,
                            skip,
                            limit,
                        },
                    );
                }
                AppEvent::QueryPageLoaded {
                    connection_name,
                    database,
                    collection,
                    skip: _,
                    limit: _,
                    result,
                } => {
                    self.set_tab_loading(&connection_name, &database, &collection, false);
                    match result {
                        Ok(page) => {
                            let returned = page.documents.len();
                            if let Some(coll) = self.state.find_collection_by_conn_mut(
                                &connection_name,
                                &database,
                                &collection,
                            ) {
                                coll.documents = page.documents;
                                coll.stats.document_count = page.total_count;
                                coll.loaded = true;
                            }

                            let docs_ref = self
                                .state
                                .find_collection_by_conn(&connection_name, &database, &collection)
                                .map(|c| c.documents.clone())
                                .unwrap_or_default();

                            for (_, tab) in self.state.dock_state.iter_all_tabs_mut() {
                                if tab.selected_user.is_none()
                                    && tab.connection_name == connection_name
                                    && tab.database == database
                                    && tab.collection == collection
                                {
                                    tab.skip = page.skip;
                                    tab.limit = page.limit;
                                    tab.total_matched = Some(page.total_count);
                                    tab.last_query_ms = Some(page.query_ms);
                                    tab.refresh_cached_columns(&docs_ref);
                                    tab.selected_row = if returned == 0 { None } else { Some(0) };
                                }
                            }

                            self.state.bottom_panel.push_log(
                                LogLevel::Success,
                                "QueryRunner",
                                format!(
                                    "Query {database}.{collection} returned {returned} doc(s) (total matched: {}) in {:.1} ms",
                                    page.total_count, page.query_ms
                                ),
                            );
                        }
                        Err(err) => {
                            self.state.bottom_panel.push_log(
                                LogLevel::Error,
                                "QueryRunner",
                                format!("Query failed on {database}.{collection}: {err}"),
                            );
                        }
                    }
                }
                AppEvent::OpenInsertDocumentModal {
                    connection_name,
                    database,
                    collection,
                } => {
                    self.state.doc_editor_conn_name = connection_name;
                    self.state.doc_editor_db = database;
                    self.state.doc_editor_coll = collection;
                    self.state.doc_editor_original_id = None;
                    self.state.doc_editor_json = "{\n  \n}".to_string();
                    self.state.show_doc_editor_modal = true;
                }
                AppEvent::InsertDocument {
                    connection_name,
                    database,
                    collection,
                    json_text,
                } => {
                    let parsed_doc = match DocumentRow::parse_json_to_bson_doc(&json_text) {
                        Ok(doc) => doc,
                        Err(err) => {
                            self.state.bottom_panel.push_log(
                                LogLevel::Error,
                                "DocumentService",
                                format!("Invalid JSON document: {err}"),
                            );
                            continue;
                        }
                    };

                    if let Some(target) = self.connection_target_by_name(&connection_name) {
                        if target.is_connected {
                            self.state.bottom_panel.push_log(
                                LogLevel::Info,
                                "DocumentService",
                                format!("Inserting document into {database}.{collection}..."),
                            );

                            let ssh_registry = self.ssh_registry.clone();
                            let tx = self.bg_tx.clone();
                            let ctx_clone = ctx.clone();
                            self.runtime.spawn(async move {
                                let result = match resolve_target_uri(&ssh_registry, &target).await
                                {
                                    Ok(effective_uri) => DocumentService::insert_document(
                                        &effective_uri,
                                        &database,
                                        &collection,
                                        &json_text,
                                        DEFAULT_CONNECT_TIMEOUT,
                                    )
                                    .await
                                    .map_err(|err| err.to_string()),
                                    Err(err) => Err(err),
                                };

                                // INTENTIONAL: receiver is dropped only on application shutdown.
                                let _ = tx.send(AppEvent::DocumentInserted {
                                    connection_name: target.name,
                                    database,
                                    collection,
                                    result,
                                });
                                ctx_clone.request_repaint();
                            });
                            continue;
                        }
                    }

                    // Disconnected / mock fallback
                    let mut doc_with_id = parsed_doc;
                    if !doc_with_id.contains_key("_id") {
                        doc_with_id.insert("_id", Bson::ObjectId(ObjectId::new()));
                    }
                    let row = DocumentRow::from_bson_doc(&doc_with_id);
                    let inserted_id = row.id.clone();
                    if let Some(coll) = self.state.find_collection_by_conn_mut(
                        &connection_name,
                        &database,
                        &collection,
                    ) {
                        coll.documents.push(row);
                        coll.stats.document_count = coll.documents.len();
                    }
                    self.requery_open_collection(ctx, &connection_name, &database, &collection);
                    self.state.bottom_panel.push_log(
                        LogLevel::Success,
                        "DocumentService",
                        format!("Inserted document (_id: {inserted_id}) into {database}.{collection} (in-memory)"),
                    );
                }
                AppEvent::DocumentInserted {
                    connection_name,
                    database,
                    collection,
                    result,
                } => match result {
                    Ok(row) => {
                        if let Some(coll) = self.state.find_collection_by_conn_mut(
                            &connection_name,
                            &database,
                            &collection,
                        ) {
                            coll.stats.document_count = coll.stats.document_count.saturating_add(1);
                        }
                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "DocumentService",
                            format!(
                                "Inserted document (_id: {}) into {database}.{collection}",
                                row.id
                            ),
                        );
                        self.requery_open_collection(ctx, &connection_name, &database, &collection);
                        self.spawn_save_profiles_task(ctx);
                    }
                    Err(err) => {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "DocumentService",
                            format!(
                                "Failed to insert document into {database}.{collection}: {err}"
                            ),
                        );
                    }
                },
                AppEvent::OpenEditDocumentModal {
                    connection_name,
                    database,
                    collection,
                    original_id,
                    json_text,
                } => {
                    self.state.doc_editor_conn_name = connection_name;
                    self.state.doc_editor_db = database;
                    self.state.doc_editor_coll = collection;
                    self.state.doc_editor_original_id = Some(original_id);
                    self.state.doc_editor_json = json_text;
                    self.state.show_doc_editor_modal = true;
                }
                AppEvent::UpdateDocument {
                    connection_name,
                    database,
                    collection,
                    original_id,
                    json_text,
                } => {
                    let mut replacement = match DocumentRow::parse_json_to_bson_doc(&json_text) {
                        Ok(doc) => doc,
                        Err(err) => {
                            self.state.bottom_panel.push_log(
                                LogLevel::Error,
                                "DocumentService",
                                format!("Invalid JSON document: {err}"),
                            );
                            continue;
                        }
                    };
                    if let Some(provided_id) = replacement.get("_id") {
                        if *provided_id != original_id {
                            self.state.bottom_panel.push_log(
                                LogLevel::Error,
                                "DocumentService",
                                "Cannot modify immutable '_id' field when updating a document.",
                            );
                            continue;
                        }
                    } else {
                        replacement.insert("_id", original_id.clone());
                    }

                    if let Some(target) = self.connection_target_by_name(&connection_name) {
                        if target.is_connected {
                            self.state.bottom_panel.push_log(
                                LogLevel::Info,
                                "DocumentService",
                                format!("Updating document in {database}.{collection}..."),
                            );

                            let ssh_registry = self.ssh_registry.clone();
                            let tx = self.bg_tx.clone();
                            let ctx_clone = ctx.clone();
                            self.runtime.spawn(async move {
                                let result = match resolve_target_uri(&ssh_registry, &target).await
                                {
                                    Ok(effective_uri) => DocumentService::update_document(
                                        &effective_uri,
                                        &database,
                                        &collection,
                                        &original_id,
                                        &json_text,
                                        DEFAULT_CONNECT_TIMEOUT,
                                    )
                                    .await
                                    .map_err(|err| err.to_string()),
                                    Err(err) => Err(err),
                                };

                                // INTENTIONAL: receiver is dropped only on application shutdown.
                                let _ = tx.send(AppEvent::DocumentUpdated {
                                    connection_name: target.name,
                                    database,
                                    collection,
                                    result,
                                });
                                ctx_clone.request_repaint();
                            });
                            continue;
                        }
                    }

                    // Disconnected / mock fallback
                    let updated_row = DocumentRow::from_bson_doc(&replacement);
                    let updated_id = updated_row.id.clone();
                    let mut matched = false;
                    if let Some(coll) = self.state.find_collection_by_conn_mut(
                        &connection_name,
                        &database,
                        &collection,
                    ) {
                        if let Some(slot) = coll
                            .documents
                            .iter_mut()
                            .find(|d| d.extract_bson_id().ok().as_ref() == Some(&original_id))
                        {
                            *slot = updated_row;
                            matched = true;
                        }
                    }
                    if matched {
                        self.requery_open_collection(ctx, &connection_name, &database, &collection);
                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "DocumentService",
                            format!("Updated document (_id: {updated_id}) in {database}.{collection} (in-memory)"),
                        );
                    } else {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "DocumentService",
                            format!("Document not found in {database}.{collection}"),
                        );
                    }
                }
                AppEvent::DocumentUpdated {
                    connection_name,
                    database,
                    collection,
                    result,
                } => match result {
                    Ok(row) => {
                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "DocumentService",
                            format!(
                                "Updated document (_id: {}) in {database}.{collection}",
                                row.id
                            ),
                        );
                        self.requery_open_collection(ctx, &connection_name, &database, &collection);
                        self.spawn_save_profiles_task(ctx);
                    }
                    Err(err) => {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "DocumentService",
                            format!("Failed to update document in {database}.{collection}: {err}"),
                        );
                    }
                },
                AppEvent::RequestDeleteDocument {
                    connection_name,
                    database,
                    collection,
                    doc_id,
                    display_id,
                } => {
                    self.state.pending_delete_doc = Some(PendingDeleteDocument {
                        connection_name,
                        database,
                        collection,
                        doc_id,
                        display_id,
                    });
                }
                AppEvent::ConfirmDeleteDocument {
                    connection_name,
                    database,
                    collection,
                    doc_id,
                    display_id,
                } => {
                    if let Some(target) = self.connection_target_by_name(&connection_name) {
                        if target.is_connected {
                            self.state.bottom_panel.push_log(
                                LogLevel::Info,
                                "DocumentService",
                                format!(
                                    "Deleting document (_id: {display_id}) from {database}.{collection}..."
                                ),
                            );

                            let ssh_registry = self.ssh_registry.clone();
                            let tx = self.bg_tx.clone();
                            let ctx_clone = ctx.clone();
                            self.runtime.spawn(async move {
                                let result = match resolve_target_uri(&ssh_registry, &target).await
                                {
                                    Ok(effective_uri) => DocumentService::delete_document(
                                        &effective_uri,
                                        &database,
                                        &collection,
                                        &doc_id,
                                        DEFAULT_CONNECT_TIMEOUT,
                                    )
                                    .await
                                    .map_err(|err| err.to_string()),
                                    Err(err) => Err(err),
                                };

                                // INTENTIONAL: receiver is dropped only on application shutdown.
                                let _ = tx.send(AppEvent::DocumentDeleted {
                                    connection_name: target.name,
                                    database,
                                    collection,
                                    display_id,
                                    result,
                                });
                                ctx_clone.request_repaint();
                            });
                            continue;
                        }
                    }

                    // Disconnected / mock fallback
                    let mut deleted = false;
                    if let Some(coll) = self.state.find_collection_by_conn_mut(
                        &connection_name,
                        &database,
                        &collection,
                    ) {
                        let before = coll.documents.len();
                        coll.documents
                            .retain(|d| d.extract_bson_id().ok().as_ref() != Some(&doc_id));
                        deleted = coll.documents.len() < before;
                        coll.stats.document_count = coll.documents.len();
                    }
                    if deleted {
                        self.requery_open_collection(ctx, &connection_name, &database, &collection);
                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "DocumentService",
                            format!(
                                "Deleted document (_id: {display_id}) from {database}.{collection} (in-memory)"
                            ),
                        );
                    } else {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "DocumentService",
                            format!(
                                "Document (_id: {display_id}) not found in {database}.{collection}"
                            ),
                        );
                    }
                }
                AppEvent::DocumentDeleted {
                    connection_name,
                    database,
                    collection,
                    display_id,
                    result,
                } => match result {
                    Ok(()) => {
                        let current_page_len = self
                            .state
                            .find_collection_by_conn_mut(&connection_name, &database, &collection)
                            .map_or(0, |coll| {
                                coll.stats.document_count =
                                    coll.stats.document_count.saturating_sub(1);
                                coll.documents.len()
                            });

                        if current_page_len <= 1 {
                            for (_, tab) in self.state.dock_state.iter_all_tabs_mut() {
                                if tab.selected_user.is_none()
                                    && tab.connection_name == connection_name
                                    && tab.database == database
                                    && tab.collection == collection
                                    && tab.skip > 0
                                {
                                    tab.skip = tab.skip.saturating_sub(tab.limit.max(1));
                                }
                            }
                        }

                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "DocumentService",
                            format!(
                                "Deleted document (_id: {display_id}) from {database}.{collection}"
                            ),
                        );
                        self.requery_open_collection(ctx, &connection_name, &database, &collection);
                        self.spawn_save_profiles_task(ctx);
                    }
                    Err(err) => {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "DocumentService",
                            format!(
                                "Failed to delete document (_id: {display_id}) from {database}.{collection}: {err}"
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
                    if let Some(target) = self.connection_target_by_index(connection_index) {
                        let effective_roles = if roles.is_empty() {
                            vec![UserRole::new("read", "admin")]
                        } else {
                            roles
                        };
                        let role_count = effective_roles.len();

                        self.pending_open_user = Some((target.name.clone(), trimmed.clone()));
                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "UserManager",
                            format!(
                                "Creating MongoDB user '{trimmed}' with {role_count} assigned database role(s)..."
                            ),
                        );

                        let ssh_registry = self.ssh_registry.clone();
                        let tx = self.bg_tx.clone();
                        let ctx_clone = ctx.clone();
                        self.runtime.spawn(async move {
                            let res = match resolve_target_uri(&ssh_registry, &target).await {
                                Ok(effective_uri) => UserService::create_user_and_list(
                                    &effective_uri,
                                    &trimmed,
                                    &password,
                                    &effective_roles,
                                    DEFAULT_CONNECT_TIMEOUT,
                                )
                                .await
                                .map_err(|err| err.to_string()),
                                Err(err) => Err(err),
                            };

                            let (message, result) = match res {
                                Ok(users) => (
                                    format!(
                                        "Created MongoDB user '{trimmed}' with {role_count} assigned database role(s)"
                                    ),
                                    Ok(users),
                                ),
                                Err(err) => (
                                    format!("Failed to create MongoDB user '{trimmed}'"),
                                    Err(err),
                                ),
                            };
                            // INTENTIONAL: receiver is dropped only on application shutdown.
                            let _ = tx.send(AppEvent::UserOperationResult {
                                connection_name: target.name,
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
                                        target: self.connection_target_from_config(conn),
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
                                        target: self.connection_target_from_config(conn),
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
                                    target: self.connection_target_from_config(conn),
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
                            format!(
                                "Saving {role_count} database role(s) for user '{username}'..."
                            ),
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
                    let change_ctx = self
                        .state
                        .connections
                        .iter()
                        .find(|c| c.name == connection_name)
                        .map(|conn| {
                            let user_db = conn
                                .users
                                .iter()
                                .find(|u| u.username == username)
                                .map_or_else(|| "admin".to_string(), |u| u.db.clone());
                            (self.connection_target_from_config(conn), user_db)
                        });
                    if let Some((target, user_db)) = change_ctx {
                        let uname = username.clone();

                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "UserManager",
                            format!("Updating password for user '{username}'..."),
                        );

                        let ssh_registry = self.ssh_registry.clone();
                        let tx = self.bg_tx.clone();
                        let ctx_clone = ctx.clone();
                        self.runtime.spawn(async move {
                            let result = match resolve_target_uri(&ssh_registry, &target).await {
                                Ok(effective_uri) => UserService::change_user_password(
                                    &effective_uri,
                                    &user_db,
                                    &uname,
                                    &new_password,
                                    DEFAULT_CONNECT_TIMEOUT,
                                )
                                .await
                                .map_err(|err| err.to_string()),
                                Err(err) => Err(err),
                            };

                            // INTENTIONAL: receiver is dropped only on application shutdown.
                            let _ = tx.send(AppEvent::UserPasswordChanged {
                                connection_name: target.name,
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
                            format!(
                                "Updated password for user '{username}' on '{connection_name}'"
                            ),
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
                    let delete_ctx = self.state.connections.get(connection_index).map(|conn| {
                        let user_db = conn
                            .users
                            .iter()
                            .find(|u| u.username == username)
                            .map_or_else(|| "admin".to_string(), |u| u.db.clone());
                        (self.connection_target_from_config(conn), user_db)
                    });
                    if let Some((target, user_db)) = delete_ctx {
                        let uname = username.clone();

                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "UserManager",
                            format!("Deleting MongoDB user '{username}'..."),
                        );

                        let ssh_registry = self.ssh_registry.clone();
                        let tx = self.bg_tx.clone();
                        let ctx_clone = ctx.clone();
                        self.runtime.spawn(async move {
                            let res = match resolve_target_uri(&ssh_registry, &target).await {
                                Ok(effective_uri) => UserService::drop_user_and_list(
                                    &effective_uri,
                                    &user_db,
                                    &uname,
                                    DEFAULT_CONNECT_TIMEOUT,
                                )
                                .await
                                .map_err(|err| err.to_string()),
                                Err(err) => Err(err),
                            };

                            let (message, result) = match res {
                                Ok(users) => (format!("Deleted MongoDB user '{uname}'"), Ok(users)),
                                Err(err) => {
                                    (format!("Failed to delete MongoDB user '{uname}'"), Err(err))
                                }
                            };
                            // INTENTIONAL: receiver is dropped only on application shutdown.
                            let _ = tx.send(AppEvent::UserOperationResult {
                                connection_name: target.name,
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
                AppEvent::RunAggregationPipeline {
                    connection_name,
                    database,
                    collection,
                    raw_json,
                    stages,
                } => {
                    let parsed_pipeline = if raw_json.trim().is_empty() {
                        AggregationStage::build_pipeline(&stages)
                    } else {
                        AggregationStage::parse_raw_pipeline(&raw_json)
                    };

                    let pipeline = match parsed_pipeline {
                        Ok(docs) => docs,
                        Err(err) => {
                            let err_msg = err.to_string();
                            for (_, tab) in self.state.dock_state.iter_all_tabs_mut() {
                                if tab.selected_user.is_none()
                                    && tab.connection_name == connection_name
                                    && tab.database == database
                                    && tab.collection == collection
                                {
                                    tab.agg_is_running = false;
                                    tab.agg_error = Some(err_msg.clone());
                                }
                            }
                            self.state.bottom_panel.push_log(
                                LogLevel::Error,
                                "Aggregation",
                                format!("Invalid pipeline for {database}.{collection}: {err_msg}"),
                            );
                            continue;
                        }
                    };

                    let stage_count = pipeline.len();
                    if let Some(target) = self.connection_target_by_name(&connection_name) {
                        if target.is_connected {
                            self.set_tab_agg_running(
                                &connection_name,
                                &database,
                                &collection,
                                true,
                            );
                            self.state.bottom_panel.push_log(
                                LogLevel::Info,
                                "Aggregation",
                                format!(
                                    "Running aggregation pipeline ({stage_count} stage(s)) on {database}.{collection}..."
                                ),
                            );

                            let ssh_registry = self.ssh_registry.clone();
                            let tx = self.bg_tx.clone();
                            let ctx_clone = ctx.clone();
                            self.runtime.spawn(async move {
                                let result = match resolve_target_uri(&ssh_registry, &target).await
                                {
                                    Ok(effective_uri) => DocumentService::run_aggregation(
                                        &effective_uri,
                                        &database,
                                        &collection,
                                        &pipeline,
                                        DEFAULT_CONNECT_TIMEOUT,
                                    )
                                    .await
                                    .map_err(|err| err.to_string()),
                                    Err(err) => Err(err),
                                };

                                // INTENTIONAL: receiver is dropped only on application shutdown.
                                let _ = tx.send(AppEvent::AggregationResultLoaded {
                                    connection_name: target.name,
                                    database,
                                    collection,
                                    stage_count,
                                    result,
                                });
                                ctx_clone.request_repaint();
                            });
                            continue;
                        }
                    }

                    // Disconnected / mock fallback
                    let fallback_stages: Vec<AggregationStage> = if raw_json.trim().is_empty() {
                        stages
                    } else {
                        pipeline
                            .iter()
                            .filter_map(|doc| {
                                doc.iter().next().map(|(op, val)| {
                                    let expr = match val {
                                        Bson::Document(sub) => serde_json::to_string(
                                            &Bson::Document(sub.clone()).into_relaxed_extjson(),
                                        )
                                        .unwrap_or_else(|_| "{}".to_string()),
                                        Bson::Int32(n) => n.to_string(),
                                        Bson::Int64(n) => n.to_string(),
                                        Bson::String(s) => s.clone(),
                                        other => other.to_string(),
                                    };
                                    AggregationStage::new(op.clone(), expr, true)
                                })
                            })
                            .collect()
                    };

                    let offline_res = self
                        .state
                        .find_collection_by_conn(&connection_name, &database, &collection)
                        .map(|coll| {
                            DocumentService::run_aggregation_in_memory(
                                &coll.documents,
                                &fallback_stages,
                            )
                        });

                    match offline_res {
                        Some(Ok(docs)) => {
                            let row_count = docs.len();
                            let cols = DocumentRow::extract_columns(&docs);
                            for (_, tab) in self.state.dock_state.iter_all_tabs_mut() {
                                if tab.selected_user.is_none()
                                    && tab.connection_name == connection_name
                                    && tab.database == database
                                    && tab.collection == collection
                                {
                                    tab.agg_is_running = false;
                                    tab.agg_error = None;
                                    tab.agg_elapsed_ms = Some(0.1);
                                    tab.agg_cached_columns = cols.clone();
                                    tab.agg_results = Some(docs.clone());
                                }
                            }
                            self.state.bottom_panel.push_log(
                                LogLevel::Success,
                                "Aggregation",
                                format!(
                                    "Aggregation on {database}.{collection} ({stage_count} stage(s)) returned {row_count} doc(s) (in-memory)"
                                ),
                            );
                        }
                        Some(Err(err)) => {
                            let err_msg = err.to_string();
                            for (_, tab) in self.state.dock_state.iter_all_tabs_mut() {
                                if tab.selected_user.is_none()
                                    && tab.connection_name == connection_name
                                    && tab.database == database
                                    && tab.collection == collection
                                {
                                    tab.agg_is_running = false;
                                    tab.agg_error = Some(err_msg.clone());
                                }
                            }
                            self.state.bottom_panel.push_log(
                                LogLevel::Error,
                                "Aggregation",
                                format!("Aggregation failed on {database}.{collection}: {err_msg}"),
                            );
                        }
                        None => {
                            self.set_tab_agg_running(
                                &connection_name,
                                &database,
                                &collection,
                                false,
                            );
                            self.state.bottom_panel.push_log(
                                LogLevel::Error,
                                "Aggregation",
                                format!("Collection {database}.{collection} not found."),
                            );
                        }
                    }
                }
                AppEvent::AggregationResultLoaded {
                    connection_name,
                    database,
                    collection,
                    stage_count,
                    result,
                } => {
                    self.set_tab_agg_running(&connection_name, &database, &collection, false);
                    match result {
                        Ok(res) => {
                            let row_count = res.documents.len();
                            let cols = DocumentRow::extract_columns(&res.documents);
                            for (_, tab) in self.state.dock_state.iter_all_tabs_mut() {
                                if tab.selected_user.is_none()
                                    && tab.connection_name == connection_name
                                    && tab.database == database
                                    && tab.collection == collection
                                {
                                    tab.agg_error = None;
                                    tab.agg_elapsed_ms = Some(res.elapsed_ms);
                                    tab.agg_cached_columns = cols.clone();
                                    tab.agg_results = Some(res.documents.clone());
                                }
                            }
                            self.state.bottom_panel.push_log(
                                LogLevel::Success,
                                "Aggregation",
                                format!(
                                    "Aggregation on {database}.{collection} ({stage_count} stage(s)) returned {row_count} doc(s) in {:.1} ms",
                                    res.elapsed_ms
                                ),
                            );
                        }
                        Err(err) => {
                            for (_, tab) in self.state.dock_state.iter_all_tabs_mut() {
                                if tab.selected_user.is_none()
                                    && tab.connection_name == connection_name
                                    && tab.database == database
                                    && tab.collection == collection
                                {
                                    tab.agg_error = Some(err.clone());
                                }
                            }
                            self.state.bottom_panel.push_log(
                                LogLevel::Error,
                                "Aggregation",
                                format!("Aggregation failed on {database}.{collection}: {err}"),
                            );
                        }
                    }
                }
                AppEvent::OpenCreateIndexModal {
                    connection_name,
                    database,
                    collection,
                } => {
                    self.state.create_index_conn_name = connection_name;
                    self.state.create_index_db = database;
                    self.state.create_index_coll = collection;
                    self.state.new_index_keys = "{\n  \"email\": 1\n}".to_string();
                    self.state.new_index_name.clear();
                    self.state.new_index_unique = false;
                    self.state.new_index_sparse = false;
                    self.state.new_index_background = false;
                    self.state.show_create_index_modal = true;
                }
                AppEvent::CreateIndex {
                    connection_name,
                    database,
                    collection,
                    request,
                } => {
                    let keys_doc =
                        match IndexService::validate_create_index(&database, &collection, &request)
                        {
                            Ok(doc) => doc,
                            Err(err) => {
                                self.state.bottom_panel.push_log(
                                    LogLevel::Error,
                                    "IndexService",
                                    format!("Invalid index request: {err}"),
                                );
                                continue;
                            }
                        };

                    if let Some(target) = self.connection_target_by_name(&connection_name) {
                        if target.is_connected {
                            self.state.bottom_panel.push_log(
                                LogLevel::Info,
                                "IndexService",
                                format!("Creating index on {database}.{collection}..."),
                            );

                            let ssh_registry = self.ssh_registry.clone();
                            let tx = self.bg_tx.clone();
                            let ctx_clone = ctx.clone();
                            self.runtime.spawn(async move {
                                let result = match resolve_target_uri(&ssh_registry, &target).await
                                {
                                    Ok(effective_uri) => IndexService::create_index(
                                        &effective_uri,
                                        &database,
                                        &collection,
                                        &request,
                                        DEFAULT_CONNECT_TIMEOUT,
                                    )
                                    .await
                                    .map_err(|err| err.to_string()),
                                    Err(err) => Err(err),
                                };

                                // INTENTIONAL: receiver is dropped only on application shutdown.
                                let _ = tx.send(AppEvent::IndexesUpdated {
                                    connection_name: target.name,
                                    database,
                                    collection,
                                    message: "Created index".to_string(),
                                    result,
                                });
                                ctx_clone.request_repaint();
                            });
                            continue;
                        }
                    }

                    // Disconnected / mock fallback
                    let idx_name = request
                        .name
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .unwrap_or_else(|| {
                            keys_doc
                                .iter()
                                .map(|(k, v)| format!("{k}_{v}"))
                                .collect::<Vec<_>>()
                                .join("_")
                        });
                    let index_type = if keys_doc
                        .values()
                        .any(|v| matches!(v, Bson::String(s) if s == "text"))
                    {
                        "Text".to_string()
                    } else if keys_doc
                        .values()
                        .any(|v| matches!(v, Bson::String(s) if s == "2dsphere" || s == "2d"))
                    {
                        "2dsphere".to_string()
                    } else if keys_doc
                        .values()
                        .any(|v| matches!(v, Bson::String(s) if s == "hashed"))
                    {
                        "Hashed".to_string()
                    } else if keys_doc.len() > 1 {
                        "Compound".to_string()
                    } else {
                        "B-Tree".to_string()
                    };

                    if let Some(coll) = self.state.find_collection_by_conn_mut(
                        &connection_name,
                        &database,
                        &collection,
                    ) {
                        coll.indexes.push(IndexInfo {
                            name: idx_name.clone(),
                            keys: format!("{}", Bson::Document(keys_doc)),
                            index_type,
                            unique: request.unique,
                            size_kb: 8.0,
                        });
                        coll.stats.index_count = coll.indexes.len();
                    }
                    self.state.bottom_panel.push_log(
                        LogLevel::Success,
                        "IndexService",
                        format!(
                            "Created index '{idx_name}' on {database}.{collection} (in-memory)"
                        ),
                    );
                }
                AppEvent::RequestDropIndex {
                    connection_name,
                    database,
                    collection,
                    index_name,
                } => {
                    if collection.trim().starts_with("system.") {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "IndexService",
                            format!("Cannot drop indexes on protected system collection '{collection}'."),
                        );
                        continue;
                    }
                    if let Err(err) = IndexService::validate_drop_index(&index_name) {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "IndexService",
                            err.to_string(),
                        );
                        continue;
                    }
                    self.state.pending_drop_index = Some(PendingDropIndex {
                        connection_name,
                        database,
                        collection,
                        index_name,
                    });
                }
                AppEvent::ConfirmDropIndex {
                    connection_name,
                    database,
                    collection,
                    index_name,
                } => {
                    if collection.trim().starts_with("system.") {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "IndexService",
                            format!("Cannot drop indexes on protected system collection '{collection}'."),
                        );
                        continue;
                    }
                    if let Err(err) = IndexService::validate_drop_index(&index_name) {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "IndexService",
                            err.to_string(),
                        );
                        continue;
                    }

                    if let Some(target) = self.connection_target_by_name(&connection_name) {
                        if target.is_connected {
                            self.state.bottom_panel.push_log(
                                LogLevel::Info,
                                "IndexService",
                                format!(
                                    "Dropping index '{index_name}' on {database}.{collection}..."
                                ),
                            );

                            let ssh_registry = self.ssh_registry.clone();
                            let tx = self.bg_tx.clone();
                            let ctx_clone = ctx.clone();
                            let idx_clone = index_name.clone();
                            self.runtime.spawn(async move {
                                let result = match resolve_target_uri(&ssh_registry, &target).await
                                {
                                    Ok(effective_uri) => IndexService::drop_index(
                                        &effective_uri,
                                        &database,
                                        &collection,
                                        &idx_clone,
                                        DEFAULT_CONNECT_TIMEOUT,
                                    )
                                    .await
                                    .map_err(|err| err.to_string()),
                                    Err(err) => Err(err),
                                };

                                // INTENTIONAL: receiver is dropped only on application shutdown.
                                let _ = tx.send(AppEvent::IndexesUpdated {
                                    connection_name: target.name,
                                    database,
                                    collection,
                                    message: format!("Dropped index '{idx_clone}'"),
                                    result,
                                });
                                ctx_clone.request_repaint();
                            });
                            continue;
                        }
                    }

                    // Disconnected / mock fallback
                    if let Some(coll) = self.state.find_collection_by_conn_mut(
                        &connection_name,
                        &database,
                        &collection,
                    ) {
                        coll.indexes.retain(|idx| idx.name != index_name);
                        coll.stats.index_count = coll.indexes.len();
                    }
                    self.state.bottom_panel.push_log(
                        LogLevel::Success,
                        "IndexService",
                        format!(
                            "Dropped index '{index_name}' from {database}.{collection} (in-memory)"
                        ),
                    );
                }
                AppEvent::IndexesUpdated {
                    connection_name,
                    database,
                    collection,
                    message,
                    result,
                } => {
                    match result {
                        Ok(indexes) => {
                            let count = indexes.len();
                            if let Some(coll) = self.state.find_collection_by_conn_mut(
                                &connection_name,
                                &database,
                                &collection,
                            ) {
                                coll.indexes = indexes;
                                coll.stats.index_count = count;
                            }
                            self.spawn_save_profiles_task(ctx);
                            self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "IndexService",
                            format!("{message} on {database}.{collection} ({count} index(es) total)"),
                        );
                        }
                        Err(err) => {
                            self.state.bottom_panel.push_log(
                                LogLevel::Error,
                                "IndexService",
                                format!("{message} on {database}.{collection} failed: {err}"),
                            );
                        }
                    }
                }
                AppEvent::AnalyzeCollectionSchema {
                    connection_name,
                    database,
                    collection,
                    sample_size,
                } => {
                    self.set_tab_schema_analyzing(&connection_name, &database, &collection, true);

                    if let Some(target) = self.connection_target_by_name(&connection_name) {
                        if target.is_connected {
                            self.state.bottom_panel.push_log(
                                LogLevel::Info,
                                "SchemaAnalyzer",
                                format!(
                                    "Analyzing schema for {database}.{collection} (sample size: {sample_size})..."
                                ),
                            );

                            let ssh_registry = self.ssh_registry.clone();
                            let tx = self.bg_tx.clone();
                            let ctx_clone = ctx.clone();
                            self.runtime.spawn(async move {
                                let result = match resolve_target_uri(&ssh_registry, &target).await
                                {
                                    Ok(effective_uri) => SchemaAnalyzer::analyze_collection(
                                        &effective_uri,
                                        &database,
                                        &collection,
                                        sample_size,
                                        DEFAULT_CONNECT_TIMEOUT,
                                    )
                                    .await
                                    .map_err(|err| err.to_string()),
                                    Err(err) => Err(err),
                                };

                                // INTENTIONAL: receiver is dropped only on application shutdown.
                                let _ = tx.send(AppEvent::SchemaAnalyzed {
                                    connection_name: target.name,
                                    database,
                                    collection,
                                    sample_size,
                                    result,
                                });
                                ctx_clone.request_repaint();
                            });
                            continue;
                        }
                    }

                    // Disconnected / mock fallback
                    let fields_opt = self
                        .state
                        .find_collection_by_conn(&connection_name, &database, &collection)
                        .map(|coll| {
                            let bson_docs: Vec<Document> = coll
                                .documents
                                .iter()
                                .take(sample_size.clamp(10, 2000))
                                .filter_map(|row| {
                                    DocumentRow::parse_json_to_bson_doc(&row.to_pretty_json()).ok()
                                })
                                .collect();
                            SchemaAnalyzer::analyze_bson_docs(&bson_docs)
                        });

                    self.set_tab_schema_analyzing(&connection_name, &database, &collection, false);
                    if let Some(fields) = fields_opt {
                        let field_count = fields.len();
                        if let Some(coll) = self.state.find_collection_by_conn_mut(
                            &connection_name,
                            &database,
                            &collection,
                        ) {
                            coll.schema_fields = fields;
                        }
                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "SchemaAnalyzer",
                            format!(
                                "Analyzed schema for {database}.{collection} ({field_count} field(s), in-memory)"
                            ),
                        );
                    } else {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "SchemaAnalyzer",
                            format!("Collection {database}.{collection} not found."),
                        );
                    }
                }
                AppEvent::SchemaAnalyzed {
                    connection_name,
                    database,
                    collection,
                    sample_size,
                    result,
                } => {
                    self.set_tab_schema_analyzing(&connection_name, &database, &collection, false);
                    match result {
                        Ok(fields) => {
                            let field_count = fields.len();
                            if let Some(coll) = self.state.find_collection_by_conn_mut(
                                &connection_name,
                                &database,
                                &collection,
                            ) {
                                coll.schema_fields = fields;
                            }
                            self.state.bottom_panel.push_log(
                                LogLevel::Success,
                                "SchemaAnalyzer",
                                format!(
                                    "Analyzed schema for {database}.{collection} ({field_count} field(s) from sample size {sample_size})"
                                ),
                            );
                        }
                        Err(err) => {
                            self.state.bottom_panel.push_log(
                                LogLevel::Error,
                                "SchemaAnalyzer",
                                format!(
                                    "Schema analysis failed for {database}.{collection}: {err}"
                                ),
                            );
                        }
                    }
                }
                AppEvent::OpenSaveQueryModal {
                    connection_name,
                    database,
                    collection,
                    filter,
                    projection,
                    sort,
                    limit,
                } => {
                    self.state.save_query_name = format!("{collection}_query");
                    self.state.save_query_conn = connection_name;
                    self.state.save_query_db = database;
                    self.state.save_query_coll = collection;
                    self.state.save_query_filter = filter;
                    self.state.save_query_projection = projection;
                    self.state.save_query_sort = sort;
                    self.state.save_query_limit = limit.clamp(1, 500);
                    self.state.show_save_query_modal = true;
                }
                AppEvent::SaveQuery { query } => {
                    if query.name.trim().is_empty() {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "QueryStore",
                            "Saved query name must not be empty.",
                        );
                        continue;
                    }
                    let qname = query.name.clone();
                    self.state.saved_queries.push(query);
                    self.spawn_save_query_store_task(ctx);
                    self.state.bottom_panel.push_log(
                        LogLevel::Success,
                        "QueryStore",
                        format!("Saved query '{qname}'"),
                    );
                }
                AppEvent::DeleteSavedQuery { id } => {
                    let before = self.state.saved_queries.len();
                    self.state.saved_queries.retain(|q| q.id != id);
                    if self.state.saved_queries.len() < before {
                        self.spawn_save_query_store_task(ctx);
                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "QueryStore",
                            "Deleted saved query",
                        );
                    }
                }
                AppEvent::RunSavedQuery { query } => {
                    self.state.open_or_focus_collection(
                        &query.connection_name,
                        &query.database,
                        &query.collection,
                    );
                    let limit = query.limit.clamp(1, 500);
                    for (_, tab) in self.state.dock_state.iter_all_tabs_mut() {
                        if tab.selected_user.is_none()
                            && tab.connection_name == query.connection_name
                            && tab.database == query.database
                            && tab.collection == query.collection
                        {
                            tab.filter_query.clone_from(&query.filter);
                            tab.projection_query.clone_from(&query.projection);
                            tab.sort_query.clone_from(&query.sort);
                            tab.skip = 0;
                            tab.limit = limit;
                            tab.active_subtab = CollectionSubTab::Documents;
                        }
                    }

                    self.dispatch_live_query(
                        ctx,
                        query.connection_name,
                        query.database,
                        query.collection,
                        DocumentQueryParams {
                            filter: query.filter,
                            projection: query.projection,
                            sort: query.sort,
                            skip: 0,
                            limit,
                        },
                    );
                }
                AppEvent::RunHistoryQuery { command } => {
                    if let Some((parsed_db, coll, filter)) = parse_history_query(&command) {
                        let fallback_tab_target = self
                            .state
                            .dock_state
                            .iter_all_nodes()
                            .filter_map(|(_, node)| node.tabs())
                            .flatten()
                            .find(|t| t.selected_user.is_none())
                            .map(|t| (t.connection_name.clone(), t.database.clone()));

                        let db = parsed_db
                            .or_else(|| fallback_tab_target.as_ref().map(|(_, d)| d.clone()))
                            .unwrap_or_else(|| self.state.shell_current_db.clone());

                        let conn_name = fallback_tab_target
                            .and_then(|(c, _)| {
                                self.state
                                    .find_collection_by_conn(&c, &db, &coll)
                                    .map(|_| c)
                            })
                            .or_else(|| {
                                self.state
                                    .connections
                                    .get(self.state.active_connection_idx)
                                    .map(|c| c.name.clone())
                            })
                            .unwrap_or_else(|| "MongoDB Local".to_string());

                        self.state.open_or_focus_collection(&conn_name, &db, &coll);

                        for (_, tab) in self.state.dock_state.iter_all_tabs_mut() {
                            if tab.selected_user.is_none()
                                && tab.connection_name == conn_name
                                && tab.database == db
                                && tab.collection == coll
                            {
                                tab.filter_query.clone_from(&filter);
                                tab.skip = 0;
                                tab.active_subtab = CollectionSubTab::Documents;
                            }
                        }

                        self.requery_open_collection(ctx, &conn_name, &db, &coll);
                    } else {
                        self.state.bottom_panel.terminal_input = command;
                        self.state.bottom_panel.active_tab = BottomPanelTab::Terminal;
                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "QueryHistory",
                            "Loaded history command into Terminal input.",
                        );
                    }
                }
                AppEvent::ExecuteShellCommand { command } => {
                    let trimmed = command.trim().to_string();
                    if trimmed.is_empty() {
                        continue;
                    }
                    self.state.bottom_panel.terminal_history.push(format!(
                        "mongosh [{}]> {trimmed}",
                        self.state.shell_current_db
                    ));

                    let new_current_db = ShellService::extract_use_db(&trimmed);
                    let active_target = self
                        .connection_target_by_index(self.state.active_connection_idx)
                        .filter(|t| t.is_connected);

                    if let Some(target) = active_target {
                        let current_db = self.state.shell_current_db.clone();
                        let ssh_registry = self.ssh_registry.clone();
                        let tx = self.bg_tx.clone();
                        let ctx_clone = ctx.clone();
                        self.runtime.spawn(async move {
                            let result = match resolve_target_uri(&ssh_registry, &target).await {
                                Ok(effective_uri) => ShellService::execute_command(
                                    &effective_uri,
                                    &current_db,
                                    &trimmed,
                                    DEFAULT_CONNECT_TIMEOUT,
                                )
                                .await
                                .map_err(|err| err.to_string()),
                                Err(err) => Err(err),
                            };

                            // INTENTIONAL: receiver is dropped only on application shutdown.
                            let _ = tx.send(AppEvent::ShellCommandCompleted {
                                command: trimmed,
                                new_current_db,
                                result,
                            });
                            ctx_clone.request_repaint();
                        });
                        continue;
                    }

                    // Disconnected / mock fallback
                    let databases = self
                        .state
                        .connections
                        .get(self.state.active_connection_idx)
                        .map(|c| c.databases.as_slice())
                        .unwrap_or(&[]);

                    match ShellService::execute_mock_command(
                        &self.state.shell_current_db,
                        databases,
                        &trimmed,
                    ) {
                        Ok(output) => {
                            if let Some(db) = new_current_db {
                                self.state.shell_current_db = db;
                            }
                            self.state.bottom_panel.push_terminal_lines(&output);
                        }
                        Err(err) => {
                            self.state
                                .bottom_panel
                                .push_terminal_lines(&format!("Error: {err}"));
                        }
                    }
                }
                AppEvent::ShellCommandCompleted {
                    command: _,
                    new_current_db,
                    result,
                } => match result {
                    Ok(output) => {
                        if let Some(db) = new_current_db {
                            self.state.shell_current_db = db;
                        }
                        self.state.bottom_panel.push_terminal_lines(&output);
                    }
                    Err(err) => {
                        self.state
                            .bottom_panel
                            .push_terminal_lines(&format!("Error: {err}"));
                    }
                },
                AppEvent::ExportCollectionData {
                    connection_name,
                    database,
                    collection,
                    filter,
                    format,
                    path,
                } => {
                    if let Some(target) = self.connection_target_by_name(&connection_name) {
                        if target.is_connected {
                            self.state.is_transferring = true;
                            self.state.bottom_panel.push_log(
                                LogLevel::Info,
                                "TransferService",
                                format!(
                                    "Exporting {database}.{collection} as {} to '{}'...",
                                    format.label(),
                                    path.display()
                                ),
                            );

                            let ssh_registry = self.ssh_registry.clone();
                            let tx = self.bg_tx.clone();
                            let ctx_clone = ctx.clone();
                            self.runtime.spawn(async move {
                                let result = match resolve_target_uri(&ssh_registry, &target).await
                                {
                                    Ok(effective_uri) => TransferService::export_collection(
                                        &effective_uri,
                                        &database,
                                        &collection,
                                        &filter,
                                        format,
                                        path,
                                        DEFAULT_CONNECT_TIMEOUT,
                                    )
                                    .await
                                    .map_err(|err| err.to_string()),
                                    Err(err) => Err(err),
                                };

                                // INTENTIONAL: receiver is dropped only on application shutdown.
                                let _ = tx.send(AppEvent::ExportCompleted {
                                    message: format!("Export {database}.{collection}"),
                                    result,
                                });
                                ctx_clone.request_repaint();
                            });
                            continue;
                        }
                    }

                    // Disconnected / mock fallback (run file I/O on Tokio blocking pool)
                    let Some(coll) = self.state.find_collection_by_conn(
                        &connection_name,
                        &database,
                        &collection,
                    ) else {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "TransferService",
                            format!("Collection {database}.{collection} not found."),
                        );
                        continue;
                    };

                    let filtered =
                        DocumentService::filter_documents(&coll.documents, &filter, usize::MAX);
                    self.state.is_transferring = true;
                    let tx = self.bg_tx.clone();
                    let ctx_clone = ctx.clone();
                    self.runtime.spawn(async move {
                        let start = Instant::now();
                        let path_clone = path.clone();
                        let res = tokio::task::spawn_blocking(move || {
                            TransferService::export_documents_to_path(
                                &filtered,
                                format,
                                &path_clone,
                            )
                        })
                        .await
                        .map_err(|err| err.to_string())
                        .and_then(|inner| inner.map_err(|err| err.to_string()));

                        let result = res.map(|count| ExportStats {
                            count,
                            path,
                            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
                        });

                        // INTENTIONAL: receiver is dropped only on application shutdown.
                        let _ = tx.send(AppEvent::ExportCompleted {
                            message: format!("Export {database}.{collection} (in-memory)"),
                            result,
                        });
                        ctx_clone.request_repaint();
                    });
                }
                AppEvent::ExportCompleted { message, result } => {
                    self.state.is_transferring = false;
                    match result {
                        Ok(stats) => {
                            self.state.bottom_panel.push_log(
                                LogLevel::Success,
                                "TransferService",
                                format!(
                                    "{message} completed: {} document(s) written to '{}' in {:.1} ms",
                                    stats.count,
                                    stats.path.display(),
                                    stats.elapsed_ms
                                ),
                            );
                        }
                        Err(err) => {
                            self.state.bottom_panel.push_log(
                                LogLevel::Error,
                                "TransferService",
                                format!("{message} failed: {err}"),
                            );
                        }
                    }
                }
                AppEvent::ImportCollectionData {
                    connection_name,
                    database,
                    collection,
                    format,
                    path,
                } => {
                    if collection.trim().starts_with("system.") {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "TransferService",
                            format!(
                                "Cannot import into protected system collection '{collection}'."
                            ),
                        );
                        continue;
                    }

                    let Some(target) = self
                        .connection_target_by_name(&connection_name)
                        .filter(|t| t.is_connected)
                    else {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "TransferService",
                            "Import requires an active MongoDB connection.",
                        );
                        continue;
                    };

                    self.state.is_transferring = true;
                    self.state.bottom_panel.push_log(
                        LogLevel::Info,
                        "TransferService",
                        format!(
                            "Importing {} into {database}.{collection} from '{}'...",
                            format.label(),
                            path.display()
                        ),
                    );

                    let ssh_registry = self.ssh_registry.clone();
                    let tx = self.bg_tx.clone();
                    let ctx_clone = ctx.clone();
                    self.runtime.spawn(async move {
                        let result = match resolve_target_uri(&ssh_registry, &target).await {
                            Ok(effective_uri) => TransferService::import_collection(
                                &effective_uri,
                                &database,
                                &collection,
                                format,
                                path,
                                DEFAULT_CONNECT_TIMEOUT,
                            )
                            .await
                            .map_err(|err| err.to_string()),
                            Err(err) => Err(err),
                        };

                        // INTENTIONAL: receiver is dropped only on application shutdown.
                        let _ = tx.send(AppEvent::ImportCompleted {
                            connection_name: target.name,
                            database,
                            collection,
                            result,
                        });
                        ctx_clone.request_repaint();
                    });
                }
                AppEvent::ImportCompleted {
                    connection_name,
                    database,
                    collection,
                    result,
                } => {
                    self.state.is_transferring = false;
                    match result {
                        Ok(stats) => {
                            if let Some(coll) = self.state.find_collection_by_conn_mut(
                                &connection_name,
                                &database,
                                &collection,
                            ) {
                                coll.stats.document_count = coll
                                    .stats
                                    .document_count
                                    .saturating_add(stats.inserted_count);
                            }
                            self.state.bottom_panel.push_log(
                                LogLevel::Success,
                                "TransferService",
                                format!(
                                    "Imported {} document(s) into {database}.{collection} from '{}' in {:.1} ms",
                                    stats.inserted_count,
                                    stats.path.display(),
                                    stats.elapsed_ms
                                ),
                            );
                            if self
                                .tab_query_params(&connection_name, &database, &collection)
                                .is_some()
                            {
                                self.requery_open_collection(
                                    ctx,
                                    &connection_name,
                                    &database,
                                    &collection,
                                );
                            }
                            self.spawn_save_profiles_task(ctx);
                        }
                        Err(err) => {
                            self.state.bottom_panel.push_log(
                                LogLevel::Error,
                                "TransferService",
                                format!("Import into {database}.{collection} failed: {err}"),
                            );
                        }
                    }
                }
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
                    self.state.reset_connection_modal_fields();
                    self.state.show_new_connection_modal = true;
                }
                AppEvent::OpenEditConnectionModal { index } => {
                    if let Some(conn) = self.state.connections.get(index).cloned() {
                        let (_base_uri, _uri_user, uri_pass) =
                            ConnectionConfig::split_uri_credentials(&conn.uri);
                        let needs_mongo_keyring = conn.has_saved_password && uri_pass.is_empty();
                        let cached_ssh_secret = self.ssh_session_secrets.get(&conn.id).cloned();
                        let needs_ssh_keyring = conn.ssh_tunnel.enabled
                            && conn.ssh_tunnel.has_saved_secret
                            && cached_ssh_secret.as_deref().is_none_or(str::is_empty);

                        self.state.populate_connection_modal(index, &conn);
                        if let Some(secret) = cached_ssh_secret {
                            self.state.new_conn_ssh_secret = secret;
                        }

                        if needs_mongo_keyring {
                            self.spawn_keyring_load_task(ctx, index, true);
                        }
                        if needs_ssh_keyring {
                            self.spawn_ssh_keyring_load_task(ctx, index, true);
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
                                    format!(
                                        "Failed to load saved password from OS Keychain: {err}"
                                    ),
                                );
                            }
                        }
                    } else {
                        let mut prompt_needed = false;
                        match result {
                            Ok(Some(pw)) if !pw.is_empty() => {
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
                            }
                            Ok(Some(_)) | Ok(None) => {
                                let mut auth_missing = false;
                                if let Some(conn) = self.state.connections.get_mut(index) {
                                    if conn.requires_auth {
                                        conn.has_saved_password = false;
                                        conn.status = ConnectionStatus::Disconnected;
                                        auth_missing = true;
                                        prompt_needed = conn.needs_auth_prompt();
                                    }
                                }
                                if auth_missing {
                                    self.spawn_save_profiles_task(ctx);
                                }
                            }
                            Err(err) => {
                                self.state.bottom_panel.push_log(
                                    LogLevel::Error,
                                    "Keyring",
                                    format!(
                                        "Failed to load saved password from OS Keychain: {err}"
                                    ),
                                );
                                prompt_needed =
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
                            }
                        }

                        if prompt_needed {
                            self.open_auth_prompt_for_profile(index);
                            continue;
                        }

                        let needs_ssh_keyring =
                            self.state.connections.get(index).is_some_and(|c| {
                                c.ssh_tunnel.enabled
                                    && c.ssh_tunnel.has_saved_secret
                                    && self
                                        .ssh_session_secrets
                                        .get(&c.id)
                                        .is_none_or(String::is_empty)
                            });
                        if needs_ssh_keyring {
                            self.spawn_ssh_keyring_load_task(ctx, index, false);
                        } else {
                            self.connect_profile_now(ctx, index);
                        }
                    }
                }
                AppEvent::SshSecretLoaded {
                    index,
                    open_edit_modal,
                    result,
                } => {
                    if open_edit_modal {
                        match result {
                            Ok(Some(pw)) => {
                                if self.state.editing_connection_idx == Some(index) {
                                    self.state.new_conn_ssh_secret = pw;
                                }
                            }
                            Ok(None) => {}
                            Err(err) => {
                                self.state.bottom_panel.push_log(
                                    LogLevel::Error,
                                    "Keyring",
                                    format!(
                                        "Failed to load saved SSH secret from OS Keychain: {err}"
                                    ),
                                );
                            }
                        }
                    } else {
                        match result {
                            Ok(Some(secret)) if !secret.is_empty() => {
                                if let Some(conn) = self.state.connections.get(index) {
                                    self.ssh_session_secrets.insert(conn.id, secret);
                                }
                                self.connect_profile_now(ctx, index);
                            }
                            Ok(Some(_)) | Ok(None) => {
                                let mut cleared_stale = false;
                                let mut needs_prompt = false;
                                if let Some(conn) = self.state.connections.get_mut(index) {
                                    if conn.ssh_tunnel.has_saved_secret {
                                        conn.ssh_tunnel.has_saved_secret = false;
                                        cleared_stale = true;
                                    }
                                    let session_secret = self
                                        .ssh_session_secrets
                                        .get(&conn.id)
                                        .map(String::as_str)
                                        .unwrap_or("");
                                    if conn.needs_auth_prompt()
                                        || conn.ssh_tunnel.needs_secret_prompt(session_secret)
                                    {
                                        conn.status = ConnectionStatus::Disconnected;
                                        needs_prompt = true;
                                    }
                                }
                                if cleared_stale {
                                    self.spawn_save_profiles_task(ctx);
                                }
                                if needs_prompt {
                                    self.open_auth_prompt_for_profile(index);
                                } else {
                                    self.connect_profile_now(ctx, index);
                                }
                            }
                            Err(err) => {
                                self.state.bottom_panel.push_log(
                                    LogLevel::Error,
                                    "Keyring",
                                    format!(
                                        "Failed to load saved SSH secret from OS Keychain: {err}"
                                    ),
                                );
                                let needs_prompt =
                                    self.state.connections.get_mut(index).is_some_and(|conn| {
                                        let has_session_secret = self
                                            .ssh_session_secrets
                                            .get(&conn.id)
                                            .is_some_and(|s| !s.is_empty());
                                        let ssh_needs_pwd = conn.ssh_tunnel.enabled
                                            && conn.ssh_tunnel.auth_method
                                                == SshAuthMethod::Password
                                            && !has_session_secret;
                                        if ssh_needs_pwd || conn.needs_auth_prompt() {
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
                AppEvent::TestConnection {
                    name: _,
                    uri,
                    ssh_tunnel,
                    ssh_secret,
                } => {
                    self.state.is_testing_connection = true;
                    self.state.test_conn_status = None;
                    let redacted = ConnectionConfig::redact_uri(&uri);
                    if ssh_tunnel.enabled {
                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "ConnectionClient",
                            format!(
                                "Pinging MongoDB server via SSH tunnel ({})...",
                                ssh_tunnel.endpoint
                            ),
                        );
                    } else {
                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "ConnectionClient",
                            format!("Pinging MongoDB server at '{redacted}'..."),
                        );
                    }
                    let editing_conn = self
                        .state
                        .editing_connection_idx
                        .and_then(|idx| self.state.connections.get(idx))
                        .map(|conn| {
                            (
                                conn.id,
                                conn.username.clone(),
                                conn.has_saved_password,
                                conn.ssh_tunnel.has_saved_secret,
                            )
                        });
                    self.spawn_ping_task(ctx, uri, ssh_tunnel, ssh_secret, editing_conn);
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
                AppEvent::AddConnection {
                    name,
                    uri,
                    ssh_tunnel,
                    ssh_secret,
                } => match ConnectionConfig::new_profile(&name, &uri) {
                    Ok(mut conn) => {
                        let password_to_save = if self.state.new_conn_save_password
                            && !self.state.new_conn_password.is_empty()
                        {
                            conn.has_saved_password = true;
                            Some(self.state.new_conn_password.clone())
                        } else {
                            None
                        };

                        let mut final_ssh = ssh_tunnel;
                        let ssh_secret_to_save = if final_ssh.enabled
                            && self.state.new_conn_ssh_save_secret
                            && !ssh_secret.is_empty()
                        {
                            final_ssh.has_saved_secret = true;
                            Some(ssh_secret.clone())
                        } else {
                            final_ssh.has_saved_secret = false;
                            None
                        };
                        conn.ssh_tunnel = final_ssh;

                        let conn_id = conn.id;
                        let conn_name = conn.name.clone();
                        let conn_uri = conn.uri.clone();
                        let ssh_enabled = conn.ssh_tunnel.enabled;
                        let ssh_endpoint = conn.ssh_tunnel.endpoint.clone();
                        let redacted = ConnectionConfig::redact_uri(&conn_uri);

                        if ssh_enabled && !ssh_secret.is_empty() {
                            self.ssh_session_secrets.insert(conn_id, ssh_secret);
                        }

                        self.state.connections.push(conn);
                        let new_idx = self.state.connections.len().saturating_sub(1);
                        self.state.active_connection_idx = new_idx;

                        if let Some(password) = password_to_save {
                            self.spawn_keyring_save_task(ctx, conn_id, password);
                        }
                        if let Some(secret) = ssh_secret_to_save {
                            self.spawn_ssh_keyring_save_task(ctx, conn_id, secret);
                        }
                        self.spawn_save_profiles_task(ctx);

                        if ssh_enabled {
                            self.state.bottom_panel.push_log(
                                    LogLevel::Info,
                                    "ConnectionManager",
                                    format!(
                                        "Connecting to '{conn_name}' ({redacted}) via SSH tunnel ({ssh_endpoint})..."
                                    ),
                                );
                        } else {
                            self.state.bottom_panel.push_log(
                                LogLevel::Info,
                                "ConnectionManager",
                                format!("Connecting to '{conn_name}' ({redacted})..."),
                            );
                        }

                        if let Some(target) = self.connection_target_by_index(new_idx) {
                            self.spawn_connect_task(ctx, target);
                        }
                        self.state.new_conn_password.clear();
                        self.state.new_conn_ssh_secret.clear();
                    }
                    Err(err) => {
                        self.state.bottom_panel.push_log(
                            LogLevel::Error,
                            "ConnectionManager",
                            format!("Invalid connection config: {err}"),
                        );
                    }
                },
                AppEvent::UpdateConnection {
                    index,
                    name,
                    uri,
                    ssh_tunnel,
                    ssh_secret,
                } => {
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

                        let prev_ssh_saved = conn.ssh_tunnel.has_saved_secret;
                        let mut final_ssh = ssh_tunnel;
                        let ssh_keyring_op = if final_ssh.enabled
                            && self.state.new_conn_ssh_save_secret
                            && !ssh_secret.is_empty()
                        {
                            final_ssh.has_saved_secret = true;
                            Some(Some(ssh_secret.clone()))
                        } else if (!final_ssh.enabled || !self.state.new_conn_ssh_save_secret)
                            && prev_ssh_saved
                        {
                            final_ssh.has_saved_secret = false;
                            Some(None)
                        } else {
                            final_ssh.has_saved_secret = final_ssh.enabled && prev_ssh_saved;
                            None
                        };
                        let ssh_enabled = final_ssh.enabled;
                        conn.ssh_tunnel = final_ssh;

                        (
                            conn.id,
                            conn.name.clone(),
                            conn.uri.clone(),
                            keyring_op,
                            ssh_keyring_op,
                            ssh_enabled,
                            prev_ssh_saved,
                        )
                    });

                    if let Some((
                        conn_id,
                        conn_name,
                        conn_uri,
                        keyring_op,
                        ssh_keyring_op,
                        ssh_enabled,
                        prev_ssh_saved,
                    )) = update_outcome
                    {
                        self.ssh_registry.close_tunnel(conn_id);
                        if !ssh_enabled {
                            self.ssh_session_secrets.remove(&conn_id);
                        } else if !ssh_secret.is_empty() {
                            self.ssh_session_secrets.insert(conn_id, ssh_secret);
                        } else if !prev_ssh_saved || !self.state.new_conn_ssh_save_secret {
                            self.ssh_session_secrets.remove(&conn_id);
                        }

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
                        match ssh_keyring_op {
                            Some(Some(secret)) => {
                                self.spawn_ssh_keyring_save_task(ctx, conn_id, secret);
                            }
                            Some(None) => {
                                self.spawn_ssh_keyring_delete_task(ctx, conn_id);
                            }
                            None => {}
                        }
                        self.spawn_save_profiles_task(ctx);

                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "ConnectionManager",
                            format!("Updated profile '{conn_name}' ({redacted}), reconnecting..."),
                        );

                        if let Some(target) = self.connection_target_by_index(index) {
                            self.spawn_connect_task(ctx, target);
                        }
                    }
                    self.state.new_conn_password.clear();
                    self.state.new_conn_ssh_secret.clear();
                }
                AppEvent::ConnectProfile { index } => {
                    let Some(conn) = self.state.connections.get(index) else {
                        continue;
                    };
                    let needs_mongo_keyring = conn.has_saved_password
                        && ConnectionConfig::split_uri_credentials(&conn.uri)
                            .2
                            .is_empty();
                    let needs_ssh_keyring = conn.ssh_tunnel.enabled
                        && conn.ssh_tunnel.has_saved_secret
                        && self
                            .ssh_session_secrets
                            .get(&conn.id)
                            .is_none_or(String::is_empty);

                    if needs_mongo_keyring || needs_ssh_keyring {
                        if let Some(conn_mut) = self.state.connections.get_mut(index) {
                            conn_mut.status = ConnectionStatus::Connecting;
                            conn_mut.expanded = false;
                            conn_mut.last_error = None;
                        }
                        if needs_mongo_keyring {
                            self.spawn_keyring_load_task(ctx, index, false);
                        } else {
                            self.spawn_ssh_keyring_load_task(ctx, index, false);
                        }
                    } else {
                        self.connect_profile_now(ctx, index);
                    }
                }
                AppEvent::DisconnectProfile { index } => {
                    let disconnected = self.state.connections.get_mut(index).map(|conn| {
                        conn.status = ConnectionStatus::Disconnected;
                        conn.expanded = false;
                        conn.last_error = None;
                        if !conn.has_saved_password {
                            conn.clear_session_password();
                        }
                        (conn.id, conn.name.clone(), conn.ssh_tunnel.has_saved_secret)
                    });
                    if let Some((conn_id, name, has_saved_ssh)) = disconnected {
                        self.ssh_registry.close_tunnel(conn_id);
                        if !has_saved_ssh {
                            self.ssh_session_secrets.remove(&conn_id);
                        }
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
                        self.ssh_registry.close_tunnel(removed.id);
                        self.ssh_session_secrets.remove(&removed.id);
                        if self.state.active_connection_idx >= self.state.connections.len()
                            && !self.state.connections.is_empty()
                        {
                            self.state.active_connection_idx = self.state.connections.len() - 1;
                        }
                        self.spawn_keyring_delete_task(ctx, removed.id);
                        self.spawn_ssh_keyring_delete_task(ctx, removed.id);
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
                        let conn_name = conn.name.clone();
                        self.state.bottom_panel.push_log(
                            LogLevel::Info,
                            "Explorer",
                            format!("Refreshing databases and collections for '{conn_name}'..."),
                        );
                    }
                    if let Some(target) = self.connection_target_by_index(active_idx) {
                        self.spawn_connect_task(ctx, target);
                    }
                }
                AppEvent::ExecuteFilterQuery {
                    database,
                    collection,
                    filter,
                } => {
                    let mut target_conn = None;
                    for (_, tab) in self.state.dock_state.iter_all_tabs_mut() {
                        if tab.selected_user.is_none()
                            && tab.database == database
                            && tab.collection == collection
                        {
                            tab.filter_query = filter.clone();
                            tab.skip = 0;
                            target_conn = Some(tab.connection_name.clone());
                            break;
                        }
                    }
                    if let Some(conn_name) = target_conn {
                        self.requery_open_collection(ctx, &conn_name, &database, &collection);
                    } else {
                        let query_cmd =
                            format!("db.getSiblingDB(\"{database}\").{collection}.find({filter})");
                        self.push_query_history(ctx, query_cmd.clone());
                        self.state.bottom_panel.push_log(
                            LogLevel::Success,
                            "QueryRunner",
                            query_cmd,
                        );
                    }
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
                            cached_json: None,
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
                    self.ssh_registry.close_all();
                    self.ssh_session_secrets.clear();
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
        let mut store = QueryStore {
            saved_queries: self.state.saved_queries.clone(),
            query_history: self.state.bottom_panel.query_history.clone(),
        };
        store.cap_history();
        if let Err(err) = store.save() {
            tracing::warn!(error = %err, "failed to save query store on shutdown");
        }
    }
}
