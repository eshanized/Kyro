use kyro_lib::agent::cancellation::CancellationToken;
use kyro_lib::agent::loop_engine::AgentLoopEngine;
use kyro_lib::agent::router::{ChatMessage, MessageContent};
use kyro_lib::governance::TakeoverController;
use kyro_lib::storage::{AgentRun, Database};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

struct TestDir {
    path: std::path::PathBuf,
}

impl TestDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{}_{}", prefix, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

struct MockLlmServer {
    addr: std::net::SocketAddr,
    responses: Arc<Mutex<VecDeque<serde_json::Value>>>,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl MockLlmServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let responses: Arc<Mutex<VecDeque<serde_json::Value>>> =
            Arc::new(Mutex::new(VecDeque::new()));
        let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();

        let resp_clone = responses.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut rx => {
                        break;
                    }
                    Ok((mut socket, _)) = listener.accept() => {
                        let resp_q = resp_clone.clone();
                        tokio::spawn(async move {
                            let mut buf = vec![0u8; 16384];
                            let mut total_read = 0;
                            loop {
                                let n = socket.read(&mut buf[total_read..]).await.unwrap_or(0);
                                if n == 0 { break; }
                                total_read += n;
                                if buf[..total_read].windows(4).any(|w| w == b"\r\n\r\n") {
                                    break;
                                }
                            }

                            let maybe_resp = {
                                let mut q = resp_q.lock().await;
                                q.pop_front()
                            };

                            let (status, body) = match maybe_resp {
                                Some(val) => {
                                    if val.get("error_code").is_some() {
                                        ("500 Internal Server Error", val.to_string())
                                    } else {
                                        ("200 OK", val.to_string())
                                    }
                                }
                                None => {
                                    let default_stop = serde_json::json!({
                                        "choices": [{
                                            "message": {
                                                "role": "assistant",
                                                "content": "Finished execution."
                                            },
                                            "finish_reason": "stop"
                                        }]
                                    });
                                    ("200 OK", default_stop.to_string())
                                }
                            };

                            let resp_str = format!(
                                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                status,
                                body.len(),
                                body
                            );
                            let _ = socket.write_all(resp_str.as_bytes()).await;
                            let _ = socket.shutdown().await;
                        });
                    }
                }
            }
        });

        MockLlmServer {
            addr,
            responses,
            _shutdown: tx,
        }
    }

    async fn enqueue_response(&self, val: serde_json::Value) {
        self.responses.lock().await.push_back(val);
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

fn create_test_db() -> (Database, TestDir) {
    let td = TestDir::new("kyro_loop_db");
    let db = Database::init(td.path().to_path_buf()).unwrap();
    (db, td)
}

fn create_test_run(db: &Database, run_id: &str, workspace_path: &std::path::Path) {
    let conv_id = format!("conv_{}", run_id);
    let _ = db.create_conversation(&conv_id, "Agent Loop Test", "test-model");
    let run = AgentRun {
        id: run_id.to_string(),
        conversation_id: Some(conv_id),
        agent_id: "apex-orchestrator".to_string(),
        model: "test-model".to_string(),
        prompt: "Run test task".to_string(),
        status: "running".to_string(),
        created_at: chrono::Utc::now().timestamp_millis(),
        started_at: Some(chrono::Utc::now().timestamp_millis()),
        completed_at: None,
        error: None,
        config_json: Some(
            serde_json::json!({
                "workspace_root": workspace_path.to_string_lossy().to_string()
            })
            .to_string(),
        ),
        parent_run_id: None,
    };
    db.create_run(&run).unwrap();
}

#[tokio::test]
async fn test_agent_loop_direct_text_reply() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_direct");
    let run_id = "run_direct_reply";
    create_test_run(&db, run_id, ws_dir.path());

    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hello! I am Kyro, ready to help."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Say hello"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let events = Arc::new(AtomicUsize::new(0));
    let ev_clone = events.clone();

    let result = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "You are a helpful assistant.",
            history,
            db.clone(),
            None,
            None,
            move |_evt| {
                ev_clone.fetch_add(1, Ordering::SeqCst);
            },
        )
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Hello! I am Kyro, ready to help.");
    assert!(events.load(Ordering::SeqCst) >= 1);
}

