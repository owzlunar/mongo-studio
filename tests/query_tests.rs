use std::str::FromStr;

use bson::{doc, oid::ObjectId};
use mongo_studio::core::AppError;
use mongo_studio::database::DocumentService;
use mongo_studio::models::DocumentRow;
use mongo_studio::query::{AggregationStage, QueryFilter, QueryProjection, SortDirection, SortSpec};

#[test]
fn query_filter_from_str_and_from_slice_parse_valid_json() {
    // Arrange
    let raw = "{\"status\": \"active\"}";

    // Act
    let parsed = QueryFilter::parse(raw);
    let from_slice = QueryFilter::from(raw);
    let from_str = QueryFilter::from_str(raw).expect("valid JSON should parse via FromStr");
    let empty_str = QueryFilter::from_str("   ").expect("blank filter should be valid");

    // Assert
    assert!(parsed.is_valid_json());
    assert_eq!(parsed, from_slice);
    assert_eq!(parsed, from_str);
    assert!(empty_str.is_valid_json());
    assert!(empty_str.parsed.is_none());
}

#[test]
fn query_filter_rejects_invalid_json_with_app_error() {
    // Arrange
    let malformed = "{status: active}";
    let docs = DocumentRow::mock_for_collection("users");

    // Act
    let filter = QueryFilter::parse(malformed);
    let parse_result = QueryFilter::from_str(malformed);

    // Assert
    assert!(!filter.is_valid_json());
    assert!(filter.parsed.is_none());
    // Invalid filters fall back to matching all documents rather than panicking
    assert!(filter.matches(&docs[0]));
    assert_eq!(
        parse_result.unwrap_err(),
        AppError::InvalidQueryJson(malformed.to_string())
    );
}

#[test]
fn query_filter_handles_empty_object_multi_key_and_unknown_fields() {
    // Arrange
    let docs = DocumentRow::mock_for_collection("users");
    let empty_obj = QueryFilter::parse("{}");
    let multi_key = QueryFilter::parse("{\"status\": \"active\", \"role\": \"admin\"}");
    let mismatched_multi = QueryFilter::parse("{\"status\": \"active\", \"role\": \"viewer\"}");
    let unknown_field = QueryFilter::parse("{\"non_existent_field\": \"Alice\"}");
    let non_string_value = QueryFilter::parse("{\"id\": 1}");

    // Act
    let all_matched: Vec<_> = docs.iter().filter(|d| empty_obj.matches(d)).collect();
    let admin_active: Vec<_> = docs.iter().filter(|d| multi_key.matches(d)).collect();
    let mismatched: Vec<_> = docs.iter().filter(|d| mismatched_multi.matches(d)).collect();
    let unknown_matched: Vec<_> = docs.iter().filter(|d| unknown_field.matches(d)).collect();
    let non_string_matched: Vec<_> = docs.iter().filter(|d| non_string_value.matches(d)).collect();

    // Assert
    assert_eq!(all_matched.len(), docs.len());
    assert_eq!(admin_active.len(), 1);
    assert_eq!(admin_active[0].name, "Alice");
    assert!(mismatched.is_empty());
    assert!(unknown_matched.is_empty());
    assert!(non_string_matched.is_empty());
}

#[test]
fn document_row_field_str_resolves_all_known_keys_and_rejects_unknown() {
    // Arrange
    let docs = DocumentRow::mock_for_collection("users");
    let alice = &docs[0];

    // Act / Assert
    assert_eq!(alice.field_str("_id"), Some("1"));
    assert_eq!(alice.field_str("id"), Some("1"));
    assert_eq!(alice.field_str("name"), Some("Alice"));
    assert_eq!(alice.field_str("email"), Some("alice@example.com"));
    assert_eq!(alice.field_str("status"), Some("active"));
    assert_eq!(alice.field_str("role"), Some("admin"));
    assert_eq!(alice.field_str("created_at"), Some("2026-10-01T09:15:00Z"));
    assert_eq!(alice.field_str("unknown_column"), None);

    let json_val = alice.to_json_value();
    assert_eq!(json_val["_id"], "1");
    assert_eq!(json_val["name"], "Alice");
    assert!(alice.to_pretty_json().contains("\"alice@example.com\""));
}

