use bson::Bson;
use egui_dock::DockState;
use uuid::Uuid;

use crate::models::{
    CollectionInfo, ConnectionConfig, MongoUser, QueryStore, QuickAccessFilter, SavedQuery,
    SshAuthMethod, SshTunnelConfig, UserRole,
};
use crate::theme::ThemeMode;
use crate::ui::activity_bar::ActivityView;
use crate::workspace::{BottomPanelState, CollectionTab, WorkspacePersistence};

#[derive(Debug, Clone, PartialEq)]
pub struct PendingDeleteDocument {
    pub connection_name: String,
    pub database: String,
    pub collection: String,
    pub doc_id: Bson,
    pub display_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDropIndex {
    pub connection_name: String,
    pub database: String,
    pub collection: String,
    pub index_name: String,
}

pub struct AppState {
    pub connections: Vec<ConnectionConfig>,
    pub active_connection_idx: usize,
    pub active_activity: ActivityView,
    pub sidebar_visible: bool,
    pub sidebar_search: String,
    pub dock_state: DockState<CollectionTab>,
    pub bottom_panel: BottomPanelState,
    pub theme_mode: ThemeMode,
    pub command_palette_open: bool,
    pub command_palette_query: String,
    pub show_new_connection_modal: bool,
    pub editing_connection_idx: Option<usize>,
    pub new_conn_name: String,
    pub new_conn_uri: String,
    pub new_conn_username: String,
    pub new_conn_password: String,
    pub new_conn_save_password: bool,
    pub new_conn_ssh_enabled: bool,
    pub new_conn_ssh_endpoint: String,
    pub new_conn_ssh_username: String,
    pub new_conn_ssh_auth_method: SshAuthMethod,
    pub new_conn_ssh_key_path: String,
    pub new_conn_ssh_secret: String,
    pub new_conn_ssh_save_secret: bool,
    pub test_conn_status: Option<Result<String, String>>,
    pub is_testing_connection: bool,
    pub show_create_db_modal: bool,
    pub create_db_conn_idx: usize,
    pub new_db_name: String,
    pub new_db_collection: String,
    pub show_create_coll_modal: bool,
    pub create_coll_conn_idx: usize,
    pub create_coll_db_name: String,
    pub new_coll_name: String,
    pub pending_drop_db: Option<(usize, String)>,
    pub pending_drop_coll: Option<(usize, String, String)>,
    pub show_doc_editor_modal: bool,
    pub doc_editor_conn_name: String,
    pub doc_editor_db: String,
    pub doc_editor_coll: String,
    pub doc_editor_original_id: Option<Bson>,
    pub doc_editor_json: String,
    pub pending_delete_doc: Option<PendingDeleteDocument>,
    pub show_create_user_modal: bool,
    pub create_user_conn_idx: usize,
    pub duplicate_source_user: Option<String>,
    pub duplicate_source_user_db: Option<String>,
    pub duplicate_source_roles: Vec<UserRole>,
    pub new_user_username: String,
    pub new_user_password: String,
    pub new_user_confirm_password: String,
    pub new_user_show_password: bool,
    pub new_user_db: String,
    pub new_user_role: String,
    pub show_change_password_modal: bool,
    pub change_pwd_conn_name: String,
    pub change_pwd_username: String,
    pub change_password_user_db: String,
    pub change_pwd_new: String,
    pub change_pwd_confirm: String,
    pub change_pwd_show: bool,
    pub pending_delete_user: Option<(usize, String)>,
    pub delete_user_target_db: String,
    pub saved_queries: Vec<SavedQuery>,
    pub show_create_index_modal: bool,
    pub create_index_conn_name: String,
    pub create_index_db: String,
    pub create_index_coll: String,
    pub new_index_keys: String,
    pub new_index_name: String,
    pub new_index_unique: bool,
    pub new_index_sparse: bool,
    pub new_index_background: bool,
    pub pending_drop_index: Option<PendingDropIndex>,
    pub show_save_query_modal: bool,
    pub save_query_name: String,
    pub save_query_conn: String,
    pub save_query_db: String,
    pub save_query_coll: String,
    pub save_query_filter: String,
    pub save_query_projection: String,
    pub save_query_sort: String,
    pub save_query_limit: usize,
    pub shell_current_db: String,
    pub is_transferring: bool,
    pub show_about_modal: bool,
    pub active_workspace_filter: Option<String>,
    pub quick_access_filter: QuickAccessFilter,
    pub recent_connections: Vec<Uuid>,
    pub show_manage_workspaces_modal: bool,
    pub new_workspace_input: String,
    pub new_folder_input: String,
    pub new_folder_workspace_target: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self::from_persistence(WorkspacePersistence::default())
    }
}

