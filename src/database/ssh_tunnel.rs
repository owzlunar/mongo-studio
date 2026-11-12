use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::core::{AppError, AppResult};
use crate::database::connection::ConnectionClient;
use crate::models::{ConnectionConfig, SshAuthMethod, SshTunnelConfig};

const MIN_TUNNEL_WAIT: Duration = Duration::from_secs(8);
const TUNNEL_POLL_INTERVAL: Duration = Duration::from_millis(40);
const LOCAL_PROBE_TIMEOUT: Duration = Duration::from_millis(120);
const MAX_STDERR_BYTES: u64 = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
struct TunnelSignature {
    ssh_host: String,
    ssh_port: u16,
    ssh_user: String,
    remote_host: String,
    remote_port: u16,
    auth_method: SshAuthMethod,
    key_path: Option<PathBuf>,
}

#[derive(Debug)]
struct ActiveTunnel {
    local_port: u16,
    child: Child,
    signature: TunnelSignature,
    askpass_dir: Option<PathBuf>,
}

impl Drop for ActiveTunnel {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(dir) = self.askpass_dir.take() {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

struct AskpassTempDir {
    dir: Option<PathBuf>,
}

impl AskpassTempDir {
    fn create() -> AppResult<(Self, PathBuf)> {
        let dir = std::env::temp_dir().join(format!("mongo_studio_askpass_{}", Uuid::new_v4()));
        #[cfg(unix)]
        {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&dir)
                .map_err(|err| {
                    AppError::Connection(format!("failed to create SSH askpass directory: {err}"))
                })?;
        }
        #[cfg(not(unix))]
        {
            fs::create_dir(&dir).map_err(|err| {
                AppError::Connection(format!("failed to create SSH askpass directory: {err}"))
            })?;
        }

        let guard = Self {
            dir: Some(dir.clone()),
        };

        #[cfg(windows)]
        let script_name = "askpass.cmd";
        #[cfg(not(windows))]
        let script_name = "askpass.sh";

        let script_path = dir.join(script_name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o700);

        let mut file = options.open(&script_path).map_err(|err| {
            AppError::Connection(format!("failed to create SSH askpass script: {err}"))
        })?;
        // Never write secrets to disk: OpenSSH executes this helper, which reads the env var.
        #[cfg(windows)]
        let script_body: &[u8] = b"@echo off\r\n\"%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" -NoProfile -NonInteractive -Command \"[Console]::Out.WriteLine([Environment]::GetEnvironmentVariable('MONGO_STUDIO_SSH_ASKPASS_SECRET'))\"\r\n";
        #[cfg(not(windows))]
        let script_body: &[u8] =
            b"#!/bin/sh\nprintf '%s\\n' \"$MONGO_STUDIO_SSH_ASKPASS_SECRET\"\n";

        file.write_all(script_body).map_err(|err| {
            AppError::Connection(format!("failed to write SSH askpass script: {err}"))
        })?;
        file.flush().map_err(|err| {
            AppError::Connection(format!("failed to flush SSH askpass script: {err}"))
        })?;

        Ok((guard, script_path))
    }

