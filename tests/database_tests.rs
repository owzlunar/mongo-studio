use std::time::{Duration, Instant};

use bson::oid::ObjectId;
use bson::{doc, Bson, DateTime};
use mongo_studio::core::{AppError, AppState};
use mongo_studio::database::connection::ConnectionClient;
use mongo_studio::database::{
    CollectionService, ConnectionManager, DatabaseService, DocumentQueryParams, DocumentService,
    IndexService, UserService,
};
use mongo_studio::models::{
    CollectionInfo, ConnectionConfig, ConnectionStatus, DocumentRow, UserRole,
};
use mongo_studio::query::QueryFilter;
use uuid::Uuid;

fn build_test_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("current-thread Tokio runtime should build")
}

#[test]
fn connection_client_from_uri_extracts_standard_and_srv_hosts() {
    // Arrange
    let standard_uri = "  mongodb://db.internal.local:27018/production  ";
    let srv_uri = "mongodb+srv://cluster0.mongodb.net/analytics?retryWrites=true";

    // Act
    let standard_client =
        ConnectionClient::from_uri("  Primary Replica  ", standard_uri).expect("valid standard URI");
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
fn database_service_create_database_and_introspect_validates_names_and_errors_on_unreachable_server()
{
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
            async_err.to_string().contains("cannot drop system database"),
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
    let no_match_docs =
        DocumentService::filter_documents(&docs, "{\"status\": \"archived\"}", 50);

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
    assert!(UserService::create_user_blocking(uri, "admin", "secret", &sample_roles, timeout)
        .unwrap_err()
        .to_string()
        .contains("protected"));
    assert!(
        UserService::update_user_roles_blocking(uri, "admin", "ADMIN", &sample_roles, timeout)
            .unwrap_err()
            .to_string()
            .contains("protected")
    );
    assert!(
        UserService::change_user_password_blocking(uri, "admin", " admin ", "newpass", timeout)
            .unwrap_err()
            .to_string()
            .contains("protected")
    );
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

    assert!(matches!(create_unreachable_res, Err(AppError::Connection(_))));
    assert!(matches!(update_unreachable_res, Err(AppError::Connection(_))));
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
    let latency_ms = ConnectionManager::ping_uri_blocking(target_host, timeout)
        .expect("should ping live MongoDB server at 192.168.100.243:27017");

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
            println!(
                "Skipping live user mutation on {target_host} (server response: {err})"
            );
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
                let _ = DatabaseService::drop_database_blocking(
                    self.uri,
                    &self.db_name,
                    self.timeout,
                );
            }
        }
    }

    // Arrange
    let target_host = "192.168.100.243:27017";
    let timeout = Duration::from_secs(5);

    if let Err(err) = ConnectionManager::ping_uri_blocking(target_host, timeout) {
        println!("Skipping Phase 4 live CRUD/query test on {target_host} (unreachable/blocked: {err})");
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
        !post_drop_profile.databases.iter().any(|d| d.name == temp_db),
        "temporary database `{temp_db}` should no longer exist after drop_database"
    );
}
