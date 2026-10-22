use kyro_lib::browser::{
    BrowserAutomationManager, BrowserOperationResult, BrowserSettings, ElementTarget,
};
use kyro_lib::storage::Database;
use std::path::{Path, PathBuf};

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{}_{}", prefix, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[tokio::test]
async fn test_browser_settings_persistence_and_defaults() {
    let td = TestDir::new("browser_settings_test");
    let db = Database::init(td.path().to_path_buf()).unwrap();

    // 1. Default settings
    let def = BrowserSettings::load(&db);
    assert_eq!(def.selected_browser, "auto");
    assert!(def.headless_default);

    // 2. Custom settings save & load
    let custom = BrowserSettings {
        selected_browser: "firefox".to_string(),
        headless_default: false,
    };
    custom.save(&db).expect("Save settings");

    let loaded = BrowserSettings::load(&db);
    assert_eq!(loaded.selected_browser, "firefox");
    assert!(!loaded.headless_default);
}

#[tokio::test]
async fn test_browser_automation_manager_lifecycle_and_state() {
    let td = TestDir::new("browser_mgr_test");
    let db = Database::init(td.path().to_path_buf()).unwrap();

    let manager = BrowserAutomationManager::new();
    manager.set_database(db.clone());

    // 1. Query settings through manager
    assert_eq!(manager.saved_settings().await.selected_browser, "auto");
    assert!(manager.default_headless().await);

    // 2. Query empty sessions
    let sessions = manager.list_sessions().await;
    assert!(sessions.is_empty());

    // 3. Unknown session lookup
    assert!(manager.get_session("non_existent").await.is_none());

    // 4. Set origin scope on non-existent session
    manager
        .set_origin_scope("non_existent", Some("https://example.com".to_string()))
        .await;

    // 5. Close unknown session succeeds idempotently
    let close_res = manager.close_session("non_existent").await;
    assert!(close_res.is_ok());

    // 6. Shutdown manager
    manager.shutdown().await;
}

#[test]
fn test_element_target_rich_parameters() {
    // Coordinate target
    let target_coords = ElementTarget {
        x: Some(150),
        y: Some(300),
        ..Default::default()
    };
    let params_coords = target_coords.to_params().unwrap();
    assert_eq!(params_coords["x"], 150);
    assert_eq!(params_coords["y"], 300);

    // Semantic target with text, tag, test_id, nth, snapshot_id
    let rich_target = ElementTarget {
        ref_id: Some("e_42".to_string()),
        tag: Some("button".to_string()),
        text: Some("Submit".to_string()),
        test_id: Some("submit-btn".to_string()),
        nth: Some(0),
        snapshot_id: Some("snap_token_99".to_string()),
        ..Default::default()
    };
    let params_rich = rich_target.to_params().unwrap();
    assert_eq!(params_rich["ref"], "e_42");
    assert_eq!(params_rich["tag"], "button");
    assert_eq!(params_rich["text"], "Submit");
    assert_eq!(params_rich["testId"], "submit-btn");
    assert_eq!(params_rich["nth"], 0);
    assert_eq!(params_rich["snapshot_id"], "snap_token_99");

    // Empty target fails with explanatory message
    let empty = ElementTarget::default();
    let err = empty.to_params().unwrap_err();
    assert!(err.contains("at least one of ref, role, name"));
}

#[test]
fn test_browser_operation_result_structure() {
    let res = BrowserOperationResult {
        session_id: "sess_100".to_string(),
        run_id: Some("run_100".to_string()),
        operation: "click".to_string(),
        status: "success".to_string(),
        url: Some("https://kyro.internal".to_string()),
        title: Some("Kyro Dashboard".to_string()),
        error_category: None,
        error_message: None,
        duration_ms: 120,
        navigation_status: Some("complete".to_string()),
        console_error_count: 0,
        failed_request_count: 0,
        data: serde_json::json!({"clicked": true}),
    };

    let serialized = serde_json::to_string(&res).unwrap();
    let deserialized: BrowserOperationResult = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.session_id, "sess_100");
    assert_eq!(deserialized.operation, "click");
    assert_eq!(deserialized.duration_ms, 120);
    assert_eq!(deserialized.status, "success");
}
