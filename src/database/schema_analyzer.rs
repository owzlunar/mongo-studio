use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use bson::{doc, Bson, Document};
use mongodb::options::FindOptions;

use crate::core::{AppError, AppResult};
use crate::database::connection::ConnectionClient;
use crate::models::{DocumentRow, SchemaField};

pub const MAX_SCHEMA_DEPTH: usize = 16;
pub const DEFAULT_SAMPLE_SIZE: usize = 100;
const MAX_ARRAY_ELEMENTS_PER_FIELD: usize = 64;

#[derive(Default)]
struct FieldAccumulator {
    doc_count: usize,
    type_counts: BTreeMap<&'static str, usize>,
    sample_value: Option<String>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SchemaAnalyzer;

impl SchemaAnalyzer {
    /// Analyzes a slice of BSON documents recursively (up to [`MAX_SCHEMA_DEPTH`]), producing
    /// flattened dot-notation and array-notation (`items[]`, `items[].sku`) [`SchemaField`]
    /// statistics with per-document frequency and type breakdown.
    #[must_use]
    pub fn analyze_bson_docs(docs: &[Document]) -> Vec<SchemaField> {
        if docs.is_empty() {
            return Vec::new();
        }

        let total_docs = docs.len() as f32;
        let mut accumulators: BTreeMap<String, FieldAccumulator> = BTreeMap::new();

        for doc in docs {
            let mut seen_in_doc = HashSet::new();
            Self::collect_doc_paths(doc, "", 0, &mut seen_in_doc, &mut accumulators);
        }

        let mut fields: Vec<SchemaField> = accumulators
            .into_iter()
            .map(|(field_name, acc)| {
                let total_type_obs: usize = acc.type_counts.values().sum();
                let denom = total_type_obs.max(1) as f32;

                let mut type_breakdown: Vec<(String, f32)> = acc
                    .type_counts
                    .into_iter()
                    .map(|(ty, cnt)| (ty.to_string(), ((cnt as f32 / denom) * 100.0).min(100.0)))
                    .collect();

                type_breakdown.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

                let bson_type = if type_breakdown.len() > 3 {
                    "Mixed".to_string()
                } else {
                    type_breakdown
                        .iter()
                        .map(|(ty, _)| ty.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ")
                };

                SchemaField {
                    field_name,
                    bson_type,
                    frequency_pct: ((acc.doc_count as f32 / total_docs) * 100.0).min(100.0),
                    sample_value: acc.sample_value.unwrap_or_default(),
                    type_breakdown,
                    doc_count: acc.doc_count,
                }
            })
            .collect();

        if let Some(id_pos) = fields.iter().position(|f| f.field_name == "_id") {
            if id_pos > 0 {
                let id_field = fields.remove(id_pos);
                fields.insert(0, id_field);
            }
        }

        fields
    }

    fn collect_doc_paths(
        doc: &Document,
        prefix: &str,
        depth: usize,
        seen_in_doc: &mut HashSet<String>,
        accumulators: &mut BTreeMap<String, FieldAccumulator>,
    ) {
        if depth >= MAX_SCHEMA_DEPTH {
            return;
        }

        for (key, val) in doc {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };

            Self::record_value(&path, val, depth, seen_in_doc, accumulators);
        }
    }

    fn record_value(
        path: &str,
        val: &Bson,
        depth: usize,
        seen_in_doc: &mut HashSet<String>,
        accumulators: &mut BTreeMap<String, FieldAccumulator>,
    ) {
        let type_name = Self::bson_type_name(val);
        let acc = accumulators.entry(path.to_string()).or_default();
        if seen_in_doc.insert(path.to_string()) {
            acc.doc_count += 1;
        }
        *acc.type_counts.entry(type_name).or_insert(0) += 1;
        if acc.sample_value.is_none() {
            acc.sample_value = Some(DocumentRow::bson_to_display(val));
        }

        if depth + 1 >= MAX_SCHEMA_DEPTH {
            return;
        }

        match val {
            Bson::Document(sub) => {
                Self::collect_doc_paths(sub, path, depth + 1, seen_in_doc, accumulators);
            }
            Bson::Array(arr) => {
                let elem_path = format!("{path}[]");
                for elem in arr.iter().take(MAX_ARRAY_ELEMENTS_PER_FIELD) {
                    Self::record_value(&elem_path, elem, depth + 1, seen_in_doc, accumulators);
                }
            }
            _ => {}
        }
    }

    #[must_use]
    pub const fn bson_type_name(val: &Bson) -> &'static str {
        match val {
            Bson::ObjectId(_) => "ObjectId",
            Bson::String(_) => "String",
            Bson::Int32(_) => "Int32",
            Bson::Int64(_) => "Int64",
            Bson::Double(_) => "Double",
            Bson::Boolean(_) => "Boolean",
            Bson::DateTime(_) => "DateTime",
            Bson::Array(_) => "Array",
            Bson::Document(_) => "Document",
            Bson::Null => "Null",
            Bson::Binary(_) => "Binary",
            Bson::Decimal128(_) => "Decimal128",
            _ => "BSON",
        }
    }