impl AppState {
    #[must_use]
    pub fn from_persistence(persisted: WorkspacePersistence) -> Self {
        #[cfg(any(debug_assertions, test))]
        let initial_tabs = if persisted.open_tabs.is_empty() {
            vec![
                CollectionTab::new("MongoDB Local", "admin", "users"),
                CollectionTab::new("MongoDB Local", "admin", "products"),
            ]
        } else {
            persisted.open_tabs
        };
        #[cfg(not(any(debug_assertions, test)))]
        let initial_tabs = persisted.open_tabs;

        let query_store = QueryStore::load();
        let mut bottom_panel = BottomPanelState {
            visible: persisted.bottom_panel_visible,
            ..BottomPanelState::default()
        };
        if !query_store.query_history.is_empty() {
            bottom_panel.query_history = query_store.query_history;
        }

        #[cfg(any(debug_assertions, test))]
        let connections = vec![
            ConnectionConfig::mock_local(),
            ConnectionConfig::mock_staging(),
        ];
        #[cfg(not(any(debug_assertions, test)))]
        let connections = Vec::new();

        Self {
            connections,
            active_connection_idx: 0,
            active_activity: persisted.active_activity,
            sidebar_visible: persisted.sidebar_visible,
            sidebar_search: String::new(),
            dock_state: DockState::new(initial_tabs),
            bottom_panel,
            theme_mode: persisted.theme_mode,
            command_palette_open: false,
            command_palette_query: String::new(),
            show_new_connection_modal: false,
            editing_connection_idx: None,
            new_conn_name: "Production ReplicaSet".to_string(),
            new_conn_uri: "mongodb://localhost:27017".to_string(),
            new_conn_username: String::new(),
            new_conn_password: String::new(),
            new_conn_save_password: false,
            new_conn_ssh_enabled: false,
            new_conn_ssh_endpoint: String::new(),
            new_conn_ssh_username: String::new(),
            new_conn_ssh_auth_method: SshAuthMethod::Password,
            new_conn_ssh_key_path: SshTunnelConfig::DEFAULT_PRIVATE_KEY_PATH.to_string(),
            new_conn_ssh_secret: String::new(),
            new_conn_ssh_save_secret: false,
            test_conn_status: None,
            is_testing_connection: false,
            show_create_db_modal: false,
            create_db_conn_idx: 0,
            new_db_name: String::new(),
            new_db_collection: "users".to_string(),
            show_create_coll_modal: false,
            create_coll_conn_idx: 0,
            create_coll_db_name: String::new(),
            new_coll_name: String::new(),
            pending_drop_db: None,
            pending_drop_coll: None,
            show_doc_editor_modal: false,
            doc_editor_conn_name: String::new(),
            doc_editor_db: String::new(),
            doc_editor_coll: String::new(),
            doc_editor_original_id: None,
            doc_editor_json: String::new(),
            pending_delete_doc: None,
            show_create_user_modal: false,
            create_user_conn_idx: 0,
            duplicate_source_user: None,
            duplicate_source_user_db: None,
            duplicate_source_roles: Vec::new(),
            new_user_username: String::new(),
            new_user_password: String::new(),
            new_user_confirm_password: String::new(),
            new_user_show_password: false,
            new_user_db: "admin".to_string(),
            new_user_role: "readWrite".to_string(),
            show_change_password_modal: false,
            change_pwd_conn_name: String::new(),
            change_pwd_username: String::new(),
            change_password_user_db: "admin".to_string(),
            change_pwd_new: String::new(),
            change_pwd_confirm: String::new(),
            change_pwd_show: false,
            pending_delete_user: None,
            delete_user_target_db: "admin".to_string(),
            saved_queries: query_store.saved_queries,
            show_create_index_modal: false,
            create_index_conn_name: String::new(),
            create_index_db: String::new(),
            create_index_coll: String::new(),
            new_index_keys: String::new(),
            new_index_name: String::new(),
            new_index_unique: false,
            new_index_sparse: false,
            new_index_background: false,
            pending_drop_index: None,
            show_save_query_modal: false,
            save_query_name: String::new(),
            save_query_conn: String::new(),
            save_query_db: String::new(),
            save_query_coll: String::new(),
            save_query_filter: String::new(),
            save_query_projection: String::new(),
            save_query_sort: String::new(),
            save_query_limit: 20,
            shell_current_db: "admin".to_string(),
            is_transferring: false,
            show_about_modal: false,
            active_workspace_filter: None,
            quick_access_filter: QuickAccessFilter::All,
            recent_connections: Vec::new(),
            show_manage_workspaces_modal: false,
            new_workspace_input: String::new(),
            new_folder_input: String::new(),
            new_folder_workspace_target: String::new(),
        }
    }

