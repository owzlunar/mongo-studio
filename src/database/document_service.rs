use crate::models::DocumentRow;
use crate::query::filter::QueryFilter;

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
}
