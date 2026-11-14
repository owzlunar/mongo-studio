use std::borrow::Cow;
use std::fs;
use std::path::PathBuf;

use bson::{doc, oid::ObjectId, Bson, DateTime};
use mongo_studio::core::{
    AppCommand, AppError, AppEvent, AppState, PendingDeleteDocument, PendingDropIndex,
};
use mongo_studio::database::{
    ConnectionManager, CreateIndexRequest, DocumentPageResult, TransferFormat,
};
use mongo_studio::models::{
    CollectionInfo, ConnectionConfig, ConnectionStatus, DocumentRow, EnvironmentTag, MongoUser,
    QueryStore, QuickAccessFilter, SavedQuery, SshAuthMethod, SshTunnelConfig, UserRole,
    WorkspaceTree, COMMON_ROLES, MAX_QUERY_HISTORY,
};
use mongo_studio::query::AggregationStage;
use mongo_studio::theme::{palette, ThemeMode};
use mongo_studio::ui::activity_bar::ActivityView;
use mongo_studio::ui::components::syntax_highlight::highlight_json_job;
use mongo_studio::workspace::{
    BottomPanelState, BottomPanelTab, CollectionSubTab, CollectionTab, DocumentViewMode, LogLevel,
    WorkspacePersistence,
};
use serde_json::{json, Value};
use uuid::Uuid;

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
    let user_tab_a = CollectionTab::new_user("MongoDB Local", "app_service", "admin");
    let mut user_tab_b = CollectionTab::new_user("MongoDB Local", "app_service", "admin");
    user_tab_b.role_db_input = "shop_db".to_string();
    user_tab_b.role_name_input = "dbAdmin".to_string();
    let admin_user_tab = CollectionTab::new_user("MongoDB Local", "admin", "admin");
    let regular_coll_tab = CollectionTab::new("MongoDB Local", "admin", "user:app_service");

    // Assert: CollectionTab::new_user properties & equality semantics
    assert_eq!(user_tab_a.selected_user.as_deref(), Some("app_service"));
    assert_eq!(user_tab_a.selected_user_db.as_deref(), Some("admin"));
    assert_eq!(user_tab_a.title(), "app_service (admin)");
    assert_eq!(
        user_tab_a.full_namespace(),
        "admin.system.users.app_service"
    );
    assert_eq!(user_tab_a, user_tab_b);
    assert_ne!(user_tab_a, admin_user_tab);
    assert_ne!(user_tab_a, regular_coll_tab);

    // Act: open_or_focus_user deduplicates existing user inspector tabs
    let before_tabs = state.to_persistence().open_tabs.len();
    state.open_or_focus_user("MongoDB Local", "app_service", "admin");
    let after_first_open = state.to_persistence().open_tabs.len();
    state.open_or_focus_user("MongoDB Local", "app_service", "admin");
    let after_second_open = state.to_persistence().open_tabs.len();

    // Assert
    assert_eq!(after_first_open, before_tabs + 1);
    assert_eq!(after_second_open, after_first_open);

    // Assert: default mock connection users & roles
    let local_conn = &state.connections[0];
    assert!(local_conn.is_admin);
    assert_eq!(local_conn.users.len(), 2);

    let admin_user = state
        .find_user("MongoDB Local", "admin", "admin")
        .expect("admin user should exist");
    assert!(admin_user.is_protected_admin());
    assert_eq!(admin_user.db, "admin");

    let service_user = state
        .find_user("MongoDB Local", "app_service", "admin")
        .expect("app_service user should exist");
    assert!(!service_user.is_protected_admin());
    assert!(state
        .find_user("MongoDB Local", "non_existent", "admin")
        .is_none());

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
    let tab = CollectionTab::new_user("LAN Mongo", "analyst", "admin");

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
        source_user_db: "admin".to_string(),
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
        user_db: "admin".to_string(),
        roles: multi_roles.clone(),
    };
    let open_pwd_modal = AppEvent::OpenChangePasswordModal {
        connection_name: "LAN Mongo".to_string(),
        username: "app_service".to_string(),
        user_db: "admin".to_string(),
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
            ref user_db,
        } if connection_name == "LAN Mongo" && username == "app_service" && user_db == "admin"
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

#[test]
fn collection_tab_phase4_fields_column_caching_and_serde_backward_compatibility() {
    // Arrange
    let mut tab = CollectionTab::new("LAN Mongo", "shop_db", "orders");
    let doc_a = DocumentRow::from_bson_doc(&doc! {
        "_id": 1_i32,
        "order_no": "ORD-001",
        "total": 120.5_f64
    });
    let doc_b = DocumentRow::from_bson_doc(&doc! {
        "_id": 2_i32,
        "order_no": "ORD-002",
        "currency": "USD"
    });
    let first_batch = vec![doc_a];
    let second_batch = vec![doc_b];

    // Assert defaults
    assert_eq!(tab.skip, 0);
    assert_eq!(tab.limit, 20);
    assert_eq!(tab.total_matched, None);
    assert!(!tab.is_loading);
    assert_eq!(tab.last_query_ms, None);
    assert!(tab.cached_columns.is_empty());
    assert_eq!(tab.projection_query, "{}");
    assert_eq!(tab.sort_query, "{\"_id\": 1}");

    // Act: ensure_cached_columns ignores empty slice, populates once, then preserves cache
    tab.ensure_cached_columns(&[]);
    assert!(tab.cached_columns.is_empty());

    tab.ensure_cached_columns(&first_batch);
    assert_eq!(tab.cached_columns, vec!["_id", "order_no", "total"]);

    tab.ensure_cached_columns(&second_batch);
    assert_eq!(tab.cached_columns, vec!["_id", "order_no", "total"]);

    // Act: refresh_cached_columns recomputes columns for the new page
    tab.refresh_cached_columns(&second_batch);
    assert_eq!(tab.cached_columns, vec!["_id", "order_no", "currency"]);

    // Act: deserialize legacy Phase 3 JSON payload without `skip` or runtime fields
    let phase3_legacy_json = json!({
        "connection_name": "MongoDB Local",
        "database": "admin",
        "collection": "users",
        "active_subtab": "Documents",
        "view_mode": "Table",
        "filter_query": "{\"status\": \"active\"}",
        "projection_query": "{}",
        "sort_query": "{\"_id\": 1}",
        "limit": 50,
        "selected_row": 0
    })
    .to_string();

    let restored: CollectionTab = serde_json::from_str(&phase3_legacy_json)
        .expect("Phase 3 CollectionTab JSON should deserialize");

    // Assert
    assert_eq!(restored.connection_name, "MongoDB Local");
    assert_eq!(restored.database, "admin");
    assert_eq!(restored.collection, "users");
    assert_eq!(restored.skip, 0);
    assert_eq!(restored.limit, 50);
    assert_eq!(restored.total_matched, None);
    assert!(!restored.is_loading);
    assert_eq!(restored.last_query_ms, None);
    assert!(restored.cached_columns.is_empty());
}