    /// Resets all New/Edit Connection modal fields (including SSH tunnel fields) to their defaults.
    pub fn reset_connection_modal_fields(&mut self) {
        self.editing_connection_idx = None;
        self.new_conn_name = "Production ReplicaSet".to_string();
        self.new_conn_uri = "mongodb://localhost:27017".to_string();
        self.new_conn_username.clear();
        self.new_conn_password.clear();
        self.new_conn_save_password = false;
        self.new_conn_ssh_enabled = false;
        self.new_conn_ssh_endpoint.clear();
        self.new_conn_ssh_username.clear();
        self.new_conn_ssh_auth_method = SshAuthMethod::Password;
        self.new_conn_ssh_key_path = SshTunnelConfig::DEFAULT_PRIVATE_KEY_PATH.to_string();
        self.new_conn_ssh_secret.clear();
        self.new_conn_ssh_save_secret = false;
        self.test_conn_status = None;
        self.is_testing_connection = false;
    }

    /// Populates the New/Edit Connection modal fields from an existing [`ConnectionConfig`].
    pub fn populate_connection_modal(&mut self, index: usize, conn: &ConnectionConfig) {
        let (base_uri, uri_user, uri_pass) = ConnectionConfig::split_uri_credentials(&conn.uri);
        self.editing_connection_idx = Some(index);
        self.new_conn_name = conn.name.clone();
        self.new_conn_uri = base_uri;
        self.new_conn_username = if conn.username.is_empty() {
            uri_user
        } else {
            conn.username.clone()
        };
        self.new_conn_password = uri_pass;
        self.new_conn_save_password = conn.has_saved_password;

        self.new_conn_ssh_enabled = conn.ssh_tunnel.enabled;
        self.new_conn_ssh_endpoint = conn.ssh_tunnel.endpoint.clone();
        self.new_conn_ssh_username = conn.ssh_tunnel.username.clone();
        self.new_conn_ssh_auth_method = conn.ssh_tunnel.auth_method;
        self.new_conn_ssh_key_path = if conn.ssh_tunnel.private_key_path.trim().is_empty() {
            SshTunnelConfig::DEFAULT_PRIVATE_KEY_PATH.to_string()
        } else {
            conn.ssh_tunnel.private_key_path.clone()
        };
        self.new_conn_ssh_secret.clear();
        self.new_conn_ssh_save_secret = conn.ssh_tunnel.has_saved_secret;

        self.test_conn_status = None;
        self.is_testing_connection = false;
        self.show_new_connection_modal = true;
    }

    /// Builds an [`SshTunnelConfig`] from the current modal form fields, preserving an existing
    /// saved keyring secret flag when editing and `new_conn_ssh_save_secret` remains enabled.
    #[must_use]
    pub fn build_ssh_tunnel_config(&self) -> SshTunnelConfig {
        let previous_has_saved = self
            .editing_connection_idx
            .and_then(|idx| self.connections.get(idx))
            .is_some_and(|conn| conn.ssh_tunnel.has_saved_secret);
        self.build_ssh_tunnel_config_with_saved(previous_has_saved)
    }

    /// Builds an [`SshTunnelConfig`] from the current modal form fields using an explicit
    /// `previous_has_saved` flag.
    #[must_use]
    pub fn build_ssh_tunnel_config_with_saved(&self, previous_has_saved: bool) -> SshTunnelConfig {
        let has_saved_secret = self.new_conn_ssh_enabled
            && self.new_conn_ssh_save_secret
            && (!self.new_conn_ssh_secret.is_empty() || previous_has_saved);

        SshTunnelConfig {
            enabled: self.new_conn_ssh_enabled,
            endpoint: self.new_conn_ssh_endpoint.trim().to_string(),
            username: self.new_conn_ssh_username.trim().to_string(),
            auth_method: self.new_conn_ssh_auth_method,
            private_key_path: self.new_conn_ssh_key_path.trim().to_string(),
            has_saved_secret,
        }
    }

