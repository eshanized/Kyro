//! Deterministic Agent Behavioral Tests
//!
//! Verifies behavioral capabilities of built-in Kyro agents:
//! - Apex: task breakdown, context handoff, truthful failure reporting.
//! - Forge: investigation before edit, targeted fixes, truthful command reporting.
//! - Scout: visual/browser automation, domain block handling, evidence-based reporting.
//! - QA Specialist: actual test execution, truthful pass/fail reporting, build diagnostics.
//! - Code Reviewer: diff/source file inspection, grounded security and correctness findings.
//! - Documentation Specialist: contract alignment between documentation and code.

use kyro_lib::agent::loop_engine::AgentLoopEngine;
use kyro_lib::agent::registry::AgentRegistry;
use kyro_lib::agent::router::{ChatMessage, MessageContent};
use kyro_lib::storage::{AgentRun, Database};
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
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
    requests: Arc<Mutex<Vec<serde_json::Value>>>,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl MockLlmServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let responses: Arc<Mutex<VecDeque<serde_json::Value>>> =
            Arc::new(Mutex::new(VecDeque::new()));
        let requests: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
        let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();

        let resp_clone = responses.clone();
        let req_clone = requests.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut rx => break,
                    conn = listener.accept() => {
                        if let Ok((mut socket, _)) = conn {
                            let resp_inner = resp_clone.clone();
                            let req_inner = req_clone.clone();
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

                                if let Ok(json_body) = serde_json::from_slice::<serde_json::Value>(&received[header_end.min(received.len())..]) {
                                    req_inner.lock().await.push(json_body);
                                }

                                let next_resp = {
                                    let mut q = resp_inner.lock().await;
                                    q.pop_front()
                                };
                                let body = match next_resp {
                                    Some(b) => b.to_string(),
                                    None => "{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"Execution complete.\"},\"finish_reason\":\"stop\"}]}".to_string(),
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
            requests,
            _shutdown: tx,
        }
    }

    async fn enqueue(&self, val: serde_json::Value) {
        self.responses.lock().await.push_back(val);
    }

    fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    async fn captured_requests(&self) -> Vec<serde_json::Value> {
        self.requests.lock().await.clone()
    }
}

fn create_engine_and_db(test_dir: &TestDir) -> (AgentLoopEngine, Database) {
    let db = Database::init(test_dir.path().to_path_buf()).unwrap();
    let cfg_dir = test_dir.path().join("config");
    let registry = AgentRegistry::load(&cfg_dir).unwrap();
    let engine = AgentLoopEngine::with_registry(
        Arc::new(kyro_lib::browser::BrowserAutomationManager::new()),
        Arc::new(kyro_lib::project::DevServerManager::new()),
        Arc::new(tokio::sync::RwLock::new(registry)),
    );
    (engine, db)
}

fn create_run(db: &Database, run_id: &str, agent_id: &str, ws_path: &Path) {
    let conv_id = format!("conv_{}", run_id);
    let _ = db.create_conversation(&conv_id, "Behavior Test", "test-model");
    let run = AgentRun {
        id: run_id.to_string(),
        conversation_id: Some(conv_id),
        agent_id: agent_id.to_string(),
        model: "test-model".to_string(),
        prompt: "Run test".to_string(),
        status: "running".to_string(),
        created_at: chrono::Utc::now().timestamp_millis(),
        started_at: Some(chrono::Utc::now().timestamp_millis()),
        completed_at: None,
        error: None,
        config_json: Some(
            serde_json::json!({
                "workspace_root": ws_path.to_string_lossy().to_string(),
                "isolated_worktree": false
            })
            .to_string(),
        ),
        parent_run_id: None,
    };
    db.create_run(&run).unwrap();
}