#[test]
fn app_state_and_events_phase4_modal_and_pending_defaults_are_clean() {
    // Arrange / Act
    let state = AppState::default();

    // Assert defaults
    assert!(!state.show_create_coll_modal);
    assert_eq!(state.create_coll_conn_idx, 0);
    assert!(state.create_coll_db_name.is_empty());
    assert!(state.new_coll_name.is_empty());
    assert_eq!(state.pending_drop_db, None);
    assert_eq!(state.pending_drop_coll, None);
    assert!(!state.show_doc_editor_modal);
    assert!(state.doc_editor_conn_name.is_empty());
    assert!(state.doc_editor_db.is_empty());
    assert!(state.doc_editor_coll.is_empty());
    assert_eq!(state.doc_editor_original_id, None);
    assert!(state.doc_editor_json.is_empty());
    assert_eq!(state.pending_delete_doc, None);

    // Assert PendingDeleteDocument & Phase 4 AppEvent construction
    let pending = PendingDeleteDocument {
        connection_name: "LAN Mongo".to_string(),
        database: "shop_db".to_string(),
        collection: "orders".to_string(),
        doc_id: Bson::Int32(42),
        display_id: "42".to_string(),
    };
    assert_eq!(pending.clone(), pending);

    let page_event = AppEvent::QueryPageLoaded {
        connection_name: "LAN Mongo".to_string(),
        database: "shop_db".to_string(),
        collection: "orders".to_string(),
        skip: 20,
        limit: 20,
        result: Ok(DocumentPageResult {
            documents: Vec::new(),
            total_count: 45,
            skip: 20,
            limit: 20,
            query_ms: 4.5,
        }),
    };
    assert!(matches!(
        page_event,
        AppEvent::QueryPageLoaded {
            skip: 20,
            limit: 20,
            result: Ok(ref res),
            ..
        } if res.total_count == 45
    ));
}

