use std::borrow::Cow;
use std::collections::HashSet;

use bson::{oid::ObjectId, Bson, DateTime, Document};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::core::{AppError, AppResult};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DocumentRow {
    pub id: String,
    pub name: String,
    pub email: String,
    pub status: String,
    pub role: String,
    pub created_at: String,
    #[serde(default)]
    pub raw_json: Option<String>,
    #[serde(skip)]
    pub cached_json: Option<Value>,
}

impl PartialEq for DocumentRow {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.name == other.name
            && self.email == other.email
            && self.status == other.status
            && self.role == other.role
            && self.created_at == other.created_at
            && self.raw_json == other.raw_json
    }
}

impl Eq for DocumentRow {}

impl DocumentRow {
    /// Converts a real MongoDB [`bson::Document`] into a [`DocumentRow`],
    /// gracefully mapping common or arbitrary schema fields for table display
    /// while preserving the full relaxed Extended JSON payload in `raw_json`
    /// and caching the parsed `Value` in `cached_json`.
    #[must_use]
    pub fn from_bson_doc(doc: &Document) -> Self {
        let id = Self::first_matching_field(doc, &["_id", "id"]).unwrap_or_else(|| "-".to_string());

        let non_id_pairs: Vec<(&String, &Bson)> =
            doc.iter().filter(|(k, _)| k.as_str() != "_id").collect();

        let name = Self::first_matching_field(
            doc,
            &["name", "title", "username", "label", "key", "code", "slug"],
        )
        .or_else(|| {
            non_id_pairs
                .first()
                .map(|(k, v)| format!("{k}: {}", Self::bson_to_display(v)))
        })
        .unwrap_or_else(|| "-".to_string());

        let email = Self::first_matching_field(
            doc,
            &[
                "email",
                "sku",
                "description",
                "summary",
                "host",
                "url",
                "value",
                "message",
            ],
        )
        .or_else(|| {
            non_id_pairs
                .get(1)
                .map(|(k, v)| format!("{k}: {}", Self::bson_to_display(v)))
        })
        .unwrap_or_else(|| "-".to_string());

        let status = Self::first_matching_field(doc, &["status", "state", "active", "enabled"])
            .unwrap_or_else(|| "active".to_string());

        let role =
            Self::first_matching_field(doc, &["role", "type", "category", "kind", "group", "tier"])
                .or_else(|| {
                    non_id_pairs
                        .get(2)
                        .map(|(k, v)| format!("{k}: {}", Self::bson_to_display(v)))
                })
                .unwrap_or_else(|| "document".to_string());

        let created_at = Self::first_matching_field(
            doc,
            &[
                "created_at",
                "createdAt",
                "updated_at",
                "updatedAt",
                "timestamp",
                "date",
                "time",
            ],
        )
        .unwrap_or_else(|| "-".to_string());

        let extjson = Bson::Document(doc.clone()).into_relaxed_extjson();
        let raw_json = serde_json::to_string_pretty(&extjson).ok();

        Self {
            id,
            name,
            email,
            status,
            role,
            created_at,
            raw_json,
            cached_json: Some(extjson),
        }
    }

    fn first_matching_field(doc: &Document, keys: &[&str]) -> Option<String> {
        for &key in keys {
            if let Some(val) = doc.get(key) {
                return Some(Self::bson_to_display(val));
            }
        }
        None
    }

    #[must_use]
    pub fn bson_to_display(val: &Bson) -> String {
        match val {
            Bson::ObjectId(oid) => oid.to_hex(),
            Bson::String(s) => s.clone(),
            Bson::Int32(n) => n.to_string(),
            Bson::Int64(n) => n.to_string(),
            Bson::Double(f) => f.to_string(),
            Bson::Boolean(b) => b.to_string(),
            Bson::DateTime(dt) => dt
                .try_to_rfc3339_string()
                .unwrap_or_else(|_| dt.timestamp_millis().to_string()),
            Bson::Null => "null".to_string(),
            Bson::Array(arr) => format!("[{} items]", arr.len()),
            Bson::Document(sub) => format!("{{{} fields}}", sub.len()),
            other => {
                let ext = other.clone().into_relaxed_extjson();
                serde_json::to_string(&ext).unwrap_or_else(|_| format!("{other:?}"))
            }
        }
    }

    #[must_use]
    pub fn field_str(&self, key: &str) -> Option<&str> {
        match key {
            "_id" | "id" => Some(&self.id),
            "name" => Some(&self.name),
            "email" => Some(&self.email),
            "status" => Some(&self.status),
            "role" => Some(&self.role),
            "created_at" => Some(&self.created_at),
            _ => None,
        }
    }

    fn fallback_json_value(&self) -> Value {
        json!({
            "_id": self.id,
            "name": self.name,
            "email": self.email,
            "status": self.status,
            "role": self.role,
            "created_at": self.created_at
        })
    }

