use kyro_lib::execution::launcher::{
    sanitize_env, validate_args, validate_working_directory, ExecutableResolver, LaunchError,
    ProgramSpec, STRIPPED_ENV_VARS,
};
use kyro_lib::execution::pty::PtySession;
use kyro_lib::mcp::client::McpClient;
use kyro_lib::teach_task::scheduler::RoutineScheduler;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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

#[test]
fn test_executable_resolver_discovery_and_errors() {
    let resolver = ExecutableResolver::new();
    assert!(!resolver.search_dirs().is_empty());

    // Explicit search dirs
    let custom = ExecutableResolver::with_search_dirs(vec![PathBuf::from("/bin")]);
    assert_eq!(custom.search_dirs(), &[PathBuf::from("/bin")]);

    // Empty executable rejected
    let empty_spec = ProgramSpec::new("   ");
    let err_empty = resolver.resolve(&empty_spec).unwrap_err();
    assert!(matches!(err_empty, LaunchError::NotFound { .. }));

    // Null byte in executable rejected
    let null_spec = ProgramSpec::new("echo\0hack");
    let err_null = resolver.resolve(&null_spec).unwrap_err();
    assert!(matches!(err_null, LaunchError::InvalidArguments { .. }));

    // ProgramSpec builder helpers
    let spec = ProgramSpec::new("ls")
        .with_args(vec!["-la"])
        .with_cwd("/tmp");
    assert_eq!(spec.executable, "ls");
    assert_eq!(spec.args, vec!["-la"]);
    assert_eq!(spec.cwd, Some(PathBuf::from("/tmp")));
}

#[test]
fn test_launch_error_display_and_validation() {
    let err1 = LaunchError::NotFound {
        executable: "mybin".to_string(),
        searched: vec!["/usr/bin".to_string(), "/bin".to_string()],
    };
    assert!(err1
        .to_string()
        .contains("Executable 'mybin' was not found"));

    let err2 = LaunchError::NotExecutable {
        path: PathBuf::from("/tmp/doc.txt"),
        reason: "not a binary".to_string(),
    };
    assert!(err2.to_string().contains("is not usable"));

    let err3 = LaunchError::UntrustedLocation {
        path: PathBuf::from("/tmp/script.sh"),
        reason: "world-writable".to_string(),
    };
    assert!(err3.to_string().contains("untrusted location"));

    let err4 = LaunchError::InvalidArguments {
        reason: "null byte".to_string(),
    };
    assert!(err4.to_string().contains("Invalid launch arguments"));

    let err5 = LaunchError::InvalidWorkingDirectory {
        path: PathBuf::from("/bad/dir"),
        reason: "does not exist".to_string(),
    };
    assert!(err5.to_string().contains("Invalid working directory"));

    let err6 = LaunchError::SpawnFailed {
        reason: "out of memory".to_string(),
    };
    assert!(err6.to_string().contains("Failed to spawn process"));

    // Argument limits: >512 entries
    let many_args: Vec<String> = (0..513).map(|i| format!("arg_{}", i)).collect();
    let err_many = validate_args(&many_args).unwrap_err();
    assert!(matches!(err_many, LaunchError::InvalidArguments { .. }));

    // Argument limits: >128 KiB
    let huge_arg = "a".repeat(129 * 1024);
    let err_huge = validate_args(&[huge_arg]).unwrap_err();
    assert!(matches!(err_huge, LaunchError::InvalidArguments { .. }));

    // Valid arguments
    let valid_args = vec!["--flag".to_string(), "val".to_string()];
    let validated = validate_args(&valid_args).unwrap();
    assert_eq!(validated, valid_args);

    // Working directory validation
    let td = TestDir::new("cwd_test");
    assert!(validate_working_directory(td.path()).is_ok());

    let relative_path = Path::new("relative/dir");
    assert!(matches!(
        validate_working_directory(relative_path).unwrap_err(),
        LaunchError::InvalidWorkingDirectory { .. }
    ));

    let dotdot_path = Path::new("/tmp/../tmp");
    assert!(matches!(
        validate_working_directory(dotdot_path).unwrap_err(),
        LaunchError::InvalidWorkingDirectory { .. }
    ));

    let non_dir = td.path().join("file.txt");
    fs::write(&non_dir, "hello").unwrap();
    assert!(matches!(
        validate_working_directory(&non_dir).unwrap_err(),
        LaunchError::InvalidWorkingDirectory { .. }
    ));
}