    /// Normalizes `new_conn_uri` (auto-prepending `mongodb://` when omitted) and injects
    /// `new_conn_username` / `new_conn_password` if provided and not already in the URI.
    #[must_use]
    pub fn build_effective_uri(&self) -> String {
        ConnectionConfig::normalize_uri(
            &self.new_conn_uri,
            &self.new_conn_username,
            &self.new_conn_password,
        )
    }

    #[must_use]
    pub fn to_persistence(&self) -> WorkspacePersistence {
        let mut open_tabs = Vec::new();
        for (_surface, node) in self.dock_state.iter_all_nodes() {
            if let Some(tabs) = node.tabs() {
                open_tabs.extend(tabs.iter().cloned());
            }
        }

        WorkspacePersistence {
            theme_mode: self.theme_mode,
            sidebar_visible: self.sidebar_visible,
            sidebar_width: 250.0,
            bottom_panel_visible: self.bottom_panel.visible,
            bottom_panel_height: 170.0,
            active_activity: self.active_activity,
            open_tabs,
        }
    }

    /// Returns `true` if there is at least one open collection or user tab in any dock leaf.
    #[must_use]
    pub fn has_open_tabs(&self) -> bool {
        for (_surface, node) in self.dock_state.iter_all_nodes() {
            if let Some(tabs) = node.tabs() {
                if !tabs.is_empty() {
                    return true;
                }
            }
        }
        false
    }

    /// Creates an empty [`AppState`] with no connections or open tabs (clean state).
    #[must_use]
    pub fn new_empty() -> Self {
        let persistence = WorkspacePersistence {
            open_tabs: Vec::new(),
            ..WorkspacePersistence::default()
        };
        let mut state = Self::from_persistence(persistence);
        state.connections = Vec::new();
        state.dock_state = DockState::new(Vec::new());
        state.active_workspace_filter = None;
        state.quick_access_filter = QuickAccessFilter::All;
        state.recent_connections.clear();
        state.show_manage_workspaces_modal = false;
        state.new_workspace_input.clear();
        state.new_folder_input.clear();
        state.new_folder_workspace_target.clear();
        state
    }

    #[must_use]
    pub fn find_collection(&self, database: &str, collection: &str) -> Option<&CollectionInfo> {
        self.connections
            .iter()
            .flat_map(|conn| &conn.databases)
            .filter(|db| db.name == database)
            .flat_map(|db| &db.collections)
            .find(|coll| coll.name == collection)
    }

    pub fn find_collection_mut(
        &mut self,
        database: &str,
        collection: &str,
    ) -> Option<&mut CollectionInfo> {
        self.connections
            .iter_mut()
            .flat_map(|conn| &mut conn.databases)
            .filter(|db| db.name == database)
            .flat_map(|db| &mut db.collections)
            .find(|coll| coll.name == collection)
    }

    #[must_use]
    pub fn find_collection_by_conn(
        &self,
        connection_name: &str,
        database: &str,
        collection: &str,
    ) -> Option<&CollectionInfo> {
        self.connections
            .iter()
            .filter(|conn| conn.name == connection_name)
            .flat_map(|conn| &conn.databases)
            .filter(|db| db.name == database)
            .flat_map(|db| &db.collections)
            .find(|coll| coll.name == collection)
            .or_else(|| self.find_collection(database, collection))
    }

    pub fn find_collection_by_conn_mut(
        &mut self,
        connection_name: &str,
        database: &str,
        collection: &str,
    ) -> Option<&mut CollectionInfo> {
        if self.connections.iter().any(|c| c.name == connection_name) {
            self.connections
                .iter_mut()
                .filter(|conn| conn.name == connection_name)
                .flat_map(|conn| &mut conn.databases)
                .filter(|db| db.name == database)
                .flat_map(|db| &mut db.collections)
                .find(|coll| coll.name == collection)
        } else {
            self.find_collection_mut(database, collection)
        }
    }

    #[must_use]
    pub fn find_user(
        &self,
        connection_name: &str,
        username: &str,
        user_db: &str,
    ) -> Option<&MongoUser> {
        self.connections
            .iter()
            .filter(|conn| conn.name == connection_name)
            .flat_map(|conn| &conn.users)
            .find(|u| u.username == username && (user_db.is_empty() || u.db == user_db))
    }