#[test]
fn document_row_extended_json_columns_cells_ids_and_cache_behave_accurately() {
    // Arrange: Extended JSON parsing
    let oid_hex = "654f1a2b3c4d5e6f7a8b9c0d";
    let parsed_ext = DocumentRow::parse_json_to_bson_doc(&format!(
        r#"{{"_id": {{"$oid": "{oid_hex}"}}, "count": 7, "created_at": {{"$date": "2026-05-01T10:30:00Z"}}}}"#
    ))
    .expect("valid Extended JSON object should parse");

    // Assert: parse_json_to_bson_doc
    assert_eq!(
        parsed_ext
            .get_object_id("_id")
            .expect("_id ObjectId")
            .to_hex(),
        oid_hex
    );
    assert!(matches!(
        DocumentRow::parse_json_to_bson_doc("   "),
        Err(AppError::InvalidQueryJson(_))
    ));
    assert!(matches!(
        DocumentRow::parse_json_to_bson_doc("[1, 2, 3]"),
        Err(AppError::InvalidQueryJson(_))
    ));
    assert!(matches!(
        DocumentRow::parse_json_to_bson_doc("42"),
        Err(AppError::InvalidQueryJson(_))
    ));
    assert!(matches!(
        DocumentRow::parse_json_to_bson_doc("{invalid: true}"),
        Err(AppError::InvalidQueryJson(_))
    ));

    // Assert: parse_bson_doc_allow_empty & parse_filter_doc
    assert_eq!(
        DocumentRow::parse_bson_doc_allow_empty("  \n ").expect("blank optional doc"),
        None
    );
    assert_eq!(
        DocumentRow::parse_bson_doc_allow_empty("{}").expect("empty object optional doc"),
        None
    );
    assert_eq!(
        DocumentRow::parse_bson_doc_allow_empty(r#"{"created_at": -1}"#)
            .expect("valid sort doc")
            .and_then(|d| d.get("_id").or_else(|| d.get("created_at")).cloned()),
        Some(Bson::Int64(-1))
    );
    assert!(DocumentRow::parse_bson_doc_allow_empty("{bad}").is_err());

    assert_eq!(
        DocumentRow::parse_filter_doc("   ").expect("blank filter -> empty doc"),
        bson::Document::new()
    );
    assert_eq!(
        DocumentRow::parse_filter_doc(r#"{"active": true}"#).expect("valid filter doc"),
        doc! { "active": true }
    );
    assert!(DocumentRow::parse_filter_doc("{bad}").is_err());

    // Arrange: heterogeneous documents for extract_columns & cell_display_value
    let oid = ObjectId::parse_str(oid_hex).expect("valid ObjectId");
    let dt = DateTime::from_millis(1_700_000_000_000);
    let bson_doc_1 = doc! {
        "title": "Sensor A",
        "_id": oid,
        "meta": { "zone": "B", "floor": 2_i32 },
        "tags": ["iot", "cold"],
        "created_at": dt,
        "notes": Bson::Null,
    };
    let bson_doc_2 = doc! {
        "_id": 105_i32,
        "title": "Sensor B",
        "enabled": false,
    };
    let bson_doc_3 = doc! {
        "_id": "custom_str_id",
        "extra_num": 99_i64,
    };

    let row_1 = DocumentRow::from_bson_doc(&bson_doc_1);
    let row_2 = DocumentRow::from_bson_doc(&bson_doc_2);
    let row_3 = DocumentRow::from_bson_doc(&bson_doc_3);

    // Assert: extract_columns puts "_id" first and preserves first-seen order
    assert_eq!(
        DocumentRow::extract_columns(&[row_1.clone(), row_2.clone(), row_3.clone()]),
        vec![
            "_id",
            "title",
            "meta",
            "tags",
            "created_at",
            "notes",
            "enabled",
            "extra_num"
        ]
    );

    // Assert: cell_display_value formatting
    assert_eq!(row_1.cell_display_value("_id"), oid_hex);
    assert_eq!(row_1.cell_display_value("title"), "Sensor A");
    assert_eq!(row_1.cell_display_value("meta"), "{2 fields}");
    assert_eq!(row_1.cell_display_value("tags"), "[2 items]");
    assert!(row_1
        .cell_display_value("created_at")
        .contains("2023-11-14"));
    assert_eq!(row_1.cell_display_value("notes"), "null");
    assert_eq!(row_1.cell_display_value("missing_col"), "");
    assert_eq!(row_2.cell_display_value("enabled"), "false");

    // Assert: format_json_value_compact Extended JSON wrappers
    assert_eq!(
        DocumentRow::format_json_value_compact(&json!({ "$oid": oid_hex })),
        oid_hex
    );
    assert_eq!(
        DocumentRow::format_json_value_compact(&json!({ "$numberLong": "9007199254740993" })),
        "9007199254740993"
    );
    assert_eq!(
        DocumentRow::format_json_value_compact(
            &json!({ "$binary": { "base64": "AQID", "subType": "00" } })
        ),
        "[Binary]"
    );
    assert_eq!(
        DocumentRow::format_json_value_compact(
            &json!({ "$regularExpression": { "pattern": "^alpha", "options": "i" } })
        ),
        "/^alpha/i"
    );

    // Assert: extract_bson_id for ObjectId, Int32, String, and missing/malformed _id errors
    assert_eq!(
        row_1.extract_bson_id().expect("ObjectId _id"),
        Bson::ObjectId(oid)
    );
    assert_eq!(
        row_2.extract_bson_id().expect("Int32 _id"),
        Bson::Int32(105)
    );
    assert_eq!(
        row_3.extract_bson_id().expect("String _id"),
        Bson::String("custom_str_id".to_string())
    );

    let missing_id_row = DocumentRow {
        id: "   ".to_string(),
        raw_json: None,
        cached_json: None,
        ..DocumentRow::default()
    };
    assert!(matches!(
        missing_id_row.extract_bson_id(),
        Err(AppError::InvalidQueryJson(_))
    ));

    let malformed_raw_row = DocumentRow {
        id: "1".to_string(),
        raw_json: Some("{not_valid_json".to_string()),
        cached_json: None,
        ..DocumentRow::default()
    };
    assert!(matches!(
        malformed_raw_row.extract_bson_id(),
        Err(AppError::InvalidQueryJson(_))
    ));

    // Assert: cached_or_parsed_json zero-copy borrow vs deserialized raw_json & ensure_cached_json
    assert!(matches!(row_1.cached_or_parsed_json(), Cow::Borrowed(_)));

    let serialized_row = serde_json::to_string(&row_1).expect("DocumentRow should serialize");
    let mut deserialized_row: DocumentRow =
        serde_json::from_str(&serialized_row).expect("DocumentRow should deserialize");
    assert_eq!(deserialized_row.cached_json, None);
    assert!(matches!(
        deserialized_row.cached_or_parsed_json(),
        Cow::Owned(Value::Object(_))
    ));
    assert_eq!(deserialized_row.cell_display_value("meta"), "{2 fields}");

    deserialized_row.ensure_cached_json();
    assert!(deserialized_row.cached_json.is_some());
    assert!(matches!(
        deserialized_row.cached_or_parsed_json(),
        Cow::Borrowed(Value::Object(_))
    ));
}

#[test]
fn collection_tab_phase5_agg_and_schema_defaults_and_legacy_serde_compatibility() {
    // Arrange / Act: fresh CollectionTab defaults
    let tab = CollectionTab::new("LAN Mongo", "shop_db", "orders");

    // Assert: Phase 5 agg_* & schema_* defaults
    assert_eq!(tab.agg_stages, vec![AggregationStage::default()]);
    assert!(!tab.agg_raw_mode);
    assert!(tab.agg_raw_json.contains("\"$match\""));
    assert_eq!(tab.agg_results, None);
    assert_eq!(tab.agg_elapsed_ms, None);
    assert_eq!(tab.agg_error, None);
    assert!(!tab.agg_is_running);
    assert!(tab.agg_cached_columns.is_empty());
    assert_eq!(tab.schema_sample_size, 100);
    assert!(!tab.schema_is_analyzing);

    // Arrange: legacy Phase 3/4 JSON payload missing all Phase 5 agg_* and schema_* fields
    let legacy_phase4_json = json!({
        "connection_name": "MongoDB Local",
        "database": "admin",
        "collection": "users",
        "active_subtab": "Documents",
        "view_mode": "Table",
        "filter_query": "{\"status\": \"active\"}",
        "projection_query": "{}",
        "sort_query": "{\"_id\": 1}",
        "skip": 20,
        "limit": 50,
        "selected_row": 0
    })
    .to_string();

    // Act
    let restored: CollectionTab = serde_json::from_str(&legacy_phase4_json)
        .expect("legacy Phase 4 CollectionTab JSON should deserialize cleanly");

    // Assert
    assert_eq!(restored.skip, 20);
    assert_eq!(restored.limit, 50);
    assert_eq!(restored.agg_stages, vec![AggregationStage::default()]);
    assert!(!restored.agg_raw_mode);
    assert!(restored.agg_raw_json.contains("\"$match\""));
    assert_eq!(restored.agg_results, None);
    assert_eq!(restored.agg_elapsed_ms, None);
    assert_eq!(restored.agg_error, None);
    assert!(!restored.agg_is_running);
    assert!(restored.agg_cached_columns.is_empty());
    assert_eq!(restored.schema_sample_size, 100);
    assert!(!restored.schema_is_analyzing);
}

#[test]
fn saved_query_and_query_store_cap_history_atomic_roundtrip_and_missing_file_fallback() {
    struct TempDirGuard(PathBuf);

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    // Arrange
    let temp_dir = std::env::temp_dir().join(format!("mongo_studio_qstore_{}", Uuid::new_v4()));
    let _guard = TempDirGuard(temp_dir.clone());
    let store_path = temp_dir.join("nested").join("queries.json");
    let missing_path = temp_dir.join("missing_queries.json");
    let corrupt_path = temp_dir.join("corrupt_queries.json");

    // Act / Assert: SavedQuery::new trims name & clamps limit >= 1
    let custom_query = SavedQuery::new(
        "  VIP Orders  ",
        "LAN Mongo",
        "shop_db",
        "orders",
        r#"{"total": {"$gt": 100}}"#,
        r#"{"order_no": 1}"#,
        r#"{"_id": -1}"#,
        0,
    );
    assert_eq!(custom_query.name, "VIP Orders");
    assert_eq!(custom_query.limit, 1);

    // Act / Assert: missing queries.json falls back cleanly to 4 defaults
    let missing_loaded = QueryStore::load_from_path(&missing_path);
    assert_eq!(missing_loaded.saved_queries.len(), 4);
    assert_eq!(missing_loaded.saved_queries[0].name, "Active Users");
    assert!(missing_loaded.query_history.is_empty());

    // Act / Assert: corrupt queries.json falls back gracefully without panicking
    fs::create_dir_all(&temp_dir).expect("should create temp dir");
    fs::write(&corrupt_path, "{corrupt json").expect("should write corrupt test file");
    let corrupt_loaded = QueryStore::load_from_path(&corrupt_path);
    assert_eq!(corrupt_loaded.saved_queries.len(), 4);

    // Act / Assert: push_history ignores blank, deduplicates consecutive entries, caps at MAX_QUERY_HISTORY
    let mut store = QueryStore::default();
    store.saved_queries.push(custom_query.clone());
    store.push_history("   ");
    assert!(store.query_history.is_empty());

    store.push_history("  db.users.find({})  ");
    store.push_history("db.users.find({})");
    assert_eq!(store.query_history.len(), 1);

    for idx in 0..(MAX_QUERY_HISTORY + 25) {
        store.push_history(format!("db.orders.find({{\"seq\": {idx}}})"));
    }
    assert_eq!(store.query_history.len(), MAX_QUERY_HISTORY);
    assert_eq!(
        store.query_history.first().map(String::as_str),
        Some("db.orders.find({\"seq\": 25})")
    );
    assert_eq!(
        store.query_history.last().map(String::as_str),
        Some("db.orders.find({\"seq\": 124})")
    );

    // Direct cap_history() verification when entries exceed MAX_QUERY_HISTORY
    store.query_history.push("extra_1".to_string());
    store.query_history.push("extra_2".to_string());
    assert_eq!(store.query_history.len(), MAX_QUERY_HISTORY + 2);
    store.cap_history();
    assert_eq!(store.query_history.len(), MAX_QUERY_HISTORY);
    assert_eq!(
        store.query_history.last().map(String::as_str),
        Some("extra_2")
    );

    // Act / Assert: atomic save_to_path -> load_from_path roundtrip
    store
        .save_to_path(&store_path)
        .expect("atomic QueryStore save should succeed");
    assert!(store_path.exists());

    let loaded = QueryStore::load_from_path(&store_path);
    assert_eq!(loaded, store);
    assert!(loaded.saved_queries.contains(&custom_query));
}

#[test]
fn bottom_panel_push_terminal_lines_handles_multiline_empty_and_cap() {
    // Arrange
    let mut panel = BottomPanelState::default();
    let initial_len = panel.terminal_history.len();

    // Act / Assert: empty output is a no-op
    panel.push_terminal_lines("");
    assert_eq!(panel.terminal_history.len(), initial_len);

    // Act / Assert: multi-line output pushes each line in order
    panel.push_terminal_lines("admin\nshop_db\nanalytics_db");
    assert_eq!(panel.terminal_history.len(), initial_len + 3);
    assert_eq!(
        &panel.terminal_history[initial_len..],
        &["admin", "shop_db", "analytics_db"]
    );

    // Act / Assert: pushing beyond 500 lines caps terminal_history at 500 and keeps newest lines
    let bulk_lines = (0..600)
        .map(|i| format!("shell line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    panel.push_terminal_lines(&bulk_lines);

    assert_eq!(panel.terminal_history.len(), 500);
    assert_eq!(
        panel.terminal_history.first().map(String::as_str),
        Some("shell line 100")
    );
    assert_eq!(
        panel.terminal_history.last().map(String::as_str),
        Some("shell line 599")
    );
}

#[test]
fn app_state_phase5_defaults_events_and_syntax_highlight_smoke_check() {
    // Arrange / Act
    let state = AppState::default();

    // Assert: Phase 5 AppState defaults
    assert_eq!(state.pending_drop_index, None);
    assert!(!state.show_create_index_modal);
    assert!(state.create_index_conn_name.is_empty());
    assert!(state.create_index_db.is_empty());
    assert!(state.create_index_coll.is_empty());
    assert!(state.new_index_keys.is_empty());
    assert!(state.new_index_name.is_empty());
    assert!(!state.new_index_unique);
    assert!(!state.new_index_sparse);
    assert!(!state.new_index_background);
    assert!(!state.show_save_query_modal);
    assert!(state.save_query_name.is_empty());
    assert_eq!(state.save_query_limit, 20);
    assert_eq!(state.shell_current_db, "admin");
    assert!(!state.is_transferring);
    assert!(!state.show_about_modal);

    // Assert: PendingDropIndex & Phase 5 AppEvent variants
    let pending_idx = PendingDropIndex {
        connection_name: "LAN Mongo".to_string(),
        database: "shop_db".to_string(),
        collection: "orders".to_string(),
        index_name: "email_1".to_string(),
    };
    assert_eq!(pending_idx.clone(), pending_idx);

    let create_idx_event = AppEvent::CreateIndex {
        connection_name: "LAN Mongo".to_string(),
        database: "shop_db".to_string(),
        collection: "orders".to_string(),
        request: CreateIndexRequest {
            keys_json: r#"{"email": 1}"#.to_string(),
            name: Some("email_1".to_string()),
            unique: true,
            sparse: false,
            background: false,
        },
    };
    let export_event = AppEvent::ExportCollectionData {
        connection_name: "LAN Mongo".to_string(),
        database: "shop_db".to_string(),
        collection: "orders".to_string(),
        filter: "{}".to_string(),
        format: TransferFormat::JsonLines,
        path: PathBuf::from("orders.jsonl"),
    };
    assert!(matches!(create_idx_event, AppEvent::CreateIndex { .. }));
    assert!(matches!(
        export_event,
        AppEvent::ExportCollectionData {
            format: TransferFormat::JsonLines,
            ..
        }
    ));

    // Assert: syntax_highlight::highlight_json_job produces non-empty sections in Dark & Light modes
    let sample_json =
        r#"{"_id": {"$oid": "654f1a2b3c4d5e6f7a8b9c0d"}, "active": true, "count": 42}"#;
    let ctx = egui::Context::default();

    for visuals in [egui::Visuals::dark(), egui::Visuals::light()] {
        ctx.set_visuals(visuals);
        let mut non_empty_sections = 0_usize;
        let mut empty_sections = 99_usize;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let job = highlight_json_job(ui, sample_json, egui::FontId::monospace(12.0), 420.0);
                non_empty_sections = job.sections.len();

                let empty_job = highlight_json_job(ui, "", egui::FontId::monospace(12.0), 420.0);
                empty_sections = empty_job.sections.len();
            });
        });
        assert!(non_empty_sections > 0);
        assert_eq!(empty_sections, 0);
    }
}