#[test]
fn test_environment_sanitization_and_stripped_vars() {
    assert!(!STRIPPED_ENV_VARS.is_empty());
    assert!(STRIPPED_ENV_VARS.contains(&"LD_PRELOAD"));
    assert!(STRIPPED_ENV_VARS.contains(&"PYTHONPATH"));
    assert!(STRIPPED_ENV_VARS.contains(&"NODE_OPTIONS"));

    let overrides = vec![
        ("SAFE_VAR".to_string(), "safe_value".to_string()),
        ("LD_PRELOAD".to_string(), "/lib/malicious.so".to_string()),
        ("PYTHONPATH".to_string(), "/bad/py".to_string()),
        ("INVALID=KEY".to_string(), "val".to_string()),
        ("".to_string(), "empty_key".to_string()),
        ("NULL\0KEY".to_string(), "val".to_string()),
        ("KEY".to_string(), "NULL\0VAL".to_string()),
        ("SAFE_VAR".to_string(), "duplicate_ignored".to_string()),
    ];

    let sanitized = sanitize_env(&overrides);
    assert_eq!(sanitized.len(), 1);
    assert_eq!(sanitized[0].0, "SAFE_VAR");
    assert_eq!(sanitized[0].1, "safe_value");
}

#[test]
fn test_pty_session_lifecycle() {
    let output_received = Arc::new(AtomicBool::new(false));
    let out_clone = output_received.clone();

    let pty = PtySession::spawn(move |out| {
        if !out.is_empty() {
            out_clone.store(true, Ordering::SeqCst);
        }
    })
    .expect("Spawn PTY session");

    // Write command
    pty.write(b"echo pty_ok\n").expect("Write to PTY");

    // Resize
    pty.resize(30, 100).expect("Resize PTY");

    // Wait briefly for output
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(output_received.load(Ordering::SeqCst));
}

#[tokio::test]
async fn test_mock_mcp_client_handshake_and_dispatch() {
    let td = TestDir::new("mcp_test");
    let script_path = td.path().join("mock_mcp.py");

    let script_content = r#"
import sys, json

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    req = json.loads(line)
    req_id = req.get("id")
    method = req.get("method")

    if method == "initialize":
        resp = {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "mock-mcp-server", "version": "1.0.0"}
            }
        }
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
    elif method == "notifications/initialized":
        pass
    elif method == "tools/list":
        resp = {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {
                "tools": [
                    {
                        "name": "mock_calculator",
                        "description": "Performs calculations",
                        "inputSchema": {
                            "type": "object",
                            "properties": {"expr": {"type": "string"}},
                            "required": ["expr"]
                        }
                    }
                ]
            }
        }
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
    elif method == "tools/call":
        params = req.get("params", {})
        args = params.get("arguments", {})
        resp = {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {
                "content": [
                    {"type": "text", "text": f"Computed: {args.get('expr')}"}
                ]
            }
        }
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
"#;
    fs::write(&script_path, script_content).unwrap();

    let envs = HashMap::new();
    let mut client = McpClient::spawn(
        "mock_server",
        "python3",
        &[script_path.to_string_lossy().to_string()],
        &envs,
    )
    .await
    .expect("Spawn mock MCP server client");

    assert!(client.is_alive());

    // Test list_tools
    let tools = client.list_tools().await.expect("list_tools succeeds");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "mock_calculator");
    assert_eq!(tools[0].description, "Performs calculations");

    // Test call_tool
    let call_res = client
        .call_tool("mock_calculator", json!({"expr": "7 * 6"}))
        .await
        .expect("call_tool succeeds");
    assert!(call_res.contains("Computed: 7 * 6"));
}