    pub fn open_or_focus_collection(
        &mut self,
        connection_name: &str,
        database: &str,
        collection: &str,
    ) {
        let target = CollectionTab::new(connection_name, database, collection);
        if let Some(location) = self.dock_state.find_tab(&target) {
            self.dock_state.set_active_tab(location);
        } else {
            self.dock_state.push_to_focused_leaf(target);
        }
    }

    pub fn open_or_focus_user(&mut self, connection_name: &str, username: &str, user_db: &str) {
        let target = CollectionTab::new_user(connection_name, username, user_db);
        if let Some(location) = self.dock_state.find_tab(&target) {
            self.dock_state.set_active_tab(location);
        } else {
            self.dock_state.push_to_focused_leaf(target);
        }
    }

    /// Toggles the favorite status of a connection profile by ID.
    pub fn toggle_favorite(&mut self, id: Uuid) {
        if let Some(conn) = self.connections.iter_mut().find(|c| c.id == id) {
            conn.is_favorite = !conn.is_favorite;
        }
    }

    /// Pushes a connection ID to the front of the recent connections list,
    /// removing any prior occurrence and capping history to 10 entries.
    pub fn push_recent_connection(&mut self, id: Uuid) {
        self.recent_connections
            .retain(|&existing_id| existing_id != id);
        self.recent_connections.insert(0, id);
        if self.recent_connections.len() > 10 {
            self.recent_connections.truncate(10);
        }
    }

    /// Sets the workspace and folder names for a connection, normalizing empty or whitespace strings to `None`.
    pub fn set_connection_workspace_folder(
        &mut self,
        id: Uuid,
        workspace: Option<String>,
        folder: Option<String>,
    ) {
        let clean_workspace = workspace
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let clean_folder = folder
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if let Some(conn) = self.connections.iter_mut().find(|c| c.id == id) {
            conn.workspace = clean_workspace;
            conn.folder = clean_folder;
        }
    }

    /// Deletes a workspace by clearing workspace/folder fields on all matching connections.
    /// Also resets the active workspace filter if it was set to this workspace.
    pub fn delete_workspace(&mut self, name: &str) {
        let trimmed = name.trim();
        for conn in &mut self.connections {
            if conn.workspace_name() == Some(trimmed) {
                conn.workspace = None;
                conn.folder = None;
            }
        }
        if self.active_workspace_filter.as_deref().map(str::trim) == Some(trimmed) {
            self.active_workspace_filter = None;
        }
    }

    /// Deletes a folder within a workspace by clearing the folder field on all matching connections.
    pub fn delete_project_folder(&mut self, workspace: &str, folder_name: &str) {
        let trimmed_ws = workspace.trim();
        let trimmed_folder = folder_name.trim();
        for conn in &mut self.connections {
            if conn.workspace_name() == Some(trimmed_ws)
                && conn.folder_name() == Some(trimmed_folder)
            {
                conn.folder = None;
            }
        }
    }

    /// Returns the number of favorite connections.
    #[must_use]
    pub fn favorite_count(&self) -> usize {
        self.connections.iter().filter(|c| c.is_favorite).count()
    }

    /// Returns the number of recent connection IDs that currently exist in `connections`.
    #[must_use]
    pub fn recent_count(&self) -> usize {
        self.recent_connections
            .iter()
            .filter(|&id| self.connections.iter().any(|c| c.id == *id))
            .count()
    }

