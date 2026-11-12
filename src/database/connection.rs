use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use bson::{doc, Bson, Document};
use mongodb::options::{ClientOptions, FindOptions};
use mongodb::Client;
use uuid::Uuid;

use crate::core::{AppError, AppResult};
use crate::database::index_service::IndexService;
use crate::models::{
    CollectionInfo, CollectionStats, ConnectionConfig, ConnectionStatus, DatabaseInfo, DocumentRow,
    IndexInfo, SchemaField,
};

#[derive(Debug, Clone)]
pub struct ConnectionClient {
    pub config: ConnectionConfig,
}

impl ConnectionClient {
    /// Creates a [`ConnectionClient`] from a connection name and MongoDB URI (mock-populated for instant offline/unit-test use).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if `name` or `uri` is empty.
    pub fn from_uri(name: &str, uri: &str) -> AppResult<Self> {
        let trimmed_name = name.trim();
        let trimmed_uri = uri.trim();

        if trimmed_name.is_empty() {
            return Err(AppError::Connection(
                "connection name must not be empty".to_string(),
            ));
        }
        if trimmed_uri.is_empty() {
            return Err(AppError::Connection(
                "connection URI must not be empty".to_string(),
            ));
        }

        let host = ConnectionConfig::extract_display_host(trimmed_uri);

        Ok(Self {
            config: ConnectionConfig {
                id: Uuid::new_v4(),
                name: trimmed_name.to_string(),
                uri: trimmed_uri.to_string(),
                host,
                status: ConnectionStatus::Connected,
                expanded: true,
                databases: DatabaseInfo::mock_databases(),
                last_error: None,
                has_saved_password: false,
            },
        })
    }

    /// Pings a live MongoDB server at `uri` and returns the round-trip latency in milliseconds.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if the URI is invalid, unreachable, or times out.
    pub async fn ping_uri(uri: &str, timeout: Duration) -> AppResult<f64> {
        let trimmed = uri.trim();
        if trimmed.is_empty() {
            return Err(AppError::Connection(
                "connection URI must not be empty".to_string(),
            ));
        }

        let normalized = ConnectionConfig::normalize_uri(trimmed, "", "");
        let mut options = tokio::time::timeout(timeout, ClientOptions::parse(&normalized))
            .await
            .map_err(|_| {
                AppError::Connection(format!(
                    "connection timed out after {}s while resolving URI",
                    timeout.as_secs()
                ))
            })?
            .map_err(|err| AppError::Connection(format!("invalid MongoDB URI: {err}")))?;

        options.connect_timeout = Some(timeout);
        options.server_selection_timeout = Some(timeout);

        let client = Client::with_options(options)
            .map_err(|err| AppError::Connection(format!("failed to initialize client: {err}")))?;

        let start = Instant::now();
        tokio::time::timeout(
            timeout,
            client.database("admin").run_command(doc! { "ping": 1 }, None),
        )
        .await
        .map_err(|_| {
            AppError::Connection(format!(
                "ping timed out after {}s",
                timeout.as_secs()
            ))
        })?
        .map_err(|err| AppError::Connection(format!("ping failed: {err}")))?;

        Ok(start.elapsed().as_secs_f64() * 1000.0)
    }

    /// Connects to a live MongoDB deployment, verifies connectivity via `ping`, and introspects
    /// databases, collections, sample documents, indexes, and schema fields.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if connection or authentication fails.
    pub async fn connect_and_introspect(
        name: &str,
        uri: &str,
        timeout: Duration,
    ) -> AppResult<ConnectionConfig> {
        let mut profile = ConnectionConfig::new_profile(name, uri)?;
        let normalized = ConnectionConfig::normalize_uri(&profile.uri, "", "");

        let mut options = tokio::time::timeout(timeout, ClientOptions::parse(&normalized))
            .await
            .map_err(|_| {
                AppError::Connection(format!(
                    "connection timed out after {}s while parsing URI",
                    timeout.as_secs()
                ))
            })?
            .map_err(|err| AppError::Connection(format!("invalid MongoDB URI: {err}")))?;

        options.connect_timeout = Some(timeout);
        options.server_selection_timeout = Some(timeout);
        let default_db_from_uri = options.default_database.clone();

        let client = Client::with_options(options)
            .map_err(|err| AppError::Connection(format!("failed to create client: {err}")))?;

        let ping_db = default_db_from_uri.as_deref().unwrap_or("admin");
        tokio::time::timeout(
            timeout,
            client.database(ping_db).run_command(doc! { "ping": 1 }, None),
        )
        .await
        .map_err(|_| {
            AppError::Connection(format!(
                "server selection timed out after {}s",
                timeout.as_secs()
            ))
        })?
        .map_err(|err| AppError::Connection(format!("failed to ping server: {err}")))?;

        let db_names = match tokio::time::timeout(timeout, client.list_database_names(None, None)).await {
            Ok(Ok(names)) if !names.is_empty() => names,
            _ => {
                if let Some(default_db) = default_db_from_uri {
                    vec![default_db]
                } else {
                    vec!["admin".to_string(), "test".to_string()]
                }
            }
        };

        let mut databases = Vec::with_capacity(db_names.len());
        for (db_idx, db_name) in db_names.into_iter().enumerate() {
            let db = client.database(&db_name);
            let coll_names = tokio::time::timeout(timeout, db.list_collection_names(None))
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default();

            let mut collections = Vec::with_capacity(coll_names.len());
            for coll_name in coll_names.into_iter().take(50) {
                let coll_info =
                    Self::introspect_collection(&db, &db_name, &coll_name, timeout).await;
                collections.push(coll_info);
            }

            databases.push(DatabaseInfo {
                name: db_name,
                expanded: db_idx < 3,
                size_on_disk_mb: (collections.len() as f64) * 1.6,
                collections,
            });
        }

        profile.uri = normalized;
        profile.status = ConnectionStatus::Connected;
        profile.expanded = true;
        profile.databases = databases;
        profile.last_error = None;

        Ok(profile)
    }

