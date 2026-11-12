use std::time::Duration;

use mongodb::options::ClientOptions;
use mongodb::Client;

use crate::core::{AppError, AppResult};
use crate::database::connection::ConnectionClient;
use crate::models::{CollectionInfo, ConnectionConfig, DatabaseInfo};

#[derive(Debug, Default, Clone, Copy)]
pub struct DatabaseService;

impl DatabaseService {
    #[must_use]
    pub fn list_mock_databases() -> Vec<DatabaseInfo> {
        DatabaseInfo::mock_databases()
    }

    fn validate_names<'a>(
        db_name: &'a str,
        initial_collection: &'a str,
    ) -> AppResult<(&'a str, &'a str)> {
        let trimmed_db = db_name.trim();
        let trimmed_coll = initial_collection.trim();
        if trimmed_db.is_empty() {
            return Err(AppError::Connection(
                "database name must not be empty".to_string(),
            ));
        }
        if trimmed_coll.is_empty() {
            return Err(AppError::Connection(
                "initial collection name must not be empty".to_string(),
            ));
        }
        Ok((trimmed_db, trimmed_coll))
    }

    /// Creates a new database on a live MongoDB server by creating an initial collection inside it.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if `db_name` or `initial_collection` is empty or the
    /// MongoDB command fails.
    pub async fn create_database(
        uri: &str,
        db_name: &str,
        initial_collection: &str,
        timeout: Duration,
    ) -> AppResult<()> {
        let (trimmed_db, trimmed_coll) = Self::validate_names(db_name, initial_collection)?;

        let normalized = ConnectionConfig::normalize_uri(uri, "", "");
        let mut options = tokio::time::timeout(timeout, ClientOptions::parse(&normalized))
            .await
            .map_err(|_| {
                AppError::Connection(format!(
                    "connection timed out after {}s",
                    timeout.as_secs()
                ))
            })?
            .map_err(|err| AppError::Connection(format!("invalid MongoDB URI: {err}")))?;

        options.connect_timeout = Some(timeout);
        options.server_selection_timeout = Some(timeout);

        let client = Client::with_options(options)
            .map_err(|err| AppError::Connection(format!("failed to create client: {err}")))?;

        tokio::time::timeout(
            timeout,
            client
                .database(trimmed_db)
                .create_collection(trimmed_coll, None),
        )
        .await
        .map_err(|_| AppError::Connection("createCollection timed out".to_string()))?
        .map_err(|err| AppError::Connection(format!("failed to create database: {err}")))?;

        Ok(())
    }

    /// Creates a new database with an initial collection and immediately introspects the created
    /// collection so callers receive full [`CollectionInfo`] metadata.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if validation, database creation, or introspection fails.
    pub async fn create_database_and_introspect(
        uri: &str,
        db_name: &str,
        initial_collection: &str,
        timeout: Duration,
    ) -> AppResult<CollectionInfo> {
        let (trimmed_db, trimmed_coll) = Self::validate_names(db_name, initial_collection)?;
        Self::create_database(uri, trimmed_db, trimmed_coll, timeout).await?;
        ConnectionClient::introspect_single_collection(uri, trimmed_db, trimmed_coll, timeout)
            .await
    }

    /// Synchronously creates a new database and initial collection on a live MongoDB server.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if validation or server creation fails.
    pub fn create_database_blocking(
        uri: &str,
        db_name: &str,
        initial_collection: &str,
        timeout: Duration,
    ) -> AppResult<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| AppError::Connection(format!("failed to build runtime: {err}")))?;

        runtime.block_on(Self::create_database(
            uri,
            db_name,
            initial_collection,
            timeout,
        ))
    }
}
