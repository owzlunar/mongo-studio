use std::fmt;
use std::net::Ipv6Addr;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::{AppError, AppResult};

use super::database::DatabaseInfo;
use super::user::MongoUser;
use super::workspace::EnvironmentTag;

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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SshAuthMethod {
    #[default]
    Password,
    PrivateKey,
}

impl SshAuthMethod {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Password => "Password",
            Self::PrivateKey => "Private Key (id_ecdsa / Key)",
        }
    }
}

impl fmt::Display for SshAuthMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshTunnelConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub auth_method: SshAuthMethod,
    #[serde(default)]
    pub private_key_path: String,
    #[serde(default)]
    pub has_saved_secret: bool,
}

impl SshTunnelConfig {
    /// Default SSH port used when omitted from an endpoint string.
    pub const DEFAULT_PORT: u16 = 22;

    /// Default SSH private key path used when `private_key_path` is empty.
    pub const DEFAULT_PRIVATE_KEY_PATH: &'static str = "~/.ssh/id_ecdsa";

    /// Parses an SSH endpoint string (`ip:port`, `host`, `[ipv6]:port`, or bare `ipv6`)
    /// after stripping any optional `ssh://` prefix and `user@` authority prefix.
    ///
    /// Defaults the port to `22` when omitted.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if the host is empty, contains whitespace or control
    /// characters, starts with `-`, or has an invalid/zero port.
    pub fn parse_endpoint(raw: &str) -> AppResult<(String, u16)> {
        let trimmed = raw.trim();
        let without_scheme = if trimmed
            .get(..6)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("ssh://"))
        {
            &trimmed[6..]
        } else {
            trimmed
        };

        let without_user = without_scheme
            .rsplit_once('@')
            .map_or(without_scheme, |(_, host_part)| host_part);

        parse_host_port(without_user, Self::DEFAULT_PORT, "SSH endpoint")
    }

    /// Expands a leading `~/` in `private_key_path` to the current user's home directory,
    /// falling back to `~/.ssh/id_ecdsa` when `private_key_path.trim()` is empty.
    #[must_use]
    pub fn expanded_key_path(&self) -> PathBuf {
        let raw = self.private_key_path.trim();
        let effective = if raw.is_empty() {
            Self::DEFAULT_PRIVATE_KEY_PATH
        } else {
            raw
        };

        if effective == "~" {
            return user_home_dir().unwrap_or_else(|| PathBuf::from(effective));
        }

        if let Some(rest) = effective
            .strip_prefix("~/")
            .or_else(|| effective.strip_prefix("~\\"))
        {
            if let Some(home) = user_home_dir() {
                return home.join(rest);
            }
        }

        PathBuf::from(effective)
    }

    /// Alias for [`Self::expanded_key_path`].
    #[must_use]
    pub fn resolved_key_path(&self) -> PathBuf {
        self.expanded_key_path()
    }

    /// Validates the SSH tunnel configuration offline before attempting to spawn `ssh`.
    ///
    /// When `self.enabled` is `false`, returns `Ok((String::new(), 22))`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if the endpoint, username, or authentication settings
    /// are invalid or incomplete.
    pub fn validate_offline(&self, secret: &str) -> AppResult<(String, u16)> {
        if !self.enabled {
            return Ok((String::new(), Self::DEFAULT_PORT));
        }

        let (host, port) = Self::parse_endpoint(&self.endpoint)?;

        let username = self.username.trim();
        if username.is_empty() {
            return Err(AppError::Connection(
                "SSH username must not be empty".to_string(),
            ));
        }
        if username.starts_with('-') {
            return Err(AppError::Connection(
                "SSH username must not start with '-'".to_string(),
            ));
        }
        if username
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(AppError::Connection(
                "SSH username must not contain whitespace or control characters".to_string(),
            ));
        }

        match self.auth_method {
            SshAuthMethod::Password => {
                if secret.is_empty() {
                    return Err(AppError::Connection(
                        "SSH password is required for password authentication".to_string(),
                    ));
                }
            }
            SshAuthMethod::PrivateKey => {
                let raw_key = self.private_key_path.trim();
                if raw_key.chars().any(|c| c.is_control()) {
                    return Err(AppError::Connection(
                        "SSH private key path contains invalid control characters".to_string(),
                    ));
                }
                let key_path = self.expanded_key_path();
                if key_path.as_os_str().is_empty() {
                    return Err(AppError::Connection(
                        "SSH private key path must not be empty".to_string(),
                    ));
                }
            }
        }

        Ok((host, port))
    }

    /// Returns `true` when the connection requires an interactive SSH password prompt before
    /// connecting (enabled, no saved secret in OS Keychain, empty session secret, and
    /// [`SshAuthMethod::Password`]).
    #[must_use]
    pub fn needs_secret_prompt(&self, session_secret: &str) -> bool {
        self.enabled
            && !self.has_saved_secret
            && session_secret.is_empty()
            && self.auth_method == SshAuthMethod::Password
    }
}

