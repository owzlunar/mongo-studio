use std::time::Duration;

use mongodb::Client;

use crate::core::{AppError, AppResult};
use crate::database::connection::ConnectionClient;
use crate::models::{CollectionInfo, DatabaseInfo};

#[derive(Debug, Default, Clone, Copy)]
pub struct DatabaseService;

impl DatabaseService {
    #[must_use]
    pub fn list_mock_databases() -> Vec<DatabaseInfo> {
        DatabaseInfo::mock_databases()
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
        if trimmed_coll.starts_with("system.") {
            return Err(AppError::Connection(format!(
                "cannot modify protected system collection '{trimmed_coll}'"
            )));
        }
        Ok((trimmed_db, trimmed_coll))
    }

    fn validate_drop_database_name(db_name: &str) -> AppResult<&str> {
        let trimmed_db = db_name.trim();
        if trimmed_db.is_empty() {
            return Err(AppError::Connection(
                "database name must not be empty".to_string(),
            ));
        }
        if trimmed_db.eq_ignore_ascii_case("admin")
            || trimmed_db.eq_ignore_ascii_case("local")
            || trimmed_db.eq_ignore_ascii_case("config")
        {
            return Err(AppError::Connection(format!(
                "cannot drop system database '{trimmed_db}'"
            )));
        }
        Ok(trimmed_db)
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
        let client = Self::connect_client(uri, timeout).await?;

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
        let runtime = Self::build_single_thread_runtime()?;
        runtime.block_on(Self::create_database(
            uri,
            db_name,
            initial_collection,
            timeout,
        ))
    }

    /// Drops a non-system database on a live MongoDB server.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if `db_name` is empty, refers to a protected system
    /// database (`admin`, `local`, `config`), or the server command fails.
    pub async fn drop_database(uri: &str, db_name: &str, timeout: Duration) -> AppResult<()> {
        let trimmed_db = Self::validate_drop_database_name(db_name)?;
        let client = Self::connect_client(uri, timeout).await?;

        tokio::time::timeout(timeout, client.database(trimmed_db).drop(None))
            .await
            .map_err(|_| AppError::Connection("dropDatabase timed out".to_string()))?
            .map_err(|err| AppError::Connection(format!("failed to drop database: {err}")))?;

        Ok(())
    }

    /// Synchronously drops a non-system database on a live MongoDB server.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if validation or dropping the database fails.
    pub fn drop_database_blocking(uri: &str, db_name: &str, timeout: Duration) -> AppResult<()> {
        let runtime = Self::build_single_thread_runtime()?;
        runtime.block_on(Self::drop_database(uri, db_name, timeout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_database_rejects_empty_and_system_databases() {
        assert!(DatabaseService::validate_drop_database_name("").is_err());
        assert!(DatabaseService::validate_drop_database_name("   ").is_err());
        assert!(DatabaseService::validate_drop_database_name("admin").is_err());
        assert!(DatabaseService::validate_drop_database_name("local").is_err());
        assert!(DatabaseService::validate_drop_database_name("config").is_err());
        assert!(DatabaseService::validate_drop_database_name("Admin").is_err());
        assert_eq!(
            DatabaseService::validate_drop_database_name(" app_db ").unwrap(),
            "app_db"
        );
    }
}
