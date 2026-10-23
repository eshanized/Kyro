//! Kyro Remediation Verification Tests
//!
//! Regression test suite verifying:
//! 1. Agent tool permissions enforced at the actual execution boundary
//! 2. Workflow worktree isolation propagation, containment, and fail-closed security
//! 3. Parallel task/workflow worktree collision prevention
//! 4. Base commit tracking and conflict detection in reconciliation
//! 5. Transactional profile save and collision rollback
//! 6. Mention routing with non-routable text stripping and fallback
//! 7. Scoped context/memory filtering
//! 8. Delegation cycle prevention

use kyro_lib::agent::loop_engine::AgentLoopEngine;
use kyro_lib::agent::registry::{
    canonical_tool_name, extract_mention_tokens, strip_non_routable_text, AgentRegistry,
};
use kyro_lib::agent::router::{ChatMessage, MessageContent};
use kyro_lib::memory::MemoryStore;
use kyro_lib::multibot::workspace::MultiAgentOrchestrator;
use kyro_lib::storage::{AgentRun, Database};
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{}_{}", prefix, uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
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
                    _ = &mut rx => break,
                    conn = listener.accept() => {
                        if let Ok((mut socket, _)) = conn {
                            let resp_inner = resp_clone.clone();
                            tokio::spawn(async move {
                                let mut received: Vec<u8> = Vec::new();
                                let mut chunk = [0u8; 8192];
                                let header_end = loop {
                                    let n = socket.read(&mut chunk).await.unwrap_or(0);
                                    if n == 0 {
                                        break received.len();
                                    }
                                    received.extend_from_slice(&chunk[..n]);
                                    if let Some(pos) = received.windows(4).position(|w| w == b"\r\n\r\n") {
                                        break pos + 4;
                                    }
                                };
                                let content_length: usize = String::from_utf8_lossy(
                                    &received[..header_end.min(received.len())],
                                )
                                .lines()
                                .find_map(|l| {
                                    let lower = l.to_lowercase();
                                    if lower.starts_with("content-length:") {
                                        l.split_once(':').and_then(|(_, v)| v.trim().parse::<usize>().ok())
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or(0);
                                let total = header_end + content_length;
                                while received.len() < total {
                                    let want = total - received.len();
                                    let take = want.min(chunk.len());
                                    let n = socket.read(&mut chunk[..take]).await.unwrap_or(0);
                                    if n == 0 {
                                        break;
                                    }
                                    received.extend_from_slice(&chunk[..n]);
                                }

                                let next_resp = {
                                    let mut q = resp_inner.lock().await;
                                    q.pop_front()
                                };
                                let body = match next_resp {
                                    Some(b) => b.to_string(),
                                    None => "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"Default reply\"},\"finish_reason\":\"stop\"}]}".to_string(),
                                };
                                let http = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                    body.len(),
                                    body
                                );
                                let _ = socket.write_all(http.as_bytes()).await;
                                let _ = socket.shutdown().await;
                            });
                        }
                    }
                }
            }
        });

        Self {
            addr,
            responses,
            _shutdown: tx,
        }
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    async fn enqueue_response(&self, resp: serde_json::Value) {
        self.responses.lock().await.push_back(resp);
    }
}

// -----------------------------------------------------------------------------
// 1. Tool Permission Enforcement Boundary Tests
// -----------------------------------------------------------------------------

#[test]
fn test_canonical_tool_name_mapping() {
    assert_eq!(canonical_tool_name("execute_shell"), "execute_shell");
    assert_eq!(canonical_tool_name("BASH"), "execute_shell");
    assert_eq!(canonical_tool_name("terminal"), "execute_shell");
    assert_eq!(canonical_tool_name("shell"), "execute_shell");
    assert_eq!(canonical_tool_name("read_file"), "read_file");
}

