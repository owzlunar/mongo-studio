use mongo_studio::core::AppError;
use mongo_studio::database::connection::ConnectionClient;
use mongo_studio::database::{
    CollectionService, ConnectionManager, DatabaseService, DocumentService, IndexService,
};
use mongo_studio::models::{ConnectionStatus, DocumentRow};

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
