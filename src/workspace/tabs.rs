use egui_phosphor::regular;
use serde::{Deserialize, Serialize};

use crate::models::{DocumentRow, UserRole};

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
    #[serde(default)]
    pub skip: usize,
    pub limit: usize,
    pub selected_row: Option<usize>,
    #[serde(skip)]
    pub total_matched: Option<usize>,
    #[serde(skip)]
    pub is_loading: bool,
    #[serde(skip)]
    pub last_query_ms: Option<f64>,
    #[serde(skip)]
    pub cached_columns: Vec<String>,
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
    #[serde(skip)]
    pub cached_db_options: Vec<String>,
    #[serde(skip)]
    pub cached_db_conn: String,
    #[serde(skip)]
    pub cached_db_count: usize,
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
            skip: 0,
            limit: 20,
            selected_row: Some(0),
            total_matched: None,
            is_loading: false,
            last_query_ms: None,
            cached_columns: Vec::new(),
            selected_user: None,
            role_db_input: "admin".to_string(),
            role_db_filter: String::new(),
            role_name_input: "readWrite".to_string(),
            draft_roles: None,
            cached_db_options: Vec::new(),
            cached_db_conn: String::new(),
            cached_db_count: 0,
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
            skip: 0,
            limit: 20,
            selected_row: None,
            total_matched: None,
            is_loading: false,
            last_query_ms: None,
            cached_columns: Vec::new(),
            selected_user: Some(username.to_string()),
            role_db_input: "admin".to_string(),
            role_db_filter: String::new(),
            role_name_input: "readWrite".to_string(),
            draft_roles: None,
            cached_db_options: Vec::new(),
            cached_db_conn: String::new(),
            cached_db_count: 0,
        }
    }

    /// Recomputes `cached_columns` from `docs` when documents change.
    pub fn refresh_cached_columns(&mut self, docs: &[DocumentRow]) {
        self.cached_columns = DocumentRow::extract_columns(docs);
    }

    /// Populates `cached_columns` lazily only if it is currently empty and `docs` is non-empty.
    pub fn ensure_cached_columns(&mut self, docs: &[DocumentRow]) {
        if self.cached_columns.is_empty() && !docs.is_empty() {
            self.cached_columns = DocumentRow::extract_columns(docs);
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
