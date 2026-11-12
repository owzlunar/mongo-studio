use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::{AppError, AppResult};

pub const MAX_QUERY_HISTORY: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedQuery {
    pub id: Uuid,
    pub name: String,
    pub connection_name: String,
    pub database: String,
    pub collection: String,
    pub filter: String,
    pub projection: String,
    pub sort: String,
    pub limit: usize,
}

impl SavedQuery {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: impl Into<String>,
        connection_name: impl Into<String>,
        database: impl Into<String>,
        collection: impl Into<String>,
        filter: impl Into<String>,
        projection: impl Into<String>,
        sort: impl Into<String>,
        limit: usize,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into().trim().to_string(),
            connection_name: connection_name.into(),
            database: database.into(),
            collection: collection.into(),
            filter: filter.into(),
            projection: projection.into(),
            sort: sort.into(),
            limit: limit.max(1),
        }
    }

    /// Default preset queries shown before the user customizes or saves their own.
    #[must_use]
    pub fn defaults() -> Vec<Self> {
        vec![
            Self::new(
                "Active Users",
                "MongoDB Local",
                "admin",
                "users",
                "{\"status\": \"active\"}",
                "{}",
                "{\"_id\": 1}",
                20,
            ),
            Self::new(
                "All Products",
                "MongoDB Local",
                "admin",
                "products",
                "{}",
                "{}",
                "{\"_id\": 1}",
                20,
            ),
            Self::new(
                "Pending Orders",
                "MongoDB Local",
                "admin",
                "orders",
                "{\"status\": \"active\"}",
                "{}",
                "{\"_id\": 1}",
                20,
            ),
            Self::new(
                "VIP Customers",
                "MongoDB Local",
                "shop_db",
                "customers",
                "{\"role\": \"admin\"}",
                "{}",
                "{\"_id\": 1}",
                20,
            ),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryStore {
    #[serde(default)]
    pub saved_queries: Vec<SavedQuery>,
    #[serde(default)]
    pub query_history: Vec<String>,
}

impl Default for QueryStore {
    fn default() -> Self {
        Self {
            saved_queries: SavedQuery::defaults(),
            query_history: Vec::new(),
        }
    }
}

impl QueryStore {
    /// Returns the path to `queries.json` inside the platform config directory.
    #[must_use]
    pub fn config_file_path() -> Option<PathBuf> {
        ProjectDirs::from("com", "mongostudio", "MongoStudio")
            .map(|dirs| dirs.config_dir().join("queries.json"))
    }

    /// Appends a query string to history, deduplicating consecutive entries and capping at
    /// [`MAX_QUERY_HISTORY`].
    pub fn push_history(&mut self, command: impl Into<String>) {
        let trimmed = command.into().trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        if self
            .query_history
            .last()
            .is_some_and(|last| last == &trimmed)
        {
            return;
        }
        self.query_history.push(trimmed);
        self.cap_history();
    }

    pub fn cap_history(&mut self) {
        if self.query_history.len() > MAX_QUERY_HISTORY {
            let excess = self.query_history.len() - MAX_QUERY_HISTORY;
            self.query_history.drain(0..excess);
        }
    }

    /// Loads the query store from `queries.json`, falling back to [`QueryStore::default`] if
    /// missing or unreadable.
    #[must_use]
    pub fn load() -> Self {
        let Some(path) = Self::config_file_path() else {
            return Self::default();
        };
        Self::load_from_path(&path)
    }

    /// Loads the query store from an explicit file path, falling back to [`QueryStore::default`]
    /// if missing or invalid.
    #[must_use]
    pub fn load_from_path(path: &Path) -> Self {
        let raw = match fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "failed to read saved queries"
                );
                return Self::default();
            }
        };

        let mut store: Self = match serde_json::from_str(&raw) {
            Ok(parsed) => parsed,
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "failed to parse saved queries"
                );
                return Self::default();
            }
        };

        store.cap_history();
        store
    }

    /// Atomically persists the query store to `queries.json` via write-to-temp + rename.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Persistence`] if serialization or file I/O fails.
    pub fn save(&self) -> AppResult<()> {
        let Some(path) = Self::config_file_path() else {
            return Ok(());
        };
        self.save_to_path(&path)
    }

    /// Atomically persists the query store to `path` via write-to-temp + rename.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Persistence`] if directory creation, serialization, or atomic rename fails.
    pub fn save_to_path(&self, path: &Path) -> AppResult<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                AppError::Persistence(format!("failed to create config directory: {err}"))
            })?;
        }

        let mut bounded = self.clone();
        bounded.cap_history();

        let serialized = serde_json::to_string_pretty(&bounded)
            .map_err(|err| AppError::Persistence(format!("failed to serialize queries: {err}")))?;

        let tmp_name = format!(
            "{}.{}.tmp",
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("queries.json"),
            Uuid::new_v4()
        );
        let tmp_path = path.with_file_name(tmp_name);

        let write_result = (|| -> std::io::Result<()> {
            let mut file = fs::File::create(&tmp_path)?;
            file.write_all(serialized.as_bytes())?;
            file.sync_all()?;
            fs::rename(&tmp_path, path)?;
            Ok(())
        })();

        if let Err(err) = write_result {
            if let Err(cleanup_err) = fs::remove_file(&tmp_path) {
                if cleanup_err.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(
                        path = %tmp_path.display(),
                        error = %cleanup_err,
                        "failed to remove temporary query store file"
                    );
                }
            }
            return Err(AppError::Persistence(format!(
                "failed to write queries atomically: {err}"
            )));
        }

        Ok(())
    }

    /// Asynchronously loads the query store on Tokio's blocking thread pool.
    pub async fn load_async() -> Self {
        match tokio::task::spawn_blocking(Self::load).await {
            Ok(store) => store,
            Err(err) => {
                tracing::warn!(error = %err, "load queries task failed");
                Self::default()
            }
        }
    }

    /// Asynchronously persists the query store on Tokio's blocking thread pool.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Persistence`] if serialization, file I/O, or the blocking task fails.
    pub async fn save_async(self) -> AppResult<()> {
        tokio::task::spawn_blocking(move || self.save())
            .await
            .map_err(|err| AppError::Persistence(format!("save queries task failed: {err}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_store_roundtrips_atomically_and_caps_history() {
        let temp_dir = std::env::temp_dir().join(format!("mongo_studio_qtest_{}", Uuid::new_v4()));
        let path = temp_dir.join("queries.json");

        let mut store = QueryStore::default();
        assert_eq!(store.saved_queries.len(), 4);

        for i in 0..115 {
            store.push_history(format!("db.users.find({{ \"idx\": {i} }})"));
        }
        assert_eq!(store.query_history.len(), MAX_QUERY_HISTORY);
        assert_eq!(
            store.query_history.first().map(String::as_str),
            Some("db.users.find({ \"idx\": 15 })")
        );

        store
            .save_to_path(&path)
            .expect("atomic save should succeed");
        let loaded = QueryStore::load_from_path(&path);
        assert_eq!(loaded, store);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
