use crate::core::events::AppEvent;
use crate::theme::ThemeMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    ToggleSidebar,
    ToggleBottomPanel,
    SwitchToDarkTheme,
    SwitchToLightTheme,
    NewConnection,
    RefreshExplorer,
    OpenUsersCollection,
    OpenProductsCollection,
    OpenOrdersCollection,
    OpenCustomersCollection,
    ClearLogs,
    ResetLayout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandItem {
    pub command: AppCommand,
    pub label: &'static str,
    pub category: &'static str,
    pub shortcut: &'static str,
}

const ALL_COMMAND_ITEMS: [CommandItem; 12] = [
    CommandItem {
        command: AppCommand::NewConnection,
        label: "Connection: Add New MongoDB Connection...",
        category: "Connections",
        shortcut: "Ctrl+N",
    },
    CommandItem {
        command: AppCommand::RefreshExplorer,
        label: "Explorer: Refresh Databases & Collections",
        category: "Explorer",
        shortcut: "F5",
    },
    CommandItem {
        command: AppCommand::ToggleSidebar,
        label: "View: Toggle Primary Sidebar",
        category: "View",
        shortcut: "Ctrl+B",
    },
    CommandItem {
        command: AppCommand::ToggleBottomPanel,
        label: "View: Toggle Bottom Panel (Terminal / Output)",
        category: "View",
        shortcut: "Ctrl+J",
    },
    CommandItem {
        command: AppCommand::SwitchToDarkTheme,
        label: "Preferences: Color Theme -> Dark (VS Code Dark+)",
        category: "Preferences",
        shortcut: "",
    },
    CommandItem {
        command: AppCommand::SwitchToLightTheme,
        label: "Preferences: Color Theme -> Light",
        category: "Preferences",
        shortcut: "",
    },
    CommandItem {
        command: AppCommand::OpenUsersCollection,
        label: "Collection: Open admin.users",
        category: "Collection",
        shortcut: "",
    },
    CommandItem {
        command: AppCommand::OpenProductsCollection,
        label: "Collection: Open admin.products",
        category: "Collection",
        shortcut: "",
    },
    CommandItem {
        command: AppCommand::OpenOrdersCollection,
        label: "Collection: Open admin.orders",
        category: "Collection",
        shortcut: "",
    },
    CommandItem {
        command: AppCommand::OpenCustomersCollection,
        label: "Collection: Open shop_db.customers",
        category: "Collection",
        shortcut: "",
    },
    CommandItem {
        command: AppCommand::ClearLogs,
        label: "Output: Clear Output & Query Logs",
        category: "Output",
        shortcut: "",
    },
    CommandItem {
        command: AppCommand::ResetLayout,
        label: "Workspace: Reset Window Layout to Default",
        category: "Workspace",
        shortcut: "",
    },
];

impl AppCommand {
    #[must_use]
    pub const fn all_items() -> &'static [CommandItem] {
        &ALL_COMMAND_ITEMS
    }

    #[must_use]
    pub fn to_event(self) -> AppEvent {
        match self {
            Self::ToggleSidebar => AppEvent::ToggleSidebar,
            Self::ToggleBottomPanel => AppEvent::ToggleBottomPanel,
            Self::SwitchToDarkTheme => AppEvent::SwitchTheme(ThemeMode::Dark),
            Self::SwitchToLightTheme => AppEvent::SwitchTheme(ThemeMode::Light),
            Self::NewConnection => AppEvent::OpenNewConnectionModal,
            Self::RefreshExplorer => AppEvent::RefreshConnections,
            Self::OpenUsersCollection => AppEvent::OpenCollection {
                connection_name: "MongoDB Local".to_string(),
                database: "admin".to_string(),
                collection: "users".to_string(),
            },
            Self::OpenProductsCollection => AppEvent::OpenCollection {
                connection_name: "MongoDB Local".to_string(),
                database: "admin".to_string(),
                collection: "products".to_string(),
            },
            Self::OpenOrdersCollection => AppEvent::OpenCollection {
                connection_name: "MongoDB Local".to_string(),
                database: "admin".to_string(),
                collection: "orders".to_string(),
            },
            Self::OpenCustomersCollection => AppEvent::OpenCollection {
                connection_name: "MongoDB Local".to_string(),
                database: "shop_db".to_string(),
                collection: "customers".to_string(),
            },
            Self::ClearLogs => AppEvent::ClearOutputLogs,
            Self::ResetLayout => AppEvent::ResetWorkspaceLayout,
        }
    }
}

impl From<AppCommand> for AppEvent {
    fn from(command: AppCommand) -> Self {
        command.to_event()
    }
}
