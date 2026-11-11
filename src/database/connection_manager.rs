use crate::core::AppResult;
use crate::database::connection::ConnectionClient;
use crate::models::ConnectionConfig;

pub struct ConnectionManager;

impl ConnectionManager {
    pub fn create_connection(name: &str, uri: &str) -> AppResult<ConnectionConfig> {
        let client = ConnectionClient::from_uri(name, uri)?;
        Ok(client.config)
    }
}
