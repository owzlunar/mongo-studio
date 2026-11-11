use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::database::DatabaseInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Connecting,
    Error,
}

impl ConnectionStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Connected => "Connected",
            Self::Disconnected => "Disconnected",
            Self::Connecting => "Connecting...",
            Self::Error => "Connection Error",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub id: Uuid,
    pub name: String,
    pub uri: String,
    pub host: String,
    pub status: ConnectionStatus,
    pub expanded: bool,
    pub databases: Vec<DatabaseInfo>,
}

impl ConnectionConfig {
    pub fn mock_local() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "MongoDB Local".to_string(),
            uri: "mongodb://localhost:27017".to_string(),
            host: "localhost:27017".to_string(),
            status: ConnectionStatus::Connected,
            expanded: true,
            databases: DatabaseInfo::mock_databases(),
        }
    }

    pub fn mock_staging() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "Staging Cluster".to_string(),
            uri: "mongodb+srv://staging.cluster.local:27017".to_string(),
            host: "staging.cluster.local:27017".to_string(),
            status: ConnectionStatus::Disconnected,
            expanded: false,
            databases: vec![DatabaseInfo::new(
                "analytics_db",
                false,
                vec!["events", "sessions", "metrics"],
            )],
        }
    }
}
