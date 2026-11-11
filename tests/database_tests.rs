use mongo_studio::database::{ConnectionManager, DatabaseService, DocumentService};
use mongo_studio::models::DocumentRow;

#[test]
fn test_mock_connection_and_databases() {
    let conn = ConnectionManager::create_connection("Local Dev", "mongodb://localhost:27017")
        .expect("should create mock connection");
    assert_eq!(conn.name, "Local Dev");
    assert_eq!(conn.host, "localhost:27017");
    assert!(!conn.databases.is_empty());

    let dbs = DatabaseService::list_mock_databases();
    assert_eq!(dbs.len(), 2);
    assert_eq!(dbs[0].name, "admin");
    assert_eq!(dbs[1].name, "shop_db");
}

#[test]
fn test_document_service_filtering() {
    let docs = DocumentRow::mock_for_collection("users");
    let active_docs = DocumentService::filter_documents(&docs, "{\"status\": \"active\"}", 50);
    assert_eq!(active_docs.len(), 3);
    assert!(active_docs.iter().all(|d| d.status == "active"));
}