#[test]
fn app_state_ssh_modal_populate_reset_and_build_config_transitions() {
    // Arrange
    let mut state = AppState::default();

    // Assert default SSH modal state
    assert!(!state.new_conn_ssh_enabled);
    assert!(state.new_conn_ssh_endpoint.is_empty());
    assert!(state.new_conn_ssh_username.is_empty());
    assert_eq!(state.new_conn_ssh_auth_method, SshAuthMethod::Password);
    assert_eq!(
        state.new_conn_ssh_key_path,
        SshTunnelConfig::DEFAULT_PRIVATE_KEY_PATH
    );
    assert!(state.new_conn_ssh_secret.is_empty());
    assert!(!state.new_conn_ssh_save_secret);

    // Arrange a profile with saved MongoDB + SSH tunnel metadata
    let mut profile = ConnectionConfig::new_profile(
        "Prod Bastion",
        "mongodb://dbadmin:mongoSecret@10.0.1.50:27017/app?authSource=admin",
    )
    .expect("valid profile");
    profile.has_saved_password = true;
    profile.ssh_tunnel = SshTunnelConfig {
        enabled: true,
        endpoint: "bastion.internal:2222".to_string(),
        username: "ops".to_string(),
        auth_method: SshAuthMethod::PrivateKey,
        private_key_path: String::new(),
        has_saved_secret: true,
    };
    state.connections[0] = profile.clone();

    // Act: populate modal from profile with empty key path (falls back to default key path)
    state.populate_connection_modal(0, &profile);

    // Assert
    assert!(state.show_new_connection_modal);
    assert_eq!(state.editing_connection_idx, Some(0));
    assert_eq!(state.new_conn_name, "Prod Bastion");
    assert_eq!(
        state.new_conn_uri,
        "mongodb://10.0.1.50:27017/app?authSource=admin"
    );
    assert_eq!(state.new_conn_username, "dbadmin");
    assert_eq!(state.new_conn_password, "mongoSecret");
    assert!(state.new_conn_save_password);
    assert!(state.new_conn_ssh_enabled);
    assert_eq!(state.new_conn_ssh_endpoint, "bastion.internal:2222");
    assert_eq!(state.new_conn_ssh_username, "ops");
    assert_eq!(state.new_conn_ssh_auth_method, SshAuthMethod::PrivateKey);
    assert_eq!(
        state.new_conn_ssh_key_path,
        SshTunnelConfig::DEFAULT_PRIVATE_KEY_PATH
    );
    assert!(state.new_conn_ssh_secret.is_empty());
    assert!(state.new_conn_ssh_save_secret);

    // Act: build_ssh_tunnel_config preserves previous_has_saved when save toggle remains checked
    state.new_conn_ssh_endpoint = "  bastion.internal:2222  ".to_string();
    state.new_conn_ssh_username = "  ops  ".to_string();
    state.new_conn_ssh_key_path = "  ~/.ssh/custom_ecdsa  ".to_string();
    let built_preserved = state.build_ssh_tunnel_config();
    assert_eq!(
        built_preserved,
        SshTunnelConfig {
            enabled: true,
            endpoint: "bastion.internal:2222".to_string(),
            username: "ops".to_string(),
            auth_method: SshAuthMethod::PrivateKey,
            private_key_path: "~/.ssh/custom_ecdsa".to_string(),
            has_saved_secret: true,
        }
    );

    // Unchecking save clears has_saved_secret even if previous_has_saved was true
    state.new_conn_ssh_save_secret = false;
    assert!(!state.build_ssh_tunnel_config().has_saved_secret);

    // Explicit build_ssh_tunnel_config_with_saved with a new secret & save enabled
    state.new_conn_ssh_save_secret = true;
    state.new_conn_ssh_secret = "new-passphrase".to_string();
    assert!(
        state
            .build_ssh_tunnel_config_with_saved(false)
            .has_saved_secret
    );

    // Disabling SSH forces has_saved_secret to false
    state.new_conn_ssh_enabled = false;
    assert!(
        !state
            .build_ssh_tunnel_config_with_saved(true)
            .has_saved_secret
    );

    // Act: reset_connection_modal_fields restores clean defaults
    state.test_conn_status = Some(Ok("Connected".to_string()));
    state.is_testing_connection = true;
    state.reset_connection_modal_fields();

    // Assert
    assert_eq!(state.editing_connection_idx, None);
    assert_eq!(state.new_conn_name, "Production ReplicaSet");
    assert_eq!(state.new_conn_uri, "mongodb://localhost:27017");
    assert!(state.new_conn_username.is_empty());
    assert!(state.new_conn_password.is_empty());
    assert!(!state.new_conn_save_password);
    assert!(!state.new_conn_ssh_enabled);
    assert!(state.new_conn_ssh_endpoint.is_empty());
    assert!(state.new_conn_ssh_username.is_empty());
    assert_eq!(state.new_conn_ssh_auth_method, SshAuthMethod::Password);
    assert_eq!(
        state.new_conn_ssh_key_path,
        SshTunnelConfig::DEFAULT_PRIVATE_KEY_PATH
    );
    assert!(state.new_conn_ssh_secret.is_empty());
    assert!(!state.new_conn_ssh_save_secret);
    assert_eq!(state.test_conn_status, None);
    assert!(!state.is_testing_connection);
}

