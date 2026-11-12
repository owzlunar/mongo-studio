use std::str::FromStr;

use mongo_studio::core::AppError;
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
