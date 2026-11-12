use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use directories::ProjectDirs;
use uuid::Uuid;

use crate::core::{AppError, AppResult};
use crate::database::connection::ConnectionClient;
use crate::models::ConnectionConfig;

const KEYRING_SERVICE: &str = "com.mongostudio.MongoStudio";

#[derive(Debug, Default, Clone, Copy)]
pub struct ConnectionManager;

impl ConnectionManager {
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
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| {
                AppError::Connection(format!("failed to build async runtime: {err}"))
            })?;
        runtime.block_on(ConnectionClient::ping_uri(uri, timeout))
    }

    /// Synchronously connects to a live MongoDB server and introspects its databases and collections.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if the connection or introspection fails.
    pub fn connect_live_blocking(
        name: &str,
        uri: &str,
        timeout: Duration,
    ) -> AppResult<ConnectionConfig> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| {
                AppError::Connection(format!("failed to build async runtime: {err}"))
            })?;
        runtime.block_on(ConnectionClient::connect_and_introspect(name, uri, timeout))
    }

    /// Saves a connection password in the OS credential keyring.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Persistence`] if the keyring entry cannot be created or written.
    pub fn save_password_to_keyring(conn_id: Uuid, password: &str) -> AppResult<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &conn_id.to_string())
            .map_err(|err| AppError::Persistence(format!("keyring init failed: {err}")))?;
        entry
            .set_password(password)
            .map_err(|err| AppError::Persistence(format!("keyring save failed: {err}")))
    }

    /// Loads a saved connection password from the OS credential keyring, if available.
    #[must_use]
    pub fn load_password_from_keyring(conn_id: Uuid) -> Option<String> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &conn_id.to_string()).ok()?;
        entry.get_password().ok()
    }

    /// Deletes a saved connection password from the OS credential keyring.
    pub fn delete_password_from_keyring(conn_id: Uuid) {
        if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, &conn_id.to_string()) {
            let _ = entry.delete_password();
        }
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
        let serialized = serde_json::to_string_pretty(connections).map_err(|err| {
            AppError::Persistence(format!("failed to serialize connections: {err}"))
        })?;
        fs::write(path, serialized)
            .map_err(|err| AppError::Persistence(format!("failed to write connections: {err}")))
    }

    /// Loads saved connection profiles from disk (`connections.json`), if present.
    #[must_use]
    pub fn load_profiles() -> Option<Vec<ConnectionConfig>> {
        let path = Self::profiles_file_path()?;
        let raw = fs::read_to_string(path).ok()?;
        let profiles: Vec<ConnectionConfig> = serde_json::from_str(&raw).ok()?;
        if profiles.is_empty() {
            None
        } else {
            Some(profiles)
        }
    }
}
