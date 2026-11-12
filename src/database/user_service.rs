use std::time::Duration;

use bson::{doc, Bson, Document};
use mongodb::options::ClientOptions;
use mongodb::Client;

use crate::core::{AppError, AppResult};
use crate::models::{ConnectionConfig, MongoUser, UserRole};

#[derive(Debug, Default, Clone, Copy)]
pub struct UserService;

impl UserService {
    /// Rejects any attempt to modify or delete the protected `"admin"` account.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if `username` matches `"admin"` (case-insensitive).
    pub fn ensure_not_protected_admin(username: &str) -> AppResult<()> {
        if username.trim().eq_ignore_ascii_case("admin") {
            return Err(AppError::Connection(
                "user 'admin' is protected and cannot be modified or deleted".to_string(),
            ));
        }
        Ok(())
    }

    fn resolve_target_db(user_db: &str) -> &str {
        let trimmed = user_db.trim();
        if trimmed.is_empty() {
            "admin"
        } else {
            trimmed
        }
    }

    fn build_single_thread_runtime() -> AppResult<tokio::runtime::Runtime> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| AppError::Connection(format!("failed to build runtime: {err}")))
    }

    async fn run_admin_command(
        client: &Client,
        target_db: &str,
        command: Document,
        timeout: Duration,
        timeout_msg: &str,
        error_prefix: &str,
    ) -> AppResult<Document> {
        tokio::time::timeout(
            timeout,
            client.database(target_db).run_command(command, None),
        )
        .await
        .map_err(|_| AppError::Connection(timeout_msg.to_string()))?
        .map_err(|err| AppError::Connection(format!("{error_prefix}: {err}")))
    }

    async fn fetch_users_result(client: &Client, timeout: Duration) -> AppResult<Vec<MongoUser>> {
        let admin_db = client.database("admin");
        let cmd_all = doc! { "usersInfo": { "forAllDBs": true } };

        let first_try = tokio::time::timeout(timeout, admin_db.run_command(cmd_all, None)).await;
        let response_doc = if let Ok(Ok(doc)) = first_try {
            doc
        } else {
            if let Ok(Err(err)) = first_try {
                tracing::debug!(
                    error = %err,
                    "usersInfo forAllDBs failed; falling back to admin db"
                );
            } else {
                tracing::debug!("usersInfo forAllDBs timed out; falling back to admin db");
            }
            Self::run_admin_command(
                client,
                "admin",
                doc! { "usersInfo": 1 },
                timeout,
                "usersInfo timed out",
                "failed to list users",
            )
            .await?
        };

        let mut users = Self::parse_users_from_doc(&response_doc);
        users.sort_by(|a, b| {
            b.is_protected_admin()
                .cmp(&a.is_protected_admin())
                .then_with(|| a.username.cmp(&b.username))
        });

        Ok(users)
    }

    /// Queries MongoDB `usersInfo` on the `admin` database and determines whether the
    /// current connection has admin privileges alongside the list of users.
    pub async fn fetch_users(client: &Client, timeout: Duration) -> (bool, Vec<MongoUser>) {
        match Self::fetch_users_result(client, timeout).await {
            Ok(users) => (true, users),
            Err(err) => {
                tracing::debug!(error = %err, "usersInfo unavailable for connection");
                (false, Vec::new())
            }
        }
    }

    /// Connects to `uri` and returns the refreshed list of users from `usersInfo`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if connecting to the MongoDB URI or querying `usersInfo` fails.
    pub async fn list_users_uri(uri: &str, timeout: Duration) -> AppResult<Vec<MongoUser>> {
        let client = Self::connect_client(uri, timeout).await?;
        Self::fetch_users_result(&client, timeout).await
    }

    fn parse_users_from_doc(doc: &Document) -> Vec<MongoUser> {
        let Some(Bson::Array(items)) = doc.get("users") else {
            return Vec::new();
        };

        let mut users = Vec::with_capacity(items.len());
        for item in items {
            let Bson::Document(user_doc) = item else {
                continue;
            };
            let Ok(username) = user_doc.get_str("user") else {
                continue;
            };
            let db = user_doc.get_str("db").unwrap_or("admin").to_string();
            let id = user_doc
                .get_str("_id")
                .map_or_else(|_| format!("{db}.{username}"), ToString::to_string);

            let mut roles = Vec::new();
            if let Some(Bson::Array(role_items)) = user_doc.get("roles") {
                for r in role_items {
                    if let Bson::Document(r_doc) = r {
                        if let Ok(r_name) = r_doc.get_str("role") {
                            let r_db = r_doc.get_str("db").unwrap_or(&db);
                            roles.push(UserRole::new(r_name, r_db));
                        }
                    }
                }
            }

            users.push(MongoUser {
                id,
                username: username.to_string(),
                db,
                roles,
            });
        }
        users
    }

    async fn connect_client(uri: &str, timeout: Duration) -> AppResult<Client> {
        let normalized = ConnectionConfig::normalize_uri(uri, "", "");
        let mut options = tokio::time::timeout(timeout, ClientOptions::parse(&normalized))
            .await
            .map_err(|_| {
                AppError::Connection(format!("connection timed out after {}s", timeout.as_secs()))
            })?
            .map_err(|err| AppError::Connection(format!("invalid MongoDB URI: {err}")))?;

        options.connect_timeout = Some(timeout);
        options.server_selection_timeout = Some(timeout);

        Client::with_options(options)
            .map_err(|err| AppError::Connection(format!("failed to create client: {err}")))
    }

    fn roles_to_bson(roles: &[UserRole]) -> Vec<Bson> {
        roles
            .iter()
            .map(|r| {
                Bson::Document(doc! {
                    "role": &r.role,
                    "db": &r.db,
                })
            })
            .collect()
    }

    fn validate_create_user<'a>(username: &'a str, password: &str) -> AppResult<&'a str> {
        let trimmed_user = username.trim();
        if trimmed_user.is_empty() {
            return Err(AppError::Connection(
                "username must not be empty".to_string(),
            ));
        }
        Self::ensure_not_protected_admin(trimmed_user)?;

        if password.is_empty() {
            return Err(AppError::Connection(
                "password must not be empty".to_string(),
            ));
        }
        Ok(trimmed_user)
    }

    async fn create_user_on_client(
        client: &Client,
        trimmed_user: &str,
        password: &str,
        roles: &[UserRole],
        timeout: Duration,
    ) -> AppResult<MongoUser> {
        let roles_bson = Self::roles_to_bson(roles);
        let cmd = doc! {
            "createUser": trimmed_user,
            "pwd": password,
            "roles": roles_bson,
        };

        Self::run_admin_command(
            client,
            "admin",
            cmd,
            timeout,
            "createUser timed out",
            "failed to create user",
        )
        .await?;

        Ok(MongoUser {
            id: format!("admin.{trimmed_user}"),
            username: trimmed_user.to_string(),
            db: "admin".to_string(),
            roles: roles.to_vec(),
        })
    }

    async fn update_user_roles_on_client(
        client: &Client,
        user_db: &str,
        trimmed_user: &str,
        roles: &[UserRole],
        timeout: Duration,
    ) -> AppResult<()> {
        let target_db = Self::resolve_target_db(user_db);
        let roles_bson = Self::roles_to_bson(roles);
        let cmd = doc! {
            "updateUser": trimmed_user,
            "roles": roles_bson,
        };

        Self::run_admin_command(
            client,
            target_db,
            cmd,
            timeout,
            "updateUser timed out",
            "failed to update user roles",
        )
        .await?;

        Ok(())
    }

    async fn change_user_password_on_client(
        client: &Client,
        user_db: &str,
        trimmed_user: &str,
        new_password: &str,
        timeout: Duration,
    ) -> AppResult<()> {
        let target_db = Self::resolve_target_db(user_db);
        let cmd = doc! {
            "updateUser": trimmed_user,
            "pwd": new_password,
        };

        Self::run_admin_command(
            client,
            target_db,
            cmd,
            timeout,
            "updateUser password timed out",
            "failed to change password",
        )
        .await?;

        Ok(())
    }

    async fn drop_user_on_client(
        client: &Client,
        user_db: &str,
        trimmed_user: &str,
        timeout: Duration,
    ) -> AppResult<()> {
        let target_db = Self::resolve_target_db(user_db);
        let cmd = doc! {
            "dropUser": trimmed_user,
        };

        Self::run_admin_command(
            client,
            target_db,
            cmd,
            timeout,
            "dropUser timed out",
            "failed to delete user",
        )
        .await?;

        Ok(())
    }

    /// Creates a new MongoDB user in the `admin` database with the specified initial roles.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if `username` is `"admin"`, empty, or the command fails.
    pub async fn create_user(
        uri: &str,
        username: &str,
        password: &str,
        roles: &[UserRole],
        timeout: Duration,
    ) -> AppResult<MongoUser> {
        let trimmed_user = Self::validate_create_user(username, password)?;
        let client = Self::connect_client(uri, timeout).await?;
        Self::create_user_on_client(&client, trimmed_user, password, roles, timeout).await
    }

    /// Creates a new MongoDB user and re-fetches the user list using the same client connection.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if validation, connection, or `createUser` fails.
    pub async fn create_user_and_list(
        uri: &str,
        username: &str,
        password: &str,
        roles: &[UserRole],
        timeout: Duration,
    ) -> AppResult<Vec<MongoUser>> {
        let trimmed_user = Self::validate_create_user(username, password)?;
        let client = Self::connect_client(uri, timeout).await?;
        Self::create_user_on_client(&client, trimmed_user, password, roles, timeout).await?;
        let (is_admin, users) = Self::fetch_users(&client, timeout).await;
        if !is_admin {
            tracing::warn!(
                username = trimmed_user,
                "user created, but refreshing user list via usersInfo failed"
            );
        }
        Ok(users)
    }

    /// Replaces the role list of a non-protected MongoDB user via `updateUser`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if `username` is `"admin"` or the command fails.
    pub async fn update_user_roles(
        uri: &str,
        user_db: &str,
        username: &str,
        roles: &[UserRole],
        timeout: Duration,
    ) -> AppResult<()> {
        let trimmed_user = username.trim();
        Self::ensure_not_protected_admin(trimmed_user)?;
        let client = Self::connect_client(uri, timeout).await?;
        Self::update_user_roles_on_client(&client, user_db, trimmed_user, roles, timeout).await
    }

    /// Replaces the role list of a non-protected MongoDB user and re-fetches the user list
    /// using the same client connection.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if `username` is `"admin"`, connection fails, or `updateUser` fails.
    pub async fn update_user_roles_and_list(
        uri: &str,
        user_db: &str,
        username: &str,
        roles: &[UserRole],
        timeout: Duration,
    ) -> AppResult<Vec<MongoUser>> {
        let trimmed_user = username.trim();
        Self::ensure_not_protected_admin(trimmed_user)?;
        let client = Self::connect_client(uri, timeout).await?;
        Self::update_user_roles_on_client(&client, user_db, trimmed_user, roles, timeout).await?;
        let (is_admin, users) = Self::fetch_users(&client, timeout).await;
        if !is_admin {
            tracing::warn!(
                username = trimmed_user,
                "user roles updated, but refreshing user list via usersInfo failed"
            );
        }
        Ok(users)
    }

    /// Updates the password of a non-protected MongoDB user via `updateUser`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if `username` is `"admin"`, `new_password` is empty, or the command fails.
    pub async fn change_user_password(
        uri: &str,
        user_db: &str,
        username: &str,
        new_password: &str,
        timeout: Duration,
    ) -> AppResult<()> {
        let trimmed_user = username.trim();
        Self::ensure_not_protected_admin(trimmed_user)?;

        if new_password.is_empty() {
            return Err(AppError::Connection(
                "new password must not be empty".to_string(),
            ));
        }

        let client = Self::connect_client(uri, timeout).await?;
        Self::change_user_password_on_client(&client, user_db, trimmed_user, new_password, timeout)
            .await
    }

    /// Drops a non-protected MongoDB user via `dropUser`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if `username` is `"admin"` or the command fails.
    pub async fn drop_user(
        uri: &str,
        user_db: &str,
        username: &str,
        timeout: Duration,
    ) -> AppResult<()> {
        let trimmed_user = username.trim();
        Self::ensure_not_protected_admin(trimmed_user)?;
        let client = Self::connect_client(uri, timeout).await?;
        Self::drop_user_on_client(&client, user_db, trimmed_user, timeout).await
    }

    /// Drops a non-protected MongoDB user and re-fetches the user list using the same client connection.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if `username` is `"admin"`, connection fails, or `dropUser` fails.
    pub async fn drop_user_and_list(
        uri: &str,
        user_db: &str,
        username: &str,
        timeout: Duration,
    ) -> AppResult<Vec<MongoUser>> {
        let trimmed_user = username.trim();
        Self::ensure_not_protected_admin(trimmed_user)?;
        let client = Self::connect_client(uri, timeout).await?;
        Self::drop_user_on_client(&client, user_db, trimmed_user, timeout).await?;
        let (is_admin, users) = Self::fetch_users(&client, timeout).await;
        if !is_admin {
            tracing::warn!(
                username = trimmed_user,
                "user dropped, but refreshing user list via usersInfo failed"
            );
        }
        Ok(users)
    }

    /// Synchronously creates a user on a live MongoDB server and returns the refreshed user list.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if creation or user list refresh fails.
    pub fn create_user_blocking(
        uri: &str,
        username: &str,
        password: &str,
        roles: &[UserRole],
        timeout: Duration,
    ) -> AppResult<Vec<MongoUser>> {
        let runtime = Self::build_single_thread_runtime()?;
        runtime.block_on(Self::create_user_and_list(
            uri, username, password, roles, timeout,
        ))
    }

    /// Synchronously updates user roles on a live MongoDB server and returns the refreshed user list.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if role update fails.
    pub fn update_user_roles_blocking(
        uri: &str,
        user_db: &str,
        username: &str,
        roles: &[UserRole],
        timeout: Duration,
    ) -> AppResult<Vec<MongoUser>> {
        let runtime = Self::build_single_thread_runtime()?;
        runtime.block_on(Self::update_user_roles_and_list(
            uri, user_db, username, roles, timeout,
        ))
    }

    /// Synchronously changes a user's password on a live MongoDB server and returns the refreshed user list.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if password update fails.
    pub fn change_user_password_blocking(
        uri: &str,
        user_db: &str,
        username: &str,
        new_password: &str,
        timeout: Duration,
    ) -> AppResult<Vec<MongoUser>> {
        let runtime = Self::build_single_thread_runtime()?;
        runtime.block_on(async {
            let trimmed_user = username.trim();
            Self::ensure_not_protected_admin(trimmed_user)?;
            if new_password.is_empty() {
                return Err(AppError::Connection(
                    "new password must not be empty".to_string(),
                ));
            }
            let client = Self::connect_client(uri, timeout).await?;
            Self::change_user_password_on_client(
                &client,
                user_db,
                trimmed_user,
                new_password,
                timeout,
            )
            .await?;
            let (is_admin, users) = Self::fetch_users(&client, timeout).await;
            if !is_admin {
                tracing::warn!(
                    username = trimmed_user,
                    "user password changed, but refreshing user list via usersInfo failed"
                );
            }
            Ok(users)
        })
    }

    /// Synchronously drops a user on a live MongoDB server and returns the refreshed user list.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if dropping the user fails.
    pub fn drop_user_blocking(
        uri: &str,
        user_db: &str,
        username: &str,
        timeout: Duration,
    ) -> AppResult<Vec<MongoUser>> {
        let runtime = Self::build_single_thread_runtime()?;
        runtime.block_on(Self::drop_user_and_list(uri, user_db, username, timeout))
    }
}
