use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregationStage {
    pub operator: String,
    pub expression: String,
    pub enabled: bool,
}

impl AggregationStage {
    #[must_use]
    pub fn new(operator: impl Into<String>, expression: impl Into<String>, enabled: bool) -> Self {
        Self {
            operator: operator.into(),
            expression: expression.into(),
            enabled,
        }
    }
}

impl Default for AggregationStage {
    fn default() -> Self {
        Self {
            operator: "$match".to_string(),
            expression: "{\n  \"status\": \"active\"\n}".to_string(),
            enabled: true,
        }
    }
}
