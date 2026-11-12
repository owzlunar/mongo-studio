use crate::models::CollectionInfo;

#[derive(Debug, Default, Clone, Copy)]
pub struct CollectionService;

impl CollectionService {
    #[must_use]
    pub fn get_collection_info(database: &str, collection: &str) -> CollectionInfo {
        CollectionInfo::mock(database, collection)
    }
}
