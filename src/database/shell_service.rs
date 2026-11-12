use std::time::Duration;

use bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use serde_json::Value;

use crate::core::{AppError, AppResult};
use crate::database::connection::ConnectionClient;
use crate::database::document_service::DocumentService;
use crate::models::{DatabaseInfo, DocumentRow};

enum ParsedShellCommand {
    Help,
    UseDb(String),
    ShowDbs,
    ShowCollections,
    RunCommand {
        db: Option<String>,
        command: Document,
    },
    Find {
        db: Option<String>,
        coll: String,
        filter: Document,
        raw_filter: String,
    },
    CountDocuments {
        db: Option<String>,
        coll: String,
        filter: Document,
        raw_filter: String,
    },
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ShellService;

impl ShellService {
    #[must_use]
    pub const fn help_text() -> &'static str {
        "Supported Mongo Studio Shell commands:\n  help                                        Show this help message\n  show dbs | show databases                   List databases\n  show collections | show tables              List collections in current database\n  use <db>                                    Switch active shell database\n  db.<coll>.find(<filterJson>)                Find up to 20 matching documents\n  db.getSiblingDB(\"<db>\").<coll>.find(...)   Query collection in another database\n  db.<coll>.countDocuments(<filterJson>)      Count matching documents\n  db.runCommand(<json>)                       Execute a database command"
    }

    /// Extracts the target database name if `command` is a `use <db>` statement.
    #[must_use]
    pub fn extract_use_db(command: &str) -> Option<String> {
        let trimmed = command.trim().trim_end_matches(';').trim();
        let rest = trimmed
            .strip_prefix("use ")
            .or_else(|| trimmed.strip_prefix("USE "))?;
        let db = rest.trim().trim_matches('"').trim_matches('\'').trim();
        if db.is_empty() || db.contains(char::is_whitespace) {
            None
        } else {
            Some(db.to_string())
        }
    }

