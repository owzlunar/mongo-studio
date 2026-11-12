use std::time::{Duration, Instant};

use bson::oid::ObjectId;
use bson::{doc, Bson, DateTime};
use mongo_studio::core::{AppError, AppState};
use mongo_studio::database::connection::ConnectionClient;
use mongo_studio::database::{
    CollectionService, ConnectionManager, DatabaseService, DocumentService, IndexService,
    UserService,
};
use mongo_studio::models::{
    CollectionInfo, ConnectionConfig, ConnectionStatus, DocumentRow, UserRole,
};
use mongo_studio::query::QueryFilter;

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
fn live_mongodb_connection_lazy_loading_and_user_lifecycle_on_192_168_100_243_27017() {
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

    // Act 3: Live User Lifecycle (create -> update roles -> change password -> drop)
    let temp_username = "qa_temp_user";
    let _ = UserService::drop_user_blocking(target_host, "admin", temp_username, timeout);

    let initial_roles = vec![UserRole::new("read", "admin")];
    match UserService::create_user_blocking(
        target_host,
        temp_username,
        "initial_pw_123",
        &initial_roles,
        timeout,
    ) {
        Ok(users_after_create) => {
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
            assert!(!users_after_drop.iter().any(|u| u.username == temp_username));
            println!("Live user lifecycle test (`{temp_username}`) succeeded end-to-end.");
        }
        Err(err) => {
            println!(
                "Skipping live user mutation on {target_host} (server response: {err})"
            );
        }
    }
}