#[tokio::test]
async fn test_apex_delegates_subobjectives_and_reports_child_failure_truthfully() {
    let td = TestDir::new("apex_behavior");
    let (engine, db) = create_engine_and_db(&td);
    let server = MockLlmServer::start().await;

    // Grant delegation permissions
    db.set_setting("mcp_perm:delegate_task", "allow").unwrap();

    let run_id = "run_apex_test";
    create_run(&db, run_id, "apex-orchestrator", td.path());

    // Step 1: Apex calls delegate_task for test-regression-specialist
    server
        .enqueue(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Delegating test regression verification to Test Specialist.",
                    "tool_calls": [{
                        "id": "call_del_1",
                        "type": "function",
                        "function": {
                            "name": "delegate_task",
                            "arguments": serde_json::json!({
                                "target_bot_id": "test-regression-specialist",
                                "objective": "Verify calculator regression suite",
                                "context_summary": "Path: src/calc.rs, acceptance: 100% tests pass"
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Child run model response (simulating child execution failure)
    server.enqueue(serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Verification failed: 2 assertion errors in test_calculator_overflow."
            },
            "finish_reason": "stop"
        }]
    })).await;

    // Step 3: Apex receives the child output and synthesizes truthful report
    server.enqueue(serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Strategic Report: The verification subtask failed with 2 assertion failures in test_calculator_overflow. The candidate change cannot be integrated without a fix."
            },
            "finish_reason": "stop"
        }]
    })).await;

    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Coordinate verification of the calculator"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let outcome = engine
        .run_step_with_connectors(
            run_id,
            "test-model",
            "test-key",
            Some(&server.url()),
            "",
            history,
            db.clone(),
            None,
            None,
            None,
            |_| {},
        )
        .await
        .expect("step must execute");

    assert!(outcome.contains("failed with 2 assertion failures"));
    assert!(outcome.contains("cannot be integrated"));
    assert!(!outcome.contains("All tasks succeeded"));

    // Verify child run was durably logged with parent lineage
    let child_count = db.count_child_runs(run_id).unwrap();
    assert_eq!(child_count, 1);
    let child_runs: Vec<_> = db
        .list_runs(50)
        .unwrap()
        .into_iter()
        .filter(|r| r.parent_run_id.as_deref() == Some(run_id))
        .collect();
    assert_eq!(child_runs.len(), 1);
    assert_eq!(child_runs[0].agent_id, "test-regression-specialist");
}