#[test]
fn query_models_projection_sort_and_aggregation_stage_behave_as_expected() {
    // Arrange / Act
    let stage_default = AggregationStage::default();
    let stage_custom = AggregationStage::new("$group", "{ \"_id\": \"$role\" }", false);
    let proj = QueryProjection::new("{ \"name\": 1 }");
    let sort_default = SortSpec::default();
    let sort_desc = SortSpec::new("created_at", SortDirection::Descending);

    // Assert
    assert_eq!(stage_default.operator, "$match");
    assert!(stage_default.enabled);
    assert_eq!(stage_custom.operator, "$group");
    assert!(!stage_custom.enabled);

    assert_eq!(proj.as_ref(), "{ \"name\": 1 }");

    assert_eq!(sort_default.field, "_id");
    assert_eq!(sort_default.direction, SortDirection::Ascending);
    assert_eq!(sort_desc.field, "created_at");
    assert_eq!(sort_desc.direction, SortDirection::Descending);
}

#[test]
fn query_filter_matches_cached_json_primitives_oid_booleans_numbers_arrays_and_objects() {
    // Arrange
    let oid_a = ObjectId::parse_str("654f1a2b3c4d5e6f7a8b9c01").expect("valid hex ObjectId");
    let oid_b = ObjectId::parse_str("654f1a2b3c4d5e6f7a8b9c02").expect("valid hex ObjectId");

    let doc_a = DocumentRow::from_bson_doc(&doc! {
        "_id": oid_a,
        "sensor": "TEMP-01",
        "enabled": true,
        "count": 42_i32,
        "ratio": 12.75_f64,
        "tags": ["rust", "mongo"],
        "meta": { "zone": "A", "floor": 3_i32 }
    });
    let doc_b = DocumentRow::from_bson_doc(&doc! {
        "_id": oid_b,
        "sensor": "TEMP-02",
        "enabled": false,
        "count": 7_i32,
        "ratio": 0.5_f64,
        "tags": ["go"],
        "meta": { "zone": "B", "floor": 1_i32 }
    });
    // Simulate a deserialized DocumentRow where cached_json is None and only raw_json is present
    let doc_deserialized: DocumentRow = serde_json::from_str(
        &serde_json::to_string(&doc_a).expect("DocumentRow should serialize"),
    )
    .expect("DocumentRow should deserialize");
    assert_eq!(doc_deserialized.cached_json, None);

    // Act
    let oid_extjson_filter =
        QueryFilter::parse(r#"{"_id": {"$oid": "654f1a2b3c4d5e6f7a8b9c01"}}"#);
    let oid_hex_filter = QueryFilter::parse(r#"{"_id": "654f1a2b3c4d5e6f7a8b9c01"}"#);
    let primitive_filter = QueryFilter::parse(r#"{"sensor": "TEMP-02"}"#);
    let bool_true_filter = QueryFilter::parse(r#"{"enabled": true}"#);
    let bool_false_filter = QueryFilter::parse(r#"{"enabled": false}"#);
    let int_filter = QueryFilter::parse(r#"{"count": 42}"#);
    let float_filter = QueryFilter::parse(r#"{"ratio": 12.75}"#);
    let array_filter = QueryFilter::parse(r#"{"tags": ["rust", "mongo"]}"#);
    let object_filter = QueryFilter::parse(r#"{"meta": {"zone": "A", "floor": 3}}"#);
    let combined_filter =
        QueryFilter::parse(r#"{"sensor": "TEMP-01", "enabled": true, "count": 42}"#);
    let mismatch_filter = QueryFilter::parse(r#"{"enabled": true, "count": 99}"#);

    // Assert
    assert!(oid_extjson_filter.matches(&doc_a));
    assert!(oid_extjson_filter.matches(&doc_deserialized));
    assert!(!oid_extjson_filter.matches(&doc_b));

    assert!(oid_hex_filter.matches(&doc_a));
    assert!(!oid_hex_filter.matches(&doc_b));

    assert!(primitive_filter.matches(&doc_b));
    assert!(!primitive_filter.matches(&doc_a));

    assert!(bool_true_filter.matches(&doc_a));
    assert!(!bool_true_filter.matches(&doc_b));
    assert!(bool_false_filter.matches(&doc_b));
    assert!(!bool_false_filter.matches(&doc_a));

    assert!(int_filter.matches(&doc_a));
    assert!(!int_filter.matches(&doc_b));
    assert!(float_filter.matches(&doc_a));
    assert!(!float_filter.matches(&doc_b));

    assert!(array_filter.matches(&doc_a));
    assert!(array_filter.matches(&doc_deserialized));
    assert!(!array_filter.matches(&doc_b));

    assert!(object_filter.matches(&doc_a));
    assert!(!object_filter.matches(&doc_b));

    assert!(combined_filter.matches(&doc_a));
    assert!(!mismatch_filter.matches(&doc_a));
}

#[test]
fn query_filter_gracefully_handles_non_object_json_and_syntax_errors() {
    // Arrange
    let doc = DocumentRow::from_bson_doc(&doc! { "_id": 1_i32, "name": "Item" });
    let array_filter = QueryFilter::parse("[1, 2, 3]");
    let number_filter = QueryFilter::parse("42");
    let syntax_err_filter = QueryFilter::parse("{unterminated");

    // Act / Assert: none panic and non-object/invalid filters degrade to pass-through
    assert!(array_filter.is_valid_json());
    assert!(number_filter.is_valid_json());
    assert!(!syntax_err_filter.is_valid_json());
    assert!(array_filter.matches(&doc));
    assert!(number_filter.matches(&doc));
    assert!(syntax_err_filter.matches(&doc));
    assert!(matches!(
        QueryFilter::from_str("{unterminated"),
        Err(AppError::InvalidQueryJson(_))
    ));
}

#[test]
fn document_service_filter_documents_respects_cached_json_and_pagination_limits() {
    // Arrange
    let docs = vec![
        DocumentRow::from_bson_doc(&doc! {
            "_id": 1_i32,
            "kind": "order",
            "paid": true,
            "items": ["book", "pen"]
        }),
        DocumentRow::from_bson_doc(&doc! {
            "_id": 2_i32,
            "kind": "order",
            "paid": false,
            "items": ["lamp"]
        }),
        DocumentRow::from_bson_doc(&doc! {
            "_id": 3_i32,
            "kind": "order",
            "paid": true,
            "items": ["desk"]
        }),
        DocumentRow::from_bson_doc(&doc! {
            "_id": 4_i32,
            "kind": "refund",
            "paid": true,
            "items": ["chair"]
        }),
    ];

    // Act
    let all_paid_orders =
        DocumentService::filter_documents(&docs, r#"{"kind": "order", "paid": true}"#, 50);
    let limited_paid_orders =
        DocumentService::filter_documents(&docs, r#"{"kind": "order", "paid": true}"#, 1);
    let clamped_zero_limit = DocumentService::filter_documents(&docs, "{}", 0);
    let array_matched = DocumentService::filter_documents(&docs, r#"{"items": ["lamp"]}"#, 10);

    // Assert
    assert_eq!(all_paid_orders.len(), 2);
    assert_eq!(all_paid_orders[0].id, "1");
    assert_eq!(all_paid_orders[1].id, "3");

    assert_eq!(limited_paid_orders.len(), 1);
    assert_eq!(limited_paid_orders[0].id, "1");

    assert_eq!(clamped_zero_limit.len(), 1);
    assert_eq!(array_matched.len(), 1);
    assert_eq!(array_matched[0].id, "2");
}
