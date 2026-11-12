use std::time::Duration;

use bson::Document;
use mongodb::Client;

use crate::core::{AppError, AppResult};
use crate::database::connection::ConnectionClient;
use crate::models::CollectionInfo;

#[derive(Debug, Default, Clone, Copy)]
pub struct CollectionService;

impl CollectionService {
    #[must_use]
    pub fn get_collection_info(database: &str, collection: &str) -> CollectionInfo {
        CollectionInfo::mock(database, collection)
    }

    async fn connect_client(uri: &str, timeout: Duration) -> AppResult<Client> {
        ConnectionClient::connect_client(uri, timeout).await
    }

    fn build_single_thread_runtime() -> AppResult<tokio::runtime::Runtime> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| AppError::Connection(format!("failed to build runtime: {err}")))
    }

    fn validate_names<'a>(db_name: &'a str, coll_name: &'a str) -> AppResult<(&'a str, &'a str)> {
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
        if trimmed_coll.starts_with("system.") {
            return Err(AppError::Connection(format!(
                "cannot modify protected system collection '{trimmed_coll}'"
            )));
        }
        Ok((trimmed_db, trimmed_coll))
    }

    /// Creates a new collection in `db_name` and introspects it to return full [`CollectionInfo`].
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if validation, connection, or `createCollection` fails.
    pub async fn create_collection(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        timeout: Duration,
    ) -> AppResult<CollectionInfo> {
        let (trimmed_db, trimmed_coll) = Self::validate_names(db_name, coll_name)?;
        let client = Self::connect_client(uri, timeout).await?;
        let db = client.database(trimmed_db);

        tokio::time::timeout(timeout, db.create_collection(trimmed_coll, None))
            .await
            .map_err(|_| AppError::Connection("createCollection timed out".to_string()))?
            .map_err(|err| AppError::Connection(format!("failed to create collection: {err}")))?;

        Ok(ConnectionClient::introspect_collection(&db, trimmed_db, trimmed_coll, timeout).await)
    }

    /// Synchronously creates a collection in `db_name` and returns its [`CollectionInfo`].
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if validation or creation fails.
    pub fn create_collection_blocking(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        timeout: Duration,
    ) -> AppResult<CollectionInfo> {
        let runtime = Self::build_single_thread_runtime()?;
        runtime.block_on(Self::create_collection(uri, db_name, coll_name, timeout))
    }

    /// Drops a non-system collection from `db_name`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if validation, connection, or `drop` fails.
    pub async fn drop_collection(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        timeout: Duration,
    ) -> AppResult<()> {
        let (trimmed_db, trimmed_coll) = Self::validate_names(db_name, coll_name)?;
        let client = Self::connect_client(uri, timeout).await?;

        tokio::time::timeout(
            timeout,
            client
                .database(trimmed_db)
                .collection::<Document>(trimmed_coll)
                .drop(None),
        )
        .await
        .map_err(|_| AppError::Connection("dropCollection timed out".to_string()))?
        .map_err(|err| AppError::Connection(format!("failed to drop collection: {err}")))?;

        Ok(())
    }

    /// Synchronously drops a non-system collection from `db_name`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if validation or dropping the collection fails.
    pub fn drop_collection_blocking(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        timeout: Duration,
    ) -> AppResult<()> {
        let runtime = Self::build_single_thread_runtime()?;
        runtime.block_on(Self::drop_collection(uri, db_name, coll_name, timeout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_names_rejects_empty_and_system_collections() {
        assert!(CollectionService::validate_names("", "users").is_err());
        assert!(CollectionService::validate_names("app", "").is_err());
        assert!(CollectionService::validate_names("app", "system.indexes").is_err());
        assert_eq!(
            CollectionService::validate_names(" app ", " users ").unwrap(),
            ("app", "users")
        );
    }
}
