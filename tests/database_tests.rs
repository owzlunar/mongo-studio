use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use bson::oid::ObjectId;
use bson::{doc, Bson, DateTime};
use mongo_studio::core::{AppError, AppState};
use mongo_studio::database::connection::ConnectionClient;
use mongo_studio::database::{
    CollectionService, ConnectionManager, CreateIndexRequest, DatabaseService, DocumentQueryParams,
    DocumentService, IndexService, SchemaAnalyzer, ShellService, SshTunnelRegistry, TransferFormat,
    TransferService, UserService,
};
use mongo_studio::models::{
    CollectionInfo, ConnectionConfig, ConnectionStatus, DatabaseInfo, DocumentRow, SshAuthMethod,
    SshTunnelConfig, UserRole,
};
use mongo_studio::query::{AggregationStage, QueryFilter};
use serde_json::Value;
use uuid::Uuid;

fn build_test_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("current-thread Tokio runtime should build")
}

struct Phase5TempDbGuard<'a> {
    uri: &'a str,
    db_name: String,
    timeout: Duration,
}

impl<'a> Phase5TempDbGuard<'a> {
    fn new(uri: &'a str, db_name: String, timeout: Duration) -> Self {
        Self {
            uri,
            db_name,
            timeout,
        }
    }
}

impl Drop for Phase5TempDbGuard<'_> {
    fn drop(&mut self) {
        let _ = DatabaseService::drop_database_blocking(self.uri, &self.db_name, self.timeout);
    }
}

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn connection_client_from_uri_extracts_standard_and_srv_hosts() {
    // Arrange
    let standard_uri = "  mongodb://db.internal.local:27018/production  ";
    let srv_uri = "mongodb+srv://cluster0.mongodb.net/analytics?retryWrites=true";

    // Act
    let standard_client = ConnectionClient::from_uri("  Primary Replica  ", standard_uri)
        .expect("valid standard URI");
    let srv_client = ConnectionClient::from_uri("Atlas SRV", srv_uri).expect("valid SRV URI");

    // Assert
    assert_eq!(standard_client.config.name, "Primary Replica");
    assert_eq!(
        standard_client.config.uri,
        "mongodb://db.internal.local:27018/production"
    );
    assert_eq!(standard_client.config.host, "db.internal.local:27018");
    assert!(standard_client.config.status.is_connected());
    assert!(standard_client.config.expanded);
    assert_eq!(standard_client.config.databases.len(), 2);

    assert_eq!(srv_client.config.name, "Atlas SRV");
    assert_eq!(srv_client.config.host, "cluster0.mongodb.net");
}

#[test]
fn connection_client_from_uri_falls_back_to_default_host_when_host_segment_empty() {
    // Arrange
    let bare_scheme_uri = "mongodb://";
    let srv_bare_scheme_uri = "mongodb+srv:///admin";
    let slash_prefixed_uri = "/local_db";

    // Act
    let bare_client =
        ConnectionClient::from_uri("Bare Scheme", bare_scheme_uri).expect("bare scheme fallback");
    let srv_bare_client = ConnectionClient::from_uri("SRV Bare", srv_bare_scheme_uri)
        .expect("srv bare scheme fallback");
    let slash_client =
        ConnectionClient::from_uri("Slash Path", slash_prefixed_uri).expect("slash path fallback");

    // Assert
    assert_eq!(bare_client.config.host, "localhost:27017");
    assert_eq!(srv_bare_client.config.host, "localhost:27017");
    assert_eq!(slash_client.config.host, "localhost:27017");
}

#[test]
fn connection_client_from_uri_rejects_empty_or_whitespace_name_and_uri() {
    // Arrange / Act
    let empty_name_err = ConnectionClient::from_uri("", "mongodb://localhost:27017");
    let blank_name_err = ConnectionClient::from_uri("   \t\n ", "mongodb://localhost:27017");
    let empty_uri_err = ConnectionClient::from_uri("Local Dev", "");
    let blank_uri_err = ConnectionClient::from_uri("Local Dev", "   ");

    // Assert
    assert_eq!(
        empty_name_err.unwrap_err(),
        AppError::Connection("connection name must not be empty".to_string())
    );
    assert_eq!(
        blank_name_err.unwrap_err(),
        AppError::Connection("connection name must not be empty".to_string())
    );
    assert_eq!(
        empty_uri_err.unwrap_err(),
        AppError::Connection("connection URI must not be empty".to_string())
    );
    assert_eq!(
        blank_uri_err.unwrap_err(),
        AppError::Connection("connection URI must not be empty".to_string())
    );
}

#[test]
fn connection_config_normalize_extract_host_redact_and_effective_uri() {
    // Arrange / Act: normalize_uri with bare host:port, credentials, and existing URI
    let lan_normalized = ConnectionConfig::normalize_uri("192.168.100.243:27017", "", "");
    let lan_with_creds =
        ConnectionConfig::normalize_uri("192.168.100.243:27017/admin", "root", "s3cr3t");
    let srv_with_user_only =
        ConnectionConfig::normalize_uri("mongodb+srv://cluster0.mongodb.net/prod", "readonly", "");
    let existing_inline_creds = ConnectionConfig::normalize_uri(
        "mongodb://existing:pass@192.168.100.243:27017/admin",
        "ignored_user",
        "ignored_pass",
    );
    let empty_normalized = ConnectionConfig::normalize_uri("   ", "", "");

    // Assert: normalize_uri
    assert_eq!(lan_normalized, "mongodb://192.168.100.243:27017");
    assert_eq!(
        lan_with_creds,
        "mongodb://root:s3cr3t@192.168.100.243:27017/admin"
    );
    assert_eq!(
        srv_with_user_only,
        "mongodb+srv://readonly@cluster0.mongodb.net/prod"
    );
    assert_eq!(
        existing_inline_creds,
        "mongodb://existing:pass@192.168.100.243:27017/admin"
    );
    assert_eq!(empty_normalized, "mongodb://localhost:27017");

    // Act / Assert: extract_display_host & redact_uri
    assert_eq!(
        ConnectionConfig::extract_display_host(&lan_with_creds),
        "192.168.100.243:27017"
    );
    assert_eq!(
        ConnectionConfig::extract_display_host("192.168.100.243:27017"),
        "192.168.100.243:27017"
    );
    assert_eq!(
        ConnectionConfig::redact_uri(&lan_with_creds),
        "mongodb://root:****@192.168.100.243:27017/admin"
    );
    assert_eq!(
        ConnectionConfig::redact_uri(&lan_normalized),
        "mongodb://192.168.100.243:27017"
    );

    // Act / Assert: split_uri_credentials for Edit Connection modal
    assert_eq!(
        ConnectionConfig::split_uri_credentials(&lan_with_creds),
        (
            "mongodb://192.168.100.243:27017/admin".to_string(),
            "root".to_string(),
            "s3cr3t".to_string()
        )
    );
    assert_eq!(
        ConnectionConfig::split_uri_credentials(&lan_normalized),
        (
            "mongodb://192.168.100.243:27017".to_string(),
            String::new(),
            String::new()
        )
    );

    // Act / Assert: new_profile & AppState::build_effective_uri
    let mut profile = ConnectionConfig::new_profile("LAN Mongo", &lan_with_creds)
        .expect("new_profile should succeed");
    assert_eq!(profile.name, "LAN Mongo");
    assert_eq!(profile.host, "192.168.100.243:27017");
    assert_eq!(profile.username, "root");
    assert!(profile.requires_auth);
    assert!(!profile.has_saved_password);
    assert!(!profile.needs_auth_prompt());

    // After disconnecting without saved password, session password is cleared and prompt is required
    profile.clear_session_password();
    assert_eq!(profile.uri, "mongodb://192.168.100.243:27017/admin");
    assert_eq!(profile.username, "root");
    assert!(profile.needs_auth_prompt());

    assert_eq!(profile.status, ConnectionStatus::Connecting);
    assert!(!profile.expanded);
    assert!(profile.databases.is_empty());

    let long_name = "123456789012345678901234567890_EXTRA_CHARACTERS";
    let clamped_profile =
        ConnectionConfig::new_profile(long_name, &lan_normalized).expect("clamped profile");
    assert_eq!(
        clamped_profile.name.chars().count(),
        ConnectionConfig::MAX_PROFILE_NAME_LEN
    );
    assert_eq!(clamped_profile.name, "123456789012345678901234567890");

    let state = AppState {
        new_conn_uri: "192.168.100.243:27017".to_string(),
        new_conn_username: "admin".to_string(),
        new_conn_password: "pw123".to_string(),
        ..AppState::default()
    };
    assert_eq!(
        state.build_effective_uri(),
        "mongodb://admin:pw123@192.168.100.243:27017"
    );
}

#[test]
fn document_row_from_bson_doc_and_query_filter_match_real_bson_types() {
    // Arrange: standard user schema with ObjectId and DateTime
    let oid = ObjectId::parse_str("654f1a2b3c4d5e6f7a8b9c0d").expect("valid hex ObjectId");
    let dt = DateTime::from_millis(1_700_000_000_000);
    let standard_bson = doc! {
        "_id": oid,
        "name": "Alice Wonderland",
        "email": "alice@wonder.land",
        "status": "active",
        "role": "architect",
        "created_at": dt,
    };

    // Arrange: custom schema with nested document, array, numbers, and booleans
    let custom_bson = doc! {
        "_id": 42_i32,
        "sensor_code": "TEMP-09",
        "reading": 23.5_f64,
        "tags": ["iot", "warehouse"],
        "meta": { "floor": 2_i32, "zone": "B" },
        "enabled": true,
    };

    // Act
    let standard_row = DocumentRow::from_bson_doc(&standard_bson);
    let custom_row = DocumentRow::from_bson_doc(&custom_bson);

    // Assert: standard row fields
    assert_eq!(standard_row.id, "654f1a2b3c4d5e6f7a8b9c0d");
    assert_eq!(standard_row.name, "Alice Wonderland");
    assert_eq!(standard_row.email, "alice@wonder.land");
    assert_eq!(standard_row.status, "active");
    assert_eq!(standard_row.role, "architect");
    assert!(standard_row.created_at.contains("2023-11-14"));
    assert!(standard_row.raw_json.is_some());

    // Assert: custom schema fallback formatting & BSON display helpers
    assert_eq!(custom_row.id, "42");
    assert_eq!(custom_row.name, "sensor_code: TEMP-09");
    assert_eq!(custom_row.email, "reading: 23.5");
    assert_eq!(custom_row.status, "true");
    assert_eq!(custom_row.role, "tags: [2 items]");
    assert_eq!(
        DocumentRow::bson_to_display(&Bson::Document(doc! { "floor": 2, "zone": "B" })),
        "{2 fields}"
    );

    // Assert: QueryFilter matches both summary fields and arbitrary raw_json BSON fields
    let sensor_filter = QueryFilter::parse("{\"sensor_code\": \"TEMP-09\", \"enabled\": true}");
    let id_filter = QueryFilter::parse("{\"_id\": 42}");
    let non_matching_filter = QueryFilter::parse("{\"sensor_code\": \"TEMP-99\"}");

    assert!(sensor_filter.matches(&custom_row));
    assert!(id_filter.matches(&custom_row));
    assert!(!non_matching_filter.matches(&custom_row));
}

