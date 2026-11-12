use mongo_studio::core::{AppCommand, AppEvent, AppState};
use mongo_studio::theme::{palette, ThemeMode};
use mongo_studio::ui::activity_bar::ActivityView;
use mongo_studio::workspace::{
    BottomPanelState, BottomPanelTab, CollectionSubTab, CollectionTab, DocumentViewMode, LogLevel,
    WorkspacePersistence,
};

#[test]
fn app_command_all_items_and_event_conversions_are_complete() {
    // Arrange
    let items = AppCommand::all_items();

    // Act / Assert
    assert_eq!(items.len(), 12);
    assert!(items.iter().all(|item| !item.label.is_empty()));
    assert!(items.iter().all(|item| !item.category.is_empty()));

    assert_eq!(
        AppEvent::from(AppCommand::ToggleSidebar),
        AppEvent::ToggleSidebar
    );
    assert_eq!(
        AppEvent::from(AppCommand::ToggleBottomPanel),
        AppEvent::ToggleBottomPanel
    );
    assert_eq!(
        AppEvent::from(AppCommand::SwitchToDarkTheme),
        AppEvent::SwitchTheme(ThemeMode::Dark)
    );
    assert_eq!(
        AppEvent::from(AppCommand::SwitchToLightTheme),
        AppEvent::SwitchTheme(ThemeMode::Light)
    );
    assert_eq!(
        AppEvent::from(AppCommand::NewConnection),
        AppEvent::OpenNewConnectionModal
    );
    assert_eq!(
        AppEvent::from(AppCommand::RefreshExplorer),
        AppEvent::RefreshConnections
    );
    assert_eq!(
        AppEvent::from(AppCommand::OpenUsersCollection),
        AppEvent::OpenCollection {
            connection_name: "MongoDB Local".to_string(),
            database: "admin".to_string(),
            collection: "users".to_string(),
        }
    );
    assert_eq!(
        AppEvent::from(AppCommand::OpenProductsCollection),
        AppEvent::OpenCollection {
            connection_name: "MongoDB Local".to_string(),
            database: "admin".to_string(),
            collection: "products".to_string(),
        }
    );
    assert_eq!(
        AppEvent::from(AppCommand::OpenOrdersCollection),
        AppEvent::OpenCollection {
            connection_name: "MongoDB Local".to_string(),
            database: "admin".to_string(),
            collection: "orders".to_string(),
        }
    );
    assert_eq!(
        AppEvent::from(AppCommand::OpenCustomersCollection),
        AppEvent::OpenCollection {
            connection_name: "MongoDB Local".to_string(),
            database: "shop_db".to_string(),
            collection: "customers".to_string(),
        }
    );
    assert_eq!(
        AppEvent::from(AppCommand::ClearLogs),
        AppEvent::ClearOutputLogs
    );
    assert_eq!(
        AppEvent::from(AppCommand::ResetLayout),
        AppEvent::ResetWorkspaceLayout
    );
}

#[test]
fn app_state_collection_lookup_and_mutation_update_in_memory_models() {
    // Arrange
    let mut state = AppState::default();

    // Act
    let existing = state.find_collection("admin", "users");
    let missing_db = state.find_collection("unknown_db", "users");
    let missing_coll = state.find_collection("admin", "unknown_coll");

    // Assert
    assert!(existing.is_some());
    assert_eq!(existing.expect("users exists").documents.len(), 5);
    assert!(missing_db.is_none());
    assert!(missing_coll.is_none());

    // Act: mutate collection via find_collection_mut
    if let Some(users_coll) = state.find_collection_mut("admin", "users") {
        users_coll.stats.document_count = 42;
    }

    // Assert
    assert_eq!(
        state
            .find_collection("admin", "users")
            .expect("users exists")
            .stats
            .document_count,
        42
    );
}

