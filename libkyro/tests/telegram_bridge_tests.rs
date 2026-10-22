use kyro_lib::agent::loop_engine::AgentLoopEngine;
use kyro_lib::computer_use::InputController;
use kyro_lib::governance::TakeoverController;
use kyro_lib::storage::db::{ApprovalRequest, Database};
use kyro_lib::telegram::bridge::TelegramBridge;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

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

fn spawn_mock_telegram_server() -> (String, Arc<AtomicBool>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Bind local port");
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}", addr.port());
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    let handle = thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        while running_clone.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buffer = [0u8; 4096];
                    let _ = stream.read(&mut buffer);

                    let response_body = json!({
                        "ok": true,
                        "result": {
                            "message_id": 101,
                            "date": 1700000000,
                            "text": "received"
                        }
                    })
                    .to_string();

                    let http_resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    );
                    let _ = stream.write_all(http_resp.as_bytes());
                    let _ = stream.flush();
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });

    (url, running, handle)
}

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn test_telegram_bridge_api_base_and_direct_send() {
    let _guard = ENV_LOCK.lock().unwrap();
    let (url, running, server_handle) = spawn_mock_telegram_server();
    std::env::set_var("KYRO_TELEGRAM_API_BASE", &url);

    assert_eq!(TelegramBridge::api_base(), url);

    let client = reqwest::Client::new();
    let send_res =
        TelegramBridge::send_message(&client, "test_token", "12345", "Hello via test").await;
    assert!(send_res.is_ok(), "send_message failed: {:?}", send_res);

    let photo_res =
        TelegramBridge::send_photo(&client, "test_token", "12345", vec![1, 2, 3, 4]).await;
    assert!(photo_res.is_ok(), "send_photo failed: {:?}", photo_res);

    running.store(false, Ordering::SeqCst);
    let _ = server_handle.join();
}

