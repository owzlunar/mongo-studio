use crate::models::CollectionInfo;

pub struct CollectionService;

impl CollectionService {
    pub fn get_collection_info(database: &str, collection: &str) -> CollectionInfo {
        CollectionInfo::mock(database, collection)
    }
}
