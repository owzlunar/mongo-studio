use bson::{Bson, Document};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentRow {
    pub id: String,
    pub name: String,
    pub email: String,
    pub status: String,
    pub role: String,
    pub created_at: String,
    #[serde(default)]
    pub raw_json: Option<String>,
}

impl DocumentRow {
    /// Converts a real MongoDB [`bson::Document`] into a [`DocumentRow`],
    /// gracefully mapping common or arbitrary schema fields for table display
    /// while preserving the full relaxed Extended JSON payload in `raw_json`.
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

    #[must_use]
    pub fn to_json_value(&self) -> Value {
        if let Some(raw) = &self.raw_json {
            if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
                return parsed;
            }
        }
        json!({
            "_id": self.id,
            "name": self.name,
            "email": self.email,
            "status": self.status,
            "role": self.role,
            "created_at": self.created_at
        })
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

    #[must_use]
    pub fn mock_for_collection(collection_name: &str) -> Vec<Self> {
        match collection_name {
            "users" | "customers" => vec![
                Self {
                    id: "1".to_string(),
                    name: "Alice".to_string(),
                    email: "alice@example.com".to_string(),
                    status: "active".to_string(),
                    role: "admin".to_string(),
                    created_at: "2026-10-01T09:15:00Z".to_string(),
                    raw_json: None,
                },
                Self {
                    id: "2".to_string(),
                    name: "Bob".to_string(),
                    email: "bob@example.com".to_string(),
                    status: "active".to_string(),
                    role: "developer".to_string(),
                    created_at: "2026-10-04T14:22:10Z".to_string(),
                    raw_json: None,
                },
                Self {
                    id: "3".to_string(),
                    name: "Charlie".to_string(),
                    email: "charlie@example.com".to_string(),
                    status: "pending".to_string(),
                    role: "viewer".to_string(),
                    created_at: "2026-10-12T11:05:45Z".to_string(),
                    raw_json: None,
                },
                Self {
                    id: "4".to_string(),
                    name: "Diana".to_string(),
                    email: "diana@example.com".to_string(),
                    status: "active".to_string(),
                    role: "editor".to_string(),
                    created_at: "2026-10-19T16:40:00Z".to_string(),
                    raw_json: None,
                },
                Self {
                    id: "5".to_string(),
                    name: "Evan".to_string(),
                    email: "evan@example.com".to_string(),
                    status: "inactive".to_string(),
                    role: "viewer".to_string(),
                    created_at: "2026-11-01T08:00:00Z".to_string(),
                    raw_json: None,
                },
            ],
            "products" => vec![
                Self {
                    id: "prd_101".to_string(),
                    name: "Mechanical Keyboard Pro".to_string(),
                    email: "sku:MK-PRO-01 ($149.00)".to_string(),
                    status: "active".to_string(),
                    role: "hardware".to_string(),
                    created_at: "2026-09-15T10:00:00Z".to_string(),
                    raw_json: None,
                },
                Self {
                    id: "prd_102".to_string(),
                    name: "4K Monitor 27-inch".to_string(),
                    email: "sku:MON-4K-27 ($399.00)".to_string(),
                    status: "active".to_string(),
                    role: "display".to_string(),
                    created_at: "2026-09-20T12:30:00Z".to_string(),
                    raw_json: None,
                },
                Self {
                    id: "prd_103".to_string(),
                    name: "USB-C Dock Station".to_string(),
                    email: "sku:DOCK-USBC ($89.00)".to_string(),
                    status: "out_of_stock".to_string(),
                    role: "accessory".to_string(),
                    created_at: "2026-10-05T15:00:00Z".to_string(),
                    raw_json: None,
                },
            ],
            "orders" | "payments" => vec![
                Self {
                    id: "ord_9001".to_string(),
                    name: "Order #9001 - Alice".to_string(),
                    email: "total: $548.00 (USD)".to_string(),
                    status: "active".to_string(),
                    role: "paid".to_string(),
                    created_at: "2026-11-10T09:30:00Z".to_string(),
                    raw_json: None,
                },
                Self {
                    id: "ord_9002".to_string(),
                    name: "Order #9002 - Bob".to_string(),
                    email: "total: $149.00 (USD)".to_string(),
                    status: "active".to_string(),
                    role: "shipped".to_string(),
                    created_at: "2026-11-10T17:45:00Z".to_string(),
                    raw_json: None,
                },
            ],
            _ => vec![Self {
                id: "1".to_string(),
                name: format!("Sample {collection_name}"),
                email: "sample@mongo.local".to_string(),
                status: "active".to_string(),
                role: "default".to_string(),
                created_at: "2026-11-11T00:00:00Z".to_string(),
                raw_json: None,
            }],
        }
    }
}