    fn cleanup(&mut self) {
        if let Some(dir) = self.dir.take() {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

impl Drop for AskpassTempDir {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[derive(Debug, Clone)]
struct TunnelLaunchParams {
    ssh_host: String,
    ssh_port: u16,
    ssh_user: String,
    remote_host: String,
    remote_port: u16,
    auth_method: SshAuthMethod,
    key_path: Option<PathBuf>,
    has_secret: bool,
}

impl TunnelLaunchParams {
    fn signature(&self) -> TunnelSignature {
        TunnelSignature {
            ssh_host: self.ssh_host.clone(),
            ssh_port: self.ssh_port,
            ssh_user: self.ssh_user.clone(),
            remote_host: self.remote_host.clone(),
            remote_port: self.remote_port,
            auth_method: self.auth_method,
            key_path: self.key_path.clone(),
        }
    }
}

/// Thread-safe registry of active OpenSSH local port-forwarding tunnels keyed by connection ID.
#[derive(Debug, Default, Clone)]
pub struct SshTunnelRegistry {
    inner: Arc<Mutex<HashMap<Uuid, ActiveTunnel>>>,
}

impl SshTunnelRegistry {
    /// Creates a new empty [`SshTunnelRegistry`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Closes and kills any active SSH tunnel associated with `conn_id`.
    pub fn close_tunnel(&self, conn_id: Uuid) {
        let removed = {
            let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            guard.remove(&conn_id)
        };
        drop(removed);
    }

    /// Closes and kills all active SSH tunnels managed by this registry.
    pub fn close_all(&self) {
        let drained: Vec<ActiveTunnel> = {
            let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            guard.drain().map(|(_, tunnel)| tunnel).collect()
        };
        drop(drained);
    }

    /// Returns the local forwarded port for `conn_id` if its `ssh` process is still running.
    #[must_use]
    pub fn active_local_port(&self, conn_id: Uuid) -> Option<u16> {
        let mut stale = None;
        let port = {
            let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(tunnel) = guard.get_mut(&conn_id) {
                if matches!(tunnel.child.try_wait(), Ok(None)) {
                    Some(tunnel.local_port)
                } else {
                    stale = guard.remove(&conn_id);
                    None
                }
            } else {
                None
            }
        };
        drop(stale);
        port
    }

    /// Ensures an encrypted OpenSSH local port-forwarding tunnel is active for `conn_id`
    /// when `ssh_config.enabled` is `true`, and rewrites `raw_mongo_uri` to route through
    /// `127.0.0.1:<local_port>` with `directConnection=true`.
    ///
    /// When `ssh_config.enabled` is `false`, returns `raw_mongo_uri` unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if SSH validation fails, the private key is missing or
    /// overly permissive, `ssh` cannot be launched, or tunnel readiness times out.
    pub async fn ensure_tunnel_and_rewrite_uri_async(
        &self,
        conn_id: Uuid,
        raw_mongo_uri: String,
        ssh_config: SshTunnelConfig,
        ssh_secret: String,
        timeout: Duration,
    ) -> AppResult<String> {
        if !ssh_config.enabled {
            return Ok(raw_mongo_uri);
        }

        let (ssh_host, ssh_port) = ssh_config.validate_offline(&ssh_secret)?;
        let (remote_host, remote_port) =
            ConnectionConfig::extract_mongo_target_host_port(&raw_mongo_uri)?;
        let key_path = (ssh_config.auth_method == SshAuthMethod::PrivateKey)
            .then(|| ssh_config.expanded_key_path());

        let params = TunnelLaunchParams {
            ssh_host,
            ssh_port,
            ssh_user: ssh_config.username.trim().to_string(),
            remote_host,
            remote_port,
            auth_method: ssh_config.auth_method,
            key_path,
            has_secret: !ssh_secret.is_empty(),
        };
        let desired_sig = params.signature();

        // Reuse existing live tunnel if its signature still matches.
        let mut stale = None;
        let existing_port = {
            let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(tunnel) = guard.get_mut(&conn_id) {
                let alive = matches!(tunnel.child.try_wait(), Ok(None));
                if alive && tunnel.signature == desired_sig {
                    Some(tunnel.local_port)
                } else {
                    stale = guard.remove(&conn_id);
                    None
                }
            } else {
                None
            }
        };
        drop(stale);

        if let Some(local_port) = existing_port {
            return ConnectionConfig::rewrite_uri_for_tunnel(&raw_mongo_uri, local_port);
        }

        let new_tunnel = tokio::task::spawn_blocking(move || {
            spawn_ssh_tunnel_blocking(params, ssh_secret, timeout)
        })
        .await
        .map_err(|err| AppError::Connection(format!("SSH tunnel task failed: {err}")))??;

        let local_port = new_tunnel.local_port;
        let replaced = {
            let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            guard.insert(conn_id, new_tunnel)
        };
        drop(replaced);

        ConnectionConfig::rewrite_uri_for_tunnel(&raw_mongo_uri, local_port)
    }

    /// Tests connectivity to a MongoDB deployment through an ephemeral SSH tunnel that is
    /// guaranteed to be terminated once the ping completes (or fails).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if establishing the SSH tunnel or pinging MongoDB fails.
    pub async fn ping_through_ssh_async(
        raw_mongo_uri: String,
        ssh_config: SshTunnelConfig,
        ssh_secret: String,
        timeout: Duration,
    ) -> AppResult<f64> {
        let registry = Self::new();
        let ephemeral_id = Uuid::new_v4();
        let effective_uri = registry
            .ensure_tunnel_and_rewrite_uri_async(
                ephemeral_id,
                raw_mongo_uri,
                ssh_config,
                ssh_secret,
                timeout,
            )
            .await?;

        let ping_result = ConnectionClient::ping_uri(&effective_uri, timeout).await;
        registry.close_tunnel(ephemeral_id);
        ping_result
    }
}

fn spawn_ssh_tunnel_blocking(
    params: TunnelLaunchParams,
    ssh_secret: String,
    timeout: Duration,
) -> AppResult<ActiveTunnel> {
    if params.auth_method == SshAuthMethod::PrivateKey {
        if let Some(ref key_path) = params.key_path {
            validate_private_key_file(key_path)?;
        }
    }

    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|err| {
        AppError::Connection(format!(
            "failed to reserve local port for SSH tunnel: {err}"
        ))
    })?;
    let local_port = listener
        .local_addr()
        .map_err(|err| AppError::Connection(format!("failed to read reserved local port: {err}")))?
        .port();
    drop(listener);

    let ready_timeout = timeout.max(MIN_TUNNEL_WAIT);
    let connect_timeout_secs = ready_timeout.as_secs().max(1);
    let args = build_ssh_args(&params, local_port, connect_timeout_secs);

    let mut cmd = Command::new("ssh");
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        // Suppress blank console flash when a GUI app spawns Win32 OpenSSH.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let needs_askpass = params.auth_method == SshAuthMethod::Password || params.has_secret;
    let mut askpass_guard = if needs_askpass {
        let (guard, script_path) = AskpassTempDir::create()?;
        cmd.env("SSH_ASKPASS", &script_path)
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env("DISPLAY", "1")
            .env("MONGO_STUDIO_SSH_ASKPASS_SECRET", &ssh_secret);
        Some(guard)
    } else {
        None
    };

    let mut child = cmd.spawn().map_err(|err| {
        AppError::Connection(format!("failed to launch OpenSSH `ssh` client: {err}"))
    })?;

    let deadline = Instant::now() + ready_timeout;
    let probe_addr = SocketAddr::from(([127, 0, 0, 1], local_port));

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stderr = read_limited_stderr(&mut child);
                if let Some(ref mut guard) = askpass_guard {
                    guard.cleanup();
                }
                let detail = if stderr.is_empty() {
                    format!("process exited with {status}")
                } else {
                    stderr
                };
                return Err(AppError::Connection(format!("SSH tunnel failed: {detail}")));
            }
            Ok(None) => {
                if let Ok(stream) = TcpStream::connect_timeout(&probe_addr, LOCAL_PROBE_TIMEOUT) {
                    drop(stream);
                    if let Some(ref mut guard) = askpass_guard {
                        guard.cleanup();
                    }
                    return Ok(ActiveTunnel {
                        local_port,
                        child,
                        signature: params.signature(),
                        askpass_dir: askpass_guard.and_then(|mut g| g.dir.take()),
                    });
                }

                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let stderr = read_limited_stderr(&mut child);
                    if let Some(ref mut guard) = askpass_guard {
                        guard.cleanup();
                    }
                    let detail = if stderr.is_empty() {
                        format!("timed out after {}s", ready_timeout.as_secs())
                    } else {
                        format!("timed out after {}s: {stderr}", ready_timeout.as_secs())
                    };
                    return Err(AppError::Connection(format!("SSH tunnel failed: {detail}")));
                }

                std::thread::sleep(TUNNEL_POLL_INTERVAL);
            }
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                if let Some(ref mut guard) = askpass_guard {
                    guard.cleanup();
                }
                return Err(AppError::Connection(format!(
                    "failed to monitor SSH tunnel process: {err}"
                )));
            }
        }
    }
}