    /// Returns a borrowed reference to `cached_json` if populated, avoiding per-frame JSON
    /// re-parsing in UI hot paths, or parses `raw_json` / constructs the fallback object.
    #[must_use]
    pub fn cached_or_parsed_json(&self) -> Cow<'_, Value> {
        if let Some(cached) = &self.cached_json {
            return Cow::Borrowed(cached);
        }
        if let Some(raw) = &self.raw_json {
            if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
                return Cow::Owned(parsed);
            }
        }
        Cow::Owned(self.fallback_json_value())
    }

    /// Populates `cached_json` in place from `raw_json` when missing (e.g. after deserialization).
    pub fn ensure_cached_json(&mut self) {
        if self.cached_json.is_none() {
            if let Some(raw) = &self.raw_json {
                if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
                    self.cached_json = Some(parsed);
                }
            }
        }
    }

    #[must_use]
    pub fn to_json_value(&self) -> Value {
        self.cached_or_parsed_json().into_owned()
    }

    #[must_use]
    pub fn to_pretty_json(&self) -> String {
        if let Some(raw) = &self.raw_json {
            if !raw.trim().is_empty() {
                return raw.clone();
            }
        }
        serde_json::to_string_pretty(&self.to_json_value()).unwrap_or_default()
    }

    /// Parses a JSON or MongoDB Extended JSON object string into a [`bson::Document`].
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidQueryJson`] if `json_str` is blank or is not a valid JSON object.
    pub fn parse_json_to_bson_doc(json_str: &str) -> AppResult<Document> {
        let trimmed = json_str.trim();
        if trimmed.is_empty() {
            return Err(AppError::InvalidQueryJson(
                "JSON document must not be empty".to_string(),
            ));
        }

        serde_json::from_str::<Document>(trimmed)
            .map_err(|err| AppError::InvalidQueryJson(format!("invalid JSON document: {err}")))
    }

    /// Parses an optional JSON object (e.g. sort or projection), returning `Ok(None)` when blank or `{}`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidQueryJson`] if `json_str` is non-empty and invalid.
    pub fn parse_bson_doc_allow_empty(json_str: &str) -> AppResult<Option<Document>> {
        let trimmed = json_str.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        let doc = Self::parse_json_to_bson_doc(trimmed)?;
        if doc.is_empty() {
            Ok(None)
        } else {
            Ok(Some(doc))
        }
    }

    /// Parses a query filter JSON string into a [`bson::Document`], returning `{}` when blank.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidQueryJson`] if `json_str` is non-empty and invalid.
    pub fn parse_filter_doc(json_str: &str) -> AppResult<Document> {
        let trimmed = json_str.trim();
        if trimmed.is_empty() {
            return Ok(Document::new());
        }
        Self::parse_json_to_bson_doc(trimmed)
    }

    /// Extracts the typed [`bson::Bson`] `_id` for CRUD operations.
    ///
    /// Parses `raw_json` as Extended JSON when present, falling back to `ObjectId`, integer, or
    /// string representation from `self.id` for mock/synthetic rows.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidQueryJson`] if `raw_json` is malformed or no `_id` can be resolved.
    pub fn extract_bson_id(&self) -> AppResult<Bson> {
        if let Some(raw) = &self.raw_json {
            let doc = serde_json::from_str::<Document>(raw).map_err(|err| {
                AppError::InvalidQueryJson(format!("failed to parse document _id: {err}"))
            })?;
            if let Some(id) = doc.get("_id") {
                return Ok(id.clone());
            }
            return Err(AppError::InvalidQueryJson(
                "document is missing _id".to_string(),
            ));
        }

        let trimmed = self.id.trim();
        if trimmed.is_empty() {
            return Err(AppError::InvalidQueryJson(
                "document is missing _id".to_string(),
            ));
        }

        if let Ok(oid) = ObjectId::parse_str(trimmed) {
            return Ok(Bson::ObjectId(oid));
        }
        if let Ok(n) = trimmed.parse::<i32>() {
            return Ok(Bson::Int32(n));
        }
        if let Ok(n) = trimmed.parse::<i64>() {
            return Ok(Bson::Int64(n));
        }
        Ok(Bson::String(self.id.clone()))
    }

    /// Collects unique top-level field names across `docs` in order of first appearance,
    /// always placing `"_id"` first. Falls back to default mock columns when no JSON objects exist.
    #[must_use]
    pub fn extract_columns(docs: &[Self]) -> Vec<String> {
        let mut columns = Vec::new();
        let mut seen = HashSet::new();
        let mut found_object = false;

        for doc in docs {
            if doc.cached_json.is_none() && doc.raw_json.is_none() {
                continue;
            }
            if let Value::Object(map) = doc.cached_or_parsed_json().as_ref() {
                found_object = true;
                for key in map.keys() {
                    if seen.insert(key.clone()) {
                        columns.push(key.clone());
                    }
                }
            }
        }

        if let Some(id_pos) = columns.iter().position(|c| c == "_id") {
            if id_pos > 0 {
                let id_col = columns.remove(id_pos);
                columns.insert(0, id_col);
            }
        }

        if !found_object {
            return ["_id", "name", "email", "status", "role", "created_at"]
                .into_iter()
                .map(str::to_string)
                .collect();
        }

        columns
    }

    /// Returns a compact cell display string for `column` using `cached_json` with zero
    /// per-frame re-parsing, falling back to `raw_json` or `field_str`.
    #[must_use]
    pub fn cell_display_value(&self, column: &str) -> String {
        if let Some(Value::Object(map)) = &self.cached_json {
            return map
                .get(column)
                .map_or_else(String::new, Self::format_json_value_compact);
        }
        if self.raw_json.is_some() {
            if let Value::Object(map) = self.cached_or_parsed_json().as_ref() {
                return map
                    .get(column)
                    .map_or_else(String::new, Self::format_json_value_compact);
            }
        }
        self.field_str(column).unwrap_or("").to_string()
    }

    /// Formats a JSON or MongoDB Extended JSON [`serde_json::Value`] concisely for DataGrid cells.
    #[must_use]
    pub fn format_json_value_compact(val: &Value) -> String {
        match val {
            Value::Null => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            Value::Array(arr) => format!("[{} items]", arr.len()),
            Value::Object(map) => {
                if map.len() == 1 {
                    if let Some(Value::String(oid)) = map.get("$oid") {
                        return oid.clone();
                    }
                    if let Some(date_val) = map.get("$date") {
                        return Self::format_extjson_date(date_val);
                    }
                    for key in [
                        "$numberLong",
                        "$numberInt",
                        "$numberDouble",
                        "$numberDecimal",
                        "$uuid",
                    ] {
                        if let Some(Value::String(num)) = map.get(key) {
                            return num.clone();
                        }
                    }
                    if map.contains_key("$binary") {
                        return "[Binary]".to_string();
                    }
                    if let Some(Value::Object(regex)) = map.get("$regularExpression") {
                        if let Some(Value::String(pattern)) = regex.get("pattern") {
                            let opts = regex
                                .get("options")
                                .and_then(Value::as_str)
                                .unwrap_or("");
                            return format!("/{pattern}/{opts}");
                        }
                    }
                }
                format!("{{{} fields}}", map.len())
            }
        }
    }

    fn format_extjson_date(date_val: &Value) -> String {
        match date_val {
            Value::String(iso) => iso.clone(),
            Value::Number(n) => n.as_i64().map_or_else(
                || n.to_string(),
                |ms| {
                    DateTime::from_millis(ms)
                        .try_to_rfc3339_string()
                        .unwrap_or_else(|_| ms.to_string())
                },
            ),
            Value::Object(inner) => {
                if let Some(ms) = inner
                    .get("$numberLong")
                    .and_then(Value::as_str)
                    .and_then(|s| s.parse::<i64>().ok())
                {
                    DateTime::from_millis(ms)
                        .try_to_rfc3339_string()
                        .unwrap_or_else(|_| ms.to_string())
                } else {
                    "{1 fields}".to_string()
                }
            }
            other => other.to_string(),
        }
    }

    fn mock_row(
        id: &str,
        name: &str,
        email: &str,
        status: &str,
        role: &str,
        created_at: &str,
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            email: email.to_string(),
            status: status.to_string(),
            role: role.to_string(),
            created_at: created_at.to_string(),
            raw_json: None,
            cached_json: None,
        }
    }

    #[must_use]
    pub fn mock_for_collection(collection_name: &str) -> Vec<Self> {
        match collection_name {
            "users" | "customers" => vec![
                Self::mock_row(
                    "1",
                    "Alice",
                    "alice@example.com",
                    "active",
                    "admin",
                    "2026-10-01T09:15:00Z",
                ),
                Self::mock_row(
                    "2",
                    "Bob",
                    "bob@example.com",
                    "active",
                    "developer",
                    "2026-10-04T14:22:10Z",
                ),
                Self::mock_row(
                    "3",
                    "Charlie",
                    "charlie@example.com",
                    "pending",
                    "viewer",
                    "2026-10-12T11:05:45Z",
                ),
                Self::mock_row(
                    "4",
                    "Diana",
                    "diana@example.com",
                    "active",
                    "editor",
                    "2026-10-19T16:40:00Z",
                ),
                Self::mock_row(
                    "5",
                    "Evan",
                    "evan@example.com",
                    "inactive",
                    "viewer",
                    "2026-11-01T08:00:00Z",
                ),
            ],
            "products" => vec![
                Self::mock_row(
                    "prd_101",
                    "Mechanical Keyboard Pro",
                    "sku:MK-PRO-01 ($149.00)",
                    "active",
                    "hardware",
                    "2026-09-15T10:00:00Z",
                ),
                Self::mock_row(
                    "prd_102",
                    "4K Monitor 27-inch",
                    "sku:MON-4K-27 ($399.00)",
                    "active",
                    "display",
                    "2026-09-20T12:30:00Z",
                ),
                Self::mock_row(
                    "prd_103",
                    "USB-C Dock Station",
                    "sku:DOCK-USBC ($89.00)",
                    "out_of_stock",
                    "accessory",
                    "2026-10-05T15:00:00Z",
                ),
            ],
            "orders" | "payments" => vec![
                Self::mock_row(
                    "ord_9001",
                    "Order #9001 - Alice",
                    "total: $548.00 (USD)",
                    "active",
                    "paid",
                    "2026-11-10T09:30:00Z",
                ),
                Self::mock_row(
                    "ord_9002",
                    "Order #9002 - Bob",
                    "total: $149.00 (USD)",
                    "active",
                    "shipped",
                    "2026-11-10T17:45:00Z",
                ),
            ],
            _ => vec![Self::mock_row(
                "1",
                &format!("Sample {collection_name}"),
                "sample@mongo.local",
                "active",
                "default",
                "2026-11-11T00:00:00Z",
            )],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;

    #[test]
    fn parse_json_to_bson_doc_handles_extended_json_and_errors() {
        let oid_hex = "507f1f77bcf86cd799439011";
        let input = format!(r#"{{"_id": {{"$oid": "{oid_hex}"}}, "count": 42}}"#);
        let parsed = DocumentRow::parse_json_to_bson_doc(&input).expect("valid extjson");
        assert_eq!(
            parsed.get_object_id("_id").expect("object id").to_hex(),
            oid_hex
        );
        assert_eq!(
            parsed
                .get_i32("count")
                .or_else(|_| parsed.get_i64("count").map(|v| v as i32))
                .expect("count"),
            42
        );

        assert!(DocumentRow::parse_json_to_bson_doc("").is_err());
        assert!(DocumentRow::parse_json_to_bson_doc("[1, 2]").is_err());
        assert!(DocumentRow::parse_json_to_bson_doc("{bad}").is_err());
    }

    #[test]
    fn parse_allow_empty_and_filter_behave_consistently() {
        assert_eq!(
            DocumentRow::parse_bson_doc_allow_empty("   ").unwrap(),
            None
        );
        assert_eq!(DocumentRow::parse_bson_doc_allow_empty("{}").unwrap(), None);
        assert!(DocumentRow::parse_bson_doc_allow_empty(r#"{"_id": -1}"#)
            .unwrap()
            .is_some());

        assert_eq!(DocumentRow::parse_filter_doc("").unwrap(), Document::new());
        assert_eq!(
            DocumentRow::parse_filter_doc(r#"{"status": "active"}"#).unwrap(),
            doc! { "status": "active" }
        );
    }

    #[test]
    fn from_bson_doc_caches_json_and_extracts_columns_and_cells() {
        let oid = ObjectId::new();
        let d1 = doc! {
            "_id": oid,
            "name": "Alice",
            "tags": ["a", "b"],
            "meta": { "tier": 1 }
        };
        let d2 = doc! {
            "name": "Bob",
            "_id": 99_i32,
            "extra": true
        };
        let r1 = DocumentRow::from_bson_doc(&d1);
        let r2 = DocumentRow::from_bson_doc(&d2);

        assert!(r1.cached_json.is_some());
        assert_eq!(
            DocumentRow::extract_columns(&[r1.clone(), r2.clone()]),
            vec!["_id", "name", "tags", "meta", "extra"]
        );
        assert_eq!(r1.cell_display_value("_id"), oid.to_hex());
        assert_eq!(r1.cell_display_value("tags"), "[2 items]");
        assert_eq!(r1.cell_display_value("meta"), "{1 fields}");
        assert_eq!(r2.cell_display_value("extra"), "true");
        assert_eq!(r1.extract_bson_id().unwrap(), Bson::ObjectId(oid));
        assert_eq!(r2.extract_bson_id().unwrap(), Bson::Int32(99));
    }

    #[test]
    fn mock_docs_fallback_columns_and_id_extraction() {
        let mocks = DocumentRow::mock_for_collection("users");
        assert_eq!(
            DocumentRow::extract_columns(&mocks),
            vec!["_id", "name", "email", "status", "role", "created_at"]
        );
        assert_eq!(mocks[0].cell_display_value("name"), "Alice");
        assert_eq!(mocks[0].extract_bson_id().unwrap(), Bson::Int32(1));
    }
}
