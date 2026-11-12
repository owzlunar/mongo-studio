use std::time::Duration;

use bson::oid::ObjectId;
use bson::{doc, Bson, DateTime};
use mongo_studio::core::{AppError, AppState};
use mongo_studio::database::connection::ConnectionClient;
use mongo_studio::database::{
    CollectionService, ConnectionManager, DatabaseService, DocumentService, IndexService,
};
use mongo_studio::models::{ConnectionConfig, ConnectionStatus, DocumentRow};
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

    // Act / Assert: new_profile & AppState::build_effective_uri
    let profile = ConnectionConfig::new_profile("LAN Mongo", &lan_with_creds)
        .expect("new_profile should succeed");
    assert_eq!(profile.name, "LAN Mongo");
    assert_eq!(profile.host, "192.168.100.243:27017");
    assert_eq!(profile.status, ConnectionStatus::Connecting);
    assert!(profile.databases.is_empty());

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

    assert_eq!(default_indexes.len(), 1);
    assert_eq!(default_indexes[0].name, "_id_");
    assert!(default_indexes[0].unique);
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
fn live_mongodb_connection_to_192_168_100_243_27017() {
    // Arrange
    let target_host = "192.168.100.243:27017";
    let timeout = Duration::from_secs(5);

    // Act: Ping and full live introspection
    let latency_ms = ConnectionManager::ping_uri_blocking(target_host, timeout)
        .expect("should ping live MongoDB server at 192.168.100.243:27017");
    let profile = ConnectionManager::connect_live_blocking("LAN Server", target_host, timeout)
        .expect("should connect and introspect live MongoDB server at 192.168.100.243:27017");

    println!(
        "Live MongoDB Ping to {} succeeded in {:.2} ms",
        profile.host, latency_ms
    );
    println!("Discovered {} databases:", profile.databases.len());
    for db in &profile.databases {
        println!("  - Database `{}` ({} collections):", db.name, db.collections.len());
        for coll in &db.collections {
            let sample_fields: Vec<&str> = coll
                .schema_fields
                .iter()
                .map(|f| f.field_name.as_str())
                .collect();
            println!(
                "      * Collection `{}.{}`: {} docs ({} sampled), {} indexes, fields: {:?}",
                db.name,
                coll.name,
                coll.stats.document_count,
                coll.documents.len(),
                coll.indexes.len(),
                sample_fields
            );
        }
    }

    // Assert
    assert!(latency_ms >= 0.0);
    assert_eq!(profile.name, "LAN Server");
    assert_eq!(profile.host, "192.168.100.243:27017");
    assert_eq!(profile.uri, "mongodb://192.168.100.243:27017");
    assert!(profile.status.is_connected());
    assert!(!profile.databases.is_empty());
}
