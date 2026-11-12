use crate::models::IndexInfo;

#[derive(Debug, Default, Clone, Copy)]
pub struct IndexService;

impl IndexService {
    #[must_use]
    pub fn default_indexes() -> Vec<IndexInfo> {
        vec![IndexInfo {
            name: "_id_".to_string(),
            keys: "{ _id: 1 }".to_string(),
            index_type: "B-Tree".to_string(),
            unique: true,
            size_kb: 16.0,
        }]
    }
}