#[test]
fn connection_config_legacy_deserialization_and_secret_free_profile_json_roundtrip() {
    // Arrange: legacy JSON missing ssh_tunnel and newer auth fields
    let legacy_json = json!({
        "id": "00000000-0000-0000-0000-000000000099",
        "name": "Legacy Profile",
        "uri": "mongodb://localhost:27017",
        "host": "localhost:27017",
        "status": "Disconnected",
        "expanded": false,
        "databases": []
    })
    .to_string();

    // Act
    let legacy_conn: ConnectionConfig =
        serde_json::from_str(&legacy_json).expect("legacy ConnectionConfig should deserialize");

    // Assert
    assert_eq!(legacy_conn.name, "Legacy Profile");
    assert_eq!(legacy_conn.ssh_tunnel, SshTunnelConfig::default());
    assert!(!legacy_conn.ssh_tunnel.enabled);
    assert_eq!(legacy_conn.ssh_tunnel.auth_method, SshAuthMethod::Password);
    assert!(legacy_conn.is_admin);
    assert!(legacy_conn.users_expanded);

    // Arrange: profile with inline MongoDB password + SSH tunnel metadata (simulating save_profiles sanitization)
    let mut profile = ConnectionConfig::new_profile(
        "Tunnel Cluster",
        "mongodb://appuser:ultra_secret_mongo_pw@10.20.0.15:27017/prod?authSource=admin",
    )
    .expect("valid profile");
    profile.status = ConnectionStatus::Connected;
    profile.has_saved_password = true;
    profile.ssh_tunnel = SshTunnelConfig {
        enabled: true,
        endpoint: "203.0.113.10:2222".to_string(),
        username: "bastion_user".to_string(),
        auth_method: SshAuthMethod::PrivateKey,
        private_key_path: "~/.ssh/id_ecdsa".to_string(),
        has_saved_secret: true,
    };

    // Act: apply exact sanitization used by ConnectionManager::save_profiles
    let mut sanitized = profile.clone();
    sanitized.clear_session_password();
    let json_text =
        serde_json::to_string_pretty(&vec![sanitized]).expect("profile list should serialize");

    // Assert: neither MongoDB password nor SSH secret leaks into JSON
    assert!(!json_text.contains("ultra_secret_mongo_pw"));
    assert!(!json_text.contains("ssh_secret"));

    let restored_list: Vec<ConnectionConfig> =
        serde_json::from_str(&json_text).expect("sanitized profile list should deserialize");
    assert_eq!(restored_list.len(), 1);
    let restored = &restored_list[0];
    assert_eq!(
        restored.uri,
        "mongodb://10.20.0.15:27017/prod?authSource=admin"
    );
    assert_eq!(restored.username, "appuser");
    assert!(restored.has_saved_password);
    assert_eq!(restored.ssh_tunnel, profile.ssh_tunnel);

    // Assert: profiles_file_path points to connections.json if available
    if let Some(path) = ConnectionManager::profiles_file_path() {
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("connections.json")
        );
    }

    // Assert: SSH AppEvent variants construct & match cleanly
    let add_evt = AppEvent::AddConnection {
        name: "SSH Profile".to_string(),
        uri: "mongodb://10.0.0.2:27017".to_string(),
        ssh_tunnel: profile.ssh_tunnel.clone(),
        ssh_secret: "ephemeral-passphrase".to_string(),
    };
    let secret_evt = AppEvent::SshSecretLoaded {
        index: 0,
        open_edit_modal: true,
        result: Ok(Some("loaded-secret".to_string())),
    };
    assert!(matches!(add_evt, AppEvent::AddConnection { .. }));
    assert!(matches!(
        secret_evt,
        AppEvent::SshSecretLoaded {
            open_edit_modal: true,
            result: Ok(Some(_)),
            ..
        }
    ));
}

