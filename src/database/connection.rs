use std::sync::Arc;
use std::time::{Duration, Instant};

use bson::{doc, Document};
use mongodb::options::{ClientOptions, FindOptions};
use mongodb::Client;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::core::{AppError, AppResult};
use crate::database::index_service::IndexService;
use crate::database::schema_analyzer::SchemaAnalyzer;
use crate::database::user_service::UserService;
use crate::models::{
    CollectionInfo, CollectionStats, ConnectionConfig, ConnectionStatus, DatabaseInfo, DocumentRow,
    MongoUser, SchemaField,
};

#[derive(Debug, Clone)]
pub struct ConnectionClient {
    pub config: ConnectionConfig,
}

impl ConnectionClient {
    /// Parses a MongoDB URI and creates a configured [`Client`] with connection and server
    /// selection timeouts applied.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if the URI is empty, invalid, or client creation fails.
    pub(crate) async fn connect_client(uri: &str, timeout: Duration) -> AppResult<Client> {
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
                AppError::Connection(format!("connection timed out after {}s", timeout.as_secs()))
            })?
            .map_err(|err| AppError::Connection(format!("invalid MongoDB URI: {err}")))?;

        options.connect_timeout = Some(timeout);
        options.server_selection_timeout = Some(timeout);

        Client::with_options(options)
            .map_err(|err| AppError::Connection(format!("failed to create client: {err}")))
    }

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
        let (_base_uri, uri_user, uri_pass) = ConnectionConfig::split_uri_credentials(trimmed_uri);
        let requires_auth = !uri_user.is_empty() || !uri_pass.is_empty();

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
                username: uri_user,
                requires_auth,
                has_saved_password: false,
                is_admin: true,
                users_expanded: true,
                users: MongoUser::mock_users(),
            },
        })
    }

    /// Pings a live MongoDB server at `uri` and returns the round-trip latency in milliseconds.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if the URI is invalid, unreachable, or times out.
    pub async fn ping_uri(uri: &str, timeout: Duration) -> AppResult<f64> {
        let client = Self::connect_client(uri, timeout).await?;

        let start = Instant::now();
        tokio::time::timeout(
            timeout,
            client
                .database("admin")
                .run_command(doc! { "ping": 1 }, None),
        )
        .await
        .map_err(|_| AppError::Connection(format!("ping timed out after {}s", timeout.as_secs())))?
        .map_err(|err| AppError::Connection(format!("ping failed: {err}")))?;

        Ok(start.elapsed().as_secs_f64() * 1000.0)
    }

    /// Fast connection & discovery: pings the server, fetches admin users if permitted,
    /// and concurrently lists database and collection names as lightweight lazy stubs.
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
            client
                .database(ping_db)
                .run_command(doc! { "ping": 1 }, None),
        )
        .await
        .map_err(|_| {
            AppError::Connection(format!(
                "server selection timed out after {}s",
                timeout.as_secs()
            ))
        })?
        .map_err(|err| AppError::Connection(format!("failed to ping server: {err}")))?;

        let (is_admin, users) = UserService::fetch_users(&client, timeout).await;

        let db_names =
            match tokio::time::timeout(timeout, client.list_database_names(None, None)).await {
                Ok(Ok(names)) if !names.is_empty() => names,
                _ => {
                    if let Some(default_db) = default_db_from_uri {
                        vec![default_db]
                    } else {
                        vec!["admin".to_string(), "test".to_string()]
                    }
                }
            };

        let semaphore = Arc::new(Semaphore::new(16));
        let mut join_set = JoinSet::new();

        for (db_idx, db_name) in db_names.into_iter().enumerate() {
            let client_clone = client.clone();
            let sem_clone = Arc::clone(&semaphore);
            join_set.spawn(async move {
                let _permit = sem_clone.acquire_owned().await.ok();
                let db = client_clone.database(&db_name);
                let mut coll_names = tokio::time::timeout(timeout, db.list_collection_names(None))
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .unwrap_or_default();
                coll_names.sort();

                let collections: Vec<CollectionInfo> = coll_names
                    .into_iter()
                    .map(|coll_name| CollectionInfo::new_lazy_stub(&db_name, &coll_name))
                    .collect();

                let db_info = DatabaseInfo {
                    name: db_name,
                    expanded: db_idx < 3,
                    size_on_disk_mb: (collections.len() as f64) * 1.6,
                    collections,
                };
                (db_idx, db_info)
            });
        }

        let mut indexed_dbs = Vec::new();
        while let Some(res) = join_set.join_next().await {
            if let Ok(pair) = res {
                indexed_dbs.push(pair);
            }
        }
        indexed_dbs.sort_by_key(|(idx, _)| *idx);
        let databases: Vec<DatabaseInfo> = indexed_dbs.into_iter().map(|(_, db)| db).collect();

        profile.uri = normalized;
        profile.status = ConnectionStatus::Connected;
        profile.expanded = true;
        profile.databases = databases;
        profile.last_error = None;
        profile.is_admin = is_admin;
        profile.users_expanded = is_admin && !users.is_empty();
        profile.users = users;

        Ok(profile)
    }

    /// Connects to `uri` and introspects a single collection on demand (documents, count, indexes, schema).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if the server cannot be reached.
    pub async fn introspect_single_collection(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        timeout: Duration,
    ) -> AppResult<CollectionInfo> {
        let client = Self::connect_client(uri, timeout).await?;
        let db = client.database(db_name);
        Ok(Self::introspect_collection(&db, db_name, coll_name, timeout).await)
    }

    pub async fn introspect_collection(
        db: &mongodb::Database,
        db_name: &str,
        coll_name: &str,
        timeout: Duration,
    ) -> CollectionInfo {
        let coll = db.collection::<Document>(coll_name);

        let mut raw_docs = Vec::new();
        let mut documents = Vec::new();

        let find_opts = FindOptions::builder().limit(50).build();
        if let Ok(Ok(mut cursor)) = tokio::time::timeout(timeout, coll.find(None, find_opts)).await
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
            .map_or(documents.len(), |c| {
                usize::try_from(c).unwrap_or(documents.len())
            });

        let mut indexes = match IndexService::list_indexes_on_collection(&coll, timeout).await {
            Ok(list) if !list.is_empty() => list,
            Ok(_) => IndexService::default_indexes(),
            Err(err) => {
                tracing::debug!(error = %err, "failed to list indexes during introspection");
                IndexService::default_indexes()
            }
        };
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
            loaded: true,
        }
    }

    fn infer_schema_fields(docs: &[Document]) -> Vec<SchemaField> {
        SchemaAnalyzer::analyze_bson_docs(docs)
    }
}