fn validate_private_key_file(key_path: &Path) -> AppResult<()> {
    let metadata = fs::metadata(key_path).map_err(|err| {
        AppError::Connection(format!(
            "SSH private key not accessible at {}: {err}",
            key_path.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(AppError::Connection(format!(
            "SSH private key path is not a regular file: {}",
            key_path.display()
        )));
    }

    #[cfg(unix)]
    {
        let mode = metadata.permissions().mode();
        if mode & 0o077 != 0 {
            return Err(AppError::Connection(format!(
                "SSH private key {} has insecure permissions ({:03o}); run `chmod 600 {}`",
                key_path.display(),
                mode & 0o777,
                key_path.display()
            )));
        }
    }

    Ok(())
}

fn format_ssh_forward_host(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

fn build_ssh_args(
    params: &TunnelLaunchParams,
    local_port: u16,
    connect_timeout_secs: u64,
) -> Vec<OsString> {
    let forward_spec = format!(
        "127.0.0.1:{local_port}:{}:{}",
        format_ssh_forward_host(&params.remote_host),
        params.remote_port
    );

    let mut args = vec![
        OsString::from("-N"),
        OsString::from("-T"),
        OsString::from("-o"),
        OsString::from("ExitOnForwardFailure=yes"),
        OsString::from("-o"),
        OsString::from(format!("ConnectTimeout={connect_timeout_secs}")),
        OsString::from("-o"),
        OsString::from("ServerAliveInterval=30"),
        OsString::from("-o"),
        OsString::from("ServerAliveCountMax=3"),
        OsString::from("-o"),
        OsString::from("StrictHostKeyChecking=accept-new"),
        OsString::from("-p"),
        OsString::from(params.ssh_port.to_string()),
        OsString::from("-L"),
        OsString::from(forward_spec),
    ];

    match params.auth_method {
        SshAuthMethod::PrivateKey => {
            if let Some(ref key_path) = params.key_path {
                args.push(OsString::from("-i"));
                args.push(key_path.as_os_str().to_os_string());
            }
            args.push(OsString::from("-o"));
            args.push(OsString::from("IdentitiesOnly=yes"));
            if !params.has_secret {
                args.push(OsString::from("-o"));
                args.push(OsString::from("BatchMode=yes"));
            }
        }
        SshAuthMethod::Password => {
            args.push(OsString::from("-o"));
            args.push(OsString::from("PubkeyAuthentication=no"));
            args.push(OsString::from("-o"));
            args.push(OsString::from(
                "PreferredAuthentications=password,keyboard-interactive",
            ));
        }
    }

    args.push(OsString::from("--"));
    args.push(OsString::from(format!(
        "{}@{}",
        params.ssh_user, params.ssh_host
    )));
    args
}

fn read_limited_stderr(child: &mut Child) -> String {
    let Some(stderr) = child.stderr.take() else {
        return String::new();
    };
    let mut buf = Vec::with_capacity(512);
    let _ = stderr.take(MAX_STDERR_BYTES).read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_ssh_args_formats_password_and_private_key_modes() {
        let pwd_params = TunnelLaunchParams {
            ssh_host: "bastion.example.com".to_string(),
            ssh_port: 2222,
            ssh_user: "ops".to_string(),
            remote_host: "10.0.1.25".to_string(),
            remote_port: 27017,
            auth_method: SshAuthMethod::Password,
            key_path: None,
            has_secret: true,
        };
        let pwd_args: Vec<String> = build_ssh_args(&pwd_params, 50123, 10)
            .into_iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert!(pwd_args.contains(&"127.0.0.1:50123:10.0.1.25:27017".to_string()));
        assert!(pwd_args.contains(&"PubkeyAuthentication=no".to_string()));
        assert!(pwd_args.contains(&"ops@bastion.example.com".to_string()));

        let key_params = TunnelLaunchParams {
            ssh_host: "bastion.example.com".to_string(),
            ssh_port: 22,
            ssh_user: "ubuntu".to_string(),
            remote_host: "::1".to_string(),
            remote_port: 27017,
            auth_method: SshAuthMethod::PrivateKey,
            key_path: Some(PathBuf::from("/tmp/id_ecdsa")),
            has_secret: false,
        };
        let key_args: Vec<String> = build_ssh_args(&key_params, 50124, 8)
            .into_iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert!(key_args.contains(&"127.0.0.1:50124:[::1]:27017".to_string()));
        assert!(key_args.contains(&"-i".to_string()));
        assert!(key_args.contains(&"/tmp/id_ecdsa".to_string()));
        assert!(key_args.contains(&"BatchMode=yes".to_string()));
    }

    #[tokio::test]
    async fn disabled_tunnel_returns_original_uri_unchanged() {
        let registry = SshTunnelRegistry::new();
        let uri = "mongodb://localhost:27017/test".to_string();
        let out = registry
            .ensure_tunnel_and_rewrite_uri_async(
                Uuid::new_v4(),
                uri.clone(),
                SshTunnelConfig::default(),
                String::new(),
                Duration::from_secs(5),
            )
            .await
            .expect("disabled tunnel should pass through");
        assert_eq!(out, uri);
    }

    #[tokio::test]
    async fn missing_private_key_and_invalid_inputs_return_connection_error() {
        let registry = SshTunnelRegistry::new();
        let missing_key = SshTunnelConfig {
            enabled: true,
            endpoint: "127.0.0.1:22".to_string(),
            username: "tester".to_string(),
            auth_method: SshAuthMethod::PrivateKey,
            private_key_path: format!("/tmp/mongo_studio_nonexistent_key_{}", Uuid::new_v4()),
            has_saved_secret: false,
        };
        let err = registry
            .ensure_tunnel_and_rewrite_uri_async(
                Uuid::new_v4(),
                "mongodb://127.0.0.1:27017".to_string(),
                missing_key,
                String::new(),
                Duration::from_secs(2),
            )
            .await
            .expect_err("missing key file must fail");
        assert!(matches!(err, AppError::Connection(_)));

        let srv_cfg = SshTunnelConfig {
            enabled: true,
            endpoint: "127.0.0.1:22".to_string(),
            username: "tester".to_string(),
            auth_method: SshAuthMethod::Password,
            private_key_path: String::new(),
            has_saved_secret: false,
        };
        let srv_err = registry
            .ensure_tunnel_and_rewrite_uri_async(
                Uuid::new_v4(),
                "mongodb+srv://cluster.example.net/db".to_string(),
                srv_cfg,
                "secret".to_string(),
                Duration::from_secs(2),
            )
            .await
            .expect_err("SRV URI must fail over SSH tunnel");
        assert!(matches!(srv_err, AppError::Connection(_)));
    }

    #[test]
    fn askpass_temp_dir_creates_script_without_secret_and_cleans_up() {
        let dir_path;
        {
            let (mut guard, script_path) =
                AskpassTempDir::create().expect("askpass temp dir should be created");
            dir_path = guard.dir.clone().expect("dir path should exist");
            assert!(script_path.is_file());
            #[cfg(windows)]
            assert_eq!(
                script_path.extension().and_then(|ext| ext.to_str()),
                Some("cmd")
            );
            #[cfg(not(windows))]
            assert_eq!(
                script_path.extension().and_then(|ext| ext.to_str()),
                Some("sh")
            );
            let content = fs::read_to_string(&script_path).expect("script should be readable");
            assert!(content.contains("MONGO_STUDIO_SSH_ASKPASS_SECRET"));
            assert!(!content.contains("super-secret-value"));
            guard.cleanup();
            assert!(!dir_path.exists());
        }
        assert!(!dir_path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn private_key_permission_check_rejects_group_or_world_readable() {
        let path = std::env::temp_dir().join(format!("mongo_studio_key_{}", Uuid::new_v4()));
        fs::write(&path, "dummy-key").expect("should create dummy key");
        let mut perms = fs::metadata(&path).expect("metadata").permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&path, perms).expect("set 644");

        let err = validate_private_key_file(&path).expect_err("644 key must be rejected");
        let _ = fs::remove_file(&path);
        assert!(err.to_string().contains("chmod 600"));
    }
}