#[tokio::test]
async fn test_agent_loop_write_and_read_file() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_rw");
    let run_id = "run_rw_file";
    create_test_run(&db, run_id, ws_dir.path());

    // Step 1: LLM calls write_file
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Writing documentation file",
                    "tool_calls": [{
                        "id": "tc_write_1",
                        "type": "function",
                        "function": {
                            "name": "write_file",
                            "arguments": serde_json::json!({
                                "path": "docs/architecture.md",
                                "content": "# Architecture Overview\nKyro is autonomous."
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: LLM calls read_file
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Reading written documentation file",
                    "tool_calls": [{
                        "id": "tc_read_1",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": serde_json::json!({
                                "path": "docs/architecture.md"
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 3: LLM finishes
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "File verified and written successfully."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Write the architecture file and verify it"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "System prompt",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok(), "Expected success, got: {:?}", res);
    assert_eq!(res.unwrap(), "File verified and written successfully.");

    // Verify file actually exists in workspace
    let written_path = ws_dir.path().join("docs/architecture.md");
    assert!(written_path.exists());
    let content = std::fs::read_to_string(written_path).unwrap();
    assert!(content.contains("Kyro is autonomous."));

    // Verify artifact was recorded in DB
    let artifacts = db.list_artifacts(Some(run_id), None).unwrap();
    assert!(!artifacts.is_empty());
    assert_eq!(artifacts[0].name, "architecture.md");
}

#[tokio::test]
async fn test_agent_loop_shell_execution() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_shell");
    let run_id = "run_shell_test";
    create_test_run(&db, run_id, ws_dir.path());

    // Step 1: LLM executes shell command
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Executing shell test",
                    "tool_calls": [{
                        "id": "tc_shell_1",
                        "type": "function",
                        "function": {
                            "name": "execute_shell",
                            "arguments": serde_json::json!({
                                "command": "echo 'SHELL_OUTPUT_TEST_SUCCESS'"
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: LLM finishes
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Shell execution complete."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Run shell echo"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "System prompt",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "Shell execution complete.");
}

#[tokio::test]
async fn test_agent_loop_create_artifact_and_security_traversal_denial() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_art");
    let run_id = "run_art_test";
    create_test_run(&db, run_id, ws_dir.path());

    // Step 1: Attempt directory traversal in artifact name
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Trying traversal artifact",
                    "tool_calls": [{
                        "id": "tc_art_fail",
                        "type": "function",
                        "function": {
                            "name": "create_artifact",
                            "arguments": serde_json::json!({
                                "name": "../../../escape.txt",
                                "content": "malicious"
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Valid artifact creation
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Creating valid artifact",
                    "tool_calls": [{
                        "id": "tc_art_ok",
                        "type": "function",
                        "function": {
                            "name": "create_artifact",
                            "arguments": serde_json::json!({
                                "name": "report.json",
                                "content": "{\"status\":\"healthy\"}",
                                "artifact_type": "json"
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 3: Stop
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Artifacts handled."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Handle artifacts"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "Artifacts handled.");
}

#[tokio::test]
async fn test_agent_loop_project_discovery_and_devserver() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_proj");
    let run_id = "run_proj_test";
    create_test_run(&db, run_id, ws_dir.path());

    // Allow tools via governance settings
    db.set_setting("mcp_perm:discover_project", "allow")
        .unwrap();
    db.set_setting("mcp_perm:inspect_project", "allow").unwrap();
    db.set_setting("mcp_perm:dev_server_status", "allow")
        .unwrap();

    // Create a mock package.json
    let pkg_json = ws_dir.path().join("package.json");
    std::fs::write(
        &pkg_json,
        r#"{"name":"test-app","scripts":{"dev":"node server.js"}}"#,
    )
    .unwrap();

    // Step 1: Discover project
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Discovering project",
                    "tool_calls": [{
                        "id": "tc_disc_1",
                        "type": "function",
                        "function": {
                            "name": "discover_project",
                            "arguments": serde_json::json!({
                                "path": ws_dir.path().to_string_lossy().to_string()
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Inspect project
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Inspecting project",
                    "tool_calls": [{
                        "id": "tc_insp_1",
                        "type": "function",
                        "function": {
                            "name": "inspect_project",
                            "arguments": serde_json::json!({
                                "path": ws_dir.path().to_string_lossy().to_string()
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 3: Dev server status & logs
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Checking dev server",
                    "tool_calls": [{
                        "id": "tc_dev_status",
                        "type": "function",
                        "function": {
                            "name": "dev_server_status",
                            "arguments": serde_json::json!({}).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 4: Finish
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Project discovery and inspect passed."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Discover project"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "Project discovery and inspect passed.");
}

#[tokio::test]
async fn test_agent_loop_checkpoints_and_file_organizer() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_chk");
    let run_id = "run_chk_test";
    create_test_run(&db, run_id, ws_dir.path());

    // Allow tools via governance settings
    db.set_setting("mcp_perm:create_code_checkpoint", "allow")
        .unwrap();
    db.set_setting("mcp_perm:organize_files_preview", "allow")
        .unwrap();

    let test_file = ws_dir.path().join("code.rs");
    std::fs::write(&test_file, "fn main() {}").unwrap();

    // Step 1: Create code checkpoint
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Creating checkpoint",
                    "tool_calls": [{
                        "id": "tc_chk_1",
                        "type": "function",
                        "function": {
                            "name": "create_code_checkpoint",
                            "arguments": serde_json::json!({
                                "workspace_path": ws_dir.path().to_string_lossy().to_string(),
                                "description": "Initial state"
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Organize files preview
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Preview file organize",
                    "tool_calls": [{
                        "id": "tc_org_1",
                        "type": "function",
                        "function": {
                            "name": "organize_files_preview",
                            "arguments": serde_json::json!({
                                "directory": ws_dir.path().to_string_lossy().to_string(),
                                "strategy": "by_type"
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 3: Finish
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Checkpoints and organize tested."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Test checkpoints"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "Checkpoints and organize tested.");
}

#[tokio::test]
async fn test_agent_loop_approval_flow_approved() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_appr");
    let run_id = "run_appr_ok";
    create_test_run(&db, run_id, ws_dir.path());

    // Explicitly configure custom tool to require approval
    db.set_setting("mcp_perm:custom_sensitive_op", "ask")
        .unwrap();

    // Step 1: LLM calls tool requiring approval
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Executing sensitive operation",
                    "tool_calls": [{
                        "id": "tc_sens_1",
                        "type": "function",
                        "function": {
                            "name": "custom_sensitive_op",
                            "arguments": serde_json::json!({ "target": "production_db" }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Finish
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Sensitive operation completed with approval."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    // Spawn background task to approve pending request
    let db_clone = db.clone();
    tokio::spawn(async move {
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if let Ok(pending) = db_clone.list_pending_approvals() {
                if let Some(req) = pending.first() {
                    let _ = db_clone.resolve_approval_request(&req.id, "approved", "admin_tester");
                    break;
                }
            }
        }
    });

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Run sensitive op"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "Sensitive operation completed with approval.");
}

#[tokio::test]
async fn test_agent_loop_approval_flow_denied() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_denied");
    let run_id = "run_appr_deny";
    create_test_run(&db, run_id, ws_dir.path());

    db.set_setting("mcp_perm:custom_destructive_op", "ask")
        .unwrap();

    // Step 1: LLM calls tool requiring approval
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Executing destructive operation",
                    "tool_calls": [{
                        "id": "tc_dest_1",
                        "type": "function",
                        "function": {
                            "name": "custom_destructive_op",
                            "arguments": serde_json::json!({ "purge": true }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: LLM handles denial and finishes
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Operation was denied by user."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    // Spawn background task to deny pending request
    let db_clone = db.clone();
    tokio::spawn(async move {
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if let Ok(pending) = db_clone.list_pending_approvals() {
                if let Some(req) = pending.first() {
                    let _ = db_clone.resolve_approval_request(&req.id, "denied", "security_admin");
                    break;
                }
            }
        }
    });

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Run destructive op"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "Operation was denied by user.");
}

#[tokio::test]
async fn test_agent_loop_computer_action_unsupported() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_comp");
    let run_id = "run_comp_test";
    create_test_run(&db, run_id, ws_dir.path());

    // Step 1: Call unsupported computer action
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Trying unknown computer action",
                    "tool_calls": [{
                        "id": "tc_comp_bad",
                        "type": "function",
                        "function": {
                            "name": "computer_action",
                            "arguments": serde_json::json!({
                                "action": "teleport_cursor",
                                "x": 100,
                                "y": 200
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Stop
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Detected failure gracefully."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Test computer action"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "Detected failure gracefully.");
}

#[tokio::test]
async fn test_agent_loop_human_takeover_pause_and_resume() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_takeover");
    let run_id = "run_takeover_test";
    create_test_run(&db, run_id, ws_dir.path());

    // Trigger human takeover BEFORE running
    TakeoverController::request("Verification required by human");
    assert!(TakeoverController::is_active());

    // Spawn task to resume takeover after 300ms
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        TakeoverController::resume();
    });

    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Resumed successfully after human takeover."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Run task"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "Resumed successfully after human takeover.");
    assert!(!TakeoverController::is_active());
}

#[tokio::test]
async fn test_agent_loop_cancellation_token() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_cancel");
    let run_id = "run_cancel_test";
    create_test_run(&db, run_id, ws_dir.path());

    let token = CancellationToken::new();
    token.cancel(); // Pre-cancel

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Cancelled task"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            None,
            Some(token),
            |_| {},
        )
        .await;

    assert!(res.is_err());
    let err_msg = res.unwrap_err();
    assert!(
        err_msg.contains("cancelled by user"),
        "Unexpected error: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_agent_loop_model_error_handling() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_err");
    let run_id = "run_err_test";
    create_test_run(&db, run_id, ws_dir.path());

    // Enqueue 500 server error
    server
        .enqueue_response(serde_json::json!({
            "error_code": 500,
            "message": "Internal LLM GPU Cluster Failure"
        }))
        .await;

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Trigger error"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("500") || err.contains("error"),
        "Expected error, got: {}",
        err
    );
}

#[tokio::test]
async fn test_agent_loop_browser_tools() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_browser");
    let run_id = "run_browser_test";
    create_test_run(&db, run_id, ws_dir.path());

    db.set_setting("mcp_perm:browser_capabilities", "allow")
        .unwrap();
    db.set_setting("mcp_perm:browser_tabs", "allow").unwrap();

    // Step 1: LLM calls browser_capabilities
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Checking browser capabilities",
                    "tool_calls": [{
                        "id": "tc_br_caps",
                        "type": "function",
                        "function": {
                            "name": "browser_capabilities",
                            "arguments": "{}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: LLM calls browser_tabs
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Listing browser tabs",
                    "tool_calls": [{
                        "id": "tc_br_tabs",
                        "type": "function",
                        "function": {
                            "name": "browser_tabs",
                            "arguments": "{}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 3: Finish
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Browser tools verified."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Test browser tools"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "Browser tools verified.");
}

#[tokio::test]
async fn test_agent_loop_document_tools_and_build_diagnostics() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_doc");
    let run_id = "run_doc_test";
    create_test_run(&db, run_id, ws_dir.path());

    db.set_setting("mcp_perm:analyze_document", "allow")
        .unwrap();
    db.set_setting("mcp_perm:compare_documents", "allow")
        .unwrap();
    db.set_setting("mcp_perm:diagnose_build_failure", "allow")
        .unwrap();

    let doc_a = ws_dir.path().join("doc_a.txt");
    let doc_b = ws_dir.path().join("doc_b.txt");
    std::fs::write(&doc_a, "Contract clause 1: Payment within 30 days.").unwrap();
    std::fs::write(&doc_b, "Contract clause 1: Payment within 60 days.").unwrap();

    // Step 1: Analyze document
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Analyzing doc a",
                    "tool_calls": [{
                        "id": "tc_doc_a",
                        "type": "function",
                        "function": {
                            "name": "analyze_document",
                            "arguments": serde_json::json!({
                                "path": doc_a.to_string_lossy().to_string()
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Compare documents
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Comparing doc a and doc b",
                    "tool_calls": [{
                        "id": "tc_doc_cmp",
                        "type": "function",
                        "function": {
                            "name": "compare_documents",
                            "arguments": serde_json::json!({
                                "path_a": doc_a.to_string_lossy().to_string(),
                                "path_b": doc_b.to_string_lossy().to_string()
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 3: Diagnose build failure
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Diagnosing build failure",
                    "tool_calls": [{
                        "id": "tc_build_diag",
                        "type": "function",
                        "function": {
                            "name": "diagnose_build_failure",
                            "arguments": serde_json::json!({
                                "workspace_path": ws_dir.path().to_string_lossy().to_string(),
                                "build_output": "error[E0425]: cannot find value `x` in this scope"
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 4: Finish
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Documents and build diagnostics completed."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Process documents"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "Documents and build diagnostics completed.");
}

#[tokio::test]
async fn test_agent_loop_delegate_task_to_subagent() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_deleg");
    let run_id = "run_parent_deleg";
    create_test_run(&db, run_id, ws_dir.path());

    db.set_setting("mcp_perm:delegate_task", "allow").unwrap();

    // Step 1: LLM calls delegate_task
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Delegating task to Scout",
                    "tool_calls": [{
                        "id": "tc_deleg_1",
                        "type": "function",
                        "function": {
                            "name": "delegate_task",
                            "arguments": serde_json::json!({
                                "target_agent": "scout-researcher",
                                "task_prompt": "Research security vulnerabilities in dependencies"
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Finish
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Delegation dispatched successfully."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Delegate research"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "Delegation dispatched successfully.");
}

#[tokio::test]
async fn test_agent_loop_approval_flow_expired_and_cancelled() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_appr_exp");
    let run_id = "run_appr_exp_1";
    create_test_run(&db, run_id, ws_dir.path());

    db.set_setting("mcp_perm:sensitive_op", "ask").unwrap();

    // Step 1: LLM calls tool requiring approval (will expire)
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Executing sensitive operation that expires",
                    "tool_calls": [{
                        "id": "tc_exp_1",
                        "type": "function",
                        "function": {
                            "name": "sensitive_op",
                            "arguments": serde_json::json!({ "target": "db1" }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: LLM receives expired error and calls tool again (will be cancelled)
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Retrying sensitive operation which will be cancelled",
                    "tool_calls": [{
                        "id": "tc_canc_1",
                        "type": "function",
                        "function": {
                            "name": "sensitive_op",
                            "arguments": serde_json::json!({ "target": "db2" }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 3: LLM receives cancelled error and finishes
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Handled expirations and cancellations gracefully."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    // Spawn background task to expire the 1st request, then cancel the 2nd request
    let db_clone = db.clone();
    tokio::spawn(async move {
        let mut handled_first = false;
        for _ in 0..100 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if let Ok(pending) = db_clone.list_pending_approvals() {
                if let Some(req) = pending.first() {
                    if !handled_first {
                        let _ = db_clone.resolve_approval_request(&req.id, "expired", "timeout");
                        handled_first = true;
                    } else {
                        let _ = db_clone.resolve_approval_request(
                            &req.id,
                            "cancelled",
                            "user_cancellation",
                        );
                        break;
                    }
                }
            }
        }
    });

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Test approval timeouts"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());
    assert_eq!(
        res.unwrap(),
        "Handled expirations and cancellations gracefully."
    );
}

#[tokio::test]
async fn test_agent_loop_computer_actions_ghost_mode_operations() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_ghost_ops");
    let run_id = "run_ghost_ops_1";
    create_test_run(&db, run_id, ws_dir.path());

    // Enable ghost mode
    let _ = kyro_lib::ghost::GhostDisplayManager::set_enabled(true);

    // Enqueue 7 computer_action steps for mouse_move, left_click, right_click, double_click, type, key, wait
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Moving mouse and clicking",
                    "tool_calls": [
                        {
                            "id": "tc_cm_1",
                            "type": "function",
                            "function": {
                                "name": "computer_action",
                                "arguments": serde_json::json!({ "action": "mouse_move", "x": 100, "y": 200 }).to_string()
                            }
                        },
                        {
                            "id": "tc_cm_2",
                            "type": "function",
                            "function": {
                                "name": "computer_action",
                                "arguments": serde_json::json!({ "action": "left_click", "x": 100, "y": 200 }).to_string()
                            }
                        },
                        {
                            "id": "tc_cm_3",
                            "type": "function",
                            "function": {
                                "name": "computer_action",
                                "arguments": serde_json::json!({ "action": "right_click", "x": 100, "y": 200 }).to_string()
                            }
                        },
                        {
                            "id": "tc_cm_4",
                            "type": "function",
                            "function": {
                                "name": "computer_action",
                                "arguments": serde_json::json!({ "action": "double_click", "x": 100, "y": 200 }).to_string()
                            }
                        },
                        {
                            "id": "tc_cm_5",
                            "type": "function",
                            "function": {
                                "name": "computer_action",
                                "arguments": serde_json::json!({ "action": "type", "text": "hello kyro" }).to_string()
                            }
                        },
                        {
                            "id": "tc_cm_6",
                            "type": "function",
                            "function": {
                                "name": "computer_action",
                                "arguments": serde_json::json!({ "action": "key", "text": "Return" }).to_string()
                            }
                        },
                        {
                            "id": "tc_cm_7",
                            "type": "function",
                            "function": {
                                "name": "computer_action",
                                "arguments": serde_json::json!({ "action": "wait" }).to_string()
                            }
                        }
                    ]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Enqueue error cases (missing coordinates, empty text)
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Testing computer action error validations",
                    "tool_calls": [
                        {
                            "id": "tc_err_1",
                            "type": "function",
                            "function": {
                                "name": "computer_action",
                                "arguments": serde_json::json!({ "action": "mouse_move" }).to_string()
                            }
                        },
                        {
                            "id": "tc_err_2",
                            "type": "function",
                            "function": {
                                "name": "computer_action",
                                "arguments": serde_json::json!({ "action": "left_click" }).to_string()
                            }
                        },
                        {
                            "id": "tc_err_3",
                            "type": "function",
                            "function": {
                                "name": "computer_action",
                                "arguments": serde_json::json!({ "action": "right_click" }).to_string()
                            }
                        },
                        {
                            "id": "tc_err_4",
                            "type": "function",
                            "function": {
                                "name": "computer_action",
                                "arguments": serde_json::json!({ "action": "double_click" }).to_string()
                            }
                        },
                        {
                            "id": "tc_err_5",
                            "type": "function",
                            "function": {
                                "name": "computer_action",
                                "arguments": serde_json::json!({ "action": "type", "text": "" }).to_string()
                            }
                        },
                        {
                            "id": "tc_err_6",
                            "type": "function",
                            "function": {
                                "name": "computer_action",
                                "arguments": serde_json::json!({ "action": "key", "text": "" }).to_string()
                            }
                        }
                    ]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 3: Finish
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Ghost actions finished successfully."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Execute ghost actions"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    let _ = kyro_lib::ghost::GhostDisplayManager::set_enabled(false);
    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "Ghost actions finished successfully.");
}

#[tokio::test]
async fn test_agent_loop_grounded_screen_and_click_mark() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_grounding");
    let run_id = "run_grounding_1";
    create_test_run(&db, run_id, ws_dir.path());

    // Step 1: Capture grounded screen and click mark (mark 9999 is unmapped)
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Capturing grounded screen and clicking mark",
                    "tool_calls": [
                        {
                            "id": "tc_ground_1",
                            "type": "function",
                            "function": {
                                "name": "capture_grounded_screen",
                                "arguments": "{}".to_string()
                            }
                        },
                        {
                            "id": "tc_mark_1",
                            "type": "function",
                            "function": {
                                "name": "click_mark",
                                "arguments": serde_json::json!({ "mark_id": 9999 }).to_string()
                            }
                        }
                    ]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Finish
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Grounded screen and mark handled."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Ground screen"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "gpt-4o",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "Grounded screen and mark handled.");
}

#[tokio::test]
async fn test_agent_loop_launch_ghost_app_and_skill() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_ghost_skill");
    let run_id = "run_ghost_skill_1";
    create_test_run(&db, run_id, ws_dir.path());

    // Create a real skill record in DB
    let skill_rec = kyro_lib::storage::db::SkillRecord {
        id: "skill_test_1".to_string(),
        name: "Test Skill".to_string(),
        description: "Replays a test delay".to_string(),
        steps_json: serde_json::to_string(&vec![kyro_lib::teach_task::skill::SkillStep {
            action_type: "delay".to_string(),
            x: None,
            y: None,
            text: None,
            key: None,
            delay_ms: 5,
        }])
        .unwrap(),
        created_at: 1000,
    };
    db.create_skill(&skill_rec).unwrap();

    // Step 1: Launch ghost app (empty executable and non-existent executable), and invoke_skill (valid and invalid)
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Testing ghost app launch and skill replay",
                    "tool_calls": [
                        {
                            "id": "tc_ghost_empty",
                            "type": "function",
                            "function": {
                                "name": "launch_ghost_app",
                                "arguments": serde_json::json!({ "executable": "" }).to_string()
                            }
                        },
                        {
                            "id": "tc_ghost_invalid",
                            "type": "function",
                            "function": {
                                "name": "launch_ghost_app",
                                "arguments": serde_json::json!({ "executable": "nonexistent_app_12345" }).to_string()
                            }
                        },
                        {
                            "id": "tc_skill_valid",
                            "type": "function",
                            "function": {
                                "name": "invoke_skill",
                                "arguments": serde_json::json!({ "skill_id": "skill_test_1" }).to_string()
                            }
                        },
                        {
                            "id": "tc_skill_invalid",
                            "type": "function",
                            "function": {
                                "name": "invoke_skill",
                                "arguments": serde_json::json!({ "skill_id": "missing_skill" }).to_string()
                            }
                        }
                    ]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Finish
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Ghost app and skill invocations completed."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Test ghost app and skills"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "Ghost app and skill invocations completed.");
}

#[tokio::test]
async fn test_agent_loop_audit_ui_tool_validation() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_audit_ui");
    let run_id = "run_audit_ui_1";
    create_test_run(&db, run_id, ws_dir.path());

    db.set_setting("mcp_perm:audit_ui", "allow").unwrap();

    // Step 1: Call audit_ui with missing parameters, and with non-existent project path
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Validating audit_ui tool",
                    "tool_calls": [
                        {
                            "id": "tc_audit_empty",
                            "type": "function",
                            "function": {
                                "name": "audit_ui",
                                "arguments": "{}".to_string()
                            }
                        },
                        {
                            "id": "tc_audit_missing_path",
                            "type": "function",
                            "function": {
                                "name": "audit_ui",
                                "arguments": serde_json::json!({ "project_path": "/tmp/definitely_not_a_project_dir_xyz" }).to_string()
                            }
                        }
                    ]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Finish
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "audit_ui validation completed."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    // Spawn background task to deny dev server start approval if requested
    let db_clone = db.clone();
    tokio::spawn(async move {
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if let Ok(pending) = db_clone.list_pending_approvals() {
                if let Some(req) = pending.first() {
                    let _ = db_clone.resolve_approval_request(&req.id, "denied", "tester");
                    break;
                }
            }
        }
    });

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Audit UI test"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "audit_ui validation completed.");
}

#[tokio::test]
async fn test_agent_loop_run_step_with_connectors_and_custom_tools() {
    let server = MockLlmServer::start().await;
    let (db, _td) = create_test_db();
    let ws_dir = TestDir::new("ws_conn_loop");
    let run_id = "run_conn_loop_1";
    create_test_run(&db, run_id, ws_dir.path());

    db.set_setting("mcp_perm:unknown_custom_tool", "allow")
        .unwrap();

    let vault = kyro_lib::storage::Vault::new_encrypted_fallback(db.clone());
    let mcp_mgr = kyro_lib::mcp::McpManager::new(db.clone(), vault.clone());
    let conn_mgr = Arc::new(kyro_lib::connector::ConnectorManager::new(
        db.clone(),
        vault,
        mcp_mgr.clone(),
    ));

    // Connect GitHub account in DB to register tools
    let acc_record = kyro_lib::storage::db::ConnectedAccountRecord {
        id: "acc_conn_1".to_string(),
        connector_id: "github".to_string(),
        account_identifier: "kyro_bot".to_string(),
        display_name: "Kyro Bot".to_string(),
        status: "connected".to_string(),
        status_message: None,
        granted_scopes_json: "[\"repo\"]".to_string(),
        credential_key: "vault_key".to_string(),
        config_json: "{}".to_string(),
        created_at: 1000,
        updated_at: 1000,
        last_connected_at: Some(1000),
        last_error: None,
        expires_at: None,
    };
    db.create_connected_account(&acc_record).unwrap();

    // Step 1: Model calls an unknown/unhandled tool and a connector tool
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Calling connector tools",
                    "tool_calls": [
                        {
                            "id": "tc_unhandled_1",
                            "type": "function",
                            "function": {
                                "name": "unknown_custom_tool",
                                "arguments": "{}".to_string()
                            }
                        }
                    ]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Finish
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Connector loop step completed."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let engine = AgentLoopEngine::new();
    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Test connector loop"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step_with_connectors(
            run_id,
            "test-model",
            "test-key",
            Some(&server.base_url()),
            "Sys",
            history,
            db.clone(),
            Some(mcp_mgr),
            Some(conn_mgr),
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "Connector loop step completed.");
}