    fn parse_command(command: &str) -> AppResult<ParsedShellCommand> {
        let trimmed = command.trim().trim_end_matches(';').trim();
        if trimmed.is_empty() {
            return Err(AppError::InvalidQueryJson(
                "shell command must not be empty".to_string(),
            ));
        }

        if trimmed.eq_ignore_ascii_case("help") || trimmed == "?" {
            return Ok(ParsedShellCommand::Help);
        }

        if let Some(db) = Self::extract_use_db(trimmed) {
            return Ok(ParsedShellCommand::UseDb(db));
        }

        if trimmed.eq_ignore_ascii_case("show dbs")
            || trimmed.eq_ignore_ascii_case("show databases")
        {
            return Ok(ParsedShellCommand::ShowDbs);
        }

        if trimmed.eq_ignore_ascii_case("show collections")
            || trimmed.eq_ignore_ascii_case("show tables")
        {
            return Ok(ParsedShellCommand::ShowCollections);
        }

        let Some(mut rest) = trimmed.strip_prefix("db.") else {
            return Err(AppError::InvalidQueryJson(format!(
                "unsupported shell command '{trimmed}'. Type 'help' for supported commands."
            )));
        };

        let mut target_db: Option<String> = None;
        if let Some(after_prefix) = rest.strip_prefix("getSiblingDB(") {
            let Some((db_arg, after_paren)) = after_prefix.split_once(')') else {
                return Err(AppError::InvalidQueryJson(
                    "unclosed getSiblingDB(...) call".to_string(),
                ));
            };
            let clean_db = db_arg.trim().trim_matches('"').trim_matches('\'').trim();
            if clean_db.is_empty() {
                return Err(AppError::InvalidQueryJson(
                    "getSiblingDB database name must not be empty".to_string(),
                ));
            }
            let Some(after_dot) = after_paren.trim_start().strip_prefix('.') else {
                return Err(AppError::InvalidQueryJson(
                    "expected '.' after getSiblingDB(...)".to_string(),
                ));
            };
            target_db = Some(clean_db.to_string());
            rest = after_dot;
        }

        let Some((target_part, args_part)) = rest.split_once('(') else {
            return Err(AppError::InvalidQueryJson(format!(
                "invalid shell expression '{trimmed}'. Expected method call like db.users.find({{}})"
            )));
        };
        let Some(raw_arg) = args_part.trim_end().strip_suffix(')') else {
            return Err(AppError::InvalidQueryJson(
                "missing closing ')' in shell command".to_string(),
            ));
        };

        let target = target_part.trim();
        let arg_trimmed = raw_arg.trim();

        if target == "runCommand" {
            let cmd_doc = DocumentRow::parse_json_to_bson_doc(arg_trimmed)?;
            if cmd_doc.is_empty() {
                return Err(AppError::InvalidQueryJson(
                    "runCommand document must not be empty".to_string(),
                ));
            }
            return Ok(ParsedShellCommand::RunCommand {
                db: target_db,
                command: cmd_doc,
            });
        }

        let Some((coll_part, method)) = target.rsplit_once('.') else {
            return Err(AppError::InvalidQueryJson(format!(
                "expected collection method in '{trimmed}', e.g. db.users.find({{}})"
            )));
        };

        let coll = coll_part.trim();
        if coll.is_empty() {
            return Err(AppError::InvalidQueryJson(
                "collection name must not be empty".to_string(),
            ));
        }

        let filter_str = if arg_trimmed.is_empty() {
            "{}".to_string()
        } else {
            arg_trimmed.to_string()
        };
        let filter_doc = DocumentRow::parse_filter_doc(&filter_str)?;

        match method.trim() {
            "find" => Ok(ParsedShellCommand::Find {
                db: target_db,
                coll: coll.to_string(),
                filter: filter_doc,
                raw_filter: filter_str,
            }),
            "countDocuments" => Ok(ParsedShellCommand::CountDocuments {
                db: target_db,
                coll: coll.to_string(),
                filter: filter_doc,
                raw_filter: filter_str,
            }),
            other => Err(AppError::InvalidQueryJson(format!(
                "unsupported collection method '{other}'. Supported: find, countDocuments"
            ))),
        }
    }

    fn format_json_docs(values: &[Value]) -> AppResult<String> {
        serde_json::to_string_pretty(values)
            .map_err(|err| AppError::Connection(format!("failed to format shell JSON: {err}")))
    }