#[test]
fn connection_manager_and_database_services_provide_mock_hierarchy() {
    // Arrange / Act
    let conn = ConnectionManager::create_connection("Local Dev", "mongodb://localhost:27017")
        .expect("should create mock connection");
    let dbs = DatabaseService::list_mock_databases();
    let coll_info = CollectionService::get_collection_info("admin", "users");
    let lazy_stub = CollectionInfo::new_lazy_stub("shop_db", "events");
    let default_indexes = IndexService::default_indexes();

    // Assert
    assert_eq!(conn.name, "Local Dev");
    assert_eq!(conn.host, "localhost:27017");
    assert_eq!(conn.status, ConnectionStatus::Connected);
    assert_eq!(conn.status.to_string(), "Connected");
    assert!(!ConnectionStatus::Disconnected.is_connected());
    assert_eq!(ConnectionStatus::Connecting.label(), "Connecting...");
    assert_eq!(ConnectionStatus::Error.label(), "Connection Error");

    assert_eq!(dbs.len(), 2);
    assert_eq!(dbs[0].name, "admin");
    assert_eq!(dbs[1].name, "shop_db");

    assert_eq!(coll_info.database, "admin");
    assert_eq!(coll_info.name, "users");
    assert_eq!(coll_info.documents.len(), 5);
    assert_eq!(coll_info.stats.document_count, 5);
    assert!(coll_info.loaded);

    assert_eq!(lazy_stub.database, "shop_db");
    assert_eq!(lazy_stub.name, "events");
    assert!(!lazy_stub.loaded);
    assert!(lazy_stub.documents.is_empty());
    assert_eq!(lazy_stub.stats.document_count, 0);

    assert_eq!(default_indexes.len(), 1);
    assert_eq!(default_indexes[0].name, "_id_");
    assert!(default_indexes[0].unique);

    assert_eq!(
        DatabaseService::create_database_blocking(
            "mongodb://localhost:27017",
            "   ",
            "items",
            Duration::from_secs(1)
        )
        .unwrap_err(),
        AppError::Connection("database name must not be empty".to_string())
    );
    assert_eq!(
        DatabaseService::create_database_blocking(
            "mongodb://localhost:27017",
            "new_db",
            "   ",
            Duration::from_secs(1)
        )
        .unwrap_err(),
        AppError::Connection("initial collection name must not be empty".to_string())
    );
}

#[test]
fn database_service_create_database_and_introspect_validates_names_and_errors_on_unreachable_server(
) {
    // Arrange
    let runtime = build_test_runtime();
    let unreachable_uri = "mongodb://127.0.0.1:1";
    let timeout = Duration::from_millis(200);

    // Act
    let empty_db_res = runtime.block_on(DatabaseService::create_database_and_introspect(
        unreachable_uri,
        "   ",
        "items",
        timeout,
    ));
    let empty_coll_res = runtime.block_on(DatabaseService::create_database_and_introspect(
        unreachable_uri,
        "inventory_db",
        " \t\n ",
        timeout,
    ));
    let unreachable_res = runtime.block_on(DatabaseService::create_database_and_introspect(
        unreachable_uri,
        "inventory_db",
        "items",
        timeout,
    ));

    // Assert
    assert_eq!(
        empty_db_res,
        Err(AppError::Connection(
            "database name must not be empty".to_string()
        ))
    );
    assert_eq!(
        empty_coll_res,
        Err(AppError::Connection(
            "initial collection name must not be empty".to_string()
        ))
    );
    assert!(matches!(unreachable_res, Err(AppError::Connection(_))));
}

#[test]
fn database_and_collection_services_enforce_phase4_offline_guards_before_network_io() {
    // Arrange
    let runtime = build_test_runtime();
    let unreachable_uri = "mongodb://127.0.0.1:1";
    let timeout = Duration::from_millis(200);

    // Act / Assert: DatabaseService::drop_database & drop_database_blocking reject empty & system DBs
    for empty_db in ["", "  \t "] {
        assert_eq!(
            runtime.block_on(DatabaseService::drop_database(
                unreachable_uri,
                empty_db,
                timeout
            )),
            Err(AppError::Connection(
                "database name must not be empty".to_string()
            ))
        );
        assert_eq!(
            DatabaseService::drop_database_blocking(unreachable_uri, empty_db, timeout),
            Err(AppError::Connection(
                "database name must not be empty".to_string()
            ))
        );
    }

    for system_db in ["admin", "local", "config", "Admin", " LOCAL ", "Config"] {
        let async_err = runtime
            .block_on(DatabaseService::drop_database(
                unreachable_uri,
                system_db,
                timeout,
            ))
            .expect_err("system database drop must be rejected");
        assert!(
            async_err
                .to_string()
                .contains("cannot drop system database"),
            "unexpected error for `{system_db}`: {async_err}"
        );

        let blocking_err =
            DatabaseService::drop_database_blocking(unreachable_uri, system_db, timeout)
                .expect_err("system database blocking drop must be rejected");
        assert!(
            blocking_err
                .to_string()
                .contains("cannot drop system database"),
            "unexpected blocking error for `{system_db}`: {blocking_err}"
        );
    }

    // Act / Assert: CollectionService::create_collection & drop_collection reject empty & system.*
    assert_eq!(
        runtime.block_on(CollectionService::create_collection(
            unreachable_uri,
            "   ",
            "orders",
            timeout
        )),
        Err(AppError::Connection(
            "database name must not be empty".to_string()
        ))
    );
    assert_eq!(
        CollectionService::create_collection_blocking(unreachable_uri, "app_db", "  ", timeout),
        Err(AppError::Connection(
            "collection name must not be empty".to_string()
        ))
    );
    assert_eq!(
        runtime.block_on(CollectionService::drop_collection(
            unreachable_uri,
            "",
            "orders",
            timeout
        )),
        Err(AppError::Connection(
            "database name must not be empty".to_string()
        ))
    );
    assert_eq!(
        CollectionService::drop_collection_blocking(unreachable_uri, "app_db", "   ", timeout),
        Err(AppError::Connection(
            "collection name must not be empty".to_string()
        ))
    );

    for sys_coll in ["system.indexes", "system.users", "  system.profile  "] {
        let create_err = runtime
            .block_on(CollectionService::create_collection(
                unreachable_uri,
                "app_db",
                sys_coll,
                timeout,
            ))
            .expect_err("system.* collection creation must be rejected");
        assert!(
            create_err
                .to_string()
                .contains("cannot modify protected system collection"),
            "unexpected error for `{sys_coll}`: {create_err}"
        );

        let drop_err = CollectionService::drop_collection_blocking(
            unreachable_uri,
            "app_db",
            sys_coll,
            timeout,
        )
        .expect_err("system.* collection drop must be rejected");
        assert!(
            drop_err
                .to_string()
                .contains("cannot modify protected system collection"),
            "unexpected drop error for `{sys_coll}`: {drop_err}"
        );
    }

    // Act / Assert: valid names against unreachable server fail cleanly with AppError::Connection
    assert!(matches!(
        runtime.block_on(CollectionService::create_collection(
            unreachable_uri,
            "app_db",
            "orders",
            timeout
        )),
        Err(AppError::Connection(_))
    ));
    assert!(matches!(
        runtime.block_on(CollectionService::drop_collection(
            unreachable_uri,
            "app_db",
            "orders",
            timeout
        )),
        Err(AppError::Connection(_))
    ));
    assert!(matches!(
        runtime.block_on(DatabaseService::drop_database(
            unreachable_uri,
            "app_db",
            timeout
        )),
        Err(AppError::Connection(_))
    ));
}

#[test]
fn document_service_enforces_phase4_offline_guards_and_validates_extended_json() {
    // Arrange
    let runtime = build_test_runtime();
    let unreachable_uri = "mongodb://127.0.0.1:1";
    let timeout = Duration::from_millis(200);
    let default_params = DocumentQueryParams::default();

    // Act / Assert: query_documents rejects empty db/collection & invalid filter/projection/sort
    assert_eq!(
        runtime.block_on(DocumentService::query_documents(
            unreachable_uri,
            "  ",
            "items",
            &default_params,
            timeout
        )),
        Err(AppError::Connection(
            "database name must not be empty".to_string()
        ))
    );
    assert_eq!(
        DocumentService::query_documents_blocking(
            unreachable_uri,
            "shop_db",
            "  ",
            &default_params,
            timeout
        ),
        Err(AppError::Connection(
            "collection name must not be empty".to_string()
        ))
    );

    for bad_params in [
        DocumentQueryParams {
            filter: "{bad_filter".to_string(),
            ..DocumentQueryParams::default()
        },
        DocumentQueryParams {
            projection: "[1, 2]".to_string(),
            ..DocumentQueryParams::default()
        },
        DocumentQueryParams {
            sort: "{bad_sort}".to_string(),
            ..DocumentQueryParams::default()
        },
    ] {
        assert!(matches!(
            runtime.block_on(DocumentService::query_documents(
                unreachable_uri,
                "shop_db",
                "items",
                &bad_params,
                timeout
            )),
            Err(AppError::InvalidQueryJson(_))
        ));
    }

    // Act / Assert: insert_document rejects empty target & invalid Extended JSON
    assert_eq!(
        runtime.block_on(DocumentService::insert_document(
            unreachable_uri,
            "",
            "items",
            "{}",
            timeout
        )),
        Err(AppError::Connection(
            "database name must not be empty".to_string()
        ))
    );
    assert_eq!(
        DocumentService::insert_document_blocking(unreachable_uri, "shop_db", "", "{}", timeout),
        Err(AppError::Connection(
            "collection name must not be empty".to_string()
        ))
    );
    for invalid_json in ["", "   ", "[1, 2]", "{bad: 1}"] {
        assert!(matches!(
            runtime.block_on(DocumentService::insert_document(
                unreachable_uri,
                "shop_db",
                "items",
                invalid_json,
                timeout
            )),
            Err(AppError::InvalidQueryJson(_))
        ));
    }

    // Act / Assert: update_document rejects empty target, invalid JSON, and immutable _id mutation
    let original_id = Bson::Int32(101);
    assert_eq!(
        runtime.block_on(DocumentService::update_document(
            unreachable_uri,
            "  ",
            "items",
            &original_id,
            r#"{"name": "Updated"}"#,
            timeout
        )),
        Err(AppError::Connection(
            "database name must not be empty".to_string()
        ))
    );
    assert_eq!(
        DocumentService::update_document_blocking(
            unreachable_uri,
            "shop_db",
            " ",
            &original_id,
            r#"{"name": "Updated"}"#,
            timeout
        ),
        Err(AppError::Connection(
            "collection name must not be empty".to_string()
        ))
    );
    assert!(matches!(
        runtime.block_on(DocumentService::update_document(
            unreachable_uri,
            "shop_db",
            "items",
            &original_id,
            "{invalid",
            timeout
        )),
        Err(AppError::InvalidQueryJson(_))
    ));
    let immutable_err = runtime
        .block_on(DocumentService::update_document(
            unreachable_uri,
            "shop_db",
            "items",
            &original_id,
            r#"{"_id": 999, "name": "Tampered"}"#,
            timeout,
        ))
        .expect_err("modifying _id must be rejected");
    assert!(
        immutable_err
            .to_string()
            .contains("cannot modify immutable '_id'"),
        "unexpected immutable _id error: {immutable_err}"
    );

    // Act / Assert: delete_document rejects empty target and fails cleanly on unreachable server
    assert_eq!(
        runtime.block_on(DocumentService::delete_document(
            unreachable_uri,
            "",
            "items",
            &original_id,
            timeout
        )),
        Err(AppError::Connection(
            "database name must not be empty".to_string()
        ))
    );
    assert_eq!(
        DocumentService::delete_document_blocking(
            unreachable_uri,
            "shop_db",
            "   ",
            &original_id,
            timeout
        ),
        Err(AppError::Connection(
            "collection name must not be empty".to_string()
        ))
    );
    assert!(matches!(
        runtime.block_on(DocumentService::delete_document(
            unreachable_uri,
            "shop_db",
            "items",
            &original_id,
            timeout
        )),
        Err(AppError::Connection(_))
    ));
}