#[test]
fn app_state_empty_clean_state_and_has_open_tabs() {
    // Arrange: Create clean empty app state
    let empty_state = AppState::new_empty();

    // Assert: connections and tabs are completely empty
    assert!(empty_state.connections.is_empty());
    assert!(!empty_state.has_open_tabs());

    // Act: add one collection tab to empty state
    let mut state_with_tab = empty_state;
    state_with_tab.open_or_focus_collection("Custom DB", "app_db", "orders");

    // Assert: has_open_tabs is now true
    assert!(state_with_tab.has_open_tabs());
    let persisted = state_with_tab.to_persistence();
    assert_eq!(persisted.open_tabs.len(), 1);
    assert_eq!(persisted.open_tabs[0].collection, "orders");
}

#[test]
fn test_distinct_users_same_username_different_auth_db() {
    // Arrange: Verify tabs for users sharing the same username on different auth databases
    let tab_account = CollectionTab::new_user("ncert-dev", "ncert", "ncert-account");
    let tab_ims = CollectionTab::new_user("ncert-dev", "ncert", "ncert-ims");

    assert_eq!(tab_account.title(), "ncert (ncert-account)");
    assert_eq!(tab_ims.title(), "ncert (ncert-ims)");
    assert_eq!(
        tab_account.full_namespace(),
        "ncert-account.system.users.ncert"
    );
    assert_eq!(tab_ims.full_namespace(), "ncert-ims.system.users.ncert");
    assert_ne!(tab_account, tab_ims);

    // Arrange: State with 2 users sharing username "ncert" across different databases
    let mut state = AppState::new_empty();
    let mut conn = ConnectionConfig::from_uri("ncert-dev", "mongodb://localhost:27017")
        .expect("valid connection config");
    let user_account = MongoUser {
        id: "ncert-account.ncert".to_string(),
        username: "ncert".to_string(),
        db: "ncert-account".to_string(),
        roles: vec![UserRole::new("readWrite", "ncert-account")],
    };
    let user_ims = MongoUser {
        id: "ncert-ims.ncert".to_string(),
        username: "ncert".to_string(),
        db: "ncert-ims".to_string(),
        roles: vec![UserRole::new("readWrite", "ncert-ims")],
    };
    conn.users = vec![user_account, user_ims];
    state.connections.push(conn);

    // Act: Open both users in state
    state.open_or_focus_user("ncert-dev", "ncert", "ncert-account");
    assert_eq!(state.to_persistence().open_tabs.len(), 1);

    state.open_or_focus_user("ncert-dev", "ncert", "ncert-ims");
    assert_eq!(state.to_persistence().open_tabs.len(), 2);

    // Re-focusing existing user does not duplicate tab
    state.open_or_focus_user("ncert-dev", "ncert", "ncert-account");
    assert_eq!(state.to_persistence().open_tabs.len(), 2);

    // Act: Lookup users by (username, user_db)
    let found_account = state
        .find_user("ncert-dev", "ncert", "ncert-account")
        .expect("account user found");
    assert_eq!(found_account.db, "ncert-account");

    let found_ims = state
        .find_user("ncert-dev", "ncert", "ncert-ims")
        .expect("ims user found");
    assert_eq!(found_ims.db, "ncert-ims");
}