    /// Executes a shell command against a live MongoDB server.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidQueryJson`] for syntax errors or unsupported commands, or
    /// [`AppError::Connection`] if the MongoDB request fails.
    pub async fn execute_command(
        uri: &str,
        default_db: &str,
        command: &str,
        timeout: Duration,
    ) -> AppResult<String> {
        let current_db = if default_db.trim().is_empty() {
            "admin"
        } else {
            default_db.trim()
        };

        match Self::parse_command(command)? {
            ParsedShellCommand::Help => Ok(Self::help_text().to_string()),
            ParsedShellCommand::UseDb(db) => Ok(format!("switched to db {db}")),
            ParsedShellCommand::ShowDbs => {
                let client = ConnectionClient::connect_client(uri, timeout).await?;
                let mut names =
                    tokio::time::timeout(timeout, client.list_database_names(None, None))
                        .await
                        .map_err(|_| AppError::Connection("show dbs timed out".to_string()))?
                        .map_err(|err| AppError::Connection(format!("show dbs failed: {err}")))?;
                names.sort();
                Ok(if names.is_empty() {
                    "(no databases)".to_string()
                } else {
                    names.join("\n")
                })
            }
            ParsedShellCommand::ShowCollections => {
                let client = ConnectionClient::connect_client(uri, timeout).await?;
                let mut names = tokio::time::timeout(
                    timeout,
                    client.database(current_db).list_collection_names(None),
                )
                .await
                .map_err(|_| AppError::Connection("show collections timed out".to_string()))?
                .map_err(|err| AppError::Connection(format!("show collections failed: {err}")))?;
                names.sort();
                Ok(if names.is_empty() {
                    format!("(no collections in {current_db})")
                } else {
                    names.join("\n")
                })
            }
            ParsedShellCommand::RunCommand {
                db,
                command: cmd_doc,
            } => {
                let target_db = db.as_deref().unwrap_or(current_db);
                let client = ConnectionClient::connect_client(uri, timeout).await?;
                let result_doc = tokio::time::timeout(
                    timeout,
                    client.database(target_db).run_command(cmd_doc, None),
                )
                .await
                .map_err(|_| AppError::Connection("runCommand timed out".to_string()))?
                .map_err(|err| AppError::Connection(format!("runCommand failed: {err}")))?;
                let ext = Bson::Document(result_doc).into_relaxed_extjson();
                serde_json::to_string_pretty(&ext).map_err(|err| {
                    AppError::Connection(format!("failed to format command result: {err}"))
                })
            }
            ParsedShellCommand::Find {
                db, coll, filter, ..
            } => {
                let target_db = db.as_deref().unwrap_or(current_db);
                let client = ConnectionClient::connect_client(uri, timeout).await?;
                let collection = client.database(target_db).collection::<Document>(&coll);
                let find_opts = FindOptions::builder().limit(20).build();
                let mut cursor = tokio::time::timeout(timeout, collection.find(filter, find_opts))
                    .await
                    .map_err(|_| AppError::Connection("shell find timed out".to_string()))?
                    .map_err(|err| AppError::Connection(format!("shell find failed: {err}")))?;

                let mut values = Vec::new();
                while tokio::time::timeout(timeout, cursor.advance())
                    .await
                    .map_err(|_| AppError::Connection("shell cursor timed out".to_string()))?
                    .map_err(|err| AppError::Connection(format!("shell cursor error: {err}")))?
                {
                    let doc = cursor.deserialize_current().map_err(|err| {
                        AppError::Connection(format!("failed to decode document: {err}"))
                    })?;
                    values.push(Bson::Document(doc).into_relaxed_extjson());
                }
                Self::format_json_docs(&values)
            }
            ParsedShellCommand::CountDocuments {
                db, coll, filter, ..
            } => {
                let target_db = db.as_deref().unwrap_or(current_db);
                let client = ConnectionClient::connect_client(uri, timeout).await?;
                let collection = client.database(target_db).collection::<Document>(&coll);
                let count = tokio::time::timeout(timeout, collection.count_documents(filter, None))
                    .await
                    .map_err(|_| AppError::Connection("countDocuments timed out".to_string()))?
                    .map_err(|err| AppError::Connection(format!("countDocuments failed: {err}")))?;
                Ok(count.to_string())
            }
        }
    }

