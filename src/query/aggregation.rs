use bson::{doc, Bson, Document};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::{AppError, AppResult};
use crate::models::DocumentRow;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregationStage {
    pub operator: String,
    pub expression: String,
    pub enabled: bool,
}

impl AggregationStage {
    pub const SUPPORTED_OPERATORS: &[&str] = &[
        "$match",
        "$group",
        "$project",
        "$sort",
        "$limit",
        "$skip",
        "$unwind",
        "$lookup",
        "$addFields",
        "$count",
    ];

    #[must_use]
    pub fn new(operator: impl Into<String>, expression: impl Into<String>, enabled: bool) -> Self {
        Self {
            operator: operator.into(),
            expression: expression.into(),
            enabled,
        }
    }

    #[must_use]
    pub fn default_expression(operator: &str) -> &'static str {
        match operator.trim() {
            "$match" => "{\n  \"status\": \"active\"\n}",
            "$group" => "{\n  \"_id\": \"$status\",\n  \"count\": { \"$sum\": 1 }\n}",
            "$project" => "{\n  \"_id\": 1,\n  \"name\": 1,\n  \"status\": 1\n}",
            "$sort" => "{\n  \"_id\": 1\n}",
            "$limit" => "20",
            "$skip" => "0",
            "$unwind" => "\"$items\"",
            "$lookup" => "{\n  \"from\": \"other_collection\",\n  \"localField\": \"user_id\",\n  \"foreignField\": \"_id\",\n  \"as\": \"joined\"\n}",
            "$addFields" => "{\n  \"processed\": true\n}",
            "$count" => "\"total\"",
            _ => "{}",
        }
    }

    /// Converts this stage into a single MongoDB pipeline stage [`bson::Document`].
    ///
    /// Supports both inner expressions (e.g. `{"status": "active"}` or `10` for `$limit`) and
    /// full wrapped stage documents (e.g. `{"$match": {"status": "active"}}`).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidQueryJson`] if the operator or expression is invalid.
    pub fn to_pipeline_doc(&self) -> AppResult<Document> {
        let op = self.operator.trim();
        if op.is_empty() || !op.starts_with('$') {
            return Err(AppError::InvalidQueryJson(format!(
                "invalid aggregation stage operator '{op}': must start with '$'"
            )));
        }

        let expr = self.expression.trim();
        if expr.is_empty() {
            return Err(AppError::InvalidQueryJson(format!(
                "expression for stage '{op}' must not be empty"
            )));
        }

        if expr.starts_with('{') {
            let doc = DocumentRow::parse_json_to_bson_doc(expr)?;
            if doc.len() == 1 && doc.contains_key(op) {
                if op == "$limit" || op == "$skip" {
                    let val = doc.get(op).ok_or_else(|| {
                        AppError::InvalidQueryJson(format!("missing value for {op}"))
                    })?;
                    let parsed = Self::extract_stage_integer(op, val)?;
                    return Ok(doc! { op: parsed });
                }
                return Ok(doc);
            }

            if op == "$limit" || op == "$skip" || op == "$count" {
                return Err(AppError::InvalidQueryJson(format!(
                    "stage '{op}' expects a scalar value, not an object"
                )));
            }

            return Ok(doc! { op: Bson::Document(doc) });
        }

        match op {
            "$limit" => {
                let n = expr.parse::<i64>().map_err(|_| {
                    AppError::InvalidQueryJson(format!(
                        "$limit expects a positive integer, got '{expr}'"
                    ))
                })?;
                if n <= 0 {
                    return Err(AppError::InvalidQueryJson(
                        "$limit must be greater than 0".to_string(),
                    ));
                }
                Ok(doc! { "$limit": n })
            }
            "$skip" => {
                let n = expr.parse::<i64>().map_err(|_| {
                    AppError::InvalidQueryJson(format!(
                        "$skip expects a non-negative integer, got '{expr}'"
                    ))
                })?;
                if n < 0 {
                    return Err(AppError::InvalidQueryJson(
                        "$skip must be 0 or greater".to_string(),
                    ));
                }
                Ok(doc! { "$skip": n })
            }
            "$unwind" | "$count" => {
                let value_str = if expr.starts_with('"') {
                    serde_json::from_str::<String>(expr).map_err(|err| {
                        AppError::InvalidQueryJson(format!(
                            "invalid string expression for {op}: {err}"
                        ))
                    })?
                } else if expr.starts_with('[') {
                    return Err(AppError::InvalidQueryJson(format!(
                        "{op} expects a string path or field name, not an array"
                    )));
                } else {
                    expr.to_string()
                };

                let trimmed_val = value_str.trim();
                if trimmed_val.is_empty() {
                    return Err(AppError::InvalidQueryJson(format!(
                        "{op} expression must not be empty"
                    )));
                }
                if op == "$unwind" && !trimmed_val.starts_with('$') {
                    return Err(AppError::InvalidQueryJson(
                        "$unwind path must start with '$' (e.g. \"$items\") or be an options object"
                            .to_string(),
                    ));
                }
                Ok(doc! { op: trimmed_val })
            }
            _ => {
                let doc = DocumentRow::parse_json_to_bson_doc(expr)?;
                Ok(doc! { op: Bson::Document(doc) })
            }
        }
    }

    fn extract_stage_integer(op: &str, val: &Bson) -> AppResult<i64> {
        let n = match val {
            Bson::Int32(v) => i64::from(*v),
            Bson::Int64(v) => *v,
            _ => {
                return Err(AppError::InvalidQueryJson(format!(
                    "{op} expects an integer value"
                )));
            }
        };
        if op == "$limit" && n <= 0 {
            return Err(AppError::InvalidQueryJson(
                "$limit must be greater than 0".to_string(),
            ));
        }
        if op == "$skip" && n < 0 {
            return Err(AppError::InvalidQueryJson(
                "$skip must be 0 or greater".to_string(),
            ));
        }
        Ok(n)
    }

    /// Builds a MongoDB aggregation pipeline from enabled stages.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidQueryJson`] if no stages are enabled or any enabled stage fails
    /// to parse.
    pub fn build_pipeline(stages: &[Self]) -> AppResult<Vec<Document>> {
        let mut pipeline = Vec::with_capacity(stages.len());
        for stage in stages.iter().filter(|s| s.enabled) {
            pipeline.push(stage.to_pipeline_doc()?);
        }
        if pipeline.is_empty() {
            return Err(AppError::InvalidQueryJson(
                "aggregation pipeline must contain at least one enabled stage".to_string(),
            ));
        }
        Ok(pipeline)
    }

    /// Serializes enabled stages into a formatted JSON pipeline array `[ ... ]`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidQueryJson`] if the pipeline is empty or invalid.
    pub fn stages_to_pretty_json(stages: &[Self]) -> AppResult<String> {
        let docs = Self::build_pipeline(stages)?;
        let json_values: Vec<Value> = docs
            .into_iter()
            .map(|d| Bson::Document(d).into_relaxed_extjson())
            .collect();
        serde_json::to_string_pretty(&json_values).map_err(|err| {
            AppError::InvalidQueryJson(format!("failed to format aggregation pipeline: {err}"))
        })
    }

    /// Parses a raw JSON pipeline array string `[ { "$match": ... }, ... ]` into BSON documents.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidQueryJson`] if the JSON is invalid, empty, not an array, or
    /// contains non-object or empty elements.
    pub fn parse_raw_pipeline(raw_json: &str) -> AppResult<Vec<Document>> {
        let trimmed = raw_json.trim();
        if trimmed.is_empty() {
            return Err(AppError::InvalidQueryJson(
                "aggregation pipeline JSON must not be empty".to_string(),
            ));
        }

        let parsed: Value = serde_json::from_str(trimmed).map_err(|err| {
            AppError::InvalidQueryJson(format!("invalid aggregation pipeline JSON: {err}"))
        })?;

        let Value::Array(items) = parsed else {
            return Err(AppError::InvalidQueryJson(
                "aggregation pipeline must be a JSON array of stage objects".to_string(),
            ));
        };

        if items.is_empty() {
            return Err(AppError::InvalidQueryJson(
                "aggregation pipeline array must not be empty".to_string(),
            ));
        }

        let mut docs = Vec::with_capacity(items.len());
        for (idx, item) in items.into_iter().enumerate() {
            if !item.is_object() {
                return Err(AppError::InvalidQueryJson(format!(
                    "pipeline stage at index {idx} must be a JSON object"
                )));
            }
            let mut doc: Document = serde_json::from_value(item).map_err(|err| {
                AppError::InvalidQueryJson(format!(
                    "invalid BSON in pipeline stage at index {idx}: {err}"
                ))
            })?;
            if doc.is_empty() {
                return Err(AppError::InvalidQueryJson(format!(
                    "pipeline stage at index {idx} must not be empty"
                )));
            }
            if let Some((op, val)) = doc.iter().next().map(|(k, v)| (k.clone(), v.clone())) {
                if op == "$limit" || op == "$skip" {
                    let n = Self::extract_stage_integer(&op, &val)?;
                    doc.insert(op, n);
                }
            }
            docs.push(doc);
        }

        Ok(docs)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_to_pipeline_doc_supports_shorthand_and_wrapped_forms() {
        let match_stage = AggregationStage::new("$match", r#"{"status": "active"}"#, true);
        assert_eq!(
            match_stage.to_pipeline_doc().unwrap(),
            doc! { "$match": { "status": "active" } }
        );

        let wrapped_match =
            AggregationStage::new("$match", r#"{"$match": {"status": "active"}}"#, true);
        assert_eq!(
            wrapped_match.to_pipeline_doc().unwrap(),
            doc! { "$match": { "status": "active" } }
        );

        let limit_raw = AggregationStage::new("$limit", "25", true);
        assert_eq!(
            limit_raw.to_pipeline_doc().unwrap(),
            doc! { "$limit": 25_i64 }
        );

        let limit_wrapped = AggregationStage::new("$limit", r#"{"$limit": 10}"#, true);
        assert_eq!(
            limit_wrapped.to_pipeline_doc().unwrap(),
            doc! { "$limit": 10_i64 }
        );

        let skip_zero = AggregationStage::new("$skip", "0", true);
        assert_eq!(
            skip_zero.to_pipeline_doc().unwrap(),
            doc! { "$skip": 0_i64 }
        );

        let unwind_unquoted = AggregationStage::new("$unwind", "$items", true);
        assert_eq!(
            unwind_unquoted.to_pipeline_doc().unwrap(),
            doc! { "$unwind": "$items" }
        );

        let unwind_quoted = AggregationStage::new("$unwind", r#""$tags""#, true);
        assert_eq!(
            unwind_quoted.to_pipeline_doc().unwrap(),
            doc! { "$unwind": "$tags" }
        );

        let unwind_obj = AggregationStage::new(
            "$unwind",
            r#"{"path": "$tags", "preserveNullAndEmptyArrays": true}"#,
            true,
        );
        assert_eq!(
            unwind_obj.to_pipeline_doc().unwrap(),
            doc! { "$unwind": { "path": "$tags", "preserveNullAndEmptyArrays": true } }
        );

        let count_unquoted = AggregationStage::new("$count", "total", true);
        assert_eq!(
            count_unquoted.to_pipeline_doc().unwrap(),
            doc! { "$count": "total" }
        );
    }

    #[test]
    fn stage_and_pipeline_builders_reject_invalid_inputs() {
        assert!(AggregationStage::new("$limit", "0", true)
            .to_pipeline_doc()
            .is_err());
        assert!(AggregationStage::new("$skip", "-1", true)
            .to_pipeline_doc()
            .is_err());
        assert!(AggregationStage::new("$unwind", "items", true)
            .to_pipeline_doc()
            .is_err());
        assert!(AggregationStage::new("match", "{}", true)
            .to_pipeline_doc()
            .is_err());

        let disabled_only = vec![AggregationStage::new("$match", "{}", false)];
        assert!(AggregationStage::build_pipeline(&disabled_only).is_err());

        let mixed = vec![
            AggregationStage::new("$match", r#"{"status": "active"}"#, true),
            AggregationStage::new("$group", "{bad", false),
            AggregationStage::new("$limit", "5", true),
        ];
        let pipeline = AggregationStage::build_pipeline(&mixed).unwrap();
        assert_eq!(pipeline.len(), 2);

        let pretty = AggregationStage::stages_to_pretty_json(&mixed).unwrap();
        let reparsed = AggregationStage::parse_raw_pipeline(&pretty).unwrap();
        assert_eq!(reparsed, pipeline);
    }

    #[test]
    fn parse_raw_pipeline_rejects_non_array_empty_and_non_objects() {
        assert!(AggregationStage::parse_raw_pipeline("").is_err());
        assert!(AggregationStage::parse_raw_pipeline("[]").is_err());
        assert!(AggregationStage::parse_raw_pipeline(r#"{"$match": {}}"#).is_err());
        assert!(AggregationStage::parse_raw_pipeline(r#"[{"$match": {}}, 42]"#).is_err());
        assert!(AggregationStage::parse_raw_pipeline(r#"[{}]"#).is_err());
    }
}