fn user_home_dir() -> Option<PathBuf> {
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

fn parse_host_port(raw: &str, default_port: u16, label: &str) -> AppResult<(String, u16)> {
    if raw.is_empty() {
        return Err(AppError::Connection(format!("{label} must not be empty")));
    }

    let (host, port) = if let Some(after_open) = raw.strip_prefix('[') {
        let Some((bracketed_host, rest)) = after_open.split_once(']') else {
            return Err(AppError::Connection(format!(
                "invalid {label}: missing closing ']'"
            )));
        };

        let port = if rest.is_empty() {
            default_port
        } else if let Some(port_str) = rest.strip_prefix(':') {
            parse_valid_port(port_str, label)?
        } else {
            return Err(AppError::Connection(format!(
                "invalid {label}: unexpected trailing characters after ']'"
            )));
        };

        (bracketed_host, port)
    } else if raw.parse::<Ipv6Addr>().is_ok() {
        (raw, default_port)
    } else if raw.matches(':').count() > 1 {
        return Err(AppError::Connection(format!(
            "invalid {label}: wrap IPv6 addresses in brackets like [::1]:{default_port}"
        )));
    } else if let Some((host_part, port_str)) = raw.split_once(':') {
        let port = parse_valid_port(port_str, label)?;
        (host_part, port)
    } else {
        (raw, default_port)
    };

    if host.is_empty() {
        return Err(AppError::Connection(format!(
            "{label} host must not be empty"
        )));
    }
    if host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(AppError::Connection(format!(
            "{label} host must not contain whitespace or control characters"
        )));
    }
    if host.starts_with('-') {
        return Err(AppError::Connection(format!(
            "{label} host must not start with '-'"
        )));
    }
    if host.contains('/') || host.contains('?') {
        return Err(AppError::Connection(format!("invalid {label} host")));
    }

    Ok((host.to_string(), port))
}

fn parse_valid_port(port_str: &str, label: &str) -> AppResult<u16> {
    if port_str.is_empty() {
        return Err(AppError::Connection(format!(
            "invalid {label}: port must not be empty after ':'"
        )));
    }
    let port = port_str
        .parse::<u16>()
        .map_err(|_| AppError::Connection(format!("invalid {label} port '{port_str}'")))?;
    if port == 0 {
        return Err(AppError::Connection(format!(
            "invalid {label} port: port must be between 1 and 65535"
        )));
    }
    Ok(port)
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
    #[serde(default)]
    pub ssh_tunnel: SshTunnelConfig,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub folder: Option<String>,
    #[serde(default)]
    pub is_favorite: bool,
    #[serde(default)]
    pub environment: Option<EnvironmentTag>,
}

impl ConnectionConfig {
    /// Maximum allowed character length for a connection profile name.
    pub const MAX_PROFILE_NAME_LEN: usize = 30;

    /// Default MongoDB port used when omitted from a `mongodb://` URI.
    pub const DEFAULT_MONGO_PORT: u16 = 27017;