#[test]
fn test_workspace_organization_backward_compatibility_and_defaults() {
    // 1. Deserializing legacy connections.json (without workspace fields)
    let legacy_json = json!({
        "id": "11111111-1111-1111-1111-111111111111",
        "name": "Legacy Localhost",
        "uri": "mongodb://localhost:27017",
        "host": "localhost:27017",
        "status": "Disconnected",
        "expanded": false,
        "databases": []
    })
    .to_string();

    let config: ConnectionConfig =
        serde_json::from_str(&legacy_json).expect("deserialize legacy json");
    assert_eq!(config.name, "Legacy Localhost");
    assert_eq!(config.workspace, None);
    assert_eq!(config.folder, None);
    assert_eq!(config.environment, None);
    assert!(!config.is_favorite);
    assert_eq!(config.effective_environment(), None);
    assert_eq!(config.workspace_name(), None);
    assert_eq!(config.folder_name(), None);

    // 2. Serializing and deserializing modern connection with workspace fields
    let mut modern = ConnectionConfig::new_profile("app-prod", "mongodb://10.0.0.1:27017")
        .expect("new profile app-prod");
    modern.workspace = Some("Customer Alpha".to_string());
    modern.folder = Some("Backend Services".to_string());
    modern.is_favorite = true;
    modern.environment = Some(EnvironmentTag::Prod);

    let serialized = serde_json::to_string(&modern).expect("serialize modern");
    assert!(serialized.contains("\"workspace\":\"Customer Alpha\""));
    assert!(serialized.contains("\"folder\":\"Backend Services\""));
    assert!(serialized.contains("\"is_favorite\":true"));
    assert!(serialized.contains("\"environment\":\"PROD\""));

    let deserialized: ConnectionConfig =
        serde_json::from_str(&serialized).expect("deserialize modern");
    assert_eq!(deserialized.workspace_name(), Some("Customer Alpha"));
    assert_eq!(deserialized.folder_name(), Some("Backend Services"));
    assert!(deserialized.is_favorite);
    assert_eq!(
        deserialized.effective_environment(),
        Some(EnvironmentTag::Prod)
    );
}