    /// Synchronously executes a shell command against a live MongoDB server.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if runtime creation, parsing, or execution fails.
    pub fn execute_command_blocking(
        uri: &str,
        default_db: &str,
        command: &str,
        timeout: Duration,
    ) -> AppResult<String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| AppError::Connection(format!("failed to build runtime: {err}")))?;
        runtime.block_on(Self::execute_command(uri, default_db, command, timeout))
    }

    /// Executes a shell command offline against in-memory mock [`DatabaseInfo`] models.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidQueryJson`] on syntax errors or [`AppError::CollectionNotFound`]
    /// if the referenced database/collection does not exist in mock state.
    pub fn execute_mock_command(
        default_db: &str,
        databases: &[DatabaseInfo],
        command: &str,
    ) -> AppResult<String> {
        let current_db = if default_db.trim().is_empty() {
            "admin"
        } else {
            default_db.trim()
        };

        match Self::parse_command(command)? {
            ParsedShellCommand::Help => Ok(Self::help_text().to_string()),
            ParsedShellCommand::UseDb(db) => Ok(format!("switched to db {db}")),
            ParsedShellCommand::ShowDbs => {
                let lines: Vec<String> = databases
                    .iter()
                    .map(|db| format!("{}  ({:.1} MB)", db.name, db.size_on_disk_mb))
                    .collect();
                Ok(if lines.is_empty() {
                    "(no databases)".to_string()
                } else {
                    lines.join("\n")
                })
            }
            ParsedShellCommand::ShowCollections => {
                let names: Vec<&str> = databases
                    .iter()
                    .find(|db| db.name == current_db)
                    .map(|db| db.collections.iter().map(|c| c.name.as_str()).collect())
                    .unwrap_or_default();
                Ok(if names.is_empty() {
                    format!("(no collections in {current_db})")
                } else {
                    names.join("\n")
                })
            }
            ParsedShellCommand::RunCommand {
                command: cmd_doc, ..
            } => {
                if cmd_doc.contains_key("ping") {
                    serde_json::to_string_pretty(
                        &Bson::Document(doc! { "ok": 1.0 }).into_relaxed_extjson(),
                    )
                    .map_err(|err| AppError::Connection(format!("format failed: {err}")))
                } else {
                    Err(AppError::Connection(
                        "runCommand requires a live MongoDB connection (only { \"ping\": 1 } is simulated offline)".to_string(),
                    ))
                }
            }
            ParsedShellCommand::Find {
                db,
                coll,
                raw_filter,
                ..
            } => {
                let target_db = db.as_deref().unwrap_or(current_db);
                let collection = databases
                    .iter()
                    .find(|d| d.name == target_db)
                    .and_then(|d| d.collections.iter().find(|c| c.name == coll))
                    .ok_or_else(|| {
                        AppError::CollectionNotFound(target_db.to_string(), coll.clone())
                    })?;
                let matched =
                    DocumentService::filter_documents(&collection.documents, &raw_filter, 20);
                let values: Vec<Value> = matched.iter().map(DocumentRow::to_json_value).collect();
                Self::format_json_docs(&values)
            }
            ParsedShellCommand::CountDocuments {
                db,
                coll,
                raw_filter,
                ..
            } => {
                let target_db = db.as_deref().unwrap_or(current_db);
                let collection = databases
                    .iter()
                    .find(|d| d.name == target_db)
                    .and_then(|d| d.collections.iter().find(|c| c.name == coll))
                    .ok_or_else(|| {
                        AppError::CollectionNotFound(target_db.to_string(), coll.clone())
                    })?;
                let matched = DocumentService::filter_documents(
                    &collection.documents,
                    &raw_filter,
                    usize::MAX,
                );
                Ok(matched.len().to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_service_parses_and_executes_mock_commands() {
        let dbs = DatabaseInfo::mock_databases();

        assert!(ShellService::execute_mock_command("admin", &dbs, "help")
            .unwrap()
            .contains("Supported Mongo Studio Shell"));
        assert_eq!(
            ShellService::extract_use_db("use shop_db;"),
            Some("shop_db".to_string())
        );
        assert_eq!(
            ShellService::execute_mock_command("admin", &dbs, "use shop_db").unwrap(),
            "switched to db shop_db"
        );
        assert!(
            ShellService::execute_mock_command("admin", &dbs, "show dbs")
                .unwrap()
                .contains("shop_db")
        );

        let active_users = ShellService::execute_mock_command(
            "admin",
            &dbs,
            r#"db.users.find({"status": "active"})"#,
        )
        .unwrap();
        assert!(active_users.contains("Alice"));

        let count = ShellService::execute_mock_command(
            "admin",
            &dbs,
            r#"db.getSiblingDB("shop_db").customers.countDocuments({"status": "active"})"#,
        )
        .unwrap();
        assert_eq!(count, "3");

        let ping =
            ShellService::execute_mock_command("admin", &dbs, r#"db.runCommand({"ping": 1})"#)
                .unwrap();
        assert!(ping.contains("\"ok\""));

        assert!(ShellService::execute_mock_command("admin", &dbs, "invalid cmd").is_err());
    }
}