#[test]
fn connection_manager_keyring_async_handles_nonexistent_entry_without_panicking() {
    // Arrange
    let runtime = build_test_runtime();
    let random_conn_id = Uuid::new_v4();

    // Act
    let load_res = runtime.block_on(ConnectionManager::load_password_from_keyring_async(
        random_conn_id,
    ));
    let delete_res = runtime.block_on(ConnectionManager::delete_password_from_keyring_async(
        random_conn_id,
    ));

    // Assert
    assert!(
        matches!(load_res, Ok(None) | Err(_)),
        "expected Ok(None) or Err for missing keyring entry, got {load_res:?}"
    );
    assert_eq!(delete_res, Ok(()));
}

#[test]
fn document_service_filtering_respects_filter_and_limit_bounds() {
    // Arrange
    let docs = DocumentRow::mock_for_collection("users");

    // Act
    let active_docs = DocumentService::filter_documents(&docs, "{\"status\": \"active\"}", 50);
    let limited_docs = DocumentService::filter_documents(&docs, "{}", 2);
    let zero_limit_clamped = DocumentService::filter_documents(&docs, "{}", 0);
    let no_match_docs = DocumentService::filter_documents(&docs, "{\"status\": \"archived\"}", 50);

    // Assert
    assert_eq!(active_docs.len(), 3);
    assert!(active_docs.iter().all(|d| d.status == "active"));
    assert_eq!(limited_docs.len(), 2);
    assert_eq!(zero_limit_clamped.len(), 1);
    assert!(no_match_docs.is_empty());
}

#[test]
fn user_service_protects_admin_account_and_validates_inputs() {
    // Arrange
    let timeout = Duration::from_secs(2);
    let uri = "mongodb://localhost:27017";
    let sample_roles = vec![UserRole::new("read", "admin")];

    // Act / Assert: ensure_not_protected_admin
    for protected_name in ["admin", "ADMIN", "  Admin  ", "\tadmin\n"] {
        let err = UserService::ensure_not_protected_admin(protected_name).unwrap_err();
        assert_eq!(
            err,
            AppError::Connection(
                "user 'admin' is protected and cannot be modified or deleted".to_string()
            )
        );
    }
    assert!(UserService::ensure_not_protected_admin("analyst").is_ok());

    // Act / Assert: blocking operations strictly reject protected "admin" before network I/O
    assert!(
        UserService::create_user_blocking(uri, "admin", "secret", &sample_roles, timeout)
            .unwrap_err()
            .to_string()
            .contains("protected")
    );
    assert!(
        UserService::update_user_roles_blocking(uri, "admin", "ADMIN", &sample_roles, timeout)
            .unwrap_err()
            .to_string()
            .contains("protected")
    );
    assert!(UserService::change_user_password_blocking(
        uri, "admin", " admin ", "newpass", timeout
    )
    .unwrap_err()
    .to_string()
    .contains("protected"));
    assert!(
        UserService::drop_user_blocking(uri, "admin", "Admin", timeout)
            .unwrap_err()
            .to_string()
            .contains("protected")
    );

    // Act / Assert: empty username or password validation
    assert_eq!(
        UserService::create_user_blocking(uri, "   ", "secret", &sample_roles, timeout)
            .unwrap_err(),
        AppError::Connection("username must not be empty".to_string())
    );
    assert_eq!(
        UserService::create_user_blocking(uri, "qa_user", "", &sample_roles, timeout).unwrap_err(),
        AppError::Connection("password must not be empty".to_string())
    );
    assert_eq!(
        UserService::change_user_password_blocking(uri, "admin", "qa_user", "", timeout)
            .unwrap_err(),
        AppError::Connection("new password must not be empty".to_string())
    );
}

#[test]
fn user_service_async_and_list_methods_reject_admin_invalid_inputs_and_unreachable_server() {
    // Arrange
    let runtime = build_test_runtime();
    let unreachable_uri = "mongodb://127.0.0.1:1";
    let timeout = Duration::from_millis(200);
    let sample_roles = vec![
        UserRole::new("readWrite", "shop_db"),
        UserRole::new("read", "admin"),
    ];

    // Act / Assert: protected "admin" is rejected before network I/O
    let create_admin_res = runtime.block_on(UserService::create_user_and_list(
        unreachable_uri,
        "admin",
        "secret",
        &sample_roles,
        timeout,
    ));
    let update_admin_res = runtime.block_on(UserService::update_user_roles_and_list(
        unreachable_uri,
        "admin",
        " ADMIN ",
        &sample_roles,
        timeout,
    ));
    let drop_admin_res = runtime.block_on(UserService::drop_user_and_list(
        unreachable_uri,
        "admin",
        "Admin",
        timeout,
    ));

    assert!(create_admin_res
        .unwrap_err()
        .to_string()
        .contains("protected"));
    assert!(update_admin_res
        .unwrap_err()
        .to_string()
        .contains("protected"));
    assert!(drop_admin_res
        .unwrap_err()
        .to_string()
        .contains("protected"));

    // Act / Assert: invalid inputs are rejected cleanly
    let empty_user_res = runtime.block_on(UserService::create_user_and_list(
        unreachable_uri,
        "   ",
        "secret",
        &sample_roles,
        timeout,
    ));
    let empty_pwd_res = runtime.block_on(UserService::create_user_and_list(
        unreachable_uri,
        "qa_user",
        "",
        &sample_roles,
        timeout,
    ));

    assert_eq!(
        empty_user_res,
        Err(AppError::Connection(
            "username must not be empty".to_string()
        ))
    );
    assert_eq!(
        empty_pwd_res,
        Err(AppError::Connection(
            "password must not be empty".to_string()
        ))
    );

    // Act / Assert: unreachable server returns Err cleanly (never silently succeeds)
    let create_unreachable_res = runtime.block_on(UserService::create_user_and_list(
        unreachable_uri,
        "qa_unreachable",
        "secret",
        &sample_roles,
        timeout,
    ));
    let update_unreachable_res = runtime.block_on(UserService::update_user_roles_and_list(
        unreachable_uri,
        "admin",
        "qa_unreachable",
        &sample_roles,
        timeout,
    ));
    let drop_unreachable_res = runtime.block_on(UserService::drop_user_and_list(
        unreachable_uri,
        "admin",
        "qa_unreachable",
        timeout,
    ));

    assert!(matches!(
        create_unreachable_res,
        Err(AppError::Connection(_))
    ));
    assert!(matches!(
        update_unreachable_res,
        Err(AppError::Connection(_))
    ));
    assert!(matches!(drop_unreachable_res, Err(AppError::Connection(_))));
}

#[test]
fn live_mongodb_connection_lazy_loading_and_user_lifecycle_on_192_168_100_243_27017() {
    struct TempUserGuard<'a> {
        uri: &'a str,
        username: &'a str,
        timeout: Duration,
        cleaned_up: bool,
    }

    impl<'a> TempUserGuard<'a> {
        fn new(uri: &'a str, username: &'a str, timeout: Duration) -> Self {
            Self {
                uri,
                username,
                timeout,
                cleaned_up: false,
            }
        }

        fn mark_cleaned_up(&mut self) {
            self.cleaned_up = true;
        }
    }

    impl Drop for TempUserGuard<'_> {
        fn drop(&mut self) {
            if !self.cleaned_up {
                let _ =
                    UserService::drop_user_blocking(self.uri, "admin", self.username, self.timeout);
            }
        }
    }

    // Arrange
    let target_host = "192.168.100.243:27017";
    let timeout = Duration::from_secs(5);

    // Act 1: Ping and fast concurrent lazy discovery
    let latency_ms = match ConnectionManager::ping_uri_blocking(target_host, timeout) {
        Ok(ms) => ms,
        Err(err) => {
            println!(
                "Skipping live lazy-loading test on {target_host} (unreachable/blocked: {err})"
            );
            return;
        }
    };

    let connect_start = Instant::now();
    let profile = ConnectionManager::connect_live_blocking("LAN Server", target_host, timeout)
        .expect("should connect and discover databases on 192.168.100.243:27017");
    let discovery_elapsed = connect_start.elapsed();

    println!(
        "Live MongoDB Ping: {:.2} ms | Fast Concurrent Discovery of {} databases completed in {:.2?}",
        latency_ms,
        profile.databases.len(),
        discovery_elapsed
    );
    println!(
        "Admin access: {} | Discovered {} existing users",
        profile.is_admin,
        profile.users.len()
    );

    // Assert 1: Fast concurrent discovery & lazy collection stubs
    assert!(latency_ms >= 0.0);
    assert!(
        discovery_elapsed < Duration::from_secs(5),
        "Concurrent discovery should complete under 5s, took {discovery_elapsed:?}"
    );
    assert_eq!(profile.name, "LAN Server");
    assert_eq!(profile.host, "192.168.100.243:27017");
    assert_eq!(profile.uri, "mongodb://192.168.100.243:27017");
    assert!(profile.status.is_connected());
    assert!(profile.databases.len() >= 10);

    for db in &profile.databases {
        for coll in &db.collections {
            assert!(
                !coll.loaded,
                "Initial discovery stub `{}.{}` must have loaded == false",
                db.name, coll.name
            );
            assert!(coll.documents.is_empty());
        }
    }

    // Act 2: On-demand lazy collection loading (`BookStore.books` or first non-system collection)
    let target_coll = profile
        .databases
        .iter()
        .find(|db| db.name == "BookStore")
        .and_then(|db| {
            db.collections
                .iter()
                .find(|c| c.name == "books")
                .map(|c| (db.name.as_str(), c.name.as_str()))
        })
        .or_else(|| {
            profile
                .databases
                .iter()
                .filter(|db| db.name != "admin" && db.name != "local" && db.name != "config")
                .flat_map(|db| {
                    db.collections
                        .iter()
                        .map(move |c| (db.name.as_str(), c.name.as_str()))
                })
                .next()
        })
        .expect("server should have at least one user collection to lazy-load");

    let loaded_coll = ConnectionManager::load_collection_blocking(
        target_host,
        target_coll.0,
        target_coll.1,
        timeout,
    )
    .expect("should lazy-load target collection on demand");

    println!(
        "Lazy-loaded `{}.{}`: loaded={}, docs={} (count={}), indexes={}, schema_fields={}",
        loaded_coll.database,
        loaded_coll.name,
        loaded_coll.loaded,
        loaded_coll.documents.len(),
        loaded_coll.stats.document_count,
        loaded_coll.indexes.len(),
        loaded_coll.schema_fields.len()
    );

    assert!(loaded_coll.loaded);
    assert_eq!(loaded_coll.database, target_coll.0);
    assert_eq!(loaded_coll.name, target_coll.1);
    assert!(!loaded_coll.indexes.is_empty());

    // Act 3: Live User Lifecycle (create -> update roles -> duplicate with multiple roles -> change password -> drop)
    let temp_username = "qa_temp_user";
    let dup_username = "qa_temp_user_copy";
    let _ = UserService::drop_user_blocking(target_host, "admin", temp_username, timeout);
    let _ = UserService::drop_user_blocking(target_host, "admin", dup_username, timeout);

    let initial_roles = vec![UserRole::new("read", "admin")];
    match UserService::create_user_blocking(
        target_host,
        temp_username,
        "initial_pw_123",
        &initial_roles,
        timeout,
    ) {
        Ok(users_after_create) => {
            let mut temp_guard = TempUserGuard::new(target_host, temp_username, timeout);
            assert!(users_after_create
                .iter()
                .any(|u| u.username == temp_username));

            let updated_roles = vec![
                UserRole::new("readWrite", "BookStore"),
                UserRole::new("read", "admin"),
            ];
            let users_after_role_update = UserService::update_user_roles_blocking(
                target_host,
                "admin",
                temp_username,
                &updated_roles,
                timeout,
            )
            .expect("should update qa_temp_user roles");

            let updated_user = users_after_role_update
                .iter()
                .find(|u| u.username == temp_username)
                .expect("qa_temp_user should be present after role update");
            assert_eq!(updated_user.roles.len(), 2);

            // Duplicate user with multiple cloned roles, update roles, and clean up
            let cloned_roles = updated_user.roles.clone();
            let users_after_dup = UserService::create_user_blocking(
                target_host,
                dup_username,
                "dup_pw_789",
                &cloned_roles,
                timeout,
            )
            .expect("should create duplicated user with multiple roles");
            let mut dup_guard = TempUserGuard::new(target_host, dup_username, timeout);

            let dup_user = users_after_dup
                .iter()
                .find(|u| u.username == dup_username)
                .expect("qa_temp_user_copy should be present after creation");
            assert_eq!(dup_user.roles.len(), 2);

            let dup_reduced_roles = vec![UserRole::new("read", "admin")];
            let users_after_dup_update = UserService::update_user_roles_blocking(
                target_host,
                "admin",
                dup_username,
                &dup_reduced_roles,
                timeout,
            )
            .expect("should update duplicated user roles");
            let dup_updated = users_after_dup_update
                .iter()
                .find(|u| u.username == dup_username)
                .expect("qa_temp_user_copy should be present after role update");
            assert_eq!(dup_updated.roles.len(), 1);

            let users_after_dup_drop =
                UserService::drop_user_blocking(target_host, "admin", dup_username, timeout)
                    .expect("should drop qa_temp_user_copy");
            dup_guard.mark_cleaned_up();
            assert!(!users_after_dup_drop
                .iter()
                .any(|u| u.username == dup_username));

            UserService::change_user_password_blocking(
                target_host,
                "admin",
                temp_username,
                "updated_pw_456",
                timeout,
            )
            .expect("should change qa_temp_user password");

            let users_after_drop =
                UserService::drop_user_blocking(target_host, "admin", temp_username, timeout)
                    .expect("should drop qa_temp_user");
            temp_guard.mark_cleaned_up();
            assert!(!users_after_drop.iter().any(|u| u.username == temp_username));
            println!(
                "Live user lifecycle & duplicate-user test (`{temp_username}`, `{dup_username}`) succeeded end-to-end."
            );
        }
        Err(err) => {
            println!("Skipping live user mutation on {target_host} (server response: {err})");
        }
    }
}

