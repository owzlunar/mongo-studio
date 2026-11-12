use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::{AppError, AppResult};

use super::database::DatabaseInfo;
use super::user::MongoUser;

const fn default_true() -> bool {
    true
}

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
    pub username: String,
    #[serde(default)]
    pub requires_auth: bool,
    #[serde(default)]
    pub has_saved_password: bool,
    #[serde(default = "default_true")]
    pub is_admin: bool,
    #[serde(default = "default_true")]
    pub users_expanded: bool,
    #[serde(default)]
    pub users: Vec<MongoUser>,
}

impl ConnectionConfig {
    /// Maximum allowed character length for a connection profile name.
    pub const MAX_PROFILE_NAME_LEN: usize = 30;

    /// Truncates a profile name to [`Self::MAX_PROFILE_NAME_LEN`] characters.
    #[must_use]
    pub fn clamp_profile_name(name: &str) -> String {
        name.trim().chars().take(Self::MAX_PROFILE_NAME_LEN).collect()
    }

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

    /// Splits a MongoDB URI into `(base_uri_without_credentials, username, password)`
    /// for pre-filling the Edit Connection modal.
    #[must_use]
    pub fn split_uri_credentials(uri: &str) -> (String, String, String) {
        let trimmed = uri.trim();
        let (scheme, rest) = if let Some(s) = trimmed.strip_prefix("mongodb+srv://") {
            ("mongodb+srv://", s)
        } else if let Some(s) = trimmed.strip_prefix("mongodb://") {
            ("mongodb://", s)
        } else {
            ("mongodb://", trimmed)
        };

        let authority = rest.split('/').next().unwrap_or(rest);
        if authority.contains('@') {
            if let Some((creds, after_at)) = rest.split_once('@') {
                let (user, pass) = creds.split_once(':').unwrap_or((creds, ""));
                return (
                    format!("{scheme}{after_at}"),
                    user.to_string(),
                    pass.to_string(),
                );
            }
        }

        (trimmed.to_string(), String::new(), String::new())
    }

    /// Returns `true` if this connection requires authentication, does not have a saved password
    /// in the OS Keychain, and currently has no session password in `self.uri`.
    #[must_use]
    pub fn needs_auth_prompt(&self) -> bool {
        let (_base_uri, _uri_user, uri_pass) = Self::split_uri_credentials(&self.uri);
        self.requires_auth && !self.has_saved_password && uri_pass.is_empty()
    }

    /// Clears any in-memory session password from `self.uri`, keeping only the base host/path URI.
    pub fn clear_session_password(&mut self) {
        let (base_uri, uri_user, _uri_pass) = Self::split_uri_credentials(&self.uri);
        if !uri_user.is_empty() && self.username.is_empty() {
            self.username = uri_user;
        }
        self.uri = base_uri;
    }

    /// Creates a new connection profile ready to initiate a live MongoDB connection.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if `name` or `uri` is empty.
    pub fn new_profile(name: &str, uri: &str) -> AppResult<Self> {
        let clamped_name = Self::clamp_profile_name(name);
        let trimmed_uri = uri.trim();

        if clamped_name.is_empty() {
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
        let (_base_uri, uri_user, uri_pass) = Self::split_uri_credentials(trimmed_uri);
        let requires_auth = !uri_user.is_empty() || !uri_pass.is_empty();

        Ok(Self {
            id: Uuid::new_v4(),
            name: clamped_name,
            uri: trimmed_uri.to_string(),
            host,
            status: ConnectionStatus::Connecting,
            expanded: false,
            databases: Vec::new(),
            last_error: None,
            username: uri_user,
            requires_auth,
            has_saved_password: false,
            is_admin: true,
            users_expanded: false,
            users: Vec::new(),
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
            username: String::new(),
            requires_auth: false,
            has_saved_password: false,
            is_admin: true,
            users_expanded: true,
            users: MongoUser::mock_users(),
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
            username: String::new(),
            requires_auth: false,
            has_saved_password: false,
            is_admin: false,
            users_expanded: false,
            users: Vec::new(),
        }
    }
}
