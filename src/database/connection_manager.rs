use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use directories::ProjectDirs;
use uuid::Uuid;

use crate::core::{AppError, AppResult};
use crate::database::connection::ConnectionClient;
use crate::models::{CollectionInfo, ConnectionConfig};

const KEYRING_SERVICE: &str = "com.mongostudio.MongoStudio";

#[derive(Debug, Default, Clone, Copy)]
pub struct ConnectionManager;

impl ConnectionManager {
    fn build_current_thread_runtime() -> AppResult<tokio::runtime::Runtime> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| AppError::Connection(format!("failed to build async runtime: {err}")))
    }

    fn ssh_keyring_account(conn_id: Uuid) -> String {
        format!("{conn_id}:ssh")
    }

    fn save_secret_to_keyring_for_account(account: &str, secret: &str) -> AppResult<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, account)
            .map_err(|err| AppError::Persistence(format!("keyring init failed: {err}")))?;
        entry
            .set_password(secret)
            .map_err(|err| AppError::Persistence(format!("keyring save failed: {err}")))
    }

    fn load_secret_from_keyring_for_account(account: &str) -> AppResult<Option<String>> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, account)
            .map_err(|err| AppError::Persistence(format!("keyring init failed: {err}")))?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(AppError::Persistence(format!("keyring load failed: {err}"))),
        }
    }

    fn delete_secret_from_keyring_for_account(account: &str) -> AppResult<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, account)
            .map_err(|err| AppError::Persistence(format!("keyring init failed: {err}")))?;
        match entry.delete_password() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(AppError::Persistence(format!(
                "keyring delete failed: {err}"
            ))),
        }
    }

    /// Creates a new mock-populated [`ConnectionConfig`] from a connection profile name and URI.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection name or URI is invalid.
    pub fn create_connection(name: &str, uri: &str) -> AppResult<ConnectionConfig> {
        let client = ConnectionClient::from_uri(name, uri)?;
        Ok(client.config)
    }

    /// Synchronously pings a MongoDB deployment using an isolated current-thread Tokio runtime.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if the runtime cannot be created or the ping fails.
    pub fn ping_uri_blocking(uri: &str, timeout: Duration) -> AppResult<f64> {
        let runtime = Self::build_current_thread_runtime()?;
        runtime.block_on(ConnectionClient::ping_uri(uri, timeout))
    }

    /// Synchronously connects to a live MongoDB server and introspects its databases, lazy collection stubs, and admin users.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if the connection or introspection fails.
    pub fn connect_live_blocking(
        name: &str,
        uri: &str,
        timeout: Duration,
    ) -> AppResult<ConnectionConfig> {
        let runtime = Self::build_current_thread_runtime()?;
        runtime.block_on(ConnectionClient::connect_and_introspect(name, uri, timeout))
    }

    /// Synchronously introspects a single collection on demand (fetching up to 50 documents, indexes, and schema).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if the runtime cannot be created or introspection fails.
    pub fn load_collection_blocking(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        timeout: Duration,
    ) -> AppResult<CollectionInfo> {
        let runtime = Self::build_current_thread_runtime()?;
        runtime.block_on(ConnectionClient::introspect_single_collection(
            uri, db_name, coll_name, timeout,
        ))
    }

    fn load_password_from_keyring_inner(conn_id: Uuid) -> AppResult<Option<String>> {
        Self::load_secret_from_keyring_for_account(&conn_id.to_string())
    }

    fn delete_password_from_keyring_inner(conn_id: Uuid) -> AppResult<()> {
        Self::delete_secret_from_keyring_for_account(&conn_id.to_string())
    }

    /// Saves a connection password in the OS credential keyring.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Persistence`] if the keyring entry cannot be created or written.
    pub fn save_password_to_keyring(conn_id: Uuid, password: &str) -> AppResult<()> {
        Self::save_secret_to_keyring_for_account(&conn_id.to_string(), password)
    }

    /// Asynchronously saves a connection password in the OS credential keyring
    /// on Tokio's blocking thread pool.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Persistence`] if the keyring write or blocking task fails.
    pub async fn save_password_to_keyring_async(conn_id: Uuid, password: String) -> AppResult<()> {
        tokio::task::spawn_blocking(move || Self::save_password_to_keyring(conn_id, &password))
            .await
            .map_err(|err| AppError::Persistence(format!("keyring save task failed: {err}")))?
    }

    /// Loads a saved connection password from the OS credential keyring, if available.
    #[must_use]
    pub fn load_password_from_keyring(conn_id: Uuid) -> Option<String> {
        match Self::load_password_from_keyring_inner(conn_id) {
            Ok(password) => password,
            Err(err) => {
                tracing::warn!(conn_id = %conn_id, error = %err, "failed to load password from keyring");
                None
            }
        }
    }

    /// Asynchronously loads a saved connection password from the OS credential keyring
    /// on Tokio's blocking thread pool.
    ///
    /// Returns `Ok(None)` when no entry exists for `conn_id`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Persistence`] if the keyring backend or blocking task fails.
    pub async fn load_password_from_keyring_async(conn_id: Uuid) -> AppResult<Option<String>> {
        tokio::task::spawn_blocking(move || Self::load_password_from_keyring_inner(conn_id))
            .await
            .map_err(|err| AppError::Persistence(format!("keyring load task failed: {err}")))?
    }

    /// Deletes a saved connection password from the OS credential keyring.
    pub fn delete_password_from_keyring(conn_id: Uuid) {
        if let Err(err) = Self::delete_password_from_keyring_inner(conn_id) {
            tracing::warn!(conn_id = %conn_id, error = %err, "failed to delete password from keyring");
        }
    }

    /// Asynchronously deletes a saved connection password from the OS credential keyring
    /// on Tokio's blocking thread pool.
    ///
    /// Missing entries (`keyring::Error::NoEntry`) are treated as `Ok(())`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Persistence`] if the keyring backend or blocking task fails.
    pub async fn delete_password_from_keyring_async(conn_id: Uuid) -> AppResult<()> {
        tokio::task::spawn_blocking(move || Self::delete_password_from_keyring_inner(conn_id))
            .await
            .map_err(|err| AppError::Persistence(format!("keyring delete task failed: {err}")))?
    }

    /// Asynchronously saves an SSH tunnel password or key passphrase in the OS credential keyring
    /// under account key `{conn_id}:ssh` on Tokio's blocking thread pool.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Persistence`] if the keyring write or blocking task fails.
    pub async fn save_ssh_secret_to_keyring_async(conn_id: Uuid, secret: String) -> AppResult<()> {
        tokio::task::spawn_blocking(move || {
            let account = Self::ssh_keyring_account(conn_id);
            Self::save_secret_to_keyring_for_account(&account, &secret)
        })
        .await
        .map_err(|err| AppError::Persistence(format!("SSH keyring save task failed: {err}")))?
    }

    /// Asynchronously loads a saved SSH tunnel secret from the OS credential keyring
    /// under account key `{conn_id}:ssh` on Tokio's blocking thread pool.
    ///
    /// Returns `Ok(None)` when no entry exists for `conn_id`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Persistence`] if the keyring backend or blocking task fails.
    pub async fn load_ssh_secret_from_keyring_async(conn_id: Uuid) -> AppResult<Option<String>> {
        tokio::task::spawn_blocking(move || {
            let account = Self::ssh_keyring_account(conn_id);
            Self::load_secret_from_keyring_for_account(&account)
        })
        .await
        .map_err(|err| AppError::Persistence(format!("SSH keyring load task failed: {err}")))?
    }

    /// Asynchronously deletes a saved SSH tunnel secret from the OS credential keyring
    /// under account key `{conn_id}:ssh` on Tokio's blocking thread pool.
    ///
    /// Missing entries (`keyring::Error::NoEntry`) are treated as `Ok(())`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Persistence`] if the keyring backend or blocking task fails.
    pub async fn delete_ssh_secret_from_keyring_async(conn_id: Uuid) -> AppResult<()> {
        tokio::task::spawn_blocking(move || {
            let account = Self::ssh_keyring_account(conn_id);
            Self::delete_secret_from_keyring_for_account(&account)
        })
        .await
        .map_err(|err| AppError::Persistence(format!("SSH keyring delete task failed: {err}")))?
    }

    /// Returns the path to `connections.json` inside the platform config directory.
    #[must_use]
    pub fn profiles_file_path() -> Option<PathBuf> {
        ProjectDirs::from("com", "mongostudio", "MongoStudio")
            .map(|dirs| dirs.config_dir().join("connections.json"))
    }

    /// Persists connection profiles to disk (`connections.json`).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Persistence`] if serialization or file writing fails.
    pub fn save_profiles(connections: &[ConnectionConfig]) -> AppResult<()> {
        let Some(path) = Self::profiles_file_path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                AppError::Persistence(format!("failed to create config directory: {err}"))
            })?;
        }
        let sanitized: Vec<ConnectionConfig> = connections
            .iter()
            .cloned()
            .map(|mut c| {
                c.clear_session_password();
                c
            })
            .collect();
        let serialized = serde_json::to_string_pretty(&sanitized).map_err(|err| {
            AppError::Persistence(format!("failed to serialize connections: {err}"))
        })?;
        fs::write(path, serialized)
            .map_err(|err| AppError::Persistence(format!("failed to write connections: {err}")))
    }

    /// Asynchronously persists connection profiles to disk (`connections.json`)
    /// on Tokio's blocking thread pool.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Persistence`] if serialization, file writing, or the blocking task fails.
    pub async fn save_profiles_async(connections: Vec<ConnectionConfig>) -> AppResult<()> {
        tokio::task::spawn_blocking(move || Self::save_profiles(&connections))
            .await
            .map_err(|err| AppError::Persistence(format!("save profiles task failed: {err}")))?
    }

    /// Loads saved connection profiles from disk (`connections.json`), if present.
    #[must_use]
    pub fn load_profiles() -> Option<Vec<ConnectionConfig>> {
        let path = Self::profiles_file_path()?;
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "failed to read saved connections"
                );
                return None;
            }
        };
        let profiles: Vec<ConnectionConfig> = match serde_json::from_str(&raw) {
            Ok(profiles) => profiles,
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "failed to parse saved connections"
                );
                return None;
            }
        };
        if profiles.is_empty() {
            None
        } else {
            Some(profiles)
        }
    }
}
