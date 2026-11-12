use std::borrow::Cow;

use bson::{doc, oid::ObjectId, Bson, DateTime};
use mongo_studio::core::{AppCommand, AppError, AppEvent, AppState, PendingDeleteDocument};
use mongo_studio::database::DocumentPageResult;
use mongo_studio::models::{CollectionInfo, DocumentRow, MongoUser, UserRole, COMMON_ROLES};
use mongo_studio::theme::{palette, ThemeMode};
use mongo_studio::ui::activity_bar::ActivityView;
use mongo_studio::workspace::{
    BottomPanelState, BottomPanelTab, CollectionSubTab, CollectionTab, DocumentViewMode, LogLevel,
    WorkspacePersistence,
};
use serde_json::{json, Value};

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
    assert!(row_1.cell_display_value("created_at").contains("2023-11-14"));
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
    assert_eq!(row_2.extract_bson_id().expect("Int32 _id"), Bson::Int32(105));
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
