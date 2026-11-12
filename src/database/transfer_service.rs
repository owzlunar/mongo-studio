use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bson::{Bson, Document};
use serde_json::{Map, Value};
use tokio::sync::mpsc;

use crate::core::{AppError, AppResult};
use crate::database::connection::ConnectionClient;
use crate::models::DocumentRow;

const IMPORT_BATCH_SIZE: usize = 500;
const EXPORT_STREAM_BATCH_SIZE: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferFormat {
    Json,
    JsonLines,
    Csv,
}

impl TransferFormat {
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::JsonLines => "jsonl",
            Self::Csv => "csv",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::JsonLines => "JSON Lines (.jsonl)",
            Self::Csv => "CSV",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportStats {
    pub count: usize,
    pub path: PathBuf,
    pub elapsed_ms: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportStats {
    pub inserted_count: usize,
    pub path: PathBuf,
    pub elapsed_ms: f64,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TransferService;

impl TransferService {
    fn validate_export_target<'a>(
        db_name: &'a str,
        coll_name: &'a str,
    ) -> AppResult<(&'a str, &'a str)> {
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

    fn validate_import_target<'a>(
        db_name: &'a str,
        coll_name: &'a str,
    ) -> AppResult<(&'a str, &'a str)> {
        let (trimmed_db, trimmed_coll) = Self::validate_export_target(db_name, coll_name)?;
        if trimmed_coll.starts_with("system.") {
            return Err(AppError::Connection(format!(
                "cannot import into protected system collection '{trimmed_coll}'"
            )));
        }
        Ok((trimmed_db, trimmed_coll))
    }

    fn ensure_parent_dir(path: &Path) -> AppResult<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|err| {
                    AppError::Persistence(format!("failed to create export directory: {err}"))
                })?;
            }
        }
        Ok(())
    }

    /// Formats a single cell according to RFC 4180 CSV rules.
    #[must_use]
    pub fn format_csv_cell(val: &str) -> String {
        if val.contains([',', '"', '\n', '\r']) {
            let escaped = val.replace('"', "\"\"");
            format!("\"{escaped}\"")
        } else {
            val.to_string()
        }
    }

    /// Parses a single CSV line according to RFC 4180 rules.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidQueryJson`] if quotes are unclosed or malformed.
    pub fn parse_csv_line(line: &str) -> AppResult<Vec<String>> {
        let trimmed = line.trim_end_matches(&['\r', '\n'][..]);
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let records = Self::parse_csv_records(trimmed)?;
        Ok(records.into_iter().next().unwrap_or_default())
    }

    /// Parses RFC 4180 CSV text into rows of cell strings, supporting quoted commas, `""` escaped
    /// quotes, and quoted newlines.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidQueryJson`] if a quoted field is malformed or unclosed.
    pub fn parse_csv_records(input: &str) -> AppResult<Vec<Vec<String>>> {
        let clean = input.strip_prefix('\u{feff}').unwrap_or(input);
        let mut records: Vec<Vec<String>> = Vec::new();
        let mut row: Vec<String> = Vec::new();
        let mut field = String::new();
        let mut in_quotes = false;
        let mut at_field_start = true;
        let mut just_closed_quote = false;

        let mut chars = clean.chars().peekable();
        while let Some(ch) = chars.next() {
            if in_quotes {
                if ch == '"' {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        field.push('"');
                    } else {
                        in_quotes = false;
                        just_closed_quote = true;
                    }
                } else {
                    field.push(ch);
                }
                continue;
            }

            if at_field_start && ch == '"' {
                in_quotes = true;
                at_field_start = false;
                continue;
            }

            match ch {
                ',' => {
                    row.push(std::mem::take(&mut field));
                    at_field_start = true;
                    just_closed_quote = false;
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    row.push(std::mem::take(&mut field));
                    if !(row.len() == 1 && row[0].is_empty()) {
                        records.push(std::mem::take(&mut row));
                    } else {
                        row.clear();
                    }
                    at_field_start = true;
                    just_closed_quote = false;
                }
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    if !(row.len() == 1 && row[0].is_empty()) {
                        records.push(std::mem::take(&mut row));
                    } else {
                        row.clear();
                    }
                    at_field_start = true;
                    just_closed_quote = false;
                }
                other => {
                    if just_closed_quote {
                        return Err(AppError::InvalidQueryJson(
                            "unexpected character after closing quote in CSV".to_string(),
                        ));
                    }
                    field.push(other);
                    at_field_start = false;
                }
            }
        }

        if in_quotes {
            return Err(AppError::InvalidQueryJson(
                "unclosed quote in CSV input".to_string(),
            ));
        }

        if !row.is_empty() || !field.is_empty() {
            row.push(field);
            if !(row.len() == 1 && row[0].is_empty()) {
                records.push(row);
            }
        }

        Ok(records)
    }

    fn csv_cell_for_doc(doc: &DocumentRow, col: &str) -> String {
        if doc.cached_json.is_some() || doc.raw_json.is_some() {
            if let Value::Object(map) = doc.cached_or_parsed_json().as_ref() {
                let Some(val) = map.get(col) else {
                    return String::new();
                };
                return match val {
                    Value::Array(_) => serde_json::to_string(val)
                        .unwrap_or_else(|_| DocumentRow::format_json_value_compact(val)),
                    Value::Object(inner) => {
                        let is_extjson_wrapper = inner.len() == 1
                            && inner.keys().next().is_some_and(|k| k.starts_with('$'));
                        if is_extjson_wrapper {
                            DocumentRow::format_json_value_compact(val)
                        } else {
                            serde_json::to_string(val)
                                .unwrap_or_else(|_| DocumentRow::format_json_value_compact(val))
                        }
                    }
                    _ => DocumentRow::format_json_value_compact(val),
                };
            }
        }
        doc.field_str(col).unwrap_or("").to_string()
    }

    /// Exports in-memory [`DocumentRow`] items to `path` in the requested [`TransferFormat`].
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Persistence`] if file creation, serialization, or writing fails.
    pub fn export_documents_to_path(
        docs: &[DocumentRow],
        format: TransferFormat,
        path: &Path,
    ) -> AppResult<usize> {
        Self::ensure_parent_dir(path)?;
        let file = File::create(path)
            .map_err(|err| AppError::Persistence(format!("failed to create export file: {err}")))?;
        let mut writer = BufWriter::new(file);

        match format {
            TransferFormat::Json => {
                writer.write_all(b"[").map_err(|err| {
                    AppError::Persistence(format!("failed to write JSON header: {err}"))
                })?;
                for (idx, doc) in docs.iter().enumerate() {
                    if idx > 0 {
                        writer.write_all(b",\n").map_err(|err| {
                            AppError::Persistence(format!("failed to write JSON separator: {err}"))
                        })?;
                    } else {
                        writer.write_all(b"\n").map_err(|err| {
                            AppError::Persistence(format!("failed to write JSON newline: {err}"))
                        })?;
                    }
                    serde_json::to_writer_pretty(&mut writer, &doc.to_json_value()).map_err(
                        |err| AppError::Persistence(format!("failed to serialize JSON doc: {err}")),
                    )?;
                }
                if !docs.is_empty() {
                    writer.write_all(b"\n").map_err(|err| {
                        AppError::Persistence(format!("failed to write JSON newline: {err}"))
                    })?;
                }
                writer.write_all(b"]\n").map_err(|err| {
                    AppError::Persistence(format!("failed to write JSON footer: {err}"))
                })?;
            }
            TransferFormat::JsonLines => {
                for doc in docs {
                    serde_json::to_writer(&mut writer, &doc.to_json_value()).map_err(|err| {
                        AppError::Persistence(format!("failed to serialize JSONL doc: {err}"))
                    })?;
                    writer.write_all(b"\n").map_err(|err| {
                        AppError::Persistence(format!("failed to write JSONL newline: {err}"))
                    })?;
                }
            }
            TransferFormat::Csv => {
                let columns = DocumentRow::extract_columns(docs);
                if !columns.is_empty() {
                    let header_line: String = columns
                        .iter()
                        .map(|c| Self::format_csv_cell(c))
                        .collect::<Vec<_>>()
                        .join(",");
                    writeln!(writer, "{header_line}").map_err(|err| {
                        AppError::Persistence(format!("failed to write CSV header: {err}"))
                    })?;
                }
                for doc in docs {
                    let row_line: String = columns
                        .iter()
                        .map(|col| Self::format_csv_cell(&Self::csv_cell_for_doc(doc, col)))
                        .collect::<Vec<_>>()
                        .join(",");
                    writeln!(writer, "{row_line}").map_err(|err| {
                        AppError::Persistence(format!("failed to write CSV row: {err}"))
                    })?;
                }
            }
        }

        writer
            .flush()
            .map_err(|err| AppError::Persistence(format!("failed to flush export file: {err}")))?;

        Ok(docs.len())
    }

    fn write_streamed_json_batch(
        writer: &mut BufWriter<File>,
        format: TransferFormat,
        batch: &[Value],
        written_count: &mut usize,
    ) -> AppResult<()> {
        for val in batch {
            match format {
                TransferFormat::Json => {
                    if *written_count > 0 {
                        writer.write_all(b",\n").map_err(|err| {
                            AppError::Persistence(format!("failed to write JSON separator: {err}"))
                        })?;
                    } else {
                        writer.write_all(b"\n").map_err(|err| {
                            AppError::Persistence(format!("failed to write JSON newline: {err}"))
                        })?;
                    }
                    serde_json::to_writer_pretty(&mut *writer, val).map_err(|err| {
                        AppError::Persistence(format!("failed to write JSON document: {err}"))
                    })?;
                }
                TransferFormat::JsonLines => {
                    serde_json::to_writer(&mut *writer, val).map_err(|err| {
                        AppError::Persistence(format!("failed to write JSONL document: {err}"))
                    })?;
                    writer.write_all(b"\n").map_err(|err| {
                        AppError::Persistence(format!("failed to write JSONL newline: {err}"))
                    })?;
                }
                TransferFormat::Csv => {}
            }
            *written_count += 1;
        }
        Ok(())
    }

    /// Exports documents matching `filter_json` from a live MongoDB collection to `path` using
    /// buffered streaming file I/O inside `tokio::task::spawn_blocking`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidQueryJson`] if `filter_json` is invalid, [`AppError::Connection`]
    /// on MongoDB failures, or [`AppError::Persistence`] on file I/O errors.
    pub async fn export_collection(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        filter_json: &str,
        format: TransferFormat,
        path: PathBuf,
        timeout: Duration,
    ) -> AppResult<ExportStats> {
        let (trimmed_db, trimmed_coll) = Self::validate_export_target(db_name, coll_name)?;
        let filter_doc = DocumentRow::parse_filter_doc(filter_json)?;

        let client = ConnectionClient::connect_client(uri, timeout).await?;
        let coll = client
            .database(trimmed_db)
            .collection::<Document>(trimmed_coll);

        let start = Instant::now();
        let mut cursor = tokio::time::timeout(timeout, coll.find(filter_doc, None))
            .await
            .map_err(|_| AppError::Connection("export query timed out".to_string()))?
            .map_err(|err| AppError::Connection(format!("export query failed: {err}")))?;

        let count = match format {
            TransferFormat::Csv => {
                let mut rows = Vec::new();
                while tokio::time::timeout(timeout, cursor.advance())
                    .await
                    .map_err(|_| AppError::Connection("export cursor timed out".to_string()))?
                    .map_err(|err| AppError::Connection(format!("export cursor error: {err}")))?
                {
                    let doc = cursor.deserialize_current().map_err(|err| {
                        AppError::Connection(format!("failed to decode export document: {err}"))
                    })?;
                    rows.push(DocumentRow::from_bson_doc(&doc));
                }

                let path_clone = path.clone();
                tokio::task::spawn_blocking(move || {
                    Self::export_documents_to_path(&rows, TransferFormat::Csv, &path_clone)
                })
                .await
                .map_err(|err| AppError::Persistence(format!("CSV export task failed: {err}")))??
            }
            TransferFormat::Json | TransferFormat::JsonLines => {
                let (tx, mut rx) = mpsc::channel::<Vec<Value>>(4);
                let path_clone = path.clone();
                let writer_handle = tokio::task::spawn_blocking(move || -> AppResult<usize> {
                    Self::ensure_parent_dir(&path_clone)?;
                    let file = File::create(&path_clone).map_err(|err| {
                        AppError::Persistence(format!("failed to create export file: {err}"))
                    })?;
                    let mut writer = BufWriter::new(file);
                    if format == TransferFormat::Json {
                        writer.write_all(b"[").map_err(|err| {
                            AppError::Persistence(format!("failed to write JSON header: {err}"))
                        })?;
                    }

                    let mut written = 0_usize;
                    while let Some(batch) = rx.blocking_recv() {
                        Self::write_streamed_json_batch(&mut writer, format, &batch, &mut written)?;
                    }

                    if format == TransferFormat::Json {
                        if written > 0 {
                            writer.write_all(b"\n").map_err(|err| {
                                AppError::Persistence(format!(
                                    "failed to write JSON newline: {err}"
                                ))
                            })?;
                        }
                        writer.write_all(b"]\n").map_err(|err| {
                            AppError::Persistence(format!("failed to write JSON footer: {err}"))
                        })?;
                    }
                    writer.flush().map_err(|err| {
                        AppError::Persistence(format!("failed to flush export file: {err}"))
                    })?;
                    Ok(written)
                });

                let mut batch = Vec::with_capacity(EXPORT_STREAM_BATCH_SIZE);
                while tokio::time::timeout(timeout, cursor.advance())
                    .await
                    .map_err(|_| AppError::Connection("export cursor timed out".to_string()))?
                    .map_err(|err| AppError::Connection(format!("export cursor error: {err}")))?
                {
                    let doc = cursor.deserialize_current().map_err(|err| {
                        AppError::Connection(format!("failed to decode export document: {err}"))
                    })?;
                    batch.push(Bson::Document(doc).into_relaxed_extjson());
                    if batch.len() >= EXPORT_STREAM_BATCH_SIZE {
                        let send_batch = std::mem::replace(
                            &mut batch,
                            Vec::with_capacity(EXPORT_STREAM_BATCH_SIZE),
                        );
                        if tx.send(send_batch).await.is_err() {
                            break;
                        }
                    }
                }
                if !batch.is_empty() {
                    if let Err(err) = tx.send(batch).await {
                        tracing::debug!(error = %err, "export writer receiver closed early");
                    }
                }
                drop(tx);

                writer_handle
                    .await
                    .map_err(|err| AppError::Persistence(format!("export task failed: {err}")))??
            }
        };

        Ok(ExportStats {
            count,
            path,
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        })
    }

    fn read_import_documents(path: &Path, format: TransferFormat) -> AppResult<Vec<Document>> {
        let file = File::open(path).map_err(|err| {
            AppError::Persistence(format!(
                "failed to open import file '{}': {err}",
                path.display()
            ))
        })?;
        let mut reader = BufReader::new(file);

        match format {
            TransferFormat::Json => {
                let mut raw = String::new();
                reader.read_to_string(&mut raw).map_err(|err| {
                    AppError::Persistence(format!("failed to read JSON import file: {err}"))
                })?;
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    return Err(AppError::InvalidQueryJson(
                        "JSON import file is empty".to_string(),
                    ));
                }
                let val: Value = serde_json::from_str(trimmed).map_err(|err| {
                    AppError::InvalidQueryJson(format!("invalid JSON import file: {err}"))
                })?;
                match val {
                    Value::Array(items) => {
                        let mut docs = Vec::with_capacity(items.len());
                        for (idx, item) in items.into_iter().enumerate() {
                            if !item.is_object() {
                                return Err(AppError::InvalidQueryJson(format!(
                                    "JSON array element at index {idx} must be an object"
                                )));
                            }
                            let doc: Document = serde_json::from_value(item).map_err(|err| {
                                AppError::InvalidQueryJson(format!(
                                    "invalid BSON document at index {idx}: {err}"
                                ))
                            })?;
                            docs.push(doc);
                        }
                        Ok(docs)
                    }
                    Value::Object(map) => {
                        let doc: Document =
                            serde_json::from_value(Value::Object(map)).map_err(|err| {
                                AppError::InvalidQueryJson(format!(
                                    "invalid BSON document in JSON file: {err}"
                                ))
                            })?;
                        Ok(vec![doc])
                    }
                    _ => Err(AppError::InvalidQueryJson(
                        "JSON import file must contain an object or array of objects".to_string(),
                    )),
                }
            }
            TransferFormat::JsonLines => {
                let mut docs = Vec::new();
                let mut line = String::new();
                let mut line_num = 0_usize;
                loop {
                    line.clear();
                    let bytes = reader.read_line(&mut line).map_err(|err| {
                        AppError::Persistence(format!("failed to read JSONL line: {err}"))
                    })?;
                    if bytes == 0 {
                        break;
                    }
                    line_num += 1;
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let doc = DocumentRow::parse_json_to_bson_doc(trimmed).map_err(|err| {
                        AppError::InvalidQueryJson(format!("JSONL line {line_num}: {err}"))
                    })?;
                    docs.push(doc);
                }
                Ok(docs)
            }
            TransferFormat::Csv => {
                let mut raw = String::new();
                reader.read_to_string(&mut raw).map_err(|err| {
                    AppError::Persistence(format!("failed to read CSV import file: {err}"))
                })?;
                let records = Self::parse_csv_records(&raw)?;
                let Some(headers) = records.first() else {
                    return Err(AppError::InvalidQueryJson(
                        "CSV import file has no header row".to_string(),
                    ));
                };
                if headers.is_empty() || headers.iter().all(|h| h.trim().is_empty()) {
                    return Err(AppError::InvalidQueryJson(
                        "CSV header row must not be empty".to_string(),
                    ));
                }

                let mut docs = Vec::with_capacity(records.len().saturating_sub(1));
                for row in records.iter().skip(1) {
                    let mut map = Map::new();
                    for (idx, header) in headers.iter().enumerate() {
                        let key = header.trim();
                        if key.is_empty() {
                            continue;
                        }
                        let cell = row.get(idx).map(String::as_str).unwrap_or("");
                        if key == "_id" && cell.trim().is_empty() {
                            continue;
                        }
                        map.insert(key.to_string(), Value::String(cell.to_string()));
                    }
                    let doc: Document =
                        serde_json::from_value(Value::Object(map)).map_err(|err| {
                            AppError::InvalidQueryJson(format!(
                                "failed to convert CSV row to BSON: {err}"
                            ))
                        })?;
                    docs.push(doc);
                }
                Ok(docs)
            }
        }
    }

    /// Imports documents from `path` in `format` into a live MongoDB collection using batched
    /// `insert_many` (chunks of 500) and `tokio::task::spawn_blocking` file reading.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if target validation fails (including `system.*`
    /// collections) or MongoDB fails, [`AppError::InvalidQueryJson`] on malformed input, or
    /// [`AppError::Persistence`] on file I/O errors.
    pub async fn import_collection(
        uri: &str,
        db_name: &str,
        coll_name: &str,
        format: TransferFormat,
        path: PathBuf,
        timeout: Duration,
    ) -> AppResult<ImportStats> {
        let (trimmed_db, trimmed_coll) = Self::validate_import_target(db_name, coll_name)?;

        let start = Instant::now();
        let path_clone = path.clone();
        let docs =
            tokio::task::spawn_blocking(move || Self::read_import_documents(&path_clone, format))
                .await
                .map_err(|err| {
                    AppError::Persistence(format!("import read task failed: {err}"))
                })??;

        if docs.is_empty() {
            return Ok(ImportStats {
                inserted_count: 0,
                path,
                elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
            });
        }

        let client = ConnectionClient::connect_client(uri, timeout).await?;
        let coll = client
            .database(trimmed_db)
            .collection::<Document>(trimmed_coll);

        let mut inserted_count = 0_usize;
        for chunk in docs.chunks(IMPORT_BATCH_SIZE) {
            let res = tokio::time::timeout(timeout, coll.insert_many(chunk.to_vec(), None))
                .await
                .map_err(|_| AppError::Connection("import insertMany timed out".to_string()))?
                .map_err(|err| AppError::Connection(format!("import insertMany failed: {err}")))?;
            inserted_count += res.inserted_ids.len();
        }

        Ok(ImportStats {
            inserted_count,
            path,
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;
    use uuid::Uuid;

    #[test]
    fn csv_escaping_and_parsing_handles_quotes_commas_and_newlines() {
        assert_eq!(TransferService::format_csv_cell("plain"), "plain");
        assert_eq!(TransferService::format_csv_cell("a,b"), "\"a,b\"");
        assert_eq!(
            TransferService::format_csv_cell("say \"hi\""),
            "\"say \"\"hi\"\"\""
        );

        let line = r#"1,"Alice, Jr.","says ""hello""",active"#;
        let parsed = TransferService::parse_csv_line(line).unwrap();
        assert_eq!(parsed, vec!["1", "Alice, Jr.", "says \"hello\"", "active"]);

        let multi = "id,notes\n1,\"line 1\nline 2\"\n2,ok\n";
        let records = TransferService::parse_csv_records(multi).unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[1], vec!["1", "line 1\nline 2"]);
        assert!(TransferService::parse_csv_records("1,\"unclosed").is_err());
    }

    #[test]
    fn json_and_jsonl_roundtrip_without_mongodb_server() {
        let dir = std::env::temp_dir().join(format!("mongo_studio_xfer_{}", Uuid::new_v4()));
        let json_path = dir.join("export.json");
        let jsonl_path = dir.join("export.jsonl");
        let csv_path = dir.join("export.csv");

        let rows = vec![
            DocumentRow::from_bson_doc(&doc! { "_id": 1_i32, "name": "Alice", "note": "a,b" }),
            DocumentRow::from_bson_doc(&doc! { "_id": 2_i32, "name": "Bob", "note": "ok" }),
        ];

        assert_eq!(
            TransferService::export_documents_to_path(&rows, TransferFormat::Json, &json_path)
                .unwrap(),
            2
        );
        assert_eq!(
            TransferService::export_documents_to_path(
                &rows,
                TransferFormat::JsonLines,
                &jsonl_path
            )
            .unwrap(),
            2
        );
        assert_eq!(
            TransferService::export_documents_to_path(&rows, TransferFormat::Csv, &csv_path)
                .unwrap(),
            2
        );

        let json_docs =
            TransferService::read_import_documents(&json_path, TransferFormat::Json).unwrap();
        let jsonl_docs =
            TransferService::read_import_documents(&jsonl_path, TransferFormat::JsonLines).unwrap();
        let csv_docs =
            TransferService::read_import_documents(&csv_path, TransferFormat::Csv).unwrap();

        assert_eq!(json_docs.len(), 2);
        assert_eq!(jsonl_docs.len(), 2);
        assert_eq!(csv_docs.len(), 2);
        assert_eq!(csv_docs[0].get_str("note").unwrap(), "a,b");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_collection_rejects_system_collections_and_empty_targets() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt
            .block_on(TransferService::import_collection(
                "mongodb://127.0.0.1:1",
                "admin",
                "system.users",
                TransferFormat::Json,
                PathBuf::from("nonexistent.json"),
                Duration::from_millis(100),
            ))
            .unwrap_err();
        assert!(err.to_string().contains("system."));

        assert!(TransferService::validate_export_target("", "users").is_err());
        assert!(TransferService::validate_export_target("admin", "").is_err());
    }
}
