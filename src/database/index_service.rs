use std::collections::HashMap;
use std::time::Duration;

use bson::{doc, Bson, Document};
use mongodb::options::IndexOptions;
use mongodb::{Client, Collection, IndexModel};

use crate::core::{AppError, AppResult};
use crate::database::connection::ConnectionClient;
use crate::models::{DocumentRow, IndexInfo};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateIndexRequest {
    pub keys_json: String,
    pub name: Option<String>,
    pub unique: bool,
    pub sparse: bool,
    pub background: bool,
}

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

    fn build_single_thread_runtime() -> AppResult<tokio::runtime::Runtime> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| AppError::Connection(format!("failed to build runtime: {err}")))
    }

    async fn connect_client(uri: &str, timeout: Duration) -> AppResult<Client> {
        ConnectionClient::connect_client(uri, timeout).await
    }

    fn validate_target<'a>(db_name: &'a str, coll_name: &'a str) -> AppResult<(&'a str, &'a str)> {
        let trimmed_db = db_name.trim();
        let trimmed_coll = coll_name.trim();
        if trimmed_db.is_empty() {
            return Err(AppError::Connection(
                "database name must not be empty".to_string(),
            ));
        }
        if trimmed_coll.is_empty() {
            return Err(AppError::Connection(
                "collection name must not be empty".to_string(),
            ));
        }
        Ok((trimmed_db, trimmed_coll))
    }

    fn validate_mutable_target<'a>(
        db_name: &'a str,
        coll_name: &'a str,
    ) -> AppResult<(&'a str, &'a str)> {
        let (trimmed_db, trimmed_coll) = Self::validate_target(db_name, coll_name)?;
        if trimmed_coll.starts_with("system.") {
            return Err(AppError::Connection(format!(
                "cannot modify indexes on protected system collection '{trimmed_coll}'"
            )));
        }
        Ok((trimmed_db, trimmed_coll))
    }

    /// Validates an index name before attempting to drop it, rejecting empty names and `_id_`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if `index_name` is empty or refers to `_id_`.
    pub fn validate_drop_index(index_name: &str) -> AppResult<&str> {
        let trimmed = index_name.trim();
        if trimmed.is_empty() {
            return Err(AppError::Connection(
                "index name must not be empty".to_string(),
            ));
        }
        if trimmed == "_id_" {
            return Err(AppError::Connection(
                "cannot drop primary '_id_' index".to_string(),
            ));
        }
        Ok(trimmed)
    }

    /// Validates a [`CreateIndexRequest`] and parses its key specification document.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if database/collection names are empty, or
    /// [`AppError::InvalidQueryJson`] if `keys_json` is invalid, empty, or has invalid directions.
    pub fn validate_create_index(
        db_name: &str,
        coll_name: &str,
        request: &CreateIndexRequest,
    ) -> AppResult<Document> {
        let _ = Self::validate_mutable_target(db_name, coll_name)?;
        let keys_doc = DocumentRow::parse_json_to_bson_doc(&request.keys_json)?;
        if keys_doc.is_empty() {
            return Err(AppError::InvalidQueryJson(
                "index key specification must not be empty".to_string(),
            ));
        }
        if keys_doc.len() == 1 && keys_doc.contains_key("_id") {
            return Err(AppError::InvalidQueryJson(
                "primary '_id_' index is managed automatically by MongoDB".to_string(),
            ));
        }

        for (field, val) in &keys_doc {
            if field.trim().is_empty() {
                return Err(AppError::InvalidQueryJson(
                    "index field name must not be empty".to_string(),
                ));
            }
            let valid_direction = match val {
                Bson::Int32(n) => *n == 1 || *n == -1,
                Bson::Int64(n) => *n == 1 || *n == -1,
                Bson::String(s) => {
                    matches!(s.as_str(), "text" | "2d" | "2dsphere" | "hashed")
                }
                _ => false,
            };
            if !valid_direction {
                return Err(AppError::InvalidQueryJson(format!(
                    "invalid index direction for '{field}': expected 1, -1, \"text\", \"2d\", \"2dsphere\", or \"hashed\""
                )));
            }
        }

        Ok(keys_doc)
    }

    fn classify_index_type(keys: &Document) -> String {
        for val in keys.values() {
            if let Bson::String(s) = val {
                match s.as_str() {
                    "text" => return "Text".to_string(),
                    "2dsphere" | "2d" => return "2dsphere".to_string(),
                    "hashed" => return "Hashed".to_string(),
                    _ => {}
                }
            }
        }
        if keys.len() > 1 {
            "Compound".to_string()
        } else {
            "B-Tree".to_string()
        }
    }

    async fn fetch_index_sizes_kb(
        coll: &Collection<Document>,
        timeout: Duration,
    ) -> HashMap<String, f64> {
        let mut sizes = HashMap::new();
        let pipeline = vec![doc! { "$collStats": { "storageStats": {} } }];
        let cursor_res = tokio::time::timeout(timeout, coll.aggregate(pipeline, None)).await;
        let mut cursor = match cursor_res {
            Ok(Ok(c)) => c,
            Ok(Err(err)) => {
                tracing::debug!(error = %err, "collStats unavailable for index size lookup");
                return sizes;
            }
            Err(_) => {
                tracing::debug!("collStats timed out during index size lookup");
                return sizes;
            }
        };

        if let Ok(Ok(true)) = tokio::time::timeout(timeout, cursor.advance()).await {
            if let Ok(doc) = cursor.deserialize_current() {
                if let Ok(storage_stats) = doc.get_document("storageStats") {
                    if let Ok(index_sizes) = storage_stats.get_document("indexSizes") {
                        for (name, val) in index_sizes {
                            let bytes = match val {
                                Bson::Int32(n) => f64::from(*n),
                                Bson::Int64(n) => *n as f64,
                                Bson::Double(f) => *f,
                                _ => continue,
                            };
                            sizes.insert(name.clone(), (bytes / 1024.0).max(0.0));
                        }
                    }
                }
            }
        }

        sizes
    }

    pub(crate) async fn list_indexes_on_collection(
        coll: &Collection<Document>,
        timeout: Duration,
    ) -> AppResult<Vec<IndexInfo>> {
        let sizes_kb = Self::fetch_index_sizes_kb(coll, timeout).await;

        let mut cursor = tokio::time::timeout(timeout, coll.list_indexes(None))
            .await
            .map_err(|_| AppError::Connection("listIndexes timed out".to_string()))?
            .map_err(|err| AppError::Connection(format!("listIndexes failed: {err}")))?;

        let mut indexes = Vec::new();
        while tokio::time::timeout(timeout, cursor.advance())
            .await
            .map_err(|_| AppError::Connection("listIndexes cursor timed out".to_string()))?
            .map_err(|err| AppError::Connection(format!("listIndexes cursor error: {err}")))?
        {
            let idx_model = cursor.deserialize_current().map_err(|err| {
                AppError::Connection(format!("failed to decode index metadata: {err}"))
            })?;

            let name = idx_model
                .options
                .as_ref()
                .and_then(|o| o.name.clone())
                .unwrap_or_else(|| "index".to_string());
            let unique = idx_model
                .options
                .as_ref()
                .and_then(|o| o.unique)
                .unwrap_or(name == "_id_");
            let index_type = Self::classify_index_type(&idx_model.keys);
            let size_kb = sizes_kb.get(&name).copied().unwrap_or(16.0);

            indexes.push(IndexInfo {
                name,
                keys: format!("{}", Bson::Document(idx_model.keys)),
                index_type,
                unique,
                size_kb,
            });
        }

        Ok(indexes)
    }

    /// Lists all indexes on `db_name.coll_name` from a live MongoDB server.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if validation, connection, or `listIndexes` fails.
    pub async fn list_indexes(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        timeout: Duration,
    ) -> AppResult<Vec<IndexInfo>> {
        let (trimmed_db, trimmed_coll) = Self::validate_target(db_name, coll_name)?;
        let client = Self::connect_client(uri, timeout).await?;
        let coll = client
            .database(trimmed_db)
            .collection::<Document>(trimmed_coll);
        Self::list_indexes_on_collection(&coll, timeout).await
    }

    /// Creates a new index on `db_name.coll_name` and reads the refreshed index list back from the
    /// same client.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidQueryJson`] if `request` is invalid, or [`AppError::Connection`]
    /// if connection, index creation, or listing fails.
    pub async fn create_index(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        request: &CreateIndexRequest,
        timeout: Duration,
    ) -> AppResult<Vec<IndexInfo>> {
        let (trimmed_db, trimmed_coll) = Self::validate_mutable_target(db_name, coll_name)?;
        let keys_doc = Self::validate_create_index(trimmed_db, trimmed_coll, request)?;

        let clean_name = request
            .name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        let index_options = IndexOptions::builder()
            .name(clean_name)
            .unique(request.unique.then_some(true))
            .sparse(request.sparse.then_some(true))
            .background(request.background.then_some(true))
            .build();

        let index_model = IndexModel::builder()
            .keys(keys_doc)
            .options(index_options)
            .build();

        let client = Self::connect_client(uri, timeout).await?;
        let coll = client
            .database(trimmed_db)
            .collection::<Document>(trimmed_coll);

        tokio::time::timeout(timeout, coll.create_index(index_model, None))
            .await
            .map_err(|_| AppError::Connection("createIndex timed out".to_string()))?
            .map_err(|err| AppError::Connection(format!("createIndex failed: {err}")))?;

        Self::list_indexes_on_collection(&coll, timeout).await
    }

    /// Synchronously creates an index on a live MongoDB collection and returns the refreshed list.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if validation, connection, or index creation fails.
    pub fn create_index_blocking(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        request: &CreateIndexRequest,
        timeout: Duration,
    ) -> AppResult<Vec<IndexInfo>> {
        let runtime = Self::build_single_thread_runtime()?;
        runtime.block_on(Self::create_index(
            uri, db_name, coll_name, request, timeout,
        ))
    }

    /// Drops an index by name from `db_name.coll_name` (strictly rejecting `_id_`) and reads the
    /// refreshed index list back from the same client.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if validation fails, `index_name` is `_id_`, or the
    /// command fails.
    pub async fn drop_index(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        index_name: &str,
        timeout: Duration,
    ) -> AppResult<Vec<IndexInfo>> {
        let (trimmed_db, trimmed_coll) = Self::validate_mutable_target(db_name, coll_name)?;
        let trimmed_index = Self::validate_drop_index(index_name)?;

        let client = Self::connect_client(uri, timeout).await?;
        let coll = client
            .database(trimmed_db)
            .collection::<Document>(trimmed_coll);

        tokio::time::timeout(timeout, coll.drop_index(trimmed_index, None))
            .await
            .map_err(|_| AppError::Connection("dropIndex timed out".to_string()))?
            .map_err(|err| AppError::Connection(format!("dropIndex failed: {err}")))?;

        Self::list_indexes_on_collection(&coll, timeout).await
    }

    /// Synchronously drops an index from a live MongoDB collection and returns the refreshed list.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if validation, connection, or dropping the index fails.
    pub fn drop_index_blocking(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        index_name: &str,
        timeout: Duration,
    ) -> AppResult<Vec<IndexInfo>> {
        let runtime = Self::build_single_thread_runtime()?;
        runtime.block_on(Self::drop_index(
            uri, db_name, coll_name, index_name, timeout,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_drop_index_rejects_empty_and_id_index() {
        assert!(IndexService::validate_drop_index("").is_err());
        assert!(IndexService::validate_drop_index("   ").is_err());
        let id_err = IndexService::validate_drop_index(" _id_ ").unwrap_err();
        assert!(id_err.to_string().contains("_id_"));
        assert_eq!(
            IndexService::validate_drop_index(" email_1 ").unwrap(),
            "email_1"
        );
    }

    #[test]
    fn validate_create_index_rejects_invalid_keys_and_directions() {
        let empty_req = CreateIndexRequest {
            keys_json: "{}".to_string(),
            name: None,
            unique: false,
            sparse: false,
            background: false,
        };
        assert!(matches!(
            IndexService::validate_create_index("app", "users", &empty_req),
            Err(AppError::InvalidQueryJson(_))
        ));

        let bad_dir_req = CreateIndexRequest {
            keys_json: r#"{"email": "ascending"}"#.to_string(),
            name: None,
            unique: false,
            sparse: false,
            background: false,
        };
        assert!(matches!(
            IndexService::validate_create_index("app", "users", &bad_dir_req),
            Err(AppError::InvalidQueryJson(_))
        ));

        let valid_req = CreateIndexRequest {
            keys_json: r#"{"email": 1, "bio": "text"}"#.to_string(),
            name: Some("email_bio_idx".to_string()),
            unique: true,
            sparse: false,
            background: true,
        };
        let keys = IndexService::validate_create_index("app", "users", &valid_req).unwrap();
        assert_eq!(keys.len(), 2);
        assert_eq!(IndexService::classify_index_type(&keys), "Text");
    }
}
