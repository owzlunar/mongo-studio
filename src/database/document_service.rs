use crate::models::DocumentRow;
use crate::query::filter::QueryFilter;

pub struct DocumentService;

impl DocumentService {
    pub fn filter_documents(
        documents: &[DocumentRow],
        filter_json: &str,
        limit: usize,
    ) -> Vec<DocumentRow> {
        let filter = QueryFilter::parse(filter_json);
        documents
            .iter()
            .filter(|doc| filter.matches(doc))
            .take(limit.max(1))
            .cloned()
            .collect()
    }
}
