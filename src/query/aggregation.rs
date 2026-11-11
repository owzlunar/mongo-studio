use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationStage {
    pub operator: String,
    pub expression: String,
    pub enabled: bool,
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