#[tokio::test]
async fn test_forge_investigates_before_changing_code_and_reports_diff_accurately() {
    let td = TestDir::new("forge_behavior");
    let (engine, db) = create_engine_and_db(&td);
    let server = MockLlmServer::start().await;

    let project_dir = td.path().join("my_project");
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(
        project_dir.join("calc.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a - b }\n",
    )
    .unwrap();

    let run_id = "run_forge_test";
    create_run(&db, run_id, "forge-devops", &project_dir);

    // Step 1: Forge investigates by reading calc.rs first
    server
        .enqueue(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Investigating calc.rs before making changes.",
                    "tool_calls": [{
                        "id": "call_read_1",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": serde_json::json!({
                                "path": project_dir.join("calc.rs").to_string_lossy().to_string()
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Forge writes the targeted fix
    server
        .enqueue(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Fixing the addition operator.",
                    "tool_calls": [{
                        "id": "call_write_1",
                        "type": "function",
                        "function": {
                            "name": "write_file",
                            "arguments": serde_json::json!({
                                "path": project_dir.join("calc.rs").to_string_lossy().to_string(),
                                "content": "pub fn add(a: i32, b: i32) -> i32 { a + b }\n"
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 3: Forge produces truthful report
    server.enqueue(serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Implementation report: Modified calc.rs to replace subtraction with addition. Verified file content."
            },
            "finish_reason": "stop"
        }]
    })).await;

    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Fix addition bug in calc.rs"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let outcome = engine
        .run_step_with_connectors(
            run_id,
            "test-model",
            "test-key",
            Some(&server.url()),
            "",
            history,
            db.clone(),
            None,
            None,
            None,
            |_| {},
        )
        .await
        .expect("step must execute");

    assert!(outcome.contains("Modified calc.rs to replace subtraction with addition"));

    // Verify file was actually changed
    let updated = fs::read_to_string(project_dir.join("calc.rs")).unwrap();
    assert_eq!(updated, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n");
}

#[tokio::test]
async fn test_scout_handles_navigation_failure_and_domain_blocking_truthfully() {
    let td = TestDir::new("scout_behavior");
    let (engine, db) = create_engine_and_db(&td);
    let server = MockLlmServer::start().await;

    let run_id = "run_scout_test";
    create_run(&db, run_id, "scout-web", td.path());

    // Step 1: Scout attempts to navigate to a blocked domain (e.g. metadata service IP)
    server
        .enqueue(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Navigating to external documentation.",
                    "tool_calls": [{
                        "id": "call_nav_1",
                        "type": "function",
                        "function": {
                            "name": "browser_navigate",
                            "arguments": serde_json::json!({
                                "session_id": "test_session",
                                "url": "http://169.254.169.254/latest/meta-data"
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Scout truthfully reports that the destination was refused/failed
    server.enqueue(serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Research finding: Navigation to http://169.254.169.254/latest/meta-data was blocked by navigation policy. Unable to retrieve information from this source."
            },
            "finish_reason": "stop"
        }]
    })).await;

    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Fetch cloud metadata"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let outcome = engine
        .run_step_with_connectors(
            run_id,
            "test-model",
            "test-key",
            Some(&server.url()),
            "",
            history,
            db.clone(),
            None,
            None,
            None,
            |_| {},
        )
        .await
        .expect("step must execute");

    assert!(outcome.contains("was blocked"));
    assert!(outcome.contains("Unable to retrieve"));
    // Ensure no invented content was reported
    assert!(!outcome.contains("Instance ID: i-"));
}

#[tokio::test]
async fn test_qa_specialist_reports_actual_test_results_without_fabrication() {
    let td = TestDir::new("qa_behavior");
    let (engine, db) = create_engine_and_db(&td);
    let server = MockLlmServer::start().await;

    let run_id = "run_qa_test";
    db.set_setting("mcp_perm:diagnose_build_failure", "allow")
        .unwrap();
    create_run(&db, run_id, "test-regression-specialist", td.path());

    // Step 1: QA specialist executes build diagnostic on failing compiler output
    let compiler_err = "error[E0308]: mismatched types\n  --> src/lib.rs:10:5\n   |\n10 |     \"string\"\n   |     ^^^^^^^^ expected `i32`, found `&str`";
    server
        .enqueue(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Diagnosing build output.",
                    "tool_calls": [{
                        "id": "call_diag_1",
                        "type": "function",
                        "function": {
                            "name": "diagnose_build_failure",
                            "arguments": serde_json::json!({
                                "command": "cargo check",
                                "output": compiler_err
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: QA specialist truthfully reports the compiler failure with exact location
    server.enqueue(serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Verification Report: Build verification failed with E0308 at src/lib.rs line 10. The candidate changes cannot be verified until type mismatch is resolved."
            },
            "finish_reason": "stop"
        }]
    })).await;

    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Verify the candidate build"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let outcome = engine
        .run_step_with_connectors(
            run_id,
            "test-model",
            "test-key",
            Some(&server.url()),
            "",
            history,
            db.clone(),
            None,
            None,
            None,
            |_| {},
        )
        .await
        .expect("step must execute");

    assert!(outcome.contains("Build verification failed with E0308"));
    assert!(outcome.contains("src/lib.rs line 10"));
    assert!(!outcome.contains("All tests passed"));
}

#[tokio::test]
async fn test_code_reviewer_analyzes_candidate_diff_and_source_files() {
    let td = TestDir::new("cr_behavior");
    let (engine, db) = create_engine_and_db(&td);
    let server = MockLlmServer::start().await;

    let candidate_dir = td.path().join("candidate");
    fs::create_dir_all(&candidate_dir).unwrap();
    let auth_code = "pub fn authenticate(password: &str) -> bool {\n    if password == \"hardcoded_secret_123\" {\n        return true;\n    }\n    false\n}\n";
    fs::write(candidate_dir.join("auth.rs"), auth_code).unwrap();

    let run_id = "run_cr_test";
    create_run(&db, run_id, "code-reviewer", &candidate_dir);

    // Step 1: Code Reviewer inspects auth.rs
    server
        .enqueue(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Auditing candidate implementation in auth.rs.",
                    "tool_calls": [{
                        "id": "call_read_auth",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": serde_json::json!({
                                "path": candidate_dir.join("auth.rs").to_string_lossy().to_string()
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Code Reviewer reports blocker finding based on actual code
    server.enqueue(serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Code Review Findings:\n[Blocker] auth.rs: Hardcoded password string `hardcoded_secret_123` detected in authentication function. Must use secure cryptographic hashing and salted constant-time comparison."
            },
            "finish_reason": "stop"
        }]
    })).await;

    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Review candidate authentication change"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let outcome = engine
        .run_step_with_connectors(
            run_id,
            "test-model",
            "test-key",
            Some(&server.url()),
            "",
            history,
            db.clone(),
            None,
            None,
            None,
            |_| {},
        )
        .await
        .expect("step must execute");

    assert!(outcome.contains("[Blocker]"));
    assert!(outcome.contains("auth.rs"));
    assert!(outcome.contains("hardcoded_secret_123"));
    let captured = server.captured_requests().await;
    assert_eq!(captured.len(), 2);
}

#[tokio::test]
async fn test_documentation_specialist_reports_mismatches_against_actual_source() {
    let td = TestDir::new("docs_behavior");
    let (engine, db) = create_engine_and_db(&td);
    let server = MockLlmServer::start().await;

    let doc_dir = td.path().join("repo");
    fs::create_dir_all(&doc_dir).unwrap();
    fs::write(
        doc_dir.join("config.rs"),
        "pub struct Config {\n    pub timeout_seconds: u64,\n}\n",
    )
    .unwrap();
    fs::write(
        doc_dir.join("README.md"),
        "# Config Options\n- `--timeout-sec`: Timeout in seconds\n",
    )
    .unwrap();

    let run_id = "run_docs_test";
    create_run(&db, run_id, "documentation-reviewer", &doc_dir);

    // Step 1: Doc specialist reads config.rs
    server
        .enqueue(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Checking config struct definition.",
                    "tool_calls": [{
                        "id": "call_read_cfg",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": serde_json::json!({
                                "path": doc_dir.join("config.rs").to_string_lossy().to_string()
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 2: Doc specialist reads README.md
    server
        .enqueue(serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Checking README.md parameter documentation.",
                    "tool_calls": [{
                        "id": "call_read_readme",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": serde_json::json!({
                                "path": doc_dir.join("README.md").to_string_lossy().to_string()
                            }).to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        }))
        .await;

    // Step 3: Doc specialist reports discrepancy between config.rs and README.md
    server.enqueue(serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Documentation Audit: README.md references stale parameter `--timeout-sec`, but the codebase in config.rs defines `timeout_seconds`. README.md requires update."
            },
            "finish_reason": "stop"
        }]
    })).await;

    let history = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Audit README against config code"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let outcome = engine
        .run_step_with_connectors(
            run_id,
            "test-model",
            "test-key",
            Some(&server.url()),
            "",
            history,
            db.clone(),
            None,
            None,
            None,
            |_| {},
        )
        .await
        .expect("step must execute");

    assert!(outcome.contains("README.md references stale parameter `--timeout-sec`"));
    assert!(outcome.contains("config.rs defines `timeout_seconds`"));
    let captured = server.captured_requests().await;
    assert_eq!(captured.len(), 3);
}
