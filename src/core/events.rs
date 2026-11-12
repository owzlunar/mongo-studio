use bson::Bson;
use uuid::Uuid;

use crate::database::DocumentPageResult;
use crate::models::{CollectionInfo, DatabaseInfo, DocumentRow, MongoUser, UserRole};
use crate::theme::ThemeMode;
use crate::ui::activity_bar::ActivityView;

#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
    OpenCollection {
        connection_name: String,
        database: String,
        collection: String,
    },
    SelectActivity(ActivityView),
    ToggleSidebar,
    ToggleBottomPanel,
    ToggleCommandPalette,
    SwitchTheme(ThemeMode),
    OpenNewConnectionModal,
    OpenEditConnectionModal {
        index: usize,
    },
    KeyringPasswordLoaded {
        index: usize,
        open_edit_modal: bool,
        result: Result<Option<String>, String>,
    },
    AddConnection {
        name: String,
        uri: String,
    },
    UpdateConnection {
        index: usize,
        name: String,
        uri: String,
    },
    TestConnection {
        name: String,
        uri: String,
    },
    ConnectProfile {
        index: usize,
    },
    DisconnectProfile {
        index: usize,
    },
    RemoveConnection {
        index: usize,
    },
    ConnectionResult {
        conn_id: Uuid,
        name: String,
        uri: String,
        result: Result<(Vec<DatabaseInfo>, Vec<MongoUser>, bool), String>,
        latency_ms: Option<f64>,
    },
    TestConnectionResult {
        uri: String,
        result: Result<f64, String>,
    },
    CollectionLoaded {
        connection_name: String,
        database: String,
        collection: String,
        result: Result<CollectionInfo, String>,
    },
    RefreshCollection {
        connection_name: String,
        database: String,
        collection: String,
    },
    OpenUserTab {
        connection_name: String,
        username: String,
    },
    OpenCreateDatabaseModal {
        connection_index: usize,
    },
    CreateDatabase {
        connection_index: usize,
        db_name: String,
        collection_name: String,
    },
    DatabaseCreated {
        connection_name: String,
        db_name: String,
        collection_name: String,
        result: Result<CollectionInfo, String>,
    },
    OpenCreateCollectionModal {
        connection_index: usize,
        db_name: String,
    },
    CreateCollection {
        connection_index: usize,
        db_name: String,
        collection_name: String,
    },
    CollectionCreated {
        connection_name: String,
        db_name: String,
        result: Result<CollectionInfo, String>,
    },
    RequestDropCollection {
        connection_index: usize,
        db_name: String,
        collection_name: String,
    },
    ConfirmDropCollection {
        connection_index: usize,
        db_name: String,
        collection_name: String,
    },
    CollectionDropped {
        connection_name: String,
        db_name: String,
        collection_name: String,
        result: Result<(), String>,
    },
    RequestDropDatabase {
        connection_index: usize,
        db_name: String,
    },
    ConfirmDropDatabase {
        connection_index: usize,
        db_name: String,
    },
    DatabaseDropped {
        connection_name: String,
        db_name: String,
        result: Result<(), String>,
    },
    ExecuteLiveQuery {
        connection_name: String,
        database: String,
        collection: String,
        filter: String,
        projection: String,
        sort: String,
        skip: usize,
        limit: usize,
    },
    QueryPageLoaded {
        connection_name: String,
        database: String,
        collection: String,
        skip: usize,
        limit: usize,
        result: Result<DocumentPageResult, String>,
    },
    OpenInsertDocumentModal {
        connection_name: String,
        database: String,
        collection: String,
    },
    InsertDocument {
        connection_name: String,
        database: String,
        collection: String,
        json_text: String,
    },
    DocumentInserted {
        connection_name: String,
        database: String,
        collection: String,
        result: Result<DocumentRow, String>,
    },
    OpenEditDocumentModal {
        connection_name: String,
        database: String,
        collection: String,
        original_id: Bson,
        json_text: String,
    },
    UpdateDocument {
        connection_name: String,
        database: String,
        collection: String,
        original_id: Bson,
        json_text: String,
    },
    DocumentUpdated {
        connection_name: String,
        database: String,
        collection: String,
        result: Result<DocumentRow, String>,
    },
    RequestDeleteDocument {
        connection_name: String,
        database: String,
        collection: String,
        doc_id: Bson,
        display_id: String,
    },
    ConfirmDeleteDocument {
        connection_name: String,
        database: String,
        collection: String,
        doc_id: Bson,
        display_id: String,
    },
    DocumentDeleted {
        connection_name: String,
        database: String,
        collection: String,
        display_id: String,
        result: Result<(), String>,
    },
    OpenCreateUserModal {
        connection_index: usize,
    },
    OpenDuplicateUserModal {
        connection_index: usize,
        source_username: String,
    },
    CreateUser {
        connection_index: usize,
        username: String,
        password: String,
        roles: Vec<UserRole>,
    },
    GrantUserRole {
        connection_name: String,
        username: String,
        db: String,
        role: String,
    },
    RevokeUserRole {
        connection_name: String,
        username: String,
        db: String,
        role: String,
    },
    SaveUserRoles {
        connection_name: String,
        username: String,
        roles: Vec<UserRole>,
    },
    OpenChangePasswordModal {
        connection_name: String,
        username: String,
    },
    ChangeUserPassword {
        connection_name: String,
        username: String,
        new_password: String,
    },
    UserPasswordChanged {
        connection_name: String,
        username: String,
        result: Result<(), String>,
    },
    RequestDeleteUser {
        connection_index: usize,
        username: String,
    },
    ConfirmDeleteUser {
        connection_index: usize,
        username: String,
    },
    UserOperationResult {
        connection_name: String,
        message: String,
        result: Result<Vec<MongoUser>, String>,
    },
    BackgroundTaskError {
        source: String,
        message: String,
    },
    RefreshConnections,
    ExecuteFilterQuery {
        database: String,
        collection: String,
        filter: String,
    },
    InsertMockDocument {
        database: String,
        collection: String,
    },
    ClearOutputLogs,
    ResetWorkspaceLayout,
}