#[test]
fn test_routine_scheduler_parsing_and_timezones() {
    // 5-field cron
    let sched_5 = RoutineScheduler::parse_schedule("0 12 * * *").unwrap();
    assert!(sched_5.after(&chrono::Utc::now()).next().is_some());

    // 6-field cron
    let sched_6 = RoutineScheduler::parse_schedule("0 0 12 * * *").unwrap();
    assert!(sched_6.after(&chrono::Utc::now()).next().is_some());

    // 7-field cron
    let sched_7 = RoutineScheduler::parse_schedule("0 0 12 * * * 2026").unwrap();
    assert_eq!(sched_7.to_string(), "0 0 12 * * * 2026");

    // Invalid field count
    let invalid = RoutineScheduler::parse_schedule("12 * *");
    assert!(invalid.is_err());

    // Extract timezone prefix
    let (c1, tz1) = RoutineScheduler::extract_timezone("CRON_TZ=America/New_York 0 9 * * 1-5");
    assert_eq!(c1, "0 9 * * 1-5");
    assert_eq!(tz1, Some("America/New_York".to_string()));

    let (c2, tz2) = RoutineScheduler::extract_timezone("TZ=Asia/Tokyo 0 12 * * *");
    assert_eq!(c2, "0 12 * * *");
    assert_eq!(tz2, Some("Asia/Tokyo".to_string()));

    let (c3, tz3) = RoutineScheduler::extract_timezone("0 12 * * *");
    assert_eq!(c3, "0 12 * * *");
    assert_eq!(tz3, None);

    // Compute next run with explicit timezone
    let next_tz =
        RoutineScheduler::compute_next_run_with_tz("CRON_TZ=Europe/London 0 12 * * *", None);
    assert!(next_tz.is_some());

    // Running state and concurrency lock
    assert!(!RoutineScheduler::is_routine_running("routine_test_1"));
    RoutineScheduler::set_routine_running_for_test("routine_test_1", true);
    assert!(RoutineScheduler::is_routine_running("routine_test_1"));
    RoutineScheduler::set_routine_running_for_test("routine_test_1", false);
    assert!(!RoutineScheduler::is_routine_running("routine_test_1"));
}

