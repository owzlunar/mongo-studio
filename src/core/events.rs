use std::path::PathBuf;

use bson::Bson;
use uuid::Uuid;

use crate::database::{
    AggregationResult, CreateIndexRequest, DocumentPageResult, ExportStats, ImportStats,
    TransferFormat,
};
use crate::models::{
    CollectionInfo, DatabaseInfo, DocumentRow, IndexInfo, MongoUser, QuickAccessFilter, SavedQuery,
    SchemaField, SshTunnelConfig, UserRole,
};
use crate::query::AggregationStage;
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
    SshSecretLoaded {
        index: usize,
        open_edit_modal: bool,
        result: Result<Option<String>, String>,
    },
    AddConnection {
        name: String,
        uri: String,
        ssh_tunnel: SshTunnelConfig,
        ssh_secret: String,
    },
    UpdateConnection {
        index: usize,
        name: String,
        uri: String,
        ssh_tunnel: SshTunnelConfig,
        ssh_secret: String,
    },
    TestConnection {
        name: String,
        uri: String,
        ssh_tunnel: SshTunnelConfig,
        ssh_secret: String,
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
        user_db: String,
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
        source_user_db: String,
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
        user_db: String,
        roles: Vec<UserRole>,
    },
    OpenChangePasswordModal {
        connection_name: String,
        username: String,
        user_db: String,
    },
    ChangeUserPassword {
        connection_name: String,
        username: String,
        user_db: String,
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
        user_db: String,
    },
    ConfirmDeleteUser {
        connection_index: usize,
        username: String,
        user_db: String,
    },
    UserOperationResult {
        connection_name: String,
        message: String,
        result: Result<Vec<MongoUser>, String>,
    },
    RunAggregationPipeline {
        connection_name: String,
        database: String,
        collection: String,
        raw_json: String,
        stages: Vec<AggregationStage>,
    },
    AggregationResultLoaded {
        connection_name: String,
        database: String,
        collection: String,
        stage_count: usize,
        result: Result<AggregationResult, String>,
    },
    OpenCreateIndexModal {
        connection_name: String,
        database: String,
        collection: String,
    },
    CreateIndex {
        connection_name: String,
        database: String,
        collection: String,
        request: CreateIndexRequest,
    },
    RequestDropIndex {
        connection_name: String,
        database: String,
        collection: String,
        index_name: String,
    },
    ConfirmDropIndex {
        connection_name: String,
        database: String,
        collection: String,
        index_name: String,
    },
    IndexesUpdated {
        connection_name: String,
        database: String,
        collection: String,
        message: String,
        result: Result<Vec<IndexInfo>, String>,
    },
    AnalyzeCollectionSchema {
        connection_name: String,
        database: String,
        collection: String,
        sample_size: usize,
    },
    SchemaAnalyzed {
        connection_name: String,
        database: String,
        collection: String,
        sample_size: usize,
        result: Result<Vec<SchemaField>, String>,
    },
    OpenSaveQueryModal {
        connection_name: String,
        database: String,
        collection: String,
        filter: String,
        projection: String,
        sort: String,
        limit: usize,
    },
    SaveQuery {
        query: SavedQuery,
    },
    DeleteSavedQuery {
        id: Uuid,
    },
    RunSavedQuery {
        query: SavedQuery,
    },
    RunHistoryQuery {
        command: String,
    },
    ExecuteShellCommand {
        command: String,
    },
    ShellCommandCompleted {
        command: String,
        new_current_db: Option<String>,
        result: Result<String, String>,
    },
    ExportCollectionData {
        connection_name: String,
        database: String,
        collection: String,
        filter: String,
        format: TransferFormat,
        path: PathBuf,
    },
    ExportCompleted {
        message: String,
        result: Result<ExportStats, String>,
    },
    ImportCollectionData {
        connection_name: String,
        database: String,
        collection: String,
        format: TransferFormat,
        path: PathBuf,
    },
    ImportCompleted {
        connection_name: String,
        database: String,
        collection: String,
        result: Result<ImportStats, String>,
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
    ToggleFavoriteConnection {
        connection_id: Uuid,
    },
    SetQuickAccessFilter {
        filter: QuickAccessFilter,
    },
    ToggleQuickAccessFilter {
        filter: QuickAccessFilter,
    },
    FilterWorkspaceView {
        workspace_name: Option<String>,
    },
    CreateWorkspace {
        name: String,
    },
    CreateProjectFolder {
        workspace: String,
        folder_name: String,
    },
    MoveConnectionToFolder {
        connection_id: Uuid,
        workspace: Option<String>,
        folder: Option<String>,
    },
    DeleteWorkspace {
        name: String,
    },
    DeleteProjectFolder {
        workspace: String,
        folder_name: String,
    },
    OpenManageWorkspacesModal,
    CloseManageWorkspacesModal,
}