#[test]
fn app_state_open_or_focus_collection_deduplicates_existing_tabs_and_roundtrips_persistence() {
    // Arrange
    let mut state = AppState::default();
    let initial_tab_count = state.to_persistence().open_tabs.len();
    assert_eq!(initial_tab_count, 2);

    // Act: focusing an already-open tab should not duplicate it
    state.open_or_focus_collection("MongoDB Local", "admin", "users");
    let after_focus_count = state.to_persistence().open_tabs.len();

    // Act: opening a new collection tab should append it
    state.open_or_focus_collection("MongoDB Local", "shop_db", "customers");
    state.theme_mode = ThemeMode::Light;
    state.sidebar_visible = false;
    state.bottom_panel.visible = false;
    state.active_activity = ActivityView::SchemaAnalyzer;

    let persisted = state.to_persistence();
    let restored = AppState::from_persistence(persisted);

    // Assert
    assert_eq!(after_focus_count, 2);
    assert_eq!(restored.to_persistence().open_tabs.len(), 3);
    assert_eq!(restored.theme_mode, ThemeMode::Light);
    assert!(!restored.sidebar_visible);
    assert!(!restored.bottom_panel.visible);
    assert_eq!(restored.active_activity, ActivityView::SchemaAnalyzer);

    // Empty open_tabs in persistence falls back to default 2 tabs
    let empty_persisted = WorkspacePersistence {
        open_tabs: Vec::new(),
        ..WorkspacePersistence::default()
    };
    let fallback_state = AppState::from_persistence(empty_persisted);
    assert_eq!(fallback_state.to_persistence().open_tabs.len(), 2);
}

#[test]
fn collection_tab_bottom_panel_and_theme_palette_behave_consistently() {
    // Arrange
    let mut tab_a = CollectionTab::new("MongoDB Local", "admin", "users");
    let mut tab_b = CollectionTab::new("MongoDB Local", "admin", "users");
    tab_b.filter_query = "{}".to_string();
    tab_b.active_subtab = CollectionSubTab::Indexes;
    tab_b.view_mode = DocumentViewMode::JsonTree;
    let tab_c = CollectionTab::new("MongoDB Local", "admin", "orders");

    let mut panel = BottomPanelState::default();
    let initial_logs = panel.logs.len();

    // Act
    panel.push_log(LogLevel::Warning, "TestRunner", "Simulated warning log");
    let dark = palette(ThemeMode::Dark);
    let light = palette(ThemeMode::Light);

    // Assert: CollectionTab identity is keyed on (connection_name, database, collection)
    assert_eq!(tab_a, tab_b);
    assert_ne!(tab_a, tab_c);
    assert_eq!(tab_a.title(), "users");
    assert_eq!(tab_a.full_namespace(), "admin.users");
    assert_eq!(CollectionSubTab::Documents.label(), "Documents");
    assert_eq!(CollectionSubTab::Indexes.label(), "Indexes");
    assert_eq!(CollectionSubTab::Schema.label(), "Schema");
    assert_eq!(CollectionSubTab::Aggregation.label(), "Aggregation");
    tab_a.selected_row = None;
    assert!(tab_a.selected_row.is_none());

    // Assert: BottomPanelState::push_log
    assert_eq!(panel.logs.len(), initial_logs + 1);
    let last_log = panel.logs.last().expect("log entry should exist");
    assert_eq!(last_log.level, LogLevel::Warning);
    assert_eq!(last_log.source, "TestRunner");
    assert_eq!(last_log.message, "Simulated warning log");
    assert_eq!(last_log.timestamp, "00:00:04");
    assert_eq!(BottomPanelTab::Terminal.label(), "TERMINAL");
    assert_eq!(BottomPanelTab::Output.label(), "OUTPUT");
    assert_eq!(BottomPanelTab::QueryHistory.label(), "QUERY HISTORY");
    assert_eq!(BottomPanelTab::Problems.label(), "PROBLEMS");

    // Assert: ThemeMode and palette
    assert_eq!(ThemeMode::default(), ThemeMode::Dark);
    assert_ne!(dark.editor_bg, light.editor_bg);
    assert_ne!(dark.sidebar_bg, light.sidebar_bg);
    assert_eq!(dark.accent_color, light.accent_color);
}

