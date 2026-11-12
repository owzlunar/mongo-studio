use bson::Bson;
use egui_dock::DockState;

use crate::models::{CollectionInfo, ConnectionConfig, MongoUser, UserRole};
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
    pub change_pwd_new: String,
    pub change_pwd_confirm: String,
    pub change_pwd_show: bool,
    pub pending_delete_user: Option<(usize, String)>,
    pub show_about_modal: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self::from_persistence(WorkspacePersistence::default())
    }
}

impl AppState {
    #[must_use]
    pub fn from_persistence(persisted: WorkspacePersistence) -> Self {
        let initial_tabs = if persisted.open_tabs.is_empty() {
            vec![
                CollectionTab::new("MongoDB Local", "admin", "users"),
                CollectionTab::new("MongoDB Local", "admin", "products"),
            ]
        } else {
            persisted.open_tabs
        };

        let bottom_panel = BottomPanelState {
            visible: persisted.bottom_panel_visible,
            ..BottomPanelState::default()
        };

        Self {
            connections: vec![
                ConnectionConfig::mock_local(),
                ConnectionConfig::mock_staging(),
            ],
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
            change_pwd_new: String::new(),
            change_pwd_confirm: String::new(),
            change_pwd_show: false,
            pending_delete_user: None,
            show_about_modal: false,
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
        if self
            .connections
            .iter()
            .any(|c| c.name == connection_name)
        {
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
    pub fn find_user(&self, connection_name: &str, username: &str) -> Option<&MongoUser> {
        self.connections
            .iter()
            .filter(|conn| conn.name == connection_name)
            .flat_map(|conn| &conn.users)
            .find(|u| u.username == username)
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

    pub fn open_or_focus_user(&mut self, connection_name: &str, username: &str) {
        let target = CollectionTab::new_user(connection_name, username);
        if let Some(location) = self.dock_state.find_tab(&target) {
            self.dock_state.set_active_tab(location);
        } else {
            self.dock_state.push_to_focused_leaf(target);
        }
    }
}
