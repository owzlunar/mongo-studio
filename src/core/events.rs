use crate::theme::ThemeMode;
use crate::ui::activity_bar::ActivityView;

#[derive(Debug, Clone)]
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
