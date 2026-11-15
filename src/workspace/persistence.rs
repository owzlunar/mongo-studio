use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::theme::ThemeMode;
use crate::ui::activity_bar::ActivityView;
use crate::workspace::tabs::CollectionTab;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspacePersistence {
    pub theme_mode: ThemeMode,
    pub sidebar_visible: bool,
    pub sidebar_width: f32,
    pub bottom_panel_visible: bool,
    pub bottom_panel_height: f32,
    pub active_activity: ActivityView,
    pub open_tabs: Vec<CollectionTab>,
    #[serde(default)]
    pub custom_workspaces: Vec<String>,
    #[serde(default)]
    pub custom_folders: Vec<(String, String)>,
}

impl Default for WorkspacePersistence {
    fn default() -> Self {
        #[cfg(any(debug_assertions, test))]
        let open_tabs = vec![
            CollectionTab::new("MongoDB Local", "admin", "users"),
            CollectionTab::new("MongoDB Local", "admin", "products"),
        ];
        #[cfg(not(any(debug_assertions, test)))]
        let open_tabs = Vec::new();

        Self {
            theme_mode: ThemeMode::Dark,
            sidebar_visible: true,
            sidebar_width: 250.0,
            bottom_panel_visible: true,
            bottom_panel_height: 170.0,
            active_activity: ActivityView::Connections,
            open_tabs,
            custom_workspaces: Vec::new(),
            custom_folders: Vec::new(),
        }
    }
}

impl WorkspacePersistence {
    pub fn config_file_path() -> Option<PathBuf> {
        ProjectDirs::from("com", "mongostudio", "MongoStudio")
            .map(|dirs| dirs.config_dir().join("workspace_layout.json"))
    }

    pub fn load_from_disk() -> Option<Self> {
        let path = Self::config_file_path()?;
        let raw = fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn save_to_disk(&self) -> anyhow::Result<()> {
        if let Some(path) = Self::config_file_path() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let serialized = serde_json::to_string_pretty(self)?;
            fs::write(path, serialized)?;
        }
        Ok(())
    }
}
