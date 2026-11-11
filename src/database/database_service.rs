use crate::models::DatabaseInfo;

pub struct DatabaseService;

impl DatabaseService {
    pub fn list_mock_databases() -> Vec<DatabaseInfo> {
        DatabaseInfo::mock_databases()
    }
}
