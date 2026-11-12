use egui_phosphor::regular;
use serde::{Deserialize, Serialize};

use crate::models::UserRole;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CollectionSubTab {
    Documents,
    Indexes,
    Schema,
    Aggregation,
}

impl CollectionSubTab {
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Documents => regular::FILES,
            Self::Indexes => regular::KEY,
            Self::Schema => regular::TREE_STRUCTURE,
            Self::Aggregation => regular::FUNNEL,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Documents => "Documents",
            Self::Indexes => "Indexes",
            Self::Schema => "Schema",
            Self::Aggregation => "Aggregation",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentViewMode {
    Table,
    JsonTree,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionTab {
    pub connection_name: String,
    pub database: String,
    pub collection: String,
    pub active_subtab: CollectionSubTab,
    pub view_mode: DocumentViewMode,
    pub filter_query: String,
    pub projection_query: String,
    pub sort_query: String,
    pub limit: usize,
    pub selected_row: Option<usize>,
    #[serde(default)]
    pub selected_user: Option<String>,
    #[serde(default)]
    pub role_db_input: String,
    #[serde(default)]
    pub role_db_filter: String,
    #[serde(default)]
    pub role_name_input: String,
    #[serde(skip)]
    pub draft_roles: Option<Vec<UserRole>>,
}

impl PartialEq for CollectionTab {
    fn eq(&self, other: &Self) -> bool {
        if self.selected_user.is_some() || other.selected_user.is_some() {
            self.connection_name == other.connection_name
                && self.selected_user == other.selected_user
        } else {
            self.connection_name == other.connection_name
                && self.database == other.database
                && self.collection == other.collection
        }
    }
}

impl Eq for CollectionTab {}

impl CollectionTab {
    #[must_use]
    pub fn new(connection_name: &str, database: &str, collection: &str) -> Self {
        Self {
            connection_name: connection_name.to_string(),
            database: database.to_string(),
            collection: collection.to_string(),
            active_subtab: CollectionSubTab::Documents,
            view_mode: DocumentViewMode::Table,
            filter_query: "{}".to_string(),
            projection_query: "{}".to_string(),
            sort_query: "{\"_id\": 1}".to_string(),
            limit: 50,
            selected_row: Some(0),
            selected_user: None,
            role_db_input: "admin".to_string(),
            role_db_filter: String::new(),
            role_name_input: "readWrite".to_string(),
            draft_roles: None,
        }
    }

    #[must_use]
    pub fn new_user(connection_name: &str, username: &str) -> Self {
        Self {
            connection_name: connection_name.to_string(),
            database: "admin".to_string(),
            collection: format!("user:{username}"),
            active_subtab: CollectionSubTab::Documents,
            view_mode: DocumentViewMode::Table,
            filter_query: "{}".to_string(),
            projection_query: "{}".to_string(),
            sort_query: "{}".to_string(),
            limit: 50,
            selected_row: None,
            selected_user: Some(username.to_string()),
            role_db_input: "admin".to_string(),
            role_db_filter: String::new(),
            role_name_input: "readWrite".to_string(),
            draft_roles: None,
        }
    }

    #[must_use]
    pub fn title(&self) -> &str {
        if let Some(ref user) = self.selected_user {
            user.as_str()
        } else {
            &self.collection
        }
    }

    #[must_use]
    pub fn full_namespace(&self) -> String {
        if let Some(ref user) = self.selected_user {
            format!("admin.system.users.{user}")
        } else {
            format!("{}.{}", self.database, self.collection)
        }
    }
}
