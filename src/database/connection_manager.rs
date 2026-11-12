use crate::core::AppResult;
use crate::database::connection::ConnectionClient;
use crate::models::ConnectionConfig;

#[derive(Debug, Default, Clone, Copy)]
pub struct ConnectionManager;

impl ConnectionManager {
    /// Creates a new [`ConnectionConfig`] from a connection profile name and URI.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection name or URI is invalid.
    pub fn create_connection(name: &str, uri: &str) -> AppResult<ConnectionConfig> {
        let client = ConnectionClient::from_uri(name, uri)?;
        Ok(client.config)
    }
}
