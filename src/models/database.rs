use serde::{Deserialize, Serialize};

use super::collection::CollectionInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseInfo {
    pub name: String,
    pub expanded: bool,
    pub size_on_disk_mb: f64,
    pub collections: Vec<CollectionInfo>,
}

impl DatabaseInfo {
    pub fn new(name: &str, expanded: bool, collection_names: Vec<&str>) -> Self {
        let collections = collection_names
            .into_iter()
            .map(|c| CollectionInfo::mock(name, c))
            .collect();

        Self {
            name: name.to_string(),
            expanded,
            size_on_disk_mb: 24.8,
            collections,
        }
    }

    pub fn mock_databases() -> Vec<Self> {
        vec![
            Self::new("admin", true, vec!["users", "products", "orders"]),
            Self::new("shop_db", true, vec!["customers", "payments"]),
        ]
    }
}
