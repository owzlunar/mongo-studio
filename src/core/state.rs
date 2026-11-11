use egui_dock::DockState;

use crate::models::{CollectionInfo, ConnectionConfig};
use crate::theme::ThemeMode;
use crate::ui::activity_bar::ActivityView;
use crate::workspace::{BottomPanelState, CollectionTab, WorkspacePersistence};

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
    pub new_conn_name: String,
    pub new_conn_uri: String,
    pub show_about_modal: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self::from_persistence(WorkspacePersistence::default())
    }
}

impl AppState {
    pub fn from_persistence(persisted: WorkspacePersistence) -> Self {
        let initial_tabs = if persisted.open_tabs.is_empty() {
            vec![
                CollectionTab::new("MongoDB Local", "admin", "users"),
                CollectionTab::new("MongoDB Local", "admin", "products"),
            ]
        } else {
            persisted.open_tabs
        };

        let mut bottom_panel = BottomPanelState::default();
        bottom_panel.visible = persisted.bottom_panel_visible;

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
            new_conn_name: "Production ReplicaSet".to_string(),
            new_conn_uri: "mongodb://localhost:27017".to_string(),
            show_about_modal: false,
        }
    }

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

    pub fn find_collection(&self, database: &str, collection: &str) -> Option<&CollectionInfo> {
        for conn in &self.connections {
            for db in &conn.databases {
                if db.name == database {
                    if let Some(coll) = db.collections.iter().find(|c| c.name == collection) {
                        return Some(coll);
                    }
                }
            }
        }
        None
    }

    pub fn find_collection_mut(
        &mut self,
        database: &str,
        collection: &str,
    ) -> Option<&mut CollectionInfo> {
        for conn in &mut self.connections {
            for db in &mut conn.databases {
                if db.name == database {
                    if let Some(coll) = db.collections.iter_mut().find(|c| c.name == collection) {
                        return Some(coll);
                    }
                }
            }
        }
        None
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
}
