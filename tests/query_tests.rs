use mongo_studio::models::DocumentRow;
use mongo_studio::query::QueryFilter;

#[test]
fn test_query_filter_json_validation() {
    let valid = QueryFilter::parse("{\"status\": \"active\"}");
    assert!(valid.is_valid_json());

    let invalid = QueryFilter::parse("{status: active}");
    assert!(!invalid.is_valid_json());
}

#[test]
fn test_query_filter_matching() {
    let docs = DocumentRow::mock_for_collection("users");
    let alice_filter = QueryFilter::parse("{\"name\": \"Alice\"}");
    let matched: Vec<_> = docs.iter().filter(|d| alice_filter.matches(d)).collect();
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].email, "alice@example.com");
}
