use serde::{Deserialize, Serialize};

use super::collection::CollectionInfo;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseInfo {
    pub name: String,
    pub expanded: bool,
    pub size_on_disk_mb: f64,
    pub collections: Vec<CollectionInfo>,
}

impl DatabaseInfo {
    #[must_use]
    pub fn new<I, S>(name: &str, expanded: bool, collection_names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let collections = collection_names
            .into_iter()
            .map(|c| CollectionInfo::mock(name, c.as_ref()))
            .collect();

        Self {
            name: name.to_string(),
            expanded,
            size_on_disk_mb: 24.8,
            collections,
        }
    }

    #[must_use]
    pub fn mock_databases() -> Vec<Self> {
        vec![
            Self::new("admin", true, ["users", "products", "orders"]),
            Self::new("shop_db", true, ["customers", "payments"]),
        ]
    }
}
