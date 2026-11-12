use std::str::FromStr;

use serde_json::Value;

use crate::core::AppError;
use crate::models::DocumentRow;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct QueryFilter {
    pub raw: String,
    pub parsed: Option<Value>,
}

impl QueryFilter {
    #[must_use]
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

    #[must_use]
    pub fn is_valid_json(&self) -> bool {
        let trimmed = self.raw.trim();
        trimmed.is_empty() || self.parsed.is_some()
    }

    #[must_use]
    pub fn matches(&self, doc: &DocumentRow) -> bool {
        let Some(Value::Object(map)) = &self.parsed else {
            return true;
        };

        if map.is_empty() {
            return true;
        }

        for (k, expected_val) in map {
            let Some(actual_str) = doc.field_str(k) else {
                return false;
            };
            if expected_val.as_str() != Some(actual_str) {
                return false;
            }
        }

        true
    }
}

impl From<&str> for QueryFilter {
    fn from(raw: &str) -> Self {
        Self::parse(raw)
    }
}

impl FromStr for QueryFilter {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let filter = Self::parse(s);
        if filter.is_valid_json() {
            Ok(filter)
        } else {
            Err(AppError::InvalidQueryJson(s.to_string()))
        }
    }
}