    /// Truncates a profile name to [`Self::MAX_PROFILE_NAME_LEN`] characters.
    #[must_use]
    pub fn clamp_profile_name(name: &str) -> String {
        name.trim()
            .chars()
            .take(Self::MAX_PROFILE_NAME_LEN)
            .collect()
    }

    /// Returns the explicit environment tag if set, or attempts to infer one from the connection name.
    #[must_use]
    pub fn effective_environment(&self) -> Option<EnvironmentTag> {
        self.environment
            .clone()
            .or_else(|| EnvironmentTag::detect_from_name(&self.name))
    }

    /// Returns the trimmed workspace name, or `None` if empty or whitespace.
    #[must_use]
    pub fn workspace_name(&self) -> Option<&str> {
        self.workspace
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// Returns the trimmed folder name, or `None` if empty or whitespace.
    #[must_use]
    pub fn folder_name(&self) -> Option<&str> {
        self.folder
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
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

        let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
        let (authority, suffix) = rest.split_at(authority_end);
        if let Some((creds, host_port)) = authority.rsplit_once('@') {
            if let Some((user, _pass)) = creds.split_once(':') {
                return format!("{scheme}{user}:****@{host_port}{suffix}");
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

    /// Extracts the single target `(host, port)` from a standard `mongodb://` URI for SSH
    /// local port forwarding.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if the URI is empty, uses `mongodb+srv://`, uses an
    /// unsupported scheme, specifies a multi-host comma seedlist, or has an invalid host/port.
    pub fn extract_mongo_target_host_port(uri: &str) -> AppResult<(String, u16)> {
        let (_, authority, _) = split_mongo_uri_parts(uri)?;
        let hosts_part = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);

        if hosts_part.contains(',') {
            return Err(AppError::Connection(
                "SSH tunnel requires a single MongoDB host:port URI (multi-host seedlists cannot be forwarded over a single SSH port)".to_string(),
            ));
        }

        parse_host_port(hosts_part, Self::DEFAULT_MONGO_PORT, "MongoDB target")
    }

    /// Rewrites a `mongodb://` URI to connect through a local SSH-forwarded port
    /// (`127.0.0.1:{local_port}`), preserving credentials, database path, and query options
    /// while ensuring `directConnection=true` is set.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if `local_port == 0` or the URI is invalid or uses
    /// `mongodb+srv://`.
    pub fn rewrite_uri_for_tunnel(uri: &str, local_port: u16) -> AppResult<String> {
        if local_port == 0 {
            return Err(AppError::Connection(
                "invalid SSH tunnel local port 0".to_string(),
            ));
        }

        let _ = Self::extract_mongo_target_host_port(uri)?;

        let (_, authority, suffix) = split_mongo_uri_parts(uri)?;
        let creds_prefix = authority.rsplit_once('@').map(|(creds, _)| creds);

        let (raw_path, raw_query) = match suffix.split_once('?') {
            Some((path, query)) => (path, Some(query)),
            None => (suffix, None),
        };

        let path = if raw_path.is_empty() { "/" } else { raw_path };
        let final_query = match raw_query {
            Some(q) => {
                let cleaned = q.trim_matches('&');
                if cleaned.is_empty() {
                    "directConnection=true".to_string()
                } else if has_query_param(cleaned, "directConnection") {
                    cleaned.to_string()
                } else {
                    format!("{cleaned}&directConnection=true")
                }
            }
            None => "directConnection=true".to_string(),
        };

        let mut rewritten = String::with_capacity(
            "mongodb://127.0.0.1:".len()
                + creds_prefix.map_or(0, |c| c.len() + 1)
                + 5
                + path.len()
                + 1
                + final_query.len(),
        );
        rewritten.push_str("mongodb://");
        if let Some(creds) = creds_prefix {
            rewritten.push_str(creds);
            rewritten.push('@');
        }
        rewritten.push_str("127.0.0.1:");
        rewritten.push_str(&local_port.to_string());
        rewritten.push_str(path);
        rewritten.push('?');
        rewritten.push_str(&final_query);

        Ok(rewritten)
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
            ssh_tunnel: SshTunnelConfig::default(),
            workspace: None,
            folder: None,
            is_favorite: false,
            environment: None,
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
            ssh_tunnel: SshTunnelConfig::default(),
            workspace: Some("Default".to_string()),
            folder: None,
            is_favorite: false,
            environment: Some(EnvironmentTag::Dev),
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
            ssh_tunnel: SshTunnelConfig::default(),
            workspace: Some("Staging".to_string()),
            folder: None,
            is_favorite: false,
            environment: Some(EnvironmentTag::Test),
        }
    }
}

fn split_mongo_uri_parts(uri: &str) -> AppResult<(&str, &str, &str)> {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return Err(AppError::Connection(
            "MongoDB URI must not be empty".to_string(),
        ));
    }

    if trimmed.starts_with("mongodb+srv://") {
        return Err(AppError::Connection(
            "SSH tunnel does not support mongodb+srv:// DNS seedlist URIs; use a standard mongodb://host:port URI".to_string(),
        ));
    }

    let rest = if let Some(stripped) = trimmed.strip_prefix("mongodb://") {
        stripped
    } else if trimmed.contains("://") {
        return Err(AppError::Connection(
            "unsupported MongoDB URI scheme; expected mongodb://".to_string(),
        ));
    } else {
        trimmed
    };

    let split_idx = rest.find(['/', '?']).unwrap_or(rest.len());
    let authority = &rest[..split_idx];
    let suffix = &rest[split_idx..];

    Ok(("mongodb://", authority, suffix))
}

fn has_query_param(query: &str, key: &str) -> bool {
    query.split('&').any(|pair| {
        let param_key = pair.split_once('=').map_or(pair, |(k, _)| k);
        param_key.eq_ignore_ascii_case(key)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_endpoint_supports_ipv4_hostname_ipv6_and_prefixes() {
        assert_eq!(
            SshTunnelConfig::parse_endpoint("192.168.10.5:22").ok(),
            Some(("192.168.10.5".to_string(), 22))
        );
        assert_eq!(
            SshTunnelConfig::parse_endpoint("bastion.example.com").ok(),
            Some(("bastion.example.com".to_string(), 22))
        );
        assert_eq!(
            SshTunnelConfig::parse_endpoint("[::1]:2222").ok(),
            Some(("::1".to_string(), 2222))
        );
        assert_eq!(
            SshTunnelConfig::parse_endpoint("::1").ok(),
            Some(("::1".to_string(), 22))
        );
        assert_eq!(
            SshTunnelConfig::parse_endpoint("ssh://ubuntu@10.0.0.8:2200").ok(),
            Some(("10.0.0.8".to_string(), 2200))
        );
    }

    #[test]
    fn parse_endpoint_rejects_invalid_inputs() {
        assert!(SshTunnelConfig::parse_endpoint("").is_err());
        assert!(SshTunnelConfig::parse_endpoint("host:0").is_err());
        assert!(SshTunnelConfig::parse_endpoint("host:notaport").is_err());
        assert!(SshTunnelConfig::parse_endpoint("host:").is_err());
        assert!(SshTunnelConfig::parse_endpoint("bad host:22").is_err());
        assert!(SshTunnelConfig::parse_endpoint("-evilhost:22").is_err());
    }

    #[test]
    fn validate_offline_and_secret_prompt_behave_as_expected() {
        let disabled = SshTunnelConfig::default();
        assert_eq!(
            disabled.validate_offline("").ok(),
            Some((String::new(), 22))
        );
        assert!(!disabled.needs_secret_prompt(""));

        let password_cfg = SshTunnelConfig {
            enabled: true,
            endpoint: "10.0.0.1:22".to_string(),
            username: "admin".to_string(),
            auth_method: SshAuthMethod::Password,
            private_key_path: String::new(),
            has_saved_secret: false,
        };
        assert!(password_cfg.needs_secret_prompt(""));
        assert!(!password_cfg.needs_secret_prompt("hunter2"));
        assert!(password_cfg.validate_offline("").is_err());
        assert_eq!(
            password_cfg.validate_offline("hunter2").ok(),
            Some(("10.0.0.1".to_string(), 22))
        );

        let key_cfg = SshTunnelConfig {
            enabled: true,
            endpoint: "bastion.internal".to_string(),
            username: "ubuntu".to_string(),
            auth_method: SshAuthMethod::PrivateKey,
            private_key_path: "~/.ssh/id_ecdsa".to_string(),
            has_saved_secret: false,
        };
        assert!(!key_cfg.needs_secret_prompt(""));
        assert_eq!(
            key_cfg.validate_offline("").ok(),
            Some(("bastion.internal".to_string(), 22))
        );
    }

    #[test]
    fn mongo_target_extraction_and_rewrite_preserve_uri_parts() {
        assert_eq!(
            ConnectionConfig::extract_mongo_target_host_port(
                "mongodb://user:pass@db.internal:27018/prod?authSource=admin"
            )
            .ok(),
            Some(("db.internal".to_string(), 27018))
        );
        assert_eq!(
            ConnectionConfig::extract_mongo_target_host_port("mongodb://[::1]/app").ok(),
            Some(("::1".to_string(), 27017))
        );
        assert!(ConnectionConfig::extract_mongo_target_host_port(
            "mongodb+srv://cluster.example.net/prod"
        )
        .is_err());
        assert!(ConnectionConfig::extract_mongo_target_host_port(
            "mongodb://n1:27017,n2:27017/prod"
        )
        .is_err());

        assert_eq!(
            ConnectionConfig::rewrite_uri_for_tunnel(
                "mongodb://admin:secret@10.20.30.40:27018/appdb?authSource=admin",
                54321
            )
            .ok(),
            Some(
                "mongodb://admin:secret@127.0.0.1:54321/appdb?authSource=admin&directConnection=true"
                    .to_string()
            )
        );
        assert_eq!(
            ConnectionConfig::rewrite_uri_for_tunnel(
                "mongodb://10.20.30.40?directConnection=true&tls=true",
                54321
            )
            .ok(),
            Some("mongodb://127.0.0.1:54321/?directConnection=true&tls=true".to_string())
        );
        assert!(ConnectionConfig::rewrite_uri_for_tunnel("mongodb://10.0.0.1:27017", 0).is_err());
    }

    #[test]
    fn connection_config_deserializes_legacy_json_without_ssh_tunnel() {
        let legacy_json = r#"{
            "id": "00000000-0000-0000-0000-000000000001",
            "name": "Legacy",
            "uri": "mongodb://localhost:27017",
            "host": "localhost:27017",
            "status": "Disconnected",
            "expanded": false,
            "databases": []
        }"#;
        let parsed: ConnectionConfig =
            serde_json::from_str(legacy_json).expect("legacy profile should deserialize");
        assert!(!parsed.ssh_tunnel.enabled);
        assert_eq!(parsed.ssh_tunnel.auth_method, SshAuthMethod::Password);
        assert_eq!(parsed.workspace, None);
        assert_eq!(parsed.folder, None);
        assert!(!parsed.is_favorite);
        assert_eq!(parsed.environment, None);
    }

    #[test]
    fn connection_config_workspace_and_environment_helpers() {
        let mut conn =
            ConnectionConfig::new_profile("billing-prod", "mongodb://localhost:27017").unwrap();
        assert_eq!(conn.effective_environment(), Some(EnvironmentTag::Prod));
        assert_eq!(conn.workspace_name(), None);
        assert_eq!(conn.folder_name(), None);

        conn.workspace = Some("  Finance  ".to_string());
        conn.folder = Some("  Payments  ".to_string());
        conn.environment = Some(EnvironmentTag::Dev);

        assert_eq!(conn.workspace_name(), Some("Finance"));
        assert_eq!(conn.folder_name(), Some("Payments"));
        assert_eq!(conn.effective_environment(), Some(EnvironmentTag::Dev));
        assert!(!conn.is_favorite);
    }
}