#[test]
fn live_phase4_crud_query_and_drop_lifecycle_on_192_168_100_243_27017() {
    struct TempDbGuard<'a> {
        uri: &'a str,
        db_name: String,
        timeout: Duration,
        cleaned_up: bool,
    }

    impl<'a> TempDbGuard<'a> {
        fn new(uri: &'a str, db_name: String, timeout: Duration) -> Self {
            Self {
                uri,
                db_name,
                timeout,
                cleaned_up: false,
            }
        }

        fn mark_cleaned_up(&mut self) {
            self.cleaned_up = true;
        }
    }

    impl Drop for TempDbGuard<'_> {
        fn drop(&mut self) {
            if !self.cleaned_up {
                let _ =
                    DatabaseService::drop_database_blocking(self.uri, &self.db_name, self.timeout);
            }
        }
    }

    // Arrange
    let target_host = "192.168.100.243:27017";
    let timeout = Duration::from_secs(5);

    if let Err(err) = ConnectionManager::ping_uri_blocking(target_host, timeout) {
        println!(
            "Skipping Phase 4 live CRUD/query test on {target_host} (unreachable/blocked: {err})"
        );
        return;
    }

    let temp_db = format!("qa_phase4_{}", Uuid::new_v4().simple());
    let primary_coll = "items";
    let secondary_coll = "orders";
    let mut db_guard = TempDbGuard::new(target_host, temp_db.clone(), timeout);

    // Act 1: Create temporary database + initial collection, then create second collection
    DatabaseService::create_database_blocking(target_host, &temp_db, primary_coll, timeout)
        .expect("should create temporary database and initial collection");

    let created_second = CollectionService::create_collection_blocking(
        target_host,
        &temp_db,
        secondary_coll,
        timeout,
    )
    .expect("should create second collection in temporary database");
    assert_eq!(created_second.database, temp_db);
    assert_eq!(created_second.name, secondary_coll);
    assert!(created_second.loaded);

    // Act 2: Insert multiple documents (Extended JSON ObjectId, Int32 _id, String _id, auto _id)
    let explicit_oid_hex = "654f1a2b3c4d5e6f7a8b9c01";
    let doc1 = DocumentService::insert_document_blocking(
        target_host,
        &temp_db,
        primary_coll,
        &format!(
            r#"{{"_id": {{"$oid": "{explicit_oid_hex}"}}, "name": "Alpha", "seq": 1, "status": "active", "tags": ["a", "b"], "meta": {{"tier": 1}}}}"#
        ),
        timeout,
    )
    .expect("should insert document with Extended JSON ObjectId");
    assert_eq!(doc1.id, explicit_oid_hex);
    assert!(doc1.cached_json.is_some());

    let doc2 = DocumentService::insert_document_blocking(
        target_host,
        &temp_db,
        primary_coll,
        r#"{"_id": 102, "name": "Beta", "seq": 2, "status": "active", "tags": ["c"], "meta": {"tier": 2}}"#,
        timeout,
    )
    .expect("should insert document with Int32 _id");
    assert_eq!(doc2.extract_bson_id().expect("Int32 _id"), Bson::Int32(102));

    let doc3 = DocumentService::insert_document_blocking(
        target_host,
        &temp_db,
        primary_coll,
        r#"{"_id": "doc_103", "name": "Gamma", "seq": 3, "status": "inactive", "tags": [], "meta": {"tier": 1}}"#,
        timeout,
    )
    .expect("should insert document with String _id");
    assert_eq!(
        doc3.extract_bson_id().expect("String _id"),
        Bson::String("doc_103".to_string())
    );

    let doc4 = DocumentService::insert_document_blocking(
        target_host,
        &temp_db,
        primary_coll,
        r#"{"name": "Delta", "seq": 4, "status": "active", "tags": ["d"], "meta": {"tier": 3}}"#,
        timeout,
    )
    .expect("should insert document with auto-generated _id");
    assert!(matches!(doc4.extract_bson_id(), Ok(Bson::ObjectId(_))));

    // Act 3: Live paginated query with filter, projection, sort, skip, and limit
    let page1 = DocumentService::query_documents_blocking(
        target_host,
        &temp_db,
        primary_coll,
        &DocumentQueryParams {
            filter: r#"{"status": "active"}"#.to_string(),
            projection: r#"{"name": 1, "seq": 1, "status": 1}"#.to_string(),
            sort: r#"{"seq": -1}"#.to_string(),
            skip: 0,
            limit: 2,
        },
        timeout,
    )
    .expect("should execute page 1 live query");

    assert_eq!(page1.total_count, 3);
    assert_eq!(page1.skip, 0);
    assert_eq!(page1.limit, 2);
    assert!(page1.query_ms >= 0.0);
    assert_eq!(page1.documents.len(), 2);
    assert_eq!(page1.documents[0].cell_display_value("name"), "Delta");
    assert_eq!(page1.documents[1].cell_display_value("name"), "Beta");
    assert_eq!(page1.documents[0].cell_display_value("tags"), "");

    let page2 = DocumentService::query_documents_blocking(
        target_host,
        &temp_db,
        primary_coll,
        &DocumentQueryParams {
            filter: r#"{"status": "active"}"#.to_string(),
            projection: r#"{"name": 1, "seq": 1, "status": 1}"#.to_string(),
            sort: r#"{"seq": -1}"#.to_string(),
            skip: 2,
            limit: 2,
        },
        timeout,
    )
    .expect("should execute page 2 live query");

    assert_eq!(page2.total_count, 3);
    assert_eq!(page2.skip, 2);
    assert_eq!(page2.documents.len(), 1);
    assert_eq!(page2.documents[0].cell_display_value("name"), "Alpha");
    assert_eq!(page2.documents[0].id, explicit_oid_hex);

    // Act 4: Update document (reject immutable _id change, then replace cleanly)
    let immutable_res = DocumentService::update_document_blocking(
        target_host,
        &temp_db,
        primary_coll,
        &Bson::Int32(102),
        r#"{"_id": 999, "name": "Tampered"}"#,
        timeout,
    );
    assert!(matches!(immutable_res, Err(AppError::InvalidQueryJson(_))));

    let updated = DocumentService::update_document_blocking(
        target_host,
        &temp_db,
        primary_coll,
        &Bson::Int32(102),
        r#"{"name": "Beta Replaced", "seq": 20, "status": "archived"}"#,
        timeout,
    )
    .expect("should replace document 102");
    assert_eq!(updated.id, "102");
    assert_eq!(updated.cell_display_value("name"), "Beta Replaced");
    assert_eq!(updated.cell_display_value("status"), "archived");

    let verify_replaced = DocumentService::query_documents_blocking(
        target_host,
        &temp_db,
        primary_coll,
        &DocumentQueryParams {
            filter: r#"{"_id": 102}"#.to_string(),
            ..DocumentQueryParams::default()
        },
        timeout,
    )
    .expect("should query replaced document 102");
    assert_eq!(verify_replaced.total_count, 1);
    assert_eq!(
        verify_replaced.documents[0].cell_display_value("name"),
        "Beta Replaced"
    );
    assert_eq!(verify_replaced.documents[0].cell_display_value("tags"), "");

    // Act 5: Delete document (verify deletion and subsequent delete fails cleanly)
    DocumentService::delete_document_blocking(
        target_host,
        &temp_db,
        primary_coll,
        &Bson::Int32(102),
        timeout,
    )
    .expect("should delete document 102");

    let second_delete_err = DocumentService::delete_document_blocking(
        target_host,
        &temp_db,
        primary_coll,
        &Bson::Int32(102),
        timeout,
    )
    .expect_err("deleting an already-deleted document must return Err");
    assert!(
        second_delete_err.to_string().contains("deleted_count = 0"),
        "unexpected second delete error: {second_delete_err}"
    );

    let remaining = DocumentService::query_documents_blocking(
        target_host,
        &temp_db,
        primary_coll,
        &DocumentQueryParams::default(),
        timeout,
    )
    .expect("should query remaining documents");
    assert_eq!(remaining.total_count, 3);

    // Act 6: Drop collections and temporary database, then verify cleanup
    CollectionService::drop_collection_blocking(target_host, &temp_db, secondary_coll, timeout)
        .expect("should drop secondary collection");
    CollectionService::drop_collection_blocking(target_host, &temp_db, primary_coll, timeout)
        .expect("should drop primary collection");

    DatabaseService::drop_database_blocking(target_host, &temp_db, timeout)
        .expect("should drop temporary database");
    db_guard.mark_cleaned_up();

    let post_drop_profile =
        ConnectionManager::connect_live_blocking("LAN Verify", target_host, timeout)
            .expect("should reconnect to verify database cleanup");
    assert!(
        !post_drop_profile
            .databases
            .iter()
            .any(|d| d.name == temp_db),
        "temporary database `{temp_db}` should no longer exist after drop_database"
    );
}