    /// Toggles the quick access filter. If already selected, resets to [`QuickAccessFilter::All`].
    pub fn toggle_quick_access_filter(&mut self, filter: QuickAccessFilter) {
        if self.quick_access_filter == filter {
            self.quick_access_filter = QuickAccessFilter::All;
        } else {
            self.quick_access_filter = filter;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toggle_favorite() {
        let mut state = AppState::default();
        let id = state.connections[0].id;
        assert!(!state.connections[0].is_favorite);
        assert_eq!(state.favorite_count(), 0);

        state.toggle_favorite(id);
        assert!(state.connections[0].is_favorite);
        assert_eq!(state.favorite_count(), 1);

        state.toggle_favorite(id);
        assert!(!state.connections[0].is_favorite);
        assert_eq!(state.favorite_count(), 0);
    }

    #[test]
    fn test_push_recent_connection() {
        let mut state = AppState::default();
        let u1 = Uuid::new_v4();
        let u2 = Uuid::new_v4();
        let u3 = Uuid::new_v4();

        state.push_recent_connection(u1);
        state.push_recent_connection(u2);
        state.push_recent_connection(u3);
        assert_eq!(state.recent_connections, vec![u3, u2, u1]);

        state.push_recent_connection(u1);
        assert_eq!(state.recent_connections, vec![u1, u3, u2]);

        for _ in 0..15 {
            state.push_recent_connection(Uuid::new_v4());
        }
        assert_eq!(state.recent_connections.len(), 10);
    }

    #[test]
    fn test_set_connection_workspace_folder() {
        let mut state = AppState::default();
        let id = state.connections[0].id;

        state.set_connection_workspace_folder(
            id,
            Some("  Workspace 1  ".to_string()),
            Some("  Folder A  ".to_string()),
        );
        assert_eq!(state.connections[0].workspace_name(), Some("Workspace 1"));
        assert_eq!(state.connections[0].folder_name(), Some("Folder A"));

        state.set_connection_workspace_folder(id, Some("   ".to_string()), None);
        assert_eq!(state.connections[0].workspace_name(), None);
        assert_eq!(state.connections[0].folder_name(), None);
    }

    #[test]
    fn test_delete_workspace() {
        let mut state = AppState::default();
        let id0 = state.connections[0].id;
        let id1 = state.connections[1].id;

        state.set_connection_workspace_folder(
            id0,
            Some("CustomerA".to_string()),
            Some("Folder1".to_string()),
        );
        state.set_connection_workspace_folder(
            id1,
            Some("CustomerB".to_string()),
            Some("Folder2".to_string()),
        );
        state.active_workspace_filter = Some("CustomerA".to_string());

        state.delete_workspace("CustomerA");
        assert_eq!(state.connections[0].workspace, None);
        assert_eq!(state.connections[0].folder, None);
        assert_eq!(state.connections[1].workspace_name(), Some("CustomerB"));
        assert_eq!(state.connections[1].folder_name(), Some("Folder2"));
        assert_eq!(state.active_workspace_filter, None);
    }

    #[test]
    fn test_delete_project_folder() {
        let mut state = AppState::default();
        let id0 = state.connections[0].id;

        state.set_connection_workspace_folder(
            id0,
            Some("CustomerA".to_string()),
            Some("Folder1".to_string()),
        );

        state.delete_project_folder("CustomerA", "Folder2");
        assert_eq!(state.connections[0].folder_name(), Some("Folder1"));

        state.delete_project_folder("CustomerB", "Folder1");
        assert_eq!(state.connections[0].folder_name(), Some("Folder1"));

        state.delete_project_folder("CustomerA", "Folder1");
        assert_eq!(state.connections[0].workspace_name(), Some("CustomerA"));
        assert_eq!(state.connections[0].folder_name(), None);
    }

    #[test]
    fn test_favorite_and_recent_count() {
        let mut state = AppState::default();
        let id0 = state.connections[0].id;
        let id1 = state.connections[1].id;

        assert_eq!(state.favorite_count(), 0);
        assert_eq!(state.recent_count(), 0);

        state.toggle_favorite(id0);
        assert_eq!(state.favorite_count(), 1);

        state.push_recent_connection(id0);
        state.push_recent_connection(Uuid::new_v4());
        assert_eq!(state.recent_count(), 1);

        state.push_recent_connection(id1);
        assert_eq!(state.recent_count(), 2);
    }

    #[test]
    fn test_toggle_quick_access_filter() {
        let mut state = AppState::default();
        assert_eq!(state.quick_access_filter, QuickAccessFilter::All);

        state.toggle_quick_access_filter(QuickAccessFilter::Favorites);
        assert_eq!(state.quick_access_filter, QuickAccessFilter::Favorites);

        // Clicking Favorites again toggles back to All
        state.toggle_quick_access_filter(QuickAccessFilter::Favorites);
        assert_eq!(state.quick_access_filter, QuickAccessFilter::All);

        // Switching from Recent to Favorites
        state.toggle_quick_access_filter(QuickAccessFilter::Recent);
        assert_eq!(state.quick_access_filter, QuickAccessFilter::Recent);
        state.toggle_quick_access_filter(QuickAccessFilter::Favorites);
        assert_eq!(state.quick_access_filter, QuickAccessFilter::Favorites);
    }
}
