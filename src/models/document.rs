use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRow {
    pub id: String,
    pub name: String,
    pub email: String,
    pub status: String,
    pub role: String,
    pub created_at: String,
}

impl DocumentRow {
    pub fn to_json_value(&self) -> Value {
        json!({
            "_id": self.id,
            "name": self.name,
            "email": self.email,
            "status": self.status,
            "role": self.role,
            "created_at": self.created_at
        })
    }

    pub fn to_pretty_json(&self) -> String {
        serde_json::to_string_pretty(&self.to_json_value()).unwrap_or_default()
    }

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
                },
                Self {
                    id: "2".to_string(),
                    name: "Bob".to_string(),
                    email: "bob@example.com".to_string(),
                    status: "active".to_string(),
                    role: "developer".to_string(),
                    created_at: "2026-10-04T14:22:10Z".to_string(),
                },
                Self {
                    id: "3".to_string(),
                    name: "Charlie".to_string(),
                    email: "charlie@example.com".to_string(),
                    status: "pending".to_string(),
                    role: "viewer".to_string(),
                    created_at: "2026-10-12T11:05:45Z".to_string(),
                },
                Self {
                    id: "4".to_string(),
                    name: "Diana".to_string(),
                    email: "diana@example.com".to_string(),
                    status: "active".to_string(),
                    role: "editor".to_string(),
                    created_at: "2026-10-19T16:40:00Z".to_string(),
                },
                Self {
                    id: "5".to_string(),
                    name: "Evan".to_string(),
                    email: "evan@example.com".to_string(),
                    status: "inactive".to_string(),
                    role: "viewer".to_string(),
                    created_at: "2026-11-01T08:00:00Z".to_string(),
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
                },
                Self {
                    id: "prd_102".to_string(),
                    name: "4K Monitor 27-inch".to_string(),
                    email: "sku:MON-4K-27 ($399.00)".to_string(),
                    status: "active".to_string(),
                    role: "display".to_string(),
                    created_at: "2026-09-20T12:30:00Z".to_string(),
                },
                Self {
                    id: "prd_103".to_string(),
                    name: "USB-C Dock Station".to_string(),
                    email: "sku:DOCK-USBC ($89.00)".to_string(),
                    status: "out_of_stock".to_string(),
                    role: "accessory".to_string(),
                    created_at: "2026-10-05T15:00:00Z".to_string(),
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
                },
                Self {
                    id: "ord_9002".to_string(),
                    name: "Order #9002 - Bob".to_string(),
                    email: "total: $149.00 (USD)".to_string(),
                    status: "active".to_string(),
                    role: "shipped".to_string(),
                    created_at: "2026-11-10T17:45:00Z".to_string(),
                },
            ],
            _ => vec![Self {
                id: "1".to_string(),
                name: format!("Sample {}", collection_name),
                email: "sample@mongo.local".to_string(),
                status: "active".to_string(),
                role: "default".to_string(),
                created_at: "2026-11-11T00:00:00Z".to_string(),
            }],
        }
    }
}
