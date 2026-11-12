use std::time::{Duration, Instant};

use bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use mongodb::Client;

use crate::core::{AppError, AppResult};
use crate::database::connection::ConnectionClient;
use crate::models::DocumentRow;
use crate::query::filter::QueryFilter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentQueryParams {
    pub filter: String,
    pub projection: String,
    pub sort: String,
    pub skip: usize,
    pub limit: usize,
}

impl Default for DocumentQueryParams {
    fn default() -> Self {
        Self {
            filter: "{}".to_string(),
            projection: String::new(),
            sort: String::new(),
            skip: 0,
            limit: 50,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentPageResult {
    pub documents: Vec<DocumentRow>,
    pub total_count: usize,
    pub skip: usize,
    pub limit: usize,
    pub query_ms: f64,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DocumentService;

impl DocumentService {
    #[must_use]
    pub fn filter_documents(
        documents: &[DocumentRow],
        filter_json: &str,
        limit: usize,
    ) -> Vec<DocumentRow> {
        let filter = QueryFilter::parse(filter_json);
        let cap = limit.max(1).min(documents.len());
        let mut results = Vec::with_capacity(cap);
        results.extend(
            documents
                .iter()
                .filter(|doc| filter.matches(doc))
                .take(limit.max(1))
                .cloned(),
        );
        results
    }

    async fn connect_client(uri: &str, timeout: Duration) -> AppResult<Client> {
        ConnectionClient::connect_client(uri, timeout).await
    }

    fn build_single_thread_runtime() -> AppResult<tokio::runtime::Runtime> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| AppError::Connection(format!("failed to build runtime: {err}")))
    }

    fn validate_target<'a>(db_name: &'a str, coll_name: &'a str) -> AppResult<(&'a str, &'a str)> {
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
        Ok((trimmed_db, trimmed_coll))
    }

    fn prepend_id(id: Bson, doc: Document) -> Document {
        let mut ordered = Document::new();
        ordered.insert("_id", id);
        for (k, v) in doc {
            ordered.insert(k, v);
        }
        ordered
    }

    /// Queries a live MongoDB collection with filter, projection, sort, skip, and limit (clamped
    /// to `1..=500`), running `count_documents` and `find` concurrently.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidQueryJson`] if filter/projection/sort JSON is invalid, or
    /// [`AppError::Connection`] if connection or query execution fails.
    pub async fn query_documents(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        params: &DocumentQueryParams,
        timeout: Duration,
    ) -> AppResult<DocumentPageResult> {
        let (trimmed_db, trimmed_coll) = Self::validate_target(db_name, coll_name)?;
        let filter_doc = DocumentRow::parse_filter_doc(&params.filter)?;
        let projection_doc = DocumentRow::parse_bson_doc_allow_empty(&params.projection)?;
        let sort_doc = DocumentRow::parse_bson_doc_allow_empty(&params.sort)?;

        let effective_limit = params.limit.clamp(1, 500);
        let find_opts = FindOptions::builder()
            .skip(u64::try_from(params.skip).ok())
            .limit(i64::try_from(effective_limit).ok())
            .projection(projection_doc)
            .sort(sort_doc)
            .build();

        let client = Self::connect_client(uri, timeout).await?;
        let coll = client
            .database(trimmed_db)
            .collection::<Document>(trimmed_coll);

        let start = Instant::now();
        let count_filter = filter_doc.clone();
        let count_coll = coll.clone();

        let count_fut = async move {
            tokio::time::timeout(timeout, count_coll.count_documents(count_filter, None))
                .await
                .map_err(|_| AppError::Connection("countDocuments timed out".to_string()))?
                .map_err(|err| AppError::Connection(format!("countDocuments failed: {err}")))
        };

        let docs_fut = async {
            let mut cursor = tokio::time::timeout(timeout, coll.find(filter_doc, find_opts))
                .await
                .map_err(|_| AppError::Connection("find query timed out".to_string()))?
                .map_err(|err| AppError::Connection(format!("find query failed: {err}")))?;

            let mut docs = Vec::with_capacity(effective_limit);
            while tokio::time::timeout(timeout, cursor.advance())
                .await
                .map_err(|_| AppError::Connection("cursor advance timed out".to_string()))?
                .map_err(|err| AppError::Connection(format!("cursor error: {err}")))?
            {
                let doc = cursor.deserialize_current().map_err(|err| {
                    AppError::Connection(format!("failed to decode document: {err}"))
                })?;
                docs.push(DocumentRow::from_bson_doc(&doc));
            }
            Ok::<Vec<DocumentRow>, AppError>(docs)
        };

        let (raw_count, documents) = tokio::try_join!(count_fut, docs_fut)?;
        let total_count = usize::try_from(raw_count).unwrap_or(usize::MAX);
        let query_ms = start.elapsed().as_secs_f64() * 1000.0;

        Ok(DocumentPageResult {
            documents,
            total_count,
            skip: params.skip,
            limit: effective_limit,
            query_ms,
        })
    }

    /// Synchronously executes a paginated document query against a live MongoDB server.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if query parsing, connection, or execution fails.
    pub fn query_documents_blocking(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        params: &DocumentQueryParams,
        timeout: Duration,
    ) -> AppResult<DocumentPageResult> {
        let runtime = Self::build_single_thread_runtime()?;
        runtime.block_on(Self::query_documents(
            uri,
            db_name,
            coll_name,
            params,
            timeout,
        ))
    }

    /// Inserts a JSON/Extended JSON document into `db_name.coll_name` and returns the resulting
    /// [`DocumentRow`] with the server/driver `_id` populated.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidQueryJson`] if `doc_json` is invalid, or [`AppError::Connection`]
    /// if insertion fails.
    pub async fn insert_document(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        doc_json: &str,
        timeout: Duration,
    ) -> AppResult<DocumentRow> {
        let (trimmed_db, trimmed_coll) = Self::validate_target(db_name, coll_name)?;
        let mut doc = DocumentRow::parse_json_to_bson_doc(doc_json)?;

        let client = Self::connect_client(uri, timeout).await?;
        let coll = client
            .database(trimmed_db)
            .collection::<Document>(trimmed_coll);

        let res = tokio::time::timeout(timeout, coll.insert_one(doc.clone(), None))
            .await
            .map_err(|_| AppError::Connection("insertOne timed out".to_string()))?
            .map_err(|err| AppError::Connection(format!("insertOne failed: {err}")))?;

        if !doc.contains_key("_id") {
            doc = Self::prepend_id(res.inserted_id, doc);
        }

        Ok(DocumentRow::from_bson_doc(&doc))
    }

    /// Synchronously inserts a JSON/Extended JSON document into a live MongoDB collection.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if parsing, connection, or insertion fails.
    pub fn insert_document_blocking(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        doc_json: &str,
        timeout: Duration,
    ) -> AppResult<DocumentRow> {
        let runtime = Self::build_single_thread_runtime()?;
        runtime.block_on(Self::insert_document(
            uri,
            db_name,
            coll_name,
            doc_json,
            timeout,
        ))
    }

    /// Replaces an existing document identified by `original_id` with `updated_json`.
    ///
    /// Rejects attempts to change `_id` to a different value and injects `original_id` when `_id`
    /// is omitted from `updated_json`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidQueryJson`] on invalid JSON or `_id` modification, or
    /// [`AppError::Connection`] if the document is not found or the command fails.
    pub async fn update_document(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        original_id: &Bson,
        updated_json: &str,
        timeout: Duration,
    ) -> AppResult<DocumentRow> {
        let (trimmed_db, trimmed_coll) = Self::validate_target(db_name, coll_name)?;
        let mut replacement = DocumentRow::parse_json_to_bson_doc(updated_json)?;

        if let Some(provided_id) = replacement.get("_id") {
            if provided_id != original_id {
                return Err(AppError::InvalidQueryJson(
                    "cannot modify immutable '_id' field when updating a document".to_string(),
                ));
            }
        } else {
            replacement = Self::prepend_id(original_id.clone(), replacement);
        }

        let client = Self::connect_client(uri, timeout).await?;
        let coll = client
            .database(trimmed_db)
            .collection::<Document>(trimmed_coll);

        let res = tokio::time::timeout(
            timeout,
            coll.replace_one(
                doc! { "_id": original_id.clone() },
                replacement.clone(),
                None,
            ),
        )
        .await
        .map_err(|_| AppError::Connection("replaceOne timed out".to_string()))?
        .map_err(|err| AppError::Connection(format!("replaceOne failed: {err}")))?;

        if res.matched_count == 0 {
            return Err(AppError::Connection(
                "document not found (matched_count = 0)".to_string(),
            ));
        }

        Ok(DocumentRow::from_bson_doc(&replacement))
    }

    /// Synchronously replaces an existing document on a live MongoDB server.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if parsing, connection, or replacement fails.
    pub fn update_document_blocking(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        original_id: &Bson,
        updated_json: &str,
        timeout: Duration,
    ) -> AppResult<DocumentRow> {
        let runtime = Self::build_single_thread_runtime()?;
        runtime.block_on(Self::update_document(
            uri,
            db_name,
            coll_name,
            original_id,
            updated_json,
            timeout,
        ))
    }

    /// Deletes a single document identified by `doc_id` from `db_name.coll_name`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if validation, connection, or deletion fails, or if no
    /// matching document was deleted (`deleted_count == 0`).
    pub async fn delete_document(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        doc_id: &Bson,
        timeout: Duration,
    ) -> AppResult<()> {
        let (trimmed_db, trimmed_coll) = Self::validate_target(db_name, coll_name)?;
        let client = Self::connect_client(uri, timeout).await?;
        let coll = client
            .database(trimmed_db)
            .collection::<Document>(trimmed_coll);

        let res = tokio::time::timeout(timeout, coll.delete_one(doc! { "_id": doc_id.clone() }, None))
            .await
            .map_err(|_| AppError::Connection("deleteOne timed out".to_string()))?
            .map_err(|err| AppError::Connection(format!("deleteOne failed: {err}")))?;

        if res.deleted_count == 0 {
            return Err(AppError::Connection(
                "document not found (deleted_count = 0)".to_string(),
            ));
        }

        Ok(())
    }

    /// Synchronously deletes a single document from a live MongoDB collection.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if validation, connection, or deletion fails.
    pub fn delete_document_blocking(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        doc_id: &Bson,
        timeout: Duration,
    ) -> AppResult<()> {
        let runtime = Self::build_single_thread_runtime()?;
        runtime.block_on(Self::delete_document(
            uri,
            db_name,
            coll_name,
            doc_id,
            timeout,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_target_rejects_empty_names() {
        assert!(DocumentService::validate_target("", "users").is_err());
        assert!(DocumentService::validate_target("db", "  ").is_err());
        assert_eq!(
            DocumentService::validate_target(" app ", " users ").unwrap(),
            ("app", "users")
        );
    }

    #[test]
    fn prepend_id_places_id_first() {
        let d = doc! { "name": "Alice", "age": 30 };
        let ordered = DocumentService::prepend_id(Bson::Int32(1), d);
        let keys: Vec<&str> = ordered.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["_id", "name", "age"]);
    }
}