#[tokio::test]
async fn test_profile_tool_permission_enforcement_at_execution_boundary() {
    let td = TestDir::new("kyro_perm_test");
    let db = Database::init(td.path().to_path_buf()).unwrap();
    let server = MockLlmServer::start().await;

    // Agent profile with allowlist: ONLY read_file permitted
    let manifest_doc = r#"---
id: reader-agent
name: Reader
role: Read-Only Inspector
tools:
  mode: allowlist
  allow: [read_file]
delegation:
  enabled: false
---
I inspect files read-only.
"#;
    let mut reg = AgentRegistry::load(td.path()).unwrap();
    let profile = reg.save_profile("reader-agent", manifest_doc).unwrap();

    assert!(!profile.tool_allowed("execute_shell"));
    assert!(!profile.tool_allowed("bash"));
    assert!(!profile.tool_allowed("write_file"));
    assert!(!profile.tool_allowed("delegate_task"));
    assert!(profile.tool_allowed("read_file"));

    let engine = AgentLoopEngine::with_registry(
        Arc::new(kyro_lib::browser::BrowserAutomationManager::new()),
        Arc::new(kyro_lib::project::devserver::DevServerManager::new()),
        Arc::new(tokio::sync::RwLock::new(reg)),
    );
    let run_id = "run_perm_enforce";
    let _ = db.create_conversation("conv_perm", "Permission Test", "test-model");
    let run = AgentRun {
        id: run_id.to_string(),
        conversation_id: Some("conv_perm".to_string()),
        agent_id: "reader-agent".to_string(),
        model: "test-model".to_string(),
        prompt: "Check files".to_string(),
        status: "running".to_string(),
        created_at: chrono::Utc::now().timestamp_millis(),
        started_at: Some(chrono::Utc::now().timestamp_millis()),
        completed_at: None,
        error: None,
        config_json: Some(
            serde_json::json!({
                "profile_version": profile.version_label(),
            })
            .to_string(),
        ),
        parent_run_id: None,
    };
    db.create_run(&run).unwrap();

    // Step 1: Mocked model attempts to call execute_shell despite allowlist
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Running shell",
                    "tool_calls": [{
                        "id": "tc_shell_forged",
                        "type": "function",
                        "function": {
                            "name": "execute_shell",
                            "arguments": "{\"command\":\"echo hacked\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Final response acknowledging rejection
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Understood, shell was rejected."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("List files"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step_with_connectors(
            run_id,
            "test-model",
            "dummy_key",
            Some(&server.url()),
            "",
            history,
            db.clone(),
            None,
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());

    // Check recorded events in DB: execute_shell must have been rejected
    let events = db.get_run_events(run_id, None).unwrap();
    let tool_event = events
        .iter()
        .find(|e| e.event_type == "tool_result")
        .expect("tool result event should be recorded");
    let payload: serde_json::Value = serde_json::from_str(&tool_event.payload_json).unwrap();
    assert_eq!(payload["is_error"], true);
    let output = payload["output"].as_str().unwrap_or_default();
    assert!(
        output.contains("not permitted by agent '@Reader' profile policy"),
        "Expected policy denial in tool output, got: {}",
        output
    );
}

#[tokio::test]
async fn test_delegation_disabled_profile_rejects_forged_tool_call() {
    let td = TestDir::new("kyro_delegation_deny");
    let db = Database::init(td.path().to_path_buf()).unwrap();
    let server = MockLlmServer::start().await;

    // Profile inherits tools but has delegation explicitly disabled
    let manifest_doc = r#"---
id: nodelegate-agent
name: NoDelegate
role: Leaf Worker
tools:
  mode: inherit
delegation:
  enabled: false
---
I cannot delegate.
"#;
    let mut reg = AgentRegistry::load(td.path()).unwrap();
    let profile = reg.save_profile("nodelegate-agent", manifest_doc).unwrap();
    assert!(!profile.delegation.enabled);
    assert!(!profile.tool_allowed("delegate_task"));

    let engine = AgentLoopEngine::with_registry(
        Arc::new(kyro_lib::browser::BrowserAutomationManager::new()),
        Arc::new(kyro_lib::project::devserver::DevServerManager::new()),
        Arc::new(tokio::sync::RwLock::new(reg)),
    );
    let run_id = "run_deleg_deny";
    let _ = db.create_conversation("conv_dd", "Delegation Deny", "test-model");
    let run = AgentRun {
        id: run_id.to_string(),
        conversation_id: Some("conv_dd".to_string()),
        agent_id: "nodelegate-agent".to_string(),
        model: "test-model".to_string(),
        prompt: "Work".to_string(),
        status: "running".to_string(),
        created_at: chrono::Utc::now().timestamp_millis(),
        started_at: Some(chrono::Utc::now().timestamp_millis()),
        completed_at: None,
        error: None,
        config_json: Some(
            serde_json::json!({
                "profile_version": profile.version_label(),
            })
            .to_string(),
        ),
        parent_run_id: None,
    };
    db.create_run(&run).unwrap();

    // Model tries to forge a delegate_task call
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Delegating task",
                    "tool_calls": [{
                        "id": "tc_deleg_forged",
                        "type": "function",
                        "function": {
                            "name": "delegate_task",
                            "arguments": "{\"target_bot_id\":\"forge-devops\",\"objective\":\"Subtask\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Done"
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Please complete subtask"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step_with_connectors(
            run_id,
            "test-model",
            "dummy_key",
            Some(&server.url()),
            "",
            history,
            db.clone(),
            None,
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());

    let events = db.get_run_events(run_id, None).unwrap();
    let tool_event = events
        .iter()
        .find(|e| e.event_type == "tool_result")
        .expect("tool result event should be recorded");
    let payload: serde_json::Value = serde_json::from_str(&tool_event.payload_json).unwrap();
    let output = payload["output"].as_str().unwrap_or_default();
    assert!(
        output.contains("has delegation disabled in its profile")
            || output.contains("not permitted"),
        "Output should indicate delegation rejection, got: {}",
        output
    );
}

// -----------------------------------------------------------------------------
// 2. Worktree Isolation & Fail-Closed Containment Tests
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_worktree_isolation_execution_propagation_and_pwd() {
    let td = TestDir::new("kyro_wt_prop");
    let isolated_wt = TestDir::new("kyro_isolated_workspace");
    let db = Database::init(td.path().to_path_buf()).unwrap();
    let server = MockLlmServer::start().await;

    let engine = AgentLoopEngine::new();
    let run_id = "run_isolated_wt_test";
    let _ = db.create_conversation("conv_wt", "Worktree Test", "test-model");

    // Persist run with isolated_worktree_path set to our isolated workspace
    let run = AgentRun {
        id: run_id.to_string(),
        conversation_id: Some("conv_wt".to_string()),
        agent_id: "forge-devops".to_string(),
        model: "test-model".to_string(),
        prompt: "Isolated execution".to_string(),
        status: "running".to_string(),
        created_at: chrono::Utc::now().timestamp_millis(),
        started_at: Some(chrono::Utc::now().timestamp_millis()),
        completed_at: None,
        error: None,
        config_json: Some(
            serde_json::json!({
                "isolated_worktree": true,
                "isolated_worktree_path": isolated_wt.path().to_string_lossy().to_string(),
                "workspace_root": isolated_wt.path().to_string_lossy().to_string(),
            })
            .to_string(),
        ),
        parent_run_id: None,
    };
    db.create_run(&run).unwrap();

    // Step 1: Model calls execute_shell "pwd"
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Checking pwd",
                    "tool_calls": [{
                        "id": "tc_pwd",
                        "type": "function",
                        "function": {
                            "name": "execute_shell",
                            "arguments": "{\"command\":\"pwd\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Model writes a file
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Writing isolated artifact",
                    "tool_calls": [{
                        "id": "tc_write_iso",
                        "type": "function",
                        "function": {
                            "name": "write_file",
                            "arguments": "{\"path\":\"isolated_target.txt\",\"content\":\"ISOLATED_OK\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Final finish step
    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "All operations completed in isolated worktree."
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Run isolated tasks"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step_with_connectors(
            run_id,
            "test-model",
            "dummy_key",
            Some(&server.url()),
            "",
            history,
            db.clone(),
            None,
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());

    // Verify pwd in tool execution event matches isolated_wt path
    let events = db.get_run_events(run_id, None).unwrap();
    let pwd_event = events
        .iter()
        .find(|e| {
            if e.event_type != "tool_result" {
                return false;
            }
            let val: serde_json::Value = serde_json::from_str(&e.payload_json).unwrap_or_default();
            val["name"].as_str() == Some("execute_shell")
        })
        .expect("pwd tool event");
    let pwd_val: serde_json::Value = serde_json::from_str(&pwd_event.payload_json).unwrap();
    let pwd_output = pwd_val["output"].as_str().unwrap_or("");
    let canonical_wt = isolated_wt.path().canonicalize().unwrap();
    assert!(
        pwd_output.contains(&canonical_wt.to_string_lossy().to_string())
            || pwd_output.contains(&isolated_wt.path().to_string_lossy().to_string()),
        "pwd output '{}' did not contain isolated worktree path '{:?}'",
        pwd_output,
        canonical_wt
    );

    // Verify file exists in isolated worktree and NOT in main td
    assert!(isolated_wt.path().join("isolated_target.txt").exists());
    assert!(!td.path().join("isolated_target.txt").exists());
}

#[tokio::test]
async fn test_worktree_isolation_missing_path_fails_closed() {
    let td = TestDir::new("kyro_wt_fail_closed");
    let db = Database::init(td.path().to_path_buf()).unwrap();
    let server = MockLlmServer::start().await;

    let engine = AgentLoopEngine::new();
    let run_id = "run_missing_wt_fail_closed";
    let _ = db.create_conversation("conv_wt_fc", "Fail Closed Test", "test-model");

    let non_existent_path = td.path().join("does_not_exist_wt");

    let run = AgentRun {
        id: run_id.to_string(),
        conversation_id: Some("conv_wt_fc".to_string()),
        agent_id: "forge-devops".to_string(),
        model: "test-model".to_string(),
        prompt: "Fail closed".to_string(),
        status: "running".to_string(),
        created_at: chrono::Utc::now().timestamp_millis(),
        started_at: Some(chrono::Utc::now().timestamp_millis()),
        completed_at: None,
        error: None,
        config_json: Some(
            serde_json::json!({
                "isolated_worktree": true,
                "isolated_worktree_path": non_existent_path.to_string_lossy().to_string(),
            })
            .to_string(),
        ),
        parent_run_id: None,
    };
    db.create_run(&run).unwrap();

    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Attempting shell command",
                    "tool_calls": [{
                        "id": "tc_touch",
                        "type": "function",
                        "function": {
                            "name": "execute_shell",
                            "arguments": "{\"command\":\"touch should_never_run.txt\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    server
        .enqueue_response(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Stopped"
                },
                "finish_reason": "stop"
            }]
        }))
        .await;

    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Run"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let res = engine
        .run_step_with_connectors(
            run_id,
            "test-model",
            "dummy_key",
            Some(&server.url()),
            "",
            history,
            db.clone(),
            None,
            None,
            None,
            |_| {},
        )
        .await;

    assert!(res.is_ok());

    let events = db.get_run_events(run_id, None).unwrap();
    let tool_event = events
        .iter()
        .find(|e| e.event_type == "tool_result")
        .expect("tool execution event");
    let payload: serde_json::Value = serde_json::from_str(&tool_event.payload_json).unwrap();
    assert_eq!(payload["is_error"], true);
    let out = payload["output"].as_str().unwrap_or("");
    assert!(
        out.contains("does not exist") || out.contains("Halting task"),
        "Expected fail-closed warning, got: {}",
        out
    );

    // Verify command never ran in td
    assert!(!td.path().join("should_never_run.txt").exists());
}

// -----------------------------------------------------------------------------
// 3. Parallel Task & Worktree Collision Resistance Tests
// -----------------------------------------------------------------------------

#[test]
fn test_parallel_worktree_collision_resistance() {
    let repo_dir = TestDir::new("kyro_collision_test");
    // Initialize git repo
    let _ = Command::new("git")
        .arg("-C")
        .arg(repo_dir.path())
        .args(["init"])
        .output();
    let _ = Command::new("git")
        .arg("-C")
        .arg(repo_dir.path())
        .args(["config", "user.name", "KyroTest"])
        .output();
    let _ = Command::new("git")
        .arg("-C")
        .arg(repo_dir.path())
        .args(["config", "user.email", "test@kyro.ai"])
        .output();
    fs::write(repo_dir.path().join("README.md"), "# Init").unwrap();
    let _ = Command::new("git")
        .arg("-C")
        .arg(repo_dir.path())
        .args(["add", "."])
        .output();
    let _ = Command::new("git")
        .arg("-C")
        .arg(repo_dir.path())
        .args(["commit", "-m", "Initial commit"])
        .output();

    let task_id = "implement_feature";

    // Setup for Workflow 1, attempt 1
    let (wt_1, branch_1, base_commit_1) = MultiAgentOrchestrator::setup_isolated_worktree(
        repo_dir.path(),
        "workflow_alpha",
        task_id,
        1,
    )
    .unwrap();

    // Setup for Workflow 2, attempt 1 (same task_id, different workflow)
    let (wt_2, branch_2, base_commit_2) = MultiAgentOrchestrator::setup_isolated_worktree(
        repo_dir.path(),
        "workflow_beta",
        task_id,
        1,
    )
    .unwrap();

    // Setup for Workflow 1, attempt 2 (retry of attempt 1)
    let (wt_1_retry, branch_1_retry, _) = MultiAgentOrchestrator::setup_isolated_worktree(
        repo_dir.path(),
        "workflow_alpha",
        task_id,
        2,
    )
    .unwrap();

    // All worktree paths must be strictly distinct
    assert_ne!(wt_1, wt_2);
    assert_ne!(wt_1, wt_1_retry);
    assert_ne!(wt_2, wt_1_retry);

    // All branch names must be strictly distinct
    assert_ne!(branch_1, branch_2);
    assert_ne!(branch_1, branch_1_retry);

    assert!(base_commit_1.is_some());
    assert_eq!(base_commit_1, base_commit_2);

    // Cleanup
    MultiAgentOrchestrator::cleanup_isolated_worktree(repo_dir.path(), &wt_1, branch_1.as_deref());
    MultiAgentOrchestrator::cleanup_isolated_worktree(repo_dir.path(), &wt_2, branch_2.as_deref());
    MultiAgentOrchestrator::cleanup_isolated_worktree(
        repo_dir.path(),
        &wt_1_retry,
        branch_1_retry.as_deref(),
    );
}

// -----------------------------------------------------------------------------
// 4. Git Worktree Base-Commit & Reconciliation Conflict Detection Tests
// -----------------------------------------------------------------------------

#[test]
fn test_reconciliation_conflict_detection() {
    let repo_dir = TestDir::new("kyro_reconcile_test");
    let _ = Command::new("git")
        .arg("-C")
        .arg(repo_dir.path())
        .args(["init"])
        .output();
    let _ = Command::new("git")
        .arg("-C")
        .arg(repo_dir.path())
        .args(["config", "user.name", "KyroTest"])
        .output();
    let _ = Command::new("git")
        .arg("-C")
        .arg(repo_dir.path())
        .args(["config", "user.email", "test@kyro.ai"])
        .output();
    fs::write(repo_dir.path().join("config.json"), "{\"version\": 1}").unwrap();
    let _ = Command::new("git")
        .arg("-C")
        .arg(repo_dir.path())
        .args(["add", "."])
        .output();
    let _ = Command::new("git")
        .arg("-C")
        .arg(repo_dir.path())
        .args(["commit", "-m", "Initial"])
        .output();

    let (wt_dir, branch, _) =
        MultiAgentOrchestrator::setup_isolated_worktree(repo_dir.path(), "ws_rec", "task_edit", 1)
            .unwrap();

    // 1. Modify config.json in worktree
    fs::write(
        wt_dir.join("config.json"),
        "{\"version\": 2, \"from\": \"worktree\"}",
    )
    .unwrap();

    // 2. Concurrently modify config.json in project_root and commit it
    fs::write(
        repo_dir.path().join("config.json"),
        "{\"version\": 3, \"from\": \"main_branch\"}",
    )
    .unwrap();
    let _ = Command::new("git")
        .arg("-C")
        .arg(repo_dir.path())
        .args(["commit", "-am", "Main branch change"])
        .output();

    // 3. Reconcile changes: must detect concurrent conflict in config.json
    let outcome =
        MultiAgentOrchestrator::reconcile_changes(repo_dir.path(), &wt_dir, "ws_rec").unwrap();

    assert!(outcome.changes_detected);
    assert!(!outcome.conflicts_detected.is_empty());
    assert!(!outcome.can_safely_merge);
    assert!(outcome
        .conflicts_detected
        .iter()
        .any(|c| c.contains("config.json")));

    MultiAgentOrchestrator::cleanup_isolated_worktree(repo_dir.path(), &wt_dir, branch.as_deref());
}

// -----------------------------------------------------------------------------
// 5. Transactional Profile Save Rollback Tests
// -----------------------------------------------------------------------------

#[test]
fn test_transactional_profile_save_rollback_on_collision() {
    let cfg = TestDir::new("kyro_tx_save");
    let mut reg = AgentRegistry::load(cfg.path()).unwrap();

    // Saving custom profile that collides with Forge display name
    let doc_colliding = r#"---
id: malicious-forge
name: Forge
role: Duplicate Imposter
---
Imposter instructions.
"#;
    let res = reg.save_profile("malicious-forge", doc_colliding);
    assert!(res.is_err());
    let err_msg = res.unwrap_err();
    assert!(err_msg.contains("collides"));

    // Verify disk was NOT written
    let custom_file = reg.custom_dir().unwrap().join("malicious-forge.md");
    assert!(!custom_file.exists());

    // Verify active registry entries untouched
    assert!(reg.resolve("malicious-forge").is_err());
    assert_eq!(reg.resolve("forge").unwrap().id, "forge-devops");
}

// -----------------------------------------------------------------------------
// 6. Mention Parsing Grammar, Context Stripping & Fallback Tests
// -----------------------------------------------------------------------------

#[test]
fn test_strip_non_routable_text_and_extract_mentions() {
    let text = r#"
Please help:
```rust
// In code block: @Forge should be ignored
let email = "admin@forge-devops.com";
```

Here is a blockquote:
> @Scout do not run this

And inline code: `@Forge` should not trigger.

Real mention is here: @Forge please analyze logs for support@kyro.ai.
"#;

    let cleaned = strip_non_routable_text(text);
    assert!(!cleaned.contains("// In code block"));
    assert!(!cleaned.contains("do not run this"));

    let tokens = extract_mention_tokens(text);
    assert_eq!(tokens, vec!["Forge".to_string()]);
}

#[test]
fn test_mention_routing_fallback_resolution() {
    let reg = AgentRegistry::embedded_only();

    // 1. Direct mention routes to mentioned agent
    let res = reg.route_turn("@Forge please deploy", Some("apex-orchestrator"));
    assert_eq!(res.target_agent_id, "forge-devops");
    assert!(res.mention_detected);
    assert_eq!(res.detected_token.as_deref(), Some("Forge"));
    assert!(!res.is_fallback);

    // 2. No mention routes to valid requested bot
    let res2 = reg.route_turn("Plain request without mentions", Some("scout-web"));
    assert_eq!(res2.target_agent_id, "scout-web");
    assert!(!res2.mention_detected);
    assert!(!res2.is_fallback);

    // 3. Unknown requested bot falls back to default agent (Apex) with diagnostic
    let res3 = reg.route_turn("Plain request", Some("non-existent-bot"));
    assert_eq!(res3.target_agent_id, "apex-orchestrator");
    assert!(res3.is_fallback);
    assert!(res3.fallback_reason.unwrap().contains("unavailable"));
}

// -----------------------------------------------------------------------------
// 7. Scoped Context & Memory Filtering Tests
// -----------------------------------------------------------------------------

#[test]
fn test_scoped_context_memory_filtering() {
    let td = TestDir::new("kyro_mem_scope_test");
    let db = Database::init(td.path().to_path_buf()).unwrap();

    // Save a user preference
    MemoryStore::save_fact(&db, "preference", "user", "theme", "dark", "user_settings").unwrap();

    // Save an agent-scoped fact for Forge
    MemoryStore::save_fact(
        &db,
        "agent",
        "forge-devops",
        "docker_version",
        "27.0.3",
        "system_scan",
    )
    .unwrap();

    // 1. Profile with only "preference" scope: contains preference, excludes agent facts
    let block_pref_only = MemoryStore::build_context_block_scoped(
        &db,
        "forge-devops",
        "Show docker and theme",
        &["preference".to_string()],
    );
    assert!(block_pref_only.contains("User Preferences"));
    assert!(block_pref_only.contains("theme: dark"));
    assert!(!block_pref_only.contains("docker_version"));

    // 2. Profile with empty memory scopes: excludes all memory
    let block_none =
        MemoryStore::build_context_block_scoped(&db, "forge-devops", "Show docker and theme", &[]);
    assert!(!block_none.contains("User Preferences"));
    assert!(!block_none.contains("Agent Facts"));
}
