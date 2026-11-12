use crate::models::DatabaseInfo;

#[derive(Debug, Default, Clone, Copy)]
pub struct DatabaseService;

impl DatabaseService {
    #[must_use]
    pub fn list_mock_databases() -> Vec<DatabaseInfo> {
        DatabaseInfo::mock_databases()
    }
}
