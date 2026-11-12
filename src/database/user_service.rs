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

    /// Queries MongoDB `usersInfo` on the `admin` database and determines whether the
    /// current connection has admin privileges alongside the list of users.
    pub async fn fetch_users(client: &Client, timeout: Duration) -> (bool, Vec<MongoUser>) {
        let admin_db = client.database("admin");

        let cmd_all = doc! { "usersInfo": { "forAllDBs": true } };
        let res = match tokio::time::timeout(timeout, admin_db.run_command(cmd_all, None)).await {
            Ok(Ok(doc)) => Some(doc),
            _ => tokio::time::timeout(timeout, admin_db.run_command(doc! { "usersInfo": 1 }, None))
                .await
                .ok()
                .and_then(Result::ok),
        };

        let Some(response_doc) = res else {
            return (false, Vec::new());
        };

        let mut users = Self::parse_users_from_doc(&response_doc);
        users.sort_by(|a, b| {
            b.is_protected_admin()
                .cmp(&a.is_protected_admin())
                .then_with(|| a.username.cmp(&b.username))
        });

        (true, users)
    }

    /// Connects to `uri` and returns the refreshed list of users from `usersInfo`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if connecting to the MongoDB URI fails.
    pub async fn list_users_uri(uri: &str, timeout: Duration) -> AppResult<Vec<MongoUser>> {
        let client = Self::connect_client(uri, timeout).await?;
        let (_is_admin, users) = Self::fetch_users(&client, timeout).await;
        Ok(users)
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
            let Some(username) = user_doc.get_str("user").ok() else {
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
                AppError::Connection(format!(
                    "connection timed out after {}s",
                    timeout.as_secs()
                ))
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

        let client = Self::connect_client(uri, timeout).await?;
        let roles_bson = Self::roles_to_bson(roles);

        let cmd = doc! {
            "createUser": trimmed_user,
            "pwd": password,
            "roles": roles_bson,
        };

        tokio::time::timeout(timeout, client.database("admin").run_command(cmd, None))
            .await
            .map_err(|_| AppError::Connection("createUser timed out".to_string()))?
            .map_err(|err| AppError::Connection(format!("failed to create user: {err}")))?;

        Ok(MongoUser {
            id: format!("admin.{trimmed_user}"),
            username: trimmed_user.to_string(),
            db: "admin".to_string(),
            roles: roles.to_vec(),
        })
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
        let roles_bson = Self::roles_to_bson(roles);
        let target_db = if user_db.trim().is_empty() {
            "admin"
        } else {
            user_db.trim()
        };

        let cmd = doc! {
            "updateUser": trimmed_user,
            "roles": roles_bson,
        };

        tokio::time::timeout(timeout, client.database(target_db).run_command(cmd, None))
            .await
            .map_err(|_| AppError::Connection("updateUser timed out".to_string()))?
            .map_err(|err| AppError::Connection(format!("failed to update user roles: {err}")))?;

        Ok(())
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
        let target_db = if user_db.trim().is_empty() {
            "admin"
        } else {
            user_db.trim()
        };

        let cmd = doc! {
            "updateUser": trimmed_user,
            "pwd": new_password,
        };

        tokio::time::timeout(timeout, client.database(target_db).run_command(cmd, None))
            .await
            .map_err(|_| AppError::Connection("updateUser password timed out".to_string()))?
            .map_err(|err| AppError::Connection(format!("failed to change password: {err}")))?;

        Ok(())
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
        let target_db = if user_db.trim().is_empty() {
            "admin"
        } else {
            user_db.trim()
        };

        let cmd = doc! {
            "dropUser": trimmed_user,
        };

        tokio::time::timeout(timeout, client.database(target_db).run_command(cmd, None))
            .await
            .map_err(|_| AppError::Connection("dropUser timed out".to_string()))?
            .map_err(|err| AppError::Connection(format!("failed to delete user: {err}")))?;

        Ok(())
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
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| AppError::Connection(format!("failed to build runtime: {err}")))?;

        runtime.block_on(async {
            Self::create_user(uri, username, password, roles, timeout).await?;
            let client = Self::connect_client(uri, timeout).await?;
            let (_is_admin, users) = Self::fetch_users(&client, timeout).await;
            Ok(users)
        })
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
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| AppError::Connection(format!("failed to build runtime: {err}")))?;

        runtime.block_on(async {
            Self::update_user_roles(uri, user_db, username, roles, timeout).await?;
            let client = Self::connect_client(uri, timeout).await?;
            let (_is_admin, users) = Self::fetch_users(&client, timeout).await;
            Ok(users)
        })
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
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| AppError::Connection(format!("failed to build runtime: {err}")))?;

        runtime.block_on(async {
            Self::change_user_password(uri, user_db, username, new_password, timeout).await?;
            let client = Self::connect_client(uri, timeout).await?;
            let (_is_admin, users) = Self::fetch_users(&client, timeout).await;
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
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| AppError::Connection(format!("failed to build runtime: {err}")))?;

        runtime.block_on(async {
            Self::drop_user(uri, user_db, username, timeout).await?;
            let client = Self::connect_client(uri, timeout).await?;
            let (_is_admin, users) = Self::fetch_users(&client, timeout).await;
            Ok(users)
        })
    }
}
