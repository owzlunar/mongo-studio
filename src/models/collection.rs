use serde::{Deserialize, Serialize};

use super::document::DocumentRow;

const fn default_loaded() -> bool {
    true
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CollectionStats {
    pub document_count: usize,
    pub avg_obj_size_bytes: usize,
    pub storage_size_kb: f64,
    pub index_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexInfo {
    pub name: String,
    pub keys: String,
    pub index_type: String,
    pub unique: bool,
    pub size_kb: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaField {
    pub field_name: String,
    pub bson_type: String,
    pub frequency_pct: f32,
    pub sample_value: String,
    #[serde(default)]
    pub type_breakdown: Vec<(String, f32)>,
    #[serde(default)]
    pub doc_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionInfo {
    pub database: String,
    pub name: String,
    pub stats: CollectionStats,
    pub documents: Vec<DocumentRow>,
    pub indexes: Vec<IndexInfo>,
    pub schema_fields: Vec<SchemaField>,
    #[serde(default = "default_loaded")]
    pub loaded: bool,
}

impl CollectionInfo {
    /// Creates a lightweight lazy-loading collection stub with `loaded: false`.
    /// Full documents, indexes, and schema are fetched on demand when the user opens the collection.
    #[must_use]
    pub fn new_lazy_stub(database: &str, name: &str) -> Self {
        let default_indexes = vec![IndexInfo {
            name: "_id_".to_string(),
            keys: "{ _id: 1 }".to_string(),
            index_type: "B-Tree".to_string(),
            unique: true,
            size_kb: 16.0,
        }];
        Self {
            database: database.to_string(),
            name: name.to_string(),
            stats: CollectionStats {
                document_count: 0,
                avg_obj_size_bytes: 0,
                storage_size_kb: 0.0,
                index_count: default_indexes.len(),
            },
            documents: Vec::new(),
            indexes: default_indexes,
            schema_fields: Vec::new(),
            loaded: false,
        }
    }

    #[must_use]
    pub fn mock(database: &str, name: &str) -> Self {
        let documents = DocumentRow::mock_for_collection(name);
        let doc_total = documents.len();
        let role_count = (doc_total * 85).div_ceil(100).min(doc_total);
        let indexes = vec![
            IndexInfo {
                name: "_id_".to_string(),
                keys: "{ _id: 1 }".to_string(),
                index_type: "B-Tree".to_string(),
                unique: true,
                size_kb: 16.0,
            },
            IndexInfo {
                name: "status_1".to_string(),
                keys: "{ status: 1 }".to_string(),
                index_type: "B-Tree".to_string(),
                unique: false,
                size_kb: 12.0,
            },
            IndexInfo {
                name: "name_text_email_1".to_string(),
                keys: "{ name: \"text\", email: 1 }".to_string(),
                index_type: "Compound".to_string(),
                unique: false,
                size_kb: 24.0,
            },
        ];
        let schema_fields = vec![
            SchemaField {
                field_name: "_id".to_string(),
                bson_type: "ObjectId / Int32".to_string(),
                frequency_pct: 100.0,
                sample_value: "1".to_string(),
                type_breakdown: vec![("ObjectId".to_string(), 50.0), ("Int32".to_string(), 50.0)],
                doc_count: doc_total,
            },
            SchemaField {
                field_name: "name".to_string(),
                bson_type: "String".to_string(),
                frequency_pct: 100.0,
                sample_value: "\"Alice\"".to_string(),
                type_breakdown: vec![("String".to_string(), 100.0)],
                doc_count: doc_total,
            },
            SchemaField {
                field_name: "email".to_string(),
                bson_type: "String".to_string(),
                frequency_pct: 100.0,
                sample_value: "\"alice@example.com\"".to_string(),
                type_breakdown: vec![("String".to_string(), 100.0)],
                doc_count: doc_total,
            },
            SchemaField {
                field_name: "status".to_string(),
                bson_type: "String".to_string(),
                frequency_pct: 100.0,
                sample_value: "\"active\"".to_string(),
                type_breakdown: vec![("String".to_string(), 100.0)],
                doc_count: doc_total,
            },
            SchemaField {
                field_name: "role".to_string(),
                bson_type: "String".to_string(),
                frequency_pct: 85.0,
                sample_value: "\"admin\"".to_string(),
                type_breakdown: vec![("String".to_string(), 100.0)],
                doc_count: role_count,
            },
        ];

        Self {
            database: database.to_string(),
            name: name.to_string(),
            stats: CollectionStats {
                document_count: documents.len(),
                avg_obj_size_bytes: 256,
                storage_size_kb: 32.0,
                index_count: indexes.len(),
            },
            documents,
            indexes,
            schema_fields,
            loaded: true,
        }
    }
}
