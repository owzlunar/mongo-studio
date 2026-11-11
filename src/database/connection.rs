use crate::core::AppResult;
use crate::models::{ConnectionConfig, ConnectionStatus, DatabaseInfo};
use uuid::Uuid;

pub struct ConnectionClient {
    pub config: ConnectionConfig,
}

impl ConnectionClient {
    pub fn from_uri(name: &str, uri: &str) -> AppResult<Self> {
        let host = uri
            .trim_start_matches("mongodb+srv://")
            .trim_start_matches("mongodb://")
            .split('/')
            .next()
            .unwrap_or("localhost:27017")
            .to_string();

        Ok(Self {
            config: ConnectionConfig {
                id: Uuid::new_v4(),
                name: name.to_string(),
                uri: uri.to_string(),
                host,
                status: ConnectionStatus::Connected,
                expanded: true,
                databases: DatabaseInfo::mock_databases(),
            },
        })
    }
}