#[tokio::test]
async fn test_telegram_bridge_fail_closed_unauthorized() {
    let td = TestDir::new("tg_auth_test");
    let db = Database::init(td.path().to_path_buf()).unwrap();
    let engine = Arc::new(AgentLoopEngine::new());
    let client = reqwest::Client::new();

    let update = json!({
        "update_id": 1,
        "message": {
            "chat": { "id": 999999 },
            "text": "/start"
        }
    });

    // Allowed is 12345, update arrives from 999999 -> rejected fail-closed
    let res = TelegramBridge::handle_update(
        &update,
        "dummy_token",
        "12345",
        None,
        &client,
        &db,
        engine,
        None,
    )
    .await;

    assert!(res.is_err(), "Unauthorized message must be rejected");
    assert!(res.unwrap_err().contains("Unauthorized chat_id"));
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn test_telegram_bridge_commands_lifecycle() {
    let _guard = ENV_LOCK.lock().unwrap();
    let (url, running, server_handle) = spawn_mock_telegram_server();
    std::env::set_var("KYRO_TELEGRAM_API_BASE", &url);

    let td = TestDir::new("tg_cmds_test");
    let db = Database::init(td.path().to_path_buf()).unwrap();
    let engine = Arc::new(AgentLoopEngine::new());
    let client = reqwest::Client::new();

    let allowed_chat_id = "555123";

    // 1. Empty message text
    let empty_update = json!({
        "update_id": 2,
        "message": {
            "chat": { "id": 555123 },
            "text": "   "
        }
    });
    let handled_empty = TelegramBridge::handle_update(
        &empty_update,
        "test_token",
        allowed_chat_id,
        None,
        &client,
        &db,
        engine.clone(),
        None,
    )
    .await
    .unwrap();
    assert!(!handled_empty);

    // 2. /start and /help
    let start_update = json!({
        "update_id": 3,
        "message": {
            "chat": { "id": 555123 },
            "text": "/start"
        }
    });
    let handled_start = TelegramBridge::handle_update(
        &start_update,
        "test_token",
        allowed_chat_id,
        None,
        &client,
        &db,
        engine.clone(),
        None,
    )
    .await
    .unwrap();
    assert!(handled_start);

    // 3. /status when idle
    let status_update = json!({
        "update_id": 4,
        "message": {
            "chat": { "id": 555123 },
            "text": "/status"
        }
    });
    let handled_status = TelegramBridge::handle_update(
        &status_update,
        "test_token",
        allowed_chat_id,
        None,
        &client,
        &db,
        engine.clone(),
        None,
    )
    .await
    .unwrap();
    assert!(handled_status);

    // 4. /status with pending approval and active takeover
    TakeoverController::request("Security verification needed");
    let now = chrono::Utc::now().timestamp_millis();
    let appr = ApprovalRequest {
        id: "appr_tg_1".to_string(),
        run_id: "run_tg_active".to_string(),
        tool_name: "write_file".to_string(),
        summary: "Write critical config".to_string(),
        args_json: "{}".to_string(),
        risk_level: "high".to_string(),
        reason: "Modifies config".to_string(),
        status: "pending".to_string(),
        created_at: now,
        expires_at: now + 60_000,
        resolved_at: None,
        resolved_by: None,
    };
    db.create_approval_request(&appr).unwrap();

    let handled_status_takeover = TelegramBridge::handle_update(
        &status_update,
        "test_token",
        allowed_chat_id,
        None,
        &client,
        &db,
        engine.clone(),
        None,
    )
    .await
    .unwrap();
    assert!(handled_status_takeover);

    // 5. /takeover_resume
    let resume_update = json!({
        "update_id": 5,
        "message": {
            "chat": { "id": 555123 },
            "text": "/takeover_resume"
        }
    });
    TelegramBridge::handle_update(
        &resume_update,
        "test_token",
        allowed_chat_id,
        None,
        &client,
        &db,
        engine.clone(),
        None,
    )
    .await
    .unwrap();
    assert!(!TakeoverController::get_status().is_active);

    // 6. /stop
    let stop_update = json!({
        "update_id": 6,
        "message": {
            "chat": { "id": 555123 },
            "text": "/stop"
        }
    });
    TelegramBridge::handle_update(
        &stop_update,
        "test_token",
        allowed_chat_id,
        None,
        &client,
        &db,
        engine.clone(),
        None,
    )
    .await
    .unwrap();
    assert!(InputController::is_aborted());
    InputController::reset_abort();

    // 7. /approve <id>
    let approve_update = json!({
        "update_id": 7,
        "message": {
            "chat": { "id": 555123 },
            "text": "/approve appr_tg_1"
        }
    });
    TelegramBridge::handle_update(
        &approve_update,
        "test_token",
        allowed_chat_id,
        None,
        &client,
        &db,
        engine.clone(),
        None,
    )
    .await
    .unwrap();
    let resolved_appr = db.get_approval_request("appr_tg_1").unwrap().unwrap();
    assert_eq!(resolved_appr.status, "approved");

    // 8. /approve unknown id
    let approve_missing = json!({
        "update_id": 8,
        "message": {
            "chat": { "id": 555123 },
            "text": "/approve missing_appr"
        }
    });
    TelegramBridge::handle_update(
        &approve_missing,
        "test_token",
        allowed_chat_id,
        None,
        &client,
        &db,
        engine.clone(),
        None,
    )
    .await
    .unwrap();

    // 9. /deny <id>
    let appr2 = ApprovalRequest {
        id: "appr_tg_2".to_string(),
        run_id: "run_tg_active".to_string(),
        tool_name: "delete_file".to_string(),
        summary: "Delete critical log".to_string(),
        args_json: "{}".to_string(),
        risk_level: "destructive".to_string(),
        reason: "Dangerous".to_string(),
        status: "pending".to_string(),
        created_at: now,
        expires_at: now + 60_000,
        resolved_at: None,
        resolved_by: None,
    };
    db.create_approval_request(&appr2).unwrap();

    let deny_update = json!({
        "update_id": 9,
        "message": {
            "chat": { "id": 555123 },
            "text": "/deny appr_tg_2"
        }
    });
    TelegramBridge::handle_update(
        &deny_update,
        "test_token",
        allowed_chat_id,
        None,
        &client,
        &db,
        engine.clone(),
        None,
    )
    .await
    .unwrap();
    let resolved_deny = db.get_approval_request("appr_tg_2").unwrap().unwrap();
    assert_eq!(resolved_deny.status, "denied");

    // 10. Task prompt without NVIDIA key returns Ok(false)
    let prompt_update = json!({
        "update_id": 10,
        "message": {
            "chat": { "id": 555123 },
            "text": "Organize my downloads folder"
        }
    });
    let no_key_res = TelegramBridge::handle_update(
        &prompt_update,
        "test_token",
        allowed_chat_id,
        None,
        &client,
        &db,
        engine.clone(),
        None,
    )
    .await
    .unwrap();
    assert!(!no_key_res);

    // 11. Task prompt with NVIDIA key creates run
    let with_key_res = TelegramBridge::handle_update(
        &prompt_update,
        "test_token",
        allowed_chat_id,
        Some("nvapi-test-dummy-key"),
        &client,
        &db,
        engine.clone(),
        None,
    )
    .await
    .unwrap();
    assert!(with_key_res);

    running.store(false, Ordering::SeqCst);
    let _ = server_handle.join();
}