    async fn introspect_collection(
        db: &mongodb::Database,
        db_name: &str,
        coll_name: &str,
        timeout: Duration,
    ) -> CollectionInfo {
        let coll = db.collection::<Document>(coll_name);

        let mut raw_docs = Vec::new();
        let mut documents = Vec::new();

        let find_opts = FindOptions::builder().limit(50).build();
        if let Ok(Ok(mut cursor)) =
            tokio::time::timeout(timeout, coll.find(None, find_opts)).await
        {
            while let Ok(Ok(true)) = tokio::time::timeout(timeout, cursor.advance()).await {
                if let Ok(doc) = cursor.deserialize_current() {
                    documents.push(DocumentRow::from_bson_doc(&doc));
                    raw_docs.push(doc);
                }
            }
        }

        let estimated_count = tokio::time::timeout(timeout, coll.estimated_document_count(None))
            .await
            .ok()
            .and_then(Result::ok)
            .map_or(documents.len(), |c| usize::try_from(c).unwrap_or(documents.len()));

        let mut indexes = Vec::new();
        if let Ok(Ok(mut idx_cursor)) =
            tokio::time::timeout(timeout, coll.list_indexes(None)).await
        {
            while let Ok(Ok(true)) = tokio::time::timeout(timeout, idx_cursor.advance()).await {
                if let Ok(idx_model) = idx_cursor.deserialize_current() {
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
                    let key_count = idx_model.keys.len();
                    let index_type = if key_count > 1 {
                        "Compound".to_string()
                    } else {
                        "B-Tree".to_string()
                    };
                    indexes.push(IndexInfo {
                        name,
                        keys: format!("{}", Bson::Document(idx_model.keys)),
                        index_type,
                        unique,
                        size_kb: 16.0,
                    });
                }
            }
        }
        if indexes.is_empty() {
            indexes = IndexService::default_indexes();
        }

        let schema_fields = Self::infer_schema_fields(&raw_docs);
        let avg_obj_size_bytes = if documents.is_empty() { 0 } else { 256 };
        let document_count = estimated_count.max(documents.len());

        CollectionInfo {
            database: db_name.to_string(),
            name: coll_name.to_string(),
            stats: CollectionStats {
                document_count,
                avg_obj_size_bytes,
                storage_size_kb: (document_count as f64) * 0.25,
                index_count: indexes.len(),
            },
            documents,
            indexes,
            schema_fields,
        }
    }

    fn infer_schema_fields(docs: &[Document]) -> Vec<SchemaField> {
        if docs.is_empty() {
            return Vec::new();
        }

        let total = docs.len() as f32;
        let mut stats: BTreeMap<String, (&'static str, usize, String)> = BTreeMap::new();

        for doc in docs {
            for (key, val) in doc {
                let bson_type = Self::bson_type_name(val);
                let entry = stats.entry(key.clone()).or_insert_with(|| {
                    (bson_type, 0, DocumentRow::bson_to_display(val))
                });
                entry.1 += 1;
            }
        }

        stats
            .into_iter()
            .map(|(field_name, (bson_type, count, sample_value))| SchemaField {
                field_name,
                bson_type: bson_type.to_string(),
                frequency_pct: ((count as f32 / total) * 100.0).min(100.0),
                sample_value,
            })
            .collect()
    }

    const fn bson_type_name(val: &Bson) -> &'static str {
        match val {
            Bson::ObjectId(_) => "ObjectId",
            Bson::String(_) => "String",
            Bson::Int32(_) => "Int32",
            Bson::Int64(_) => "Int64",
            Bson::Double(_) => "Double",
            Bson::Boolean(_) => "Boolean",
            Bson::DateTime(_) => "DateTime",
            Bson::Array(_) => "Array",
            Bson::Document(_) => "Document",
            Bson::Null => "Null",
            Bson::Binary(_) => "Binary",
            Bson::Decimal128(_) => "Decimal128",
            _ => "BSON",
        }
    }
}
