use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::{AppError, AppResult};

use super::database::DatabaseInfo;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Connected,
    #[default]
    Disconnected,
    Connecting,
    Error,
}

impl ConnectionStatus {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Connected => "Connected",
            Self::Disconnected => "Disconnected",
            Self::Connecting => "Connecting...",
            Self::Error => "Connection Error",
        }
    }

    #[must_use]
    pub const fn is_connected(self) -> bool {
        matches!(self, Self::Connected)
    }
}

impl fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub id: Uuid,
    pub name: String,
    pub uri: String,
    pub host: String,
    pub status: ConnectionStatus,
    pub expanded: bool,
    pub databases: Vec<DatabaseInfo>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub has_saved_password: bool,
}

impl ConnectionConfig {
    /// Extracts the display host from a MongoDB URI, stripping any `mongodb://` or
    /// `mongodb+srv://` scheme, path/query parameters, and `user:pass@` credentials.
    #[must_use]
    pub fn extract_display_host(uri: &str) -> String {
        let trimmed = uri.trim();
        let without_scheme = trimmed
            .strip_prefix("mongodb+srv://")
            .or_else(|| trimmed.strip_prefix("mongodb://"))
            .unwrap_or(trimmed);

        let authority = without_scheme
            .split('/')
            .next()
            .unwrap_or("")
            .split('?')
            .next()
            .unwrap_or("");

        let host_only = authority.rsplit('@').next().unwrap_or(authority).trim();

        if host_only.is_empty() {
            "localhost:27017".to_string()
        } else {
            host_only.to_string()
        }
    }

    /// Normalizes a user-supplied URI or host:port string into a valid MongoDB URI,
    /// optionally injecting `username` and `password` if not already present in the URI.
    #[must_use]
    pub fn normalize_uri(raw_uri: &str, username: &str, password: &str) -> String {
        let trimmed = raw_uri.trim();
        if trimmed.is_empty() {
            return "mongodb://localhost:27017".to_string();
        }

        let (scheme, rest) = if let Some(stripped) = trimmed.strip_prefix("mongodb+srv://") {
            ("mongodb+srv://", stripped)
        } else if let Some(stripped) = trimmed.strip_prefix("mongodb://") {
            ("mongodb://", stripped)
        } else if trimmed.starts_with('/') {
            ("mongodb://", "")
        } else {
            ("mongodb://", trimmed)
        };

        let host_and_path = if rest.is_empty() {
            "localhost:27017"
        } else {
            rest
        };

        let authority = host_and_path.split('/').next().unwrap_or(host_and_path);
        let user_trimmed = username.trim();

        if !user_trimmed.is_empty() && !authority.contains('@') {
            if password.is_empty() {
                format!("{scheme}{user_trimmed}@{host_and_path}")
            } else {
                format!("{scheme}{user_trimmed}:{password}@{host_and_path}")
            }
        } else {
            format!("{scheme}{host_and_path}")
        }
    }

    /// Returns a masked version of a URI safe for displaying in logs and UI tooltips.
    #[must_use]
    pub fn redact_uri(uri: &str) -> String {
        let trimmed = uri.trim();
        let (scheme, rest) = if let Some(s) = trimmed.strip_prefix("mongodb+srv://") {
            ("mongodb+srv://", s)
        } else if let Some(s) = trimmed.strip_prefix("mongodb://") {
            ("mongodb://", s)
        } else {
            ("", trimmed)
        };

        if let Some((creds, after_at)) = rest.split_once('@') {
            if let Some((user, _pass)) = creds.split_once(':') {
                return format!("{scheme}{user}:****@{after_at}");
            }
        }
        trimmed.to_string()
    }

    /// Creates a new connection profile ready to initiate a live MongoDB connection.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if `name` or `uri` is empty.
    pub fn new_profile(name: &str, uri: &str) -> AppResult<Self> {
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

        let host = Self::extract_display_host(trimmed_uri);

        Ok(Self {
            id: Uuid::new_v4(),
            name: trimmed_name.to_string(),
            uri: trimmed_uri.to_string(),
            host,
            status: ConnectionStatus::Connecting,
            expanded: true,
            databases: Vec::new(),
            last_error: None,
            has_saved_password: false,
        })
    }

    /// Alias for [`Self::new_profile`] for ergonomic construction from a URI.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if `name` or `uri` is empty.
    pub fn from_uri(name: &str, uri: &str) -> AppResult<Self> {
        Self::new_profile(name, uri)
    }

    #[must_use]
    pub fn mock_local() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "MongoDB Local".to_string(),
            uri: "mongodb://localhost:27017".to_string(),
            host: "localhost:27017".to_string(),
            status: ConnectionStatus::Connected,
            expanded: true,
            databases: DatabaseInfo::mock_databases(),
            last_error: None,
            has_saved_password: false,
        }
    }

    #[must_use]
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
                ["events", "sessions", "metrics"],
            )],
            last_error: None,
            has_saved_password: false,
        }
    }
}