#[test]
fn test_workspace_environment_tag_detection_and_badge_colors() {
    // 1. Detection patterns
    assert_eq!(
        EnvironmentTag::detect_from_name("billing-prod"),
        Some(EnvironmentTag::Prod)
    );
    assert_eq!(
        EnvironmentTag::detect_from_name("auth_production"),
        Some(EnvironmentTag::Prod)
    );
    assert_eq!(
        EnvironmentTag::detect_from_name("api-staging"),
        Some(EnvironmentTag::Test)
    );
    assert_eq!(
        EnvironmentTag::detect_from_name("gateway-uat"),
        Some(EnvironmentTag::Test)
    );
    assert_eq!(
        EnvironmentTag::detect_from_name("users-test"),
        Some(EnvironmentTag::Test)
    );
    assert_eq!(
        EnvironmentTag::detect_from_name("erp-dev"),
        Some(EnvironmentTag::Dev)
    );
    assert_eq!(
        EnvironmentTag::detect_from_name("local_development"),
        Some(EnvironmentTag::Dev)
    );
    assert_eq!(
        EnvironmentTag::detect_from_name("sandbox-local"),
        Some(EnvironmentTag::Dev)
    );
    assert_eq!(EnvironmentTag::detect_from_name("unnamed_mongo"), None);

    // 2. Badge colors in dark and light modes
    for is_dark in [true, false] {
        let (bg_dev, text_dev, border_dev) = EnvironmentTag::Dev.badge_colors(is_dark);
        assert!(bg_dev.a() > 0);
        assert!(text_dev.a() > 0);
        assert!(border_dev.a() > 0);

        let (bg_prod, text_prod, border_prod) = EnvironmentTag::Prod.badge_colors(is_dark);
        assert!(bg_prod.a() > 0);
        assert!(text_prod.a() > 0);
        assert!(border_prod.a() > 0);

        let (bg_test, text_test, border_test) = EnvironmentTag::Test.badge_colors(is_dark);
        assert!(bg_test.a() > 0);
        assert!(text_test.a() > 0);
        assert!(border_test.a() > 0);
    }
}

#[test]
fn test_workspace_tree_grouping_and_filtering_lifecycle() {
    let mut c1 = ConnectionConfig::new_profile("erp-dev", "mongodb://localhost:27017")
        .expect("erp-dev profile");
    c1.workspace = Some("Customer A".to_string());
    c1.folder = Some("ERP".to_string());
    c1.is_favorite = true;

    let mut c2 = ConnectionConfig::new_profile("erp-prod", "mongodb://10.0.0.2:27017")
        .expect("erp-prod profile");
    c2.workspace = Some("Customer A".to_string());
    c2.folder = Some("ERP".to_string());
    c2.is_favorite = false;

    let mut c3 = ConnectionConfig::new_profile("ecom-stage", "mongodb://10.0.0.3:27017")
        .expect("ecom-stage profile");
    c3.workspace = Some("Customer A".to_string());
    c3.folder = Some("E-Commerce".to_string());
    c3.is_favorite = false;

    let mut c4 = ConnectionConfig::new_profile("analytics-prod", "mongodb://10.0.0.4:27017")
        .expect("analytics-prod profile");
    c4.workspace = Some("Customer B".to_string());
    c4.folder = None; // Standalone connection under Customer B
    c4.is_favorite = true;

    let mut c5 = ConnectionConfig::new_profile("sandbox-raw", "mongodb://127.0.0.1:27019")
        .expect("sandbox-raw profile");
    c5.workspace = None; // Unassigned
    c5.folder = None;
    c5.is_favorite = false;

    let connections = vec![c1.clone(), c2.clone(), c3.clone(), c4.clone(), c5.clone()];

    // 1. Build full tree
    let full_tree = WorkspaceTree::build(&connections, "", None);
    assert_eq!(full_tree.workspaces.len(), 2);
    assert_eq!(full_tree.workspaces[0].name, "Customer A");
    assert_eq!(full_tree.workspaces[0].folders.len(), 2);
    assert_eq!(full_tree.workspaces[0].folders[0].name, "E-Commerce");
    assert_eq!(
        full_tree.workspaces[0].folders[0].connection_ids,
        vec![c3.id]
    );
    assert_eq!(full_tree.workspaces[0].folders[1].name, "ERP");
    assert_eq!(
        full_tree.workspaces[0].folders[1].connection_ids,
        vec![c1.id, c2.id]
    );

    assert_eq!(full_tree.workspaces[1].name, "Customer B");
    assert_eq!(full_tree.workspaces[1].standalone_connections, vec![c4.id]);
    assert_eq!(full_tree.unassigned_connections, vec![c5.id]);
    assert_eq!(full_tree.total_connections(), 5);

    // 2. Extract helpers
    let workspaces = WorkspaceTree::extract_all_workspaces(&connections);
    assert_eq!(workspaces, vec!["Customer A", "Customer B"]);

    let cust_a_folders = WorkspaceTree::extract_folders_for_workspace(&connections, "Customer A");
    assert_eq!(cust_a_folders, vec!["E-Commerce", "ERP"]);

    // 3. Search filtering by environment tag
    let prod_tree = WorkspaceTree::build(&connections, "PROD", None);
    assert_eq!(prod_tree.total_connections(), 2); // erp-prod, analytics-prod
    assert_eq!(prod_tree.workspaces.len(), 2);

    // 4. Filter by active workspace
    let filtered_tree = WorkspaceTree::build(&connections, "", Some("Customer A"));
    assert_eq!(filtered_tree.workspaces.len(), 1);
    assert_eq!(filtered_tree.workspaces[0].name, "Customer A");
    assert_eq!(filtered_tree.total_connections(), 3);
    assert!(filtered_tree.unassigned_connections.is_empty());

    // 5. Quick Access filter: Favorites
    let fav_tree =
        WorkspaceTree::build_filtered(&connections, "", None, QuickAccessFilter::Favorites, &[]);
    assert_eq!(fav_tree.total_connections(), 2); // c1 (erp-dev), c4 (analytics-prod)

    // 6. Quick Access filter: Recent
    let recent_ids = vec![c2.id, c5.id];
    let recent_tree = WorkspaceTree::build_filtered(
        &connections,
        "",
        None,
        QuickAccessFilter::Recent,
        &recent_ids,
    );
    assert_eq!(recent_tree.total_connections(), 2); // c2, c5
}