#[test]
fn index_service_validates_create_and_drop_index_specs_and_protections() {
    // Valid index specs
    for (keys_json, unique) in [
        (r#"{"email": 1}"#, true),
        (r#"{"created_at": -1}"#, false),
        (r#"{"status": 1, "created_at": -1}"#, false),
        (r#"{"bio": "text"}"#, false),
        (r#"{"location": "2dsphere"}"#, false),
        (r#"{"coords": "2d"}"#, false),
        (r#"{"token": "hashed"}"#, false),
        (r#"{"_id": 1, "tenant_id": 1}"#, false),
    ] {
        let req = CreateIndexRequest {
            keys_json: keys_json.to_string(),
            name: None,
            unique,
            sparse: false,
            background: false,
        };
        assert!(
            IndexService::validate_create_index("shop_db", "users", &req).is_ok(),
            "expected valid index spec for `{keys_json}`"
        );
    }

    // Empty / invalid JSON / empty field / empty db or collection
    for bad_keys in ["", "   ", "{}", "[1, 2]", "{bad_json", r#"{"  ": 1}"#] {
        let req = CreateIndexRequest {
            keys_json: bad_keys.to_string(),
            name: None,
            unique: false,
            sparse: false,
            background: false,
        };
        assert!(
            matches!(
                IndexService::validate_create_index("shop_db", "users", &req),
                Err(AppError::InvalidQueryJson(_))
            ),
            "expected InvalidQueryJson for `{bad_keys}`"
        );
    }

    let valid_req = CreateIndexRequest {
        keys_json: r#"{"email": 1}"#.to_string(),
        name: None,
        unique: false,
        sparse: false,
        background: false,
    };
    assert!(IndexService::validate_create_index("  ", "users", &valid_req).is_err());
    assert!(IndexService::validate_create_index("shop_db", "  ", &valid_req).is_err());

    // Bad directions
    for bad_dir in [
        r#"{"email": 0}"#,
        r#"{"email": 2}"#,
        r#"{"email": "ascending"}"#,
        r#"{"email": true}"#,
        r#"{"email": null}"#,
    ] {
        let req = CreateIndexRequest {
            keys_json: bad_dir.to_string(),
            name: None,
            unique: false,
            sparse: false,
            background: false,
        };
        assert!(
            matches!(
                IndexService::validate_create_index("shop_db", "users", &req),
                Err(AppError::InvalidQueryJson(_))
            ),
            "expected InvalidQueryJson for bad direction `{bad_dir}`"
        );
    }

    // `_id`-only edge cases must be rejected (primary `_id_` is managed by MongoDB)
    for id_only in [r#"{"_id": 1}"#, r#"{"_id": -1}"#] {
        let req = CreateIndexRequest {
            keys_json: id_only.to_string(),
            name: None,
            unique: false,
            sparse: false,
            background: false,
        };
        assert!(
            IndexService::validate_create_index("shop_db", "users", &req).is_err(),
            "expected _id-only index spec `{id_only}` to be rejected"
        );
    }

    // `system.*` collection rejection
    for sys_coll in ["system.users", "system.indexes", "  system.profile  "] {
        assert!(
            IndexService::validate_create_index("admin", sys_coll, &valid_req).is_err(),
            "expected system.* collection `{sys_coll}` to be rejected"
        );
    }

    // `validate_drop_index` protections
    assert!(IndexService::validate_drop_index("").is_err());
    assert!(IndexService::validate_drop_index("   ").is_err());
    assert!(IndexService::validate_drop_index("_id_").is_err());
    assert!(IndexService::validate_drop_index("  _id_  ").is_err());
    assert_eq!(
        IndexService::validate_drop_index("  email_1  ").expect("valid index name"),
        "email_1"
    );

    // Offline guard before network I/O
    let runtime = build_test_runtime();
    let timeout = Duration::from_millis(200);
    assert!(runtime
        .block_on(IndexService::drop_index(
            "mongodb://127.0.0.1:1",
            "admin",
            "users",
            "_id_",
            timeout
        ))
        .is_err());
    assert!(runtime
        .block_on(IndexService::drop_index(
            "mongodb://127.0.0.1:1",
            "admin",
            "system.users",
            "email_1",
            timeout
        ))
        .is_err());
}

#[test]
fn schema_analyzer_extracts_nested_dot_paths_arrays_mixed_types_and_frequencies() {
    // Empty slice
    assert!(SchemaAnalyzer::analyze_bson_docs(&[]).is_empty());

    // Arrange: nested docs, arrays with multiple elements per doc, and >3 mixed types
    let docs = vec![
        doc! {
            "_id": ObjectId::new(),
            "address": { "city": "Bangkok", "geo": { "lat": 13.75_f64 } },
            "items": [
                { "sku": "SKU-1", "qty": 2_i32 },
                { "sku": "SKU-2", "qty": 5_i32 }
            ],
            "mixed": "text",
        },
        doc! {
            "_id": 2_i32,
            "address": { "city": "Tokyo" },
            "items": [
                { "sku": "SKU-3", "qty": 1_i32 }
            ],
            "mixed": 42_i32,
        },
        doc! {
            "_id": "str-3",
            "mixed": true,
        },
        doc! {
            "_id": 4_i64,
            "mixed": Bson::Null,
        },
    ];

    // Act
    let fields = SchemaAnalyzer::analyze_bson_docs(&docs);

    // Assert: `_id` is first
    assert_eq!(fields[0].field_name, "_id");
    assert_eq!(fields[0].doc_count, 4);
    assert!((fields[0].frequency_pct - 100.0).abs() < 0.01);

    let city = fields
        .iter()
        .find(|f| f.field_name == "address.city")
        .expect("address.city field");
    assert_eq!(city.doc_count, 2);
    assert!((city.frequency_pct - 50.0).abs() < 0.01);
    assert_eq!(city.bson_type, "String");

    let lat = fields
        .iter()
        .find(|f| f.field_name == "address.geo.lat")
        .expect("address.geo.lat field");
    assert_eq!(lat.doc_count, 1);
    assert!((lat.frequency_pct - 25.0).abs() < 0.01);
    assert_eq!(lat.bson_type, "Double");

    let items_arr = fields
        .iter()
        .find(|f| f.field_name == "items")
        .expect("items array field");
    assert_eq!(items_arr.doc_count, 2);
    assert_eq!(items_arr.bson_type, "Array");

    let items_elem = fields
        .iter()
        .find(|f| f.field_name == "items[]")
        .expect("items[] element path");
    assert_eq!(items_elem.doc_count, 2);
    assert_eq!(items_elem.bson_type, "Document");

    let items_sku = fields
        .iter()
        .find(|f| f.field_name == "items[].sku")
        .expect("items[].sku path");
    // Multi-element array in doc 1 still counts doc_count once per document
    assert_eq!(items_sku.doc_count, 2);
    assert!((items_sku.frequency_pct - 50.0).abs() < 0.01);

    let mixed = fields
        .iter()
        .find(|f| f.field_name == "mixed")
        .expect("mixed field");
    assert_eq!(mixed.doc_count, 4);
    assert_eq!(mixed.type_breakdown.len(), 4);
    assert_eq!(mixed.bson_type, "Mixed");
    let breakdown_sum: f32 = mixed.type_breakdown.iter().map(|(_, pct)| *pct).sum();
    assert!((breakdown_sum - 100.0).abs() < 0.1);
}

#[test]
fn transfer_service_json_jsonl_and_rfc4180_csv_roundtrips_and_guards() {
    // Arrange
    let temp_dir = std::env::temp_dir().join(format!("mongo_studio_xfer_qa_{}", Uuid::new_v4()));
    let _guard = TempDirGuard(temp_dir.clone());
    let json_path = temp_dir.join("data.json");
    let jsonl_path = temp_dir.join("data.jsonl");
    let csv_path = temp_dir.join("data.csv");

    let rows = vec![
        DocumentRow::from_bson_doc(&doc! {
            "_id": "row_1",
            "company": "Acme, Inc.",
            "quote_note": "She said \"hello\"\nLine 2",
            "active": true,
        }),
        DocumentRow::from_bson_doc(&doc! {
            "_id": "row_2",
            "company": "Beta Labs",
            "quote_note": "Simple note",
            "active": false,
        }),
    ];

    // Act: export JSON, JSONL, CSV
    assert_eq!(
        TransferService::export_documents_to_path(&rows, TransferFormat::Json, &json_path)
            .expect("JSON export"),
        2
    );
    assert_eq!(
        TransferService::export_documents_to_path(&rows, TransferFormat::JsonLines, &jsonl_path)
            .expect("JSONL export"),
        2
    );
    assert_eq!(
        TransferService::export_documents_to_path(&rows, TransferFormat::Csv, &csv_path)
            .expect("CSV export"),
        2
    );

    // Assert JSON roundtrip
    let json_raw = fs::read_to_string(&json_path).expect("read JSON export");
    let json_val: Vec<Value> = serde_json::from_str(&json_raw).expect("valid JSON array");
    assert_eq!(json_val.len(), 2);
    assert_eq!(json_val[0]["company"], "Acme, Inc.");
    assert_eq!(json_val[0]["quote_note"], "She said \"hello\"\nLine 2");

    // Assert JSONL roundtrip
    let jsonl_raw = fs::read_to_string(&jsonl_path).expect("read JSONL export");
    let jsonl_docs: Vec<_> = jsonl_raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| DocumentRow::parse_json_to_bson_doc(l).expect("valid JSONL line"))
        .collect();
    assert_eq!(jsonl_docs.len(), 2);
    assert_eq!(
        jsonl_docs[0].get_str("company").expect("company"),
        "Acme, Inc."
    );

    // Assert RFC-4180 CSV roundtrip with embedded commas, quotes, and newlines
    let csv_raw = fs::read_to_string(&csv_path).expect("read CSV export");
    let csv_records = TransferService::parse_csv_records(&csv_raw).expect("valid RFC-4180 CSV");
    assert_eq!(csv_records.len(), 3);
    assert_eq!(
        csv_records[0],
        vec!["_id", "company", "quote_note", "active"]
    );
    assert_eq!(
        csv_records[1],
        vec!["row_1", "Acme, Inc.", "She said \"hello\"\nLine 2", "true"]
    );
    assert_eq!(
        csv_records[2],
        vec!["row_2", "Beta Labs", "Simple note", "false"]
    );

    // BOM + CRLF + single-line parsing + unclosed quote error
    let bom_crlf = "\u{feff}_id,note\r\n1,\"hello, \"\"world\"\"\"\r\n";
    let bom_records = TransferService::parse_csv_records(bom_crlf).expect("BOM CRLF CSV");
    assert_eq!(bom_records.len(), 2);
    assert_eq!(bom_records[1], vec!["1", "hello, \"world\""]);
    assert_eq!(
        TransferService::parse_csv_line(r#"a,"b,c","d""e""#).expect("CSV line"),
        vec!["a", "b,c", "d\"e"]
    );
    assert!(matches!(
        TransferService::parse_csv_records("1,\"unterminated"),
        Err(AppError::InvalidQueryJson(_))
    ));

    // Offline import_collection guards
    let runtime = build_test_runtime();
    let timeout = Duration::from_millis(200);
    let unreachable_uri = "mongodb://127.0.0.1:1";

    let sys_err = runtime
        .block_on(TransferService::import_collection(
            unreachable_uri,
            "admin",
            "system.users",
            TransferFormat::Json,
            json_path.clone(),
            timeout,
        ))
        .expect_err("system.* import must be rejected");
    assert!(sys_err.to_string().contains("system."));

    assert!(matches!(
        runtime.block_on(TransferService::import_collection(
            unreachable_uri,
            "shop_db",
            "items",
            TransferFormat::Json,
            temp_dir.join("missing.json"),
            timeout,
        )),
        Err(AppError::Persistence(_))
    ));

    let bad_json_path = temp_dir.join("bad.json");
    fs::write(&bad_json_path, "[1, 2, 3]").expect("write bad json");
    assert!(matches!(
        runtime.block_on(TransferService::import_collection(
            unreachable_uri,
            "shop_db",
            "items",
            TransferFormat::Json,
            bad_json_path,
            timeout,
        )),
        Err(AppError::InvalidQueryJson(_))
    ));
}

#[test]
fn shell_service_extracts_use_db_and_executes_mock_commands() {
    // `extract_use_db` cases
    assert_eq!(
        ShellService::extract_use_db("use shop_db"),
        Some("shop_db".to_string())
    );
    assert_eq!(
        ShellService::extract_use_db("  USE \"analytics_db\";  "),
        Some("analytics_db".to_string())
    );
    assert_eq!(
        ShellService::extract_use_db("use 'qa_db'"),
        Some("qa_db".to_string())
    );
    assert_eq!(ShellService::extract_use_db("use   "), None);
    assert_eq!(ShellService::extract_use_db("use two words"), None);
    assert_eq!(ShellService::extract_use_db("show dbs"), None);

    // `execute_mock_command` cases
    let dbs = DatabaseInfo::mock_databases();

    let help_out = ShellService::execute_mock_command("admin", &dbs, "help").expect("help");
    assert!(help_out.contains("Supported Mongo Studio Shell"));
    assert_eq!(
        ShellService::execute_mock_command("admin", &dbs, "?").expect("?"),
        help_out
    );

    assert_eq!(
        ShellService::execute_mock_command("admin", &dbs, "use shop_db;").expect("use"),
        "switched to db shop_db"
    );

    let show_dbs = ShellService::execute_mock_command("admin", &dbs, "show dbs").expect("show dbs");
    assert!(show_dbs.contains("admin"));
    assert!(show_dbs.contains("shop_db"));

    let show_colls =
        ShellService::execute_mock_command("admin", &dbs, "show collections").expect("show colls");
    assert!(show_colls.contains("users"));
    assert!(show_colls.contains("products"));

    let active_users =
        ShellService::execute_mock_command("admin", &dbs, r#"db.users.find({"status": "active"})"#)
            .expect("db.users.find");
    let parsed_users: Vec<Value> =
        serde_json::from_str(&active_users).expect("find output is valid JSON array");
    assert_eq!(parsed_users.len(), 3);

    let sib_count = ShellService::execute_mock_command(
        "admin",
        &dbs,
        r#"db.getSiblingDB("shop_db").customers.countDocuments({"status": "active"})"#,
    )
    .expect("countDocuments");
    assert_eq!(sib_count, "3");

    let ping = ShellService::execute_mock_command("admin", &dbs, r#"db.runCommand({"ping": 1})"#)
        .expect("ping");
    assert!(ping.contains("\"ok\""));

    // Invalid commands & missing collections
    for bad_cmd in [
        "",
        "   ",
        "drop database",
        "db.users.drop()",
        "db.users.find({bad_json})",
        "db.nonexistent.find({})",
    ] {
        assert!(
            ShellService::execute_mock_command("admin", &dbs, bad_cmd).is_err(),
            "expected error for invalid/unknown command `{bad_cmd}`"
        );
    }
}

#[test]
fn live_phase5_aggregation_pipeline_on_192_168_100_243_27017() {
    let target_host = "192.168.100.243:27017";
    let timeout = Duration::from_secs(5);

    if let Err(err) = ConnectionManager::ping_uri_blocking(target_host, timeout) {
        println!("Skipping Phase 5 live aggregation test on {target_host} (unreachable: {err})");
        return;
    }

    let temp_db = format!("qa_phase5_agg_{}", Uuid::new_v4().simple());
    let coll_name = "sales";
    let _db_guard = Phase5TempDbGuard::new(target_host, temp_db.clone(), timeout);

    DatabaseService::create_database_blocking(target_host, &temp_db, coll_name, timeout)
        .expect("should create temp db");

    for payload in [
        r#"{"category": "books", "amount": 30, "status": "active"}"#,
        r#"{"category": "books", "amount": 20, "status": "active"}"#,
        r#"{"category": "hardware", "amount": 150, "status": "active"}"#,
        r#"{"category": "draft", "amount": 999, "status": "inactive"}"#,
    ] {
        DocumentService::insert_document_blocking(
            target_host,
            &temp_db,
            coll_name,
            payload,
            timeout,
        )
        .expect("should seed aggregation doc");
    }

    let stages = vec![
        AggregationStage::new("$match", r#"{"status": "active"}"#, true),
        AggregationStage::new(
            "$group",
            r#"{"_id": "$category", "total": {"$sum": "$amount"}, "count": {"$sum": 1}}"#,
            true,
        ),
        AggregationStage::new("$sort", r#"{"total": -1}"#, true),
        AggregationStage::new("$limit", "2", true),
    ];
    let pipeline = AggregationStage::build_pipeline(&stages).expect("pipeline builds");

    let result = DocumentService::run_aggregation_blocking(
        target_host,
        &temp_db,
        coll_name,
        &pipeline,
        timeout,
    )
    .expect("live aggregation should succeed");

    assert_eq!(result.stage_count, 4);
    assert!(result.elapsed_ms >= 0.0);
    assert_eq!(result.documents.len(), 2);
    assert_eq!(result.documents[0].id, "hardware");
    assert_eq!(result.documents[0].cell_display_value("total"), "150");
    assert_eq!(result.documents[0].cell_display_value("count"), "1");
    assert_eq!(result.documents[1].id, "books");
    assert_eq!(result.documents[1].cell_display_value("total"), "50");
    assert_eq!(result.documents[1].cell_display_value("count"), "2");
}

#[test]
fn live_phase5_index_management_lifecycle_on_192_168_100_243_27017() {
    let target_host = "192.168.100.243:27017";
    let timeout = Duration::from_secs(5);

    if let Err(err) = ConnectionManager::ping_uri_blocking(target_host, timeout) {
        println!("Skipping Phase 5 live index test on {target_host} (unreachable: {err})");
        return;
    }

    let temp_db = format!("qa_phase5_idx_{}", Uuid::new_v4().simple());
    let coll_name = "accounts";
    let _db_guard = Phase5TempDbGuard::new(target_host, temp_db.clone(), timeout);

    DatabaseService::create_database_blocking(target_host, &temp_db, coll_name, timeout)
        .expect("should create temp db");

    let req = CreateIndexRequest {
        keys_json: r#"{"email": 1}"#.to_string(),
        name: Some("idx_email_unique".to_string()),
        unique: true,
        sparse: false,
        background: false,
    };

    let created_indexes =
        IndexService::create_index_blocking(target_host, &temp_db, coll_name, &req, timeout)
            .expect("should create unique ascending index");
    let created = created_indexes
        .iter()
        .find(|idx| idx.name == "idx_email_unique")
        .expect("created index should appear in list");
    assert!(created.unique);
    assert_eq!(created.index_type, "B-Tree");

    let runtime = build_test_runtime();
    let listed = runtime
        .block_on(IndexService::list_indexes(
            target_host,
            &temp_db,
            coll_name,
            timeout,
        ))
        .expect("list_indexes should succeed");
    assert!(listed.iter().any(|idx| idx.name == "_id_"));
    assert!(listed.iter().any(|idx| idx.name == "idx_email_unique"));

    // Dropping `_id_` must fail safely
    assert!(
        IndexService::drop_index_blocking(target_host, &temp_db, coll_name, "_id_", timeout)
            .is_err()
    );

    // Drop created index and verify removal
    let remaining = IndexService::drop_index_blocking(
        target_host,
        &temp_db,
        coll_name,
        "idx_email_unique",
        timeout,
    )
    .expect("should drop custom index");
    assert!(!remaining.iter().any(|idx| idx.name == "idx_email_unique"));
    assert!(remaining.iter().any(|idx| idx.name == "_id_"));
}

#[test]
fn live_phase5_schema_analysis_on_192_168_100_243_27017() {
    let target_host = "192.168.100.243:27017";
    let timeout = Duration::from_secs(5);

    if let Err(err) = ConnectionManager::ping_uri_blocking(target_host, timeout) {
        println!("Skipping Phase 5 live schema test on {target_host} (unreachable: {err})");
        return;
    }

    let temp_db = format!("qa_phase5_schema_{}", Uuid::new_v4().simple());
    let coll_name = "profiles";
    let _db_guard = Phase5TempDbGuard::new(target_host, temp_db.clone(), timeout);

    DatabaseService::create_database_blocking(target_host, &temp_db, coll_name, timeout)
        .expect("should create temp db");

    for payload in [
        r#"{"address": {"city": "Bangkok", "zip": "10110"}, "items": [{"sku": "A1", "qty": 2}], "score": 95}"#,
        r#"{"address": {"city": "Tokyo"}, "items": [{"sku": "B1", "qty": 1}, {"sku": "B2", "qty": 3}], "score": "A+"}"#,
    ] {
        DocumentService::insert_document_blocking(
            target_host,
            &temp_db,
            coll_name,
            payload,
            timeout,
        )
        .expect("should seed schema doc");
    }

    let fields =
        SchemaAnalyzer::analyze_collection_blocking(target_host, &temp_db, coll_name, 50, timeout)
            .expect("schema analysis should succeed");

    assert_eq!(fields[0].field_name, "_id");
    let city = fields
        .iter()
        .find(|f| f.field_name == "address.city")
        .expect("address.city");
    assert_eq!(city.doc_count, 2);
    assert!((city.frequency_pct - 100.0).abs() < 0.01);

    let sku = fields
        .iter()
        .find(|f| f.field_name == "items[].sku")
        .expect("items[].sku");
    assert_eq!(sku.doc_count, 2);

    let score = fields
        .iter()
        .find(|f| f.field_name == "score")
        .expect("score mixed field");
    assert_eq!(score.doc_count, 2);
    assert_eq!(score.type_breakdown.len(), 2);
}

#[test]
fn live_phase5_export_and_import_lifecycle_on_192_168_100_243_27017() {
    let target_host = "192.168.100.243:27017";
    let timeout = Duration::from_secs(5);

    if let Err(err) = ConnectionManager::ping_uri_blocking(target_host, timeout) {
        println!("Skipping Phase 5 live export/import test on {target_host} (unreachable: {err})");
        return;
    }

    let temp_db = format!("qa_phase5_xfer_{}", Uuid::new_v4().simple());
    let src_coll = "source_docs";
    let _db_guard = Phase5TempDbGuard::new(target_host, temp_db.clone(), timeout);

    let temp_dir = std::env::temp_dir().join(format!("mongo_studio_live_xfer_{}", Uuid::new_v4()));
    let _dir_guard = TempDirGuard(temp_dir.clone());

    DatabaseService::create_database_blocking(target_host, &temp_db, src_coll, timeout)
        .expect("should create temp db");

    for payload in [
        r#"{"_id": "d1", "company": "Acme, Inc.", "note": "She said \"hi\"\nLine 2", "status": "active"}"#,
        r#"{"_id": "d2", "company": "Beta Co", "note": "Normal text", "status": "active"}"#,
        r#"{"_id": "d3", "company": "Gamma", "note": "Comma, separated", "status": "archived"}"#,
    ] {
        DocumentService::insert_document_blocking(
            target_host,
            &temp_db,
            src_coll,
            payload,
            timeout,
        )
        .expect("should seed export doc");
    }

    let runtime = build_test_runtime();

    for (fmt, filename, target_coll) in [
        (TransferFormat::Json, "export.json", "imported_json"),
        (TransferFormat::JsonLines, "export.jsonl", "imported_jsonl"),
        (TransferFormat::Csv, "export.csv", "imported_csv"),
    ] {
        let file_path = temp_dir.join(filename);
        let export_stats = runtime
            .block_on(TransferService::export_collection(
                target_host,
                &temp_db,
                src_coll,
                "{}",
                fmt,
                file_path.clone(),
                timeout,
            ))
            .expect("live export should succeed");
        assert_eq!(export_stats.count, 3);
        assert!(file_path.exists());

        let import_stats = runtime
            .block_on(TransferService::import_collection(
                target_host,
                &temp_db,
                target_coll,
                fmt,
                file_path,
                timeout,
            ))
            .expect("live import should succeed");
        assert_eq!(import_stats.inserted_count, 3);

        let queried = DocumentService::query_documents_blocking(
            target_host,
            &temp_db,
            target_coll,
            &DocumentQueryParams {
                sort: r#"{"_id": 1}"#.to_string(),
                ..DocumentQueryParams::default()
            },
            timeout,
        )
        .expect("should query imported collection");
        assert_eq!(queried.total_count, 3);
        assert_eq!(queried.documents[0].id, "d1");
        assert_eq!(
            queried.documents[0].cell_display_value("company"),
            "Acme, Inc."
        );
        assert_eq!(
            queried.documents[0].cell_display_value("note"),
            "She said \"hi\"\nLine 2"
        );
    }
}

#[test]
fn live_phase5_shell_commands_on_192_168_100_243_27017() {
    let target_host = "192.168.100.243:27017";
    let timeout = Duration::from_secs(5);

    if let Err(err) = ConnectionManager::ping_uri_blocking(target_host, timeout) {
        println!("Skipping Phase 5 live shell test on {target_host} (unreachable: {err})");
        return;
    }

    let temp_db = format!("qa_phase5_shell_{}", Uuid::new_v4().simple());
    let coll_name = "orders";
    let _db_guard = Phase5TempDbGuard::new(target_host, temp_db.clone(), timeout);

    DatabaseService::create_database_blocking(target_host, &temp_db, coll_name, timeout)
        .expect("should create temp db");
    DocumentService::insert_document_blocking(
        target_host,
        &temp_db,
        coll_name,
        r#"{"_id": 1, "item": "Laptop", "status": "active"}"#,
        timeout,
    )
    .expect("seed active order");
    DocumentService::insert_document_blocking(
        target_host,
        &temp_db,
        coll_name,
        r#"{"_id": 2, "item": "Mouse", "status": "archived"}"#,
        timeout,
    )
    .expect("seed archived order");

    let show_dbs_out =
        ShellService::execute_command_blocking(target_host, "admin", "show dbs", timeout)
            .expect("show dbs");
    assert!(show_dbs_out.contains(&temp_db));

    let use_cmd = format!("use {temp_db}");
    let use_out = ShellService::execute_command_blocking(target_host, "admin", &use_cmd, timeout)
        .expect("use temp_db");
    assert_eq!(use_out, format!("switched to db {temp_db}"));

    let show_colls_out =
        ShellService::execute_command_blocking(target_host, &temp_db, "show collections", timeout)
            .expect("show collections");
    assert!(show_colls_out.contains(coll_name));

    let find_out = ShellService::execute_command_blocking(
        target_host,
        &temp_db,
        r#"db.orders.find({"status": "active"})"#,
        timeout,
    )
    .expect("shell find");
    let found_docs: Vec<Value> = serde_json::from_str(&find_out).expect("valid shell find JSON");
    assert_eq!(found_docs.len(), 1);
    assert_eq!(found_docs[0]["item"], "Laptop");

    let count_out = ShellService::execute_command_blocking(
        target_host,
        &temp_db,
        r#"db.orders.countDocuments({"status": "active"})"#,
        timeout,
    )
    .expect("shell countDocuments");
    assert_eq!(count_out, "1");

    let ping_out = ShellService::execute_command_blocking(
        target_host,
        &temp_db,
        r#"db.runCommand({"ping": 1})"#,
        timeout,
    )
    .expect("shell runCommand ping");
    let ping_json: Value = serde_json::from_str(&ping_out).expect("valid ping JSON");
    assert!(ping_json.get("ok").is_some());
}

#[test]
fn ssh_tunnel_config_parse_endpoint_handles_valid_formats_and_rejects_injection() {
    // Valid IPv4, hostname, IPv6, and ssh:// / user@ forms
    assert_eq!(
        SshTunnelConfig::parse_endpoint(" 192.168.1.50:2222 ").expect("IPv4 with port"),
        ("192.168.1.50".to_string(), 2222)
    );
    assert_eq!(
        SshTunnelConfig::parse_endpoint("bastion.internal").expect("hostname defaults to 22"),
        ("bastion.internal".to_string(), 22)
    );
    assert_eq!(
        SshTunnelConfig::parse_endpoint("[::1]:2222").expect("bracketed IPv6 with port"),
        ("::1".to_string(), 2222)
    );
    assert_eq!(
        SshTunnelConfig::parse_endpoint("[2001:db8::1]").expect("bracketed IPv6 default port"),
        ("2001:db8::1".to_string(), 22)
    );
    assert_eq!(
        SshTunnelConfig::parse_endpoint("::1").expect("bare IPv6 default port"),
        ("::1".to_string(), 22)
    );
    assert_eq!(
        SshTunnelConfig::parse_endpoint("ssh://ops@10.0.0.5:2200").expect("ssh:// prefix"),
        ("10.0.0.5".to_string(), 2200)
    );
    assert_eq!(
        SshTunnelConfig::parse_endpoint("SSH://ops@bastion.local")
            .expect("case-insensitive ssh://"),
        ("bastion.local".to_string(), 22)
    );

    assert_eq!(
        SshTunnelConfig::parse_endpoint("2001:db8::1:22").expect("bare IPv6 address default port"),
        ("2001:db8::1:22".to_string(), 22)
    );

    // Invalid / option-injection inputs must be rejected
    for invalid in [
        "",
        "   ",
        "-oProxyCommand=id:22",
        "-evil",
        "[-oProxyCommand=id]:22",
        "bad host:22",
        "host\n:22",
        "host\t:22",
        "[::1\x00]:22",
        "host:0",
        "host:65536",
        "host:abc",
        "host:",
        "[::1",
        "[::1]extra",
        "[]:22",
        "2001:db8::1::22",
        "ubuntu@",
        "host/path",
        "host?foo=bar",
    ] {
        assert!(
            matches!(
                SshTunnelConfig::parse_endpoint(invalid),
                Err(AppError::Connection(_))
            ),
            "expected endpoint `{invalid}` to be rejected"
        );
    }
}

#[test]
fn ssh_tunnel_config_validate_offline_and_expanded_key_path_guards() {
    // Disabled tunnel passes through cleanly
    let disabled = SshTunnelConfig::default();
    assert_eq!(
        disabled
            .validate_offline("")
            .expect("disabled tunnel passes"),
        (String::new(), SshTunnelConfig::DEFAULT_PORT)
    );
    assert!(!disabled.needs_secret_prompt(""));

    // Username injection & empty username checks
    for bad_user in ["", "   ", "-oProxyCommand=calc", "bad user", "bad\nuser"] {
        let cfg = SshTunnelConfig {
            enabled: true,
            endpoint: "10.0.0.5:22".to_string(),
            username: bad_user.to_string(),
            auth_method: SshAuthMethod::Password,
            private_key_path: String::new(),
            has_saved_secret: false,
        };
        assert!(
            matches!(cfg.validate_offline("secret"), Err(AppError::Connection(_))),
            "expected username `{bad_user}` to be rejected"
        );
    }

    // Password auth validation & prompt logic
    let pwd_cfg = SshTunnelConfig {
        enabled: true,
        endpoint: "10.0.0.5:22".to_string(),
        username: "ubuntu".to_string(),
        auth_method: SshAuthMethod::Password,
        private_key_path: String::new(),
        has_saved_secret: false,
    };
    assert!(pwd_cfg.needs_secret_prompt(""));
    assert!(!pwd_cfg.needs_secret_prompt("s3cr3t"));
    assert!(pwd_cfg.validate_offline("").is_err());
    assert_eq!(
        pwd_cfg
            .validate_offline("s3cr3t")
            .expect("valid password config"),
        ("10.0.0.5".to_string(), 22)
    );

    let saved_pwd_cfg = SshTunnelConfig {
        has_saved_secret: true,
        ..pwd_cfg
    };
    assert!(!saved_pwd_cfg.needs_secret_prompt(""));

    // PrivateKey auth: passphrase optional, control chars rejected, ~ expansion works
    let key_cfg = SshTunnelConfig {
        enabled: true,
        endpoint: "bastion.internal:2222".to_string(),
        username: "ops".to_string(),
        auth_method: SshAuthMethod::PrivateKey,
        private_key_path: String::new(),
        has_saved_secret: false,
    };
    assert!(!key_cfg.needs_secret_prompt(""));
    assert_eq!(
        key_cfg
            .validate_offline("")
            .expect("private key without passphrase passes offline validation"),
        ("bastion.internal".to_string(), 2222)
    );
    let default_expanded = key_cfg.expanded_key_path();
    assert_eq!(default_expanded, key_cfg.resolved_key_path());
    assert!(
        default_expanded.ends_with(PathBuf::from(".ssh").join("id_ecdsa")),
        "expected default key path to end with .ssh/id_ecdsa, got {}",
        default_expanded.display()
    );

    let custom_tilde_cfg = SshTunnelConfig {
        private_key_path: "~/keys/mongo_ecdsa".to_string(),
        ..key_cfg.clone()
    };
    assert!(custom_tilde_cfg
        .expanded_key_path()
        .ends_with(PathBuf::from("keys").join("mongo_ecdsa")));

    let ctrl_key_cfg = SshTunnelConfig {
        private_key_path: "/tmp/bad\nkey".to_string(),
        ..key_cfg
    };
    assert!(matches!(
        ctrl_key_cfg.validate_offline(""),
        Err(AppError::Connection(_))
    ));
}

#[test]
fn connection_config_mongo_target_extraction_and_tunnel_uri_rewriting() {
    // Target extraction
    assert_eq!(
        ConnectionConfig::extract_mongo_target_host_port(
            "mongodb://db.internal:27018/prod?authSource=admin"
        )
        .expect("standard URI target"),
        ("db.internal".to_string(), 27018)
    );
    assert_eq!(
        ConnectionConfig::extract_mongo_target_host_port("mongodb://user:pass@10.0.0.5/app")
            .expect("default mongo port"),
        ("10.0.0.5".to_string(), 27017)
    );
    assert_eq!(
        ConnectionConfig::extract_mongo_target_host_port("mongodb://[::1]:27019/app")
            .expect("IPv6 mongo target"),
        ("::1".to_string(), 27019)
    );

    for bad_uri in [
        "",
        "mongodb+srv://cluster.example.net/prod",
        "http://localhost:27017",
        "mongodb://n1:27017,n2:27017/prod",
        "mongodb://-evilhost:27017/prod",
    ] {
        assert!(
            matches!(
                ConnectionConfig::extract_mongo_target_host_port(bad_uri),
                Err(AppError::Connection(_))
            ),
            "expected URI `{bad_uri}` to be rejected"
        );
    }

    // URI rewriting preserves credentials, path, and query options + ensures directConnection=true
    assert_eq!(
        ConnectionConfig::rewrite_uri_for_tunnel(
            "mongodb://admin:s3cr3t@10.20.30.40:27018/appdb?authSource=admin&tls=true",
            51234
        )
        .expect("rewrite with creds and query"),
        "mongodb://admin:s3cr3t@127.0.0.1:51234/appdb?authSource=admin&tls=true&directConnection=true"
    );
    assert_eq!(
        ConnectionConfig::rewrite_uri_for_tunnel(
            "mongodb://10.20.30.40?DirectConnection=true&replicaSet=rs0",
            51234
        )
        .expect("rewrite without duplicate directConnection"),
        "mongodb://127.0.0.1:51234/?DirectConnection=true&replicaSet=rs0"
    );
    assert!(matches!(
        ConnectionConfig::rewrite_uri_for_tunnel("mongodb://10.0.0.1:27017", 0),
        Err(AppError::Connection(_))
    ));
    assert!(matches!(
        ConnectionConfig::rewrite_uri_for_tunnel("mongodb+srv://cluster.example.net/db", 51234),
        Err(AppError::Connection(_))
    ));

    // Redact URI masks password and preserves query params containing '@'
    assert_eq!(
        ConnectionConfig::redact_uri("mongodb://admin:supersecret@10.0.0.5:27017/admin"),
        "mongodb://admin:****@10.0.0.5:27017/admin"
    );
    assert_eq!(
        ConnectionConfig::redact_uri("mongodb://admin:p@ss:1@10.0.0.5:27017/app?appName=a@b"),
        "mongodb://admin:****@10.0.0.5:27017/app?appName=a@b"
    );
    assert_eq!(
        ConnectionConfig::redact_uri("mongodb://10.0.0.5:27017/app?appName=a@b"),
        "mongodb://10.0.0.5:27017/app?appName=a@b"
    );
}

#[test]
fn ssh_tunnel_registry_disabled_passthrough_and_fail_fast_offline_guards() {
    let runtime = build_test_runtime();
    let registry = SshTunnelRegistry::new();
    let conn_id = Uuid::new_v4();
    let raw_uri = "mongodb://10.0.0.10:27017/prod".to_string();

    // Disabled tunnel passes URI through untouched
    let passthrough = runtime
        .block_on(registry.ensure_tunnel_and_rewrite_uri_async(
            conn_id,
            raw_uri.clone(),
            SshTunnelConfig::default(),
            String::new(),
            Duration::from_secs(2),
        ))
        .expect("disabled tunnel must pass through");
    assert_eq!(passthrough, raw_uri);
    assert_eq!(registry.active_local_port(conn_id), None);
    registry.close_tunnel(conn_id);
    registry.close_all();

    // Missing private key fails fast
    let missing_key_cfg = SshTunnelConfig {
        enabled: true,
        endpoint: "127.0.0.1:22".to_string(),
        username: "ops".to_string(),
        auth_method: SshAuthMethod::PrivateKey,
        private_key_path: format!("/tmp/mongo_studio_missing_key_{}", Uuid::new_v4()),
        has_saved_secret: false,
    };
    let missing_err = runtime
        .block_on(registry.ensure_tunnel_and_rewrite_uri_async(
            conn_id,
            raw_uri.clone(),
            missing_key_cfg,
            String::new(),
            Duration::from_millis(500),
        ))
        .expect_err("missing private key must fail");
    assert!(matches!(missing_err, AppError::Connection(_)));

    // Directory passed as private key path fails fast
    let temp_dir = std::env::temp_dir().join(format!("mongo_studio_keydir_{}", Uuid::new_v4()));
    let _dir_guard = TempDirGuard(temp_dir.clone());
    fs::create_dir_all(&temp_dir).expect("create temp key dir");
    let dir_key_cfg = SshTunnelConfig {
        enabled: true,
        endpoint: "127.0.0.1:22".to_string(),
        username: "ops".to_string(),
        auth_method: SshAuthMethod::PrivateKey,
        private_key_path: temp_dir.display().to_string(),
        has_saved_secret: false,
    };
    let dir_err = runtime
        .block_on(registry.ensure_tunnel_and_rewrite_uri_async(
            conn_id,
            raw_uri.clone(),
            dir_key_cfg,
            String::new(),
            Duration::from_millis(500),
        ))
        .expect_err("directory as key path must fail");
    assert!(dir_err.to_string().contains("not a regular file"));

    // Empty password fails fast
    let empty_pwd_cfg = SshTunnelConfig {
        enabled: true,
        endpoint: "127.0.0.1:22".to_string(),
        username: "ops".to_string(),
        auth_method: SshAuthMethod::Password,
        private_key_path: String::new(),
        has_saved_secret: false,
    };
    assert!(matches!(
        runtime.block_on(registry.ensure_tunnel_and_rewrite_uri_async(
            conn_id,
            raw_uri,
            empty_pwd_cfg.clone(),
            String::new(),
            Duration::from_millis(500),
        )),
        Err(AppError::Connection(_))
    ));

    // SRV URI fails fast
    assert!(matches!(
        runtime.block_on(SshTunnelRegistry::ping_through_ssh_async(
            "mongodb+srv://cluster.example.net/prod".to_string(),
            empty_pwd_cfg,
            "secret".to_string(),
            Duration::from_millis(500),
        )),
        Err(AppError::Connection(_))
    ));
}

#[cfg(unix)]
#[test]
fn ssh_tunnel_registry_unix_private_key_permission_check_and_closed_port_fail_fast() {
    use std::os::unix::fs::PermissionsExt;

    let runtime = build_test_runtime();
    let registry = SshTunnelRegistry::new();
    let conn_id = Uuid::new_v4();

    let temp_dir = std::env::temp_dir().join(format!("mongo_studio_keyperm_{}", Uuid::new_v4()));
    let _dir_guard = TempDirGuard(temp_dir.clone());
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let key_path = temp_dir.join("id_ecdsa");
    fs::write(&key_path, "dummy-private-key-material").expect("write dummy key");

    // 0o644 must be rejected before spawning ssh
    let mut perms_644 = fs::metadata(&key_path).expect("metadata").permissions();
    perms_644.set_mode(0o644);
    fs::set_permissions(&key_path, perms_644).expect("set 0644");

    let key_cfg = SshTunnelConfig {
        enabled: true,
        endpoint: "127.0.0.1:22".to_string(),
        username: "ops".to_string(),
        auth_method: SshAuthMethod::PrivateKey,
        private_key_path: key_path.display().to_string(),
        has_saved_secret: false,
    };
    let perm_err = runtime
        .block_on(registry.ensure_tunnel_and_rewrite_uri_async(
            conn_id,
            "mongodb://127.0.0.1:27017/app".to_string(),
            key_cfg,
            String::new(),
            Duration::from_millis(500),
        ))
        .expect_err("0644 key must be rejected");
    let perm_msg = perm_err.to_string();
    assert!(perm_msg.contains("insecure permissions"));
    assert!(perm_msg.contains("chmod 600"));

    // 0o600 passes permission validation and fails fast against a closed loopback port
    let mut perms_600 = fs::metadata(&key_path).expect("metadata").permissions();
    perms_600.set_mode(0o600);
    fs::set_permissions(&key_path, perms_600).expect("set 0600");

    let closed_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral port");
    let closed_port = closed_listener.local_addr().expect("local_addr").port();
    drop(closed_listener);

    let closed_port_cfg = SshTunnelConfig {
        enabled: true,
        endpoint: format!("127.0.0.1:{closed_port}"),
        username: "ops".to_string(),
        auth_method: SshAuthMethod::PrivateKey,
        private_key_path: key_path.display().to_string(),
        has_saved_secret: false,
    };
    let tunnel_err = runtime
        .block_on(registry.ensure_tunnel_and_rewrite_uri_async(
            conn_id,
            "mongodb://127.0.0.1:27017/app".to_string(),
            closed_port_cfg,
            String::new(),
            Duration::from_millis(500),
        ))
        .expect_err("closed loopback SSH port must fail");
    let tunnel_msg = tunnel_err.to_string();
    assert!(!tunnel_msg.contains("insecure permissions"));
    assert!(!tunnel_msg.contains("chmod 600"));
    assert_eq!(registry.active_local_port(conn_id), None);
}

#[test]
fn connection_manager_ssh_keyring_async_handles_nonexistent_entry_without_panicking() {
    let runtime = build_test_runtime();
    let random_conn_id = Uuid::new_v4();

    let load_res = runtime.block_on(ConnectionManager::load_ssh_secret_from_keyring_async(
        random_conn_id,
    ));
    let delete_res = runtime.block_on(ConnectionManager::delete_ssh_secret_from_keyring_async(
        random_conn_id,
    ));

    assert!(
        matches!(load_res, Ok(None) | Err(_)),
        "expected Ok(None) or Err for missing SSH keyring entry, got {load_res:?}"
    );
    assert_eq!(delete_res, Ok(()));
}
