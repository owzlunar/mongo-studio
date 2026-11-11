use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CollectionSubTab {
    Documents,
    Indexes,
    Schema,
    Aggregation,
}

impl CollectionSubTab {
    pub fn label(&self) -> &'static str {
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
}

impl PartialEq for CollectionTab {
    fn eq(&self, other: &Self) -> bool {
        self.connection_name == other.connection_name
            && self.database == other.database
            && self.collection == other.collection
    }
}

impl Eq for CollectionTab {}

impl CollectionTab {
    pub fn new(connection_name: &str, database: &str, collection: &str) -> Self {
        Self {
            connection_name: connection_name.to_string(),
            database: database.to_string(),
            collection: collection.to_string(),
            active_subtab: CollectionSubTab::Documents,
            view_mode: DocumentViewMode::Table,
            filter_query: "{\"status\": \"active\"}".to_string(),
            projection_query: "{}".to_string(),
            sort_query: "{\"_id\": 1}".to_string(),
            limit: 50,
            selected_row: Some(0),
        }
    }

    pub fn title(&self) -> String {
        self.collection.clone()
    }

    pub fn full_namespace(&self) -> String {
        format!("{}.{}", self.database, self.collection)
    }
}
