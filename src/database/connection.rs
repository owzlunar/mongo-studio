use crate::core::{AppError, AppResult};
use crate::models::{ConnectionConfig, ConnectionStatus, DatabaseInfo};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ConnectionClient {
    pub config: ConnectionConfig,
}

impl ConnectionClient {
    /// Creates a new [`ConnectionClient`] from a connection name and MongoDB URI.
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

        let host = trimmed_uri
            .trim_start_matches("mongodb+srv://")
            .trim_start_matches("mongodb://")
            .split('/')
            .next()
            .filter(|h| !h.is_empty())
            .unwrap_or("localhost:27017")
            .to_string();

        Ok(Self {
            config: ConnectionConfig {
                id: Uuid::new_v4(),
                name: trimmed_name.to_string(),
                uri: trimmed_uri.to_string(),
                host,
                status: ConnectionStatus::Connected,
                expanded: true,
                databases: DatabaseInfo::mock_databases(),
            },
        })
    }
}