    /// Samples up to `sample_size.clamp(10, 2000)` documents from `db_name.coll_name` using
    /// `$sample` (falling back to `find().limit(...)` if `$sample` is unsupported) and returns
    /// computed schema fields.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if names are empty, connection fails, or sampling fails.
    pub async fn analyze_collection(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        sample_size: usize,
        timeout: Duration,
    ) -> AppResult<Vec<SchemaField>> {
        let trimmed_db = db_name.trim();
        let trimmed_coll = coll_name.trim();
        if trimmed_db.is_empty() {
            return Err(AppError::Connection(
                "database name must not be empty".to_string(),
            ));
        }
        if trimmed_coll.is_empty() {
            return Err(AppError::Connection(
                "collection name must not be empty".to_string(),
            ));
        }

        let clamped_size = sample_size.clamp(10, 2000);
        let client = ConnectionClient::connect_client(uri, timeout).await?;
        let coll = client
            .database(trimmed_db)
            .collection::<Document>(trimmed_coll);

        let sample_i64 = i64::try_from(clamped_size).unwrap_or(2000);
        let sample_pipeline = vec![doc! { "$sample": { "size": sample_i64 } }];

        let mut docs = Vec::with_capacity(clamped_size.min(256));
        let sample_cursor_res =
            tokio::time::timeout(timeout, coll.aggregate(sample_pipeline, None)).await;

        let used_sample = match sample_cursor_res {
            Ok(Ok(mut cursor)) => {
                let mut ok = true;
                while docs.len() < clamped_size {
                    match tokio::time::timeout(timeout, cursor.advance()).await {
                        Ok(Ok(true)) => {
                            if let Ok(doc) = cursor.deserialize_current() {
                                docs.push(doc);
                            }
                        }
                        Ok(Ok(false)) => break,
                        Ok(Err(err)) => {
                            tracing::debug!(error = %err, "$sample cursor failed; falling back to find");
                            ok = false;
                            break;
                        }
                        Err(_) => {
                            return Err(AppError::Connection(
                                "schema $sample timed out".to_string(),
                            ));
                        }
                    }
                }
                ok
            }
            Ok(Err(err)) => {
                tracing::debug!(error = %err, "$sample unsupported; falling back to find().limit()");
                false
            }
            Err(_) => {
                return Err(AppError::Connection("schema $sample timed out".to_string()));
            }
        };

        if !used_sample {
            docs.clear();
            let find_opts = FindOptions::builder().limit(sample_i64).build();
            let mut cursor = tokio::time::timeout(timeout, coll.find(None, find_opts))
                .await
                .map_err(|_| AppError::Connection("schema find fallback timed out".to_string()))?
                .map_err(|err| AppError::Connection(format!("schema analysis failed: {err}")))?;

            while docs.len() < clamped_size
                && tokio::time::timeout(timeout, cursor.advance())
                    .await
                    .map_err(|_| AppError::Connection("schema cursor timed out".to_string()))?
                    .map_err(|err| AppError::Connection(format!("schema cursor error: {err}")))?
            {
                let doc = cursor.deserialize_current().map_err(|err| {
                    AppError::Connection(format!("failed to decode sampled document: {err}"))
                })?;
                docs.push(doc);
            }
        }

        Ok(Self::analyze_bson_docs(&docs))
    }

    /// Synchronously samples a live MongoDB collection and returns computed schema fields.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if runtime creation, validation, or analysis fails.
    pub fn analyze_collection_blocking(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        sample_size: usize,
        timeout: Duration,
    ) -> AppResult<Vec<SchemaField>> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| AppError::Connection(format!("failed to build runtime: {err}")))?;
        runtime.block_on(Self::analyze_collection(
            uri,
            db_name,
            coll_name,
            sample_size,
            timeout,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_bson_docs_extracts_nested_arrays_and_mixed_types() {
        let docs = vec![
            doc! {
                "_id": 1_i32,
                "name": "Alice",
                "address": { "city": "Bangkok", "zip": "10110" },
                "items": [
                    { "sku": "A1", "qty": 2_i32 },
                    { "sku": "A2", "qty": 1_i32 }
                ]
            },
            doc! {
                "_id": 2_i32,
                "name": Bson::Null,
                "address": { "city": "Tokyo" },
                "items": [
                    { "sku": "B1", "qty": 3_i32 }
                ]
            },
        ];

        let fields = SchemaAnalyzer::analyze_bson_docs(&docs);
        assert_eq!(fields[0].field_name, "_id");
        assert_eq!(fields[0].doc_count, 2);
        assert!((fields[0].frequency_pct - 100.0).abs() < 0.01);

        let name_field = fields.iter().find(|f| f.field_name == "name").unwrap();
        assert_eq!(name_field.doc_count, 2);
        assert_eq!(name_field.type_breakdown.len(), 2);
        assert!(name_field.bson_type.contains("String"));
        assert!(name_field.bson_type.contains("Null"));

        let city_field = fields
            .iter()
            .find(|f| f.field_name == "address.city")
            .unwrap();
        assert_eq!(city_field.doc_count, 2);
        assert_eq!(city_field.bson_type, "String");

        let zip_field = fields
            .iter()
            .find(|f| f.field_name == "address.zip")
            .unwrap();
        assert_eq!(zip_field.doc_count, 1);
        assert!((zip_field.frequency_pct - 50.0).abs() < 0.01);

        let sku_field = fields
            .iter()
            .find(|f| f.field_name == "items[].sku")
            .unwrap();
        assert_eq!(sku_field.doc_count, 2);
        assert!((sku_field.frequency_pct - 100.0).abs() < 0.01);
    }
}
