use serde_json::Value;

use crate::models::DocumentRow;

#[derive(Debug, Clone)]
pub struct QueryFilter {
    pub raw: String,
    pub parsed: Option<Value>,
}

impl QueryFilter {
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        let parsed = if trimmed.is_empty() {
            None
        } else {
            serde_json::from_str::<Value>(trimmed).ok()
        };
        Self {
            raw: raw.to_string(),
            parsed,
        }
    }

    pub fn is_valid_json(&self) -> bool {
        let trimmed = self.raw.trim();
        trimmed.is_empty() || self.parsed.is_some()
    }

    pub fn matches(&self, doc: &DocumentRow) -> bool {
        let Some(Value::Object(map)) = &self.parsed else {
            return true;
        };

        if map.is_empty() {
            return true;
        }

        let doc_json = doc.to_json_value();
        let Some(doc_obj) = doc_json.as_object() else {
            return true;
        };

        for (k, expected_val) in map {
            match doc_obj.get(k) {
                Some(actual_val) => {
                    if actual_val != expected_val {
                        return false;
                    }
                }
                None => return false,
            }
        }

        true
    }
}