#[tokio::test]
async fn mcp_alias_registry_never_shadows_builtins_or_collides() {
    use kyro_lib::mcp::McpManager;
    use kyro_lib::storage::{Database, McpServerRecord, Vault};

    let td = TestDir::new("mcp_alias");
    let db = Database::init(td.path().join("db")).expect("database");
    let vault = Vault::new_encrypted_fallback(db.clone());
    let manager = McpManager::new(db.clone(), vault);

    // Three local MCP servers over stdio:
    //   * server A exposes a unique benign tool plus a tool whose raw name is
    //     a reserved platform builtin name (`execute_shell`),
    //   * servers B and C BOTH expose the same raw tool name (collision).
    let script = |tools: &str| {
        format!(
            r#"
import sys, json

TOOLS = {tools}

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    req = json.loads(line)
    req_id = req.get("id")
    method = req.get("method")

    if method == "initialize":
        resp = {{
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {{
                "protocolVersion": "2024-11-05",
                "capabilities": {{"tools": {{}}}},
                "serverInfo": {{"name": "alias-mock", "version": "1.0.0"}}
            }}
        }}
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
    elif method == "notifications/initialized":
        pass
    elif method == "tools/list":
        resp = {{"jsonrpc": "2.0", "id": req_id, "result": {{"tools": TOOLS}}}}
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
    elif method == "tools/call":
        params = req.get("params") or {{}}
        resp = {{
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {{
                "content": [{{"type": "text", "text": "ran " + params.get("name", "?")}}],
            }}
        }}
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
"#
        )
    };

    let tool = |name: &str| json!({ "name": name, "description": format!("Mock tool {}", name), "inputSchema": { "type": "object", "properties": {} } });

    let server_a = td.path().join("mock_a.py");
    fs::write(
        &server_a,
        script(
            &json!([
                tool("frobnicate_report"),
                tool("execute_shell"),
                tool("read_file")
            ])
            .to_string(),
        ),
    )
    .unwrap();
    let server_b = td.path().join("mock_b.py");
    fs::write(
        &server_b,
        script(&json!([tool("search_notes")]).to_string()),
    )
    .unwrap();
    let server_c = td.path().join("mock_c.py");
    fs::write(
        &server_c,
        script(&json!([tool("search_notes")]).to_string()),
    )
    .unwrap();

    let record = |id: &str, path: &Path| McpServerRecord {
        id: id.to_string(),
        name: id.to_string(),
        command: "python3".to_string(),
        args_json: json!([path.to_string_lossy().to_string()]).to_string(),
        env_json: "{}".to_string(),
        enabled: true,
    };
    db.create_mcp_server(&record("srv_alias_a", &server_a))
        .unwrap();
    db.create_mcp_server(&record("srv_alias_b", &server_b))
        .unwrap();
    db.create_mcp_server(&record("srv_alias_c", &server_c))
        .unwrap();

    let exposed = manager.get_all_mcp_tools().await;

    let names: Vec<&str> = exposed.iter().map(|t| t.name.as_str()).collect();
    // Everything is reachable under its namespaced identity — including the
    // reserved-name tool and both colliding `search_notes` copies.
    assert!(names.contains(&"mcp__srv_alias_a__frobnicate_report"));
    assert!(
        names.contains(&"mcp__srv_alias_a__execute_shell"),
        "reserved-name tools stay reachable namespaced"
    );
    assert!(names.contains(&"mcp__srv_alias_b__search_notes"));
    assert!(names.contains(&"mcp__srv_alias_c__search_notes"));

    // 1. The unique benign tool IS exposed under its bare alias.
    let benign_alias = manager
        .call_mcp_tool("frobnicate_report", json!({}))
        .await
        .expect("unique tool keeps its bare alias");
    assert!(
        benign_alias.is_ok(),
        "unique tool dispatches through its alias"
    );

    // 2. A raw name that is a platform builtin NEVER becomes an alias, even
    // when unambiguous: builtin dispatch would win the name while connector
    // policy authorized the external tool (Bug 13). The namespaced identity
    // is the only way in.
    assert!(
        manager
            .call_mcp_tool("execute_shell", json!({ "command": "env" }))
            .await
            .is_none(),
        "reserved builtin names must not be circled through external MCP dispatch"
    );

    // 3. A name exposed by two servers NEVER becomes an alias: the alias
    // would silently bind the whole name to whichever server won the loop.
    assert!(
        manager
            .call_mcp_tool("search_notes", json!({}))
            .await
            .is_none(),
        "multi-server collision must not create a bare alias"
    );
    // Both namespaced copies still dispatch.
    for sid in ["srv_alias_b", "srv_alias_c"] {
        let scoped = manager
            .call_mcp_tool(&format!("mcp__{}__search_notes", sid), json!({}))
            .await
            .expect("namespaced collision copy dispatches");
        assert!(scoped.is_ok(), "{} copy dispatches", sid);
    }

    // 4. And a builtin name like `read_file`, exposed externally, gains no
    // alias either.
    assert!(
        manager
            .call_mcp_tool("read_file", json!({ "path": "x" }))
            .await
            .is_none(),
        "builtin read_file must never route to the external server"
    );

    // Clean shutdown of the spawned python processes.
    for sid in ["srv_alias_a", "srv_alias_b", "srv_alias_c"] {
        manager.disconnect_server(sid).await;
    }
}
