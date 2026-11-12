use uuid::Uuid;

use crate::models::DatabaseInfo;
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
    AddConnection {
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
        result: Result<Vec<DatabaseInfo>, String>,
        latency_ms: Option<f64>,
    },
    TestConnectionResult {
        uri: String,
        result: Result<f64, String>,
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
