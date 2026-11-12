use mongo_studio::core::{AppCommand, AppEvent, AppState};
use mongo_studio::models::{CollectionInfo, MongoUser, UserRole, COMMON_ROLES};
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
    let by_conn = state.find_collection_by_conn("MongoDB Local", "admin", "users");
    let by_staging = state.find_collection_by_conn("Staging Cluster", "analytics_db", "events");
    let missing_db = state.find_collection("unknown_db", "users");
    let missing_coll = state.find_collection("admin", "unknown_coll");

    // Assert
    assert!(existing.is_some());
    assert_eq!(existing.expect("users exists").documents.len(), 5);
    assert!(by_conn.is_some());
    assert!(by_staging.is_some());
    assert!(missing_db.is_none());
    assert!(missing_coll.is_none());

    // Act: mutate collection via find_collection_by_conn_mut
    if let Some(users_coll) = state.find_collection_by_conn_mut("MongoDB Local", "admin", "users") {
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
fn collection_tab_user_inspector_and_app_state_user_management() {
    // Arrange
    let mut state = AppState::default();
    let user_tab_a = CollectionTab::new_user("MongoDB Local", "app_service");
    let mut user_tab_b = CollectionTab::new_user("MongoDB Local", "app_service");
    user_tab_b.role_db_input = "shop_db".to_string();
    user_tab_b.role_name_input = "dbAdmin".to_string();
    let admin_user_tab = CollectionTab::new_user("MongoDB Local", "admin");
    let regular_coll_tab = CollectionTab::new("MongoDB Local", "admin", "user:app_service");

    // Assert: CollectionTab::new_user properties & equality semantics
    assert_eq!(user_tab_a.selected_user.as_deref(), Some("app_service"));
    assert_eq!(user_tab_a.title(), "app_service");
    assert_eq!(
        user_tab_a.full_namespace(),
        "admin.system.users.app_service"
    );
    assert_eq!(user_tab_a, user_tab_b);
    assert_ne!(user_tab_a, admin_user_tab);
    assert_ne!(user_tab_a, regular_coll_tab);

    // Act: open_or_focus_user deduplicates existing user inspector tabs
    let before_tabs = state.to_persistence().open_tabs.len();
    state.open_or_focus_user("MongoDB Local", "app_service");
    let after_first_open = state.to_persistence().open_tabs.len();
    state.open_or_focus_user("MongoDB Local", "app_service");
    let after_second_open = state.to_persistence().open_tabs.len();

    // Assert
    assert_eq!(after_first_open, before_tabs + 1);
    assert_eq!(after_second_open, after_first_open);

    // Assert: default mock connection users & roles
    let local_conn = &state.connections[0];
    assert!(local_conn.is_admin);
    assert_eq!(local_conn.users.len(), 2);

    let admin_user = state
        .find_user("MongoDB Local", "admin")
        .expect("admin user should exist");
    assert!(admin_user.is_protected_admin());
    assert_eq!(admin_user.db, "admin");

    let service_user = state
        .find_user("MongoDB Local", "app_service")
        .expect("app_service user should exist");
    assert!(!service_user.is_protected_admin());
    assert!(state.find_user("MongoDB Local", "non_existent").is_none());

    // Assert: MongoUser::mock_users and UserRole descriptions
    let mocks = MongoUser::mock_users();
    assert_eq!(mocks.len(), 2);
    assert!(COMMON_ROLES.contains(&"readWrite"));
    assert!(UserRole::COMMON_ROLES.contains(&"root"));
    let rw_role = UserRole::new(" readWrite ", " shop_db ");
    assert_eq!(rw_role.role, "readWrite");
    assert_eq!(rw_role.db, "shop_db");
    assert!(rw_role.description().contains("Read and modify"));
    assert!(UserRole::role_description("customRole").contains("Custom"));
}

#[test]
fn collection_tab_new_user_initializes_phase3_draft_and_cache_fields() {
    // Arrange / Act
    let tab = CollectionTab::new_user("LAN Mongo", "analyst");

    // Assert
    assert_eq!(tab.draft_roles, None);
    assert!(tab.cached_db_options.is_empty());
    assert!(tab.cached_db_conn.is_empty());
    assert_eq!(tab.cached_db_count, 0);
    assert!(tab.role_db_filter.is_empty());
    assert_eq!(tab.role_db_input, "admin");
    assert_eq!(tab.role_name_input, "readWrite");
    assert_eq!(tab.selected_row, None);
}

#[test]
fn app_event_phase3_polish_variants_construct_and_compare_correctly() {
    // Arrange
    let multi_roles = vec![
        UserRole::new("readWrite", "shop_db"),
        UserRole::new("read", "admin"),
    ];
    let duplicate_modal = AppEvent::OpenDuplicateUserModal {
        connection_index: 1,
        source_username: "app_service".to_string(),
    };
    let create_user = AppEvent::CreateUser {
        connection_index: 1,
        username: "app_service_copy".to_string(),
        password: "secret".to_string(),
        roles: multi_roles.clone(),
    };
    let save_roles = AppEvent::SaveUserRoles {
        connection_name: "LAN Mongo".to_string(),
        username: "app_service".to_string(),
        roles: multi_roles.clone(),
    };
    let open_pwd_modal = AppEvent::OpenChangePasswordModal {
        connection_name: "LAN Mongo".to_string(),
        username: "app_service".to_string(),
    };
    let pwd_ok = AppEvent::UserPasswordChanged {
        connection_name: "LAN Mongo".to_string(),
        username: "app_service".to_string(),
        result: Ok(()),
    };
    let pwd_err = AppEvent::UserPasswordChanged {
        connection_name: "LAN Mongo".to_string(),
        username: "app_service".to_string(),
        result: Err("permission denied".to_string()),
    };
    let db_created_ok = AppEvent::DatabaseCreated {
        connection_name: "LAN Mongo".to_string(),
        db_name: "inventory_db".to_string(),
        collection_name: "items".to_string(),
        result: Ok(CollectionInfo::new_lazy_stub("inventory_db", "items")),
    };
    let db_created_err = AppEvent::DatabaseCreated {
        connection_name: "LAN Mongo".to_string(),
        db_name: "inventory_db".to_string(),
        collection_name: "items".to_string(),
        result: Err("connection timed out".to_string()),
    };
    let keyring_some = AppEvent::KeyringPasswordLoaded {
        index: 0,
        open_edit_modal: true,
        result: Ok(Some("pw123".to_string())),
    };
    let keyring_none = AppEvent::KeyringPasswordLoaded {
        index: 0,
        open_edit_modal: false,
        result: Ok(None),
    };
    let keyring_err = AppEvent::KeyringPasswordLoaded {
        index: 0,
        open_edit_modal: false,
        result: Err("keychain locked".to_string()),
    };
    let bg_err = AppEvent::BackgroundTaskError {
        source: "Keyring".to_string(),
        message: "Failed to save password".to_string(),
    };

    // Act / Assert
    assert_eq!(duplicate_modal.clone(), duplicate_modal);
    assert_ne!(pwd_ok, pwd_err);
    assert_ne!(db_created_ok, db_created_err);
    assert_ne!(keyring_some, keyring_none);
    assert_ne!(keyring_none, keyring_err);

    assert!(matches!(
        create_user,
        AppEvent::CreateUser {
            connection_index: 1,
            ref roles,
            ..
        } if roles == &multi_roles
    ));
    assert!(matches!(
        save_roles,
        AppEvent::SaveUserRoles {
            ref roles,
            ..
        } if roles.len() == 2
    ));
    assert!(matches!(
        open_pwd_modal,
        AppEvent::OpenChangePasswordModal {
            ref connection_name,
            ref username,
        } if connection_name == "LAN Mongo" && username == "app_service"
    ));
    assert!(matches!(
        bg_err,
        AppEvent::BackgroundTaskError {
            ref source,
            ref message,
        } if source == "Keyring" && message.contains("Failed")
    ));
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
    assert!(!CollectionSubTab::Documents.icon().is_empty());
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
