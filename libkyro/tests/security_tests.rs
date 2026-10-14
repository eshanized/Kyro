//! Security regression tests.
//!
//! These tests cover the command-execution, authorisation, browser-navigation
//! and filesystem boundaries that were repaired in this task.  They are
//! deliberately adversarial: each one describes an attack that the previous
//! implementation would have allowed.

use kyro_lib::agent::tools;
use kyro_lib::browser::safety::NavigationPolicy;
use kyro_lib::execution::launcher::{ExecutableResolver, ProgramSpec};
use kyro_lib::ghost::GhostDisplayManager;
use kyro_lib::governance::{AuthDecision, AuthorizationEngine, ExecutionContext, GovernancePolicy};
use kyro_lib::storage::Database;
use std::path::PathBuf;

fn temp_db(tag: &str) -> (PathBuf, Database) {
    let dir = std::env::temp_dir().join(format!("kyro_sec_{}_{}", tag, uuid::Uuid::new_v4()));
    let db = Database::init(dir.clone()).expect("database");
    (dir, db)
}

fn context() -> ExecutionContext {
    ExecutionContext {
        run_id: "run_security".to_string(),
        agent_id: "apex-orchestrator".to_string(),
        conversation_id: Some("conv_security".to_string()),
        workspace_root: None,
    }
}

fn decide(db: &Database, tool: &str, args: serde_json::Value) -> AuthDecision {
    AuthorizationEngine::evaluate(db, &context(), tool, &args)
}

fn is_allow(decision: &AuthDecision) -> bool {
    matches!(decision, AuthDecision::Allow)
}

fn is_approval(decision: &AuthDecision) -> bool {
    matches!(decision, AuthDecision::RequireApproval { .. })
}

fn is_deny(decision: &AuthDecision) -> bool {
    matches!(decision, AuthDecision::Deny { .. })
}

// ---------------------------------------------------------------------
// Ghost application launching
// ---------------------------------------------------------------------

#[test]
fn ghost_launch_cannot_execute_a_shell_metacharacter_payload() {
    // The previous launcher ran the command through `sh -c`, so this payload
    // would have executed `rm -rf /`.
    for payload in [
        "firefox; rm -rf /",
        "firefox && rm -rf /",
        "firefox | sh",
        "firefox `id`",
        "firefox $(id)",
        "firefox\nrm -rf /",
    ] {
        let spec = ProgramSpec::new(payload);
        let err = GhostDisplayManager::launch_in_ghost(&spec)
            .expect_err("a shell payload must never be launched");
        assert!(
            !err.is_empty(),
            "the failure for '{}' must explain itself",
            payload
        );
    }
}

#[test]
fn ghost_launch_rejects_a_model_controlled_executable_path() {
    for payload in ["../../bin/sh", "/tmp/evil", "./evil", "sh -c 'id'"] {
        let spec = ProgramSpec::new(payload);
        assert!(
            GhostDisplayManager::launch_in_ghost(&spec).is_err(),
            "'{}' must not be launchable",
            payload
        );
    }
}

#[test]
fn the_launcher_keeps_arguments_literal() {
    let resolver = ExecutableResolver::new();
    let payload = "; rm -rf / ; $(id) `id` | tee /etc/passwd";
    let spec = ProgramSpec::new("echo").with_args(vec![payload]);
    let resolved = resolver.resolve(&spec).expect("echo should resolve");
    assert_eq!(resolved.args.len(), 1);
    assert_eq!(resolved.args[0], payload);
}

#[test]
fn a_free_form_launch_command_is_denied() {
    let (dir, db) = temp_db("ghost_free_form");
    let decision = decide(
        &db,
        "launch_ghost_app",
        serde_json::json!({ "command": "firefox https://example.com" }),
    );
    assert!(is_deny(&decision), "{:?}", decision);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_launch_of_a_shell_interpreter_is_denied() {
    let (dir, db) = temp_db("ghost_interpreter");
    for executable in ["sh", "bash", "/bin/bash", "python3", "powershell"] {
        let decision = decide(
            &db,
            "launch_ghost_app",
            serde_json::json!({ "executable": executable, "args": ["-c", "id"] }),
        );
        assert!(is_deny(&decision), "'{}' must be denied", executable);
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_launch_of_a_missing_executable_is_denied() {
    let (dir, db) = temp_db("ghost_missing");
    let decision = decide(
        &db,
        "launch_ghost_app",
        serde_json::json!({ "executable": "kyro-not-a-real-binary" }),
    );
    assert!(is_deny(&decision), "{:?}", decision);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_launch_of_a_blocked_domain_is_denied() {
    let (dir, db) = temp_db("ghost_blocked");
    let decision = decide(
        &db,
        "launch_ghost_app",
        serde_json::json!({ "executable": "firefox", "args": ["https://www.paypal.com/login"] }),
    );
    assert!(is_deny(&decision), "{:?}", decision);
    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------
// Shell authorisation
// ---------------------------------------------------------------------

#[test]
fn find_delete_cannot_bypass_the_allowlist() {
    let (dir, db) = temp_db("find_delete");
    for command in [
        "find . -delete",
        "find . -name '*.log' -delete",
        "find . -exec rm {} ;",
        "find . -execdir rm {} ;",
        "find . -ok rm {} ;",
        "find . -fprint /tmp/out.txt",
        "find . -fls /tmp/out.txt",
    ] {
        let decision = decide(
            &db,
            "execute_shell",
            serde_json::json!({ "command": command }),
        );
        assert!(
            !is_allow(&decision),
            "'{}' must not be auto-approved: {:?}",
            command,
            decision
        );
        assert!(
            is_approval(&decision),
            "'{}' must require an approval: {:?}",
            command,
            decision
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn read_utilities_cannot_read_sensitive_system_files() {
    let (dir, db) = temp_db("sensitive_read");
    for command in [
        "cat /etc/shadow",
        "cat /etc/passwd",
        "head /etc/sudoers",
        "tail /etc/passwd",
        "grep root /etc/shadow",
        "wc -l /etc/passwd",
        "stat /etc/shadow",
        "file /etc/passwd",
    ] {
        let decision = decide(
            &db,
            "execute_shell",
            serde_json::json!({ "command": command }),
        );
        assert!(
            !is_allow(&decision),
            "'{}' must not be auto-approved: {:?}",
            command,
            decision
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_similar_but_legitimate_path_is_still_allowed() {
    let (dir, db) = temp_db("similar_path");
    for command in [
        "ls ethereum",
        "cat README.md",
        "ls /home/snigdha/Desktop/Kyro",
    ] {
        let decision = decide(
            &db,
            "execute_shell",
            serde_json::json!({ "command": command }),
        );
        assert!(
            is_allow(&decision),
            "'{}' should be auto-approved: {:?}",
            command,
            decision
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn shell_only_constructs_require_an_approval() {
    let (dir, db) = temp_db("shell_constructs");
    for command in [
        "ls | grep x",
        "ls && rm -rf /",
        "ls; rm -rf /",
        "ls > /etc/passwd",
        "ls $(whoami)",
        "ls `whoami`",
        "ls ${HOME}",
        "cat file\nrm -rf /",
        "FOO=bar ls",
    ] {
        let decision = decide(
            &db,
            "execute_shell",
            serde_json::json!({ "command": command }),
        );
        assert!(
            !is_allow(&decision),
            "'{}' must not be auto-approved: {:?}",
            command,
            decision
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn interpreters_and_wrappers_require_an_approval() {
    let (dir, db) = temp_db("interpreters");
    for command in [
        "sh -c id",
        "bash -c id",
        "python3 -c 'import os'",
        "node -e 'process.exit()'",
        "env FOO=bar ls",
        "sudo ls",
        "xargs rm",
    ] {
        let decision = decide(
            &db,
            "execute_shell",
            serde_json::json!({ "command": command }),
        );
        assert!(
            is_approval(&decision),
            "'{}' must require an approval: {:?}",
            command,
            decision
        );
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_authorised_argv_is_the_argv_that_will_run() {
    let policy = GovernancePolicy::default();
    let analysis = AuthorizationEngine::analyze_shell_command(
        &policy,
        None,
        &serde_json::json!({ "command": "ls -la /tmp" }),
    );
    assert!(is_allow(&analysis.decision));
    assert!(!analysis.requires_shell);
    assert_eq!(
        analysis.argv,
        Some(vec![
            "ls".to_string(),
            "-la".to_string(),
            "/tmp".to_string()
        ])
    );

    // A command that needs a real shell is never given a pre-computed argv.
    let complex = AuthorizationEngine::analyze_shell_command(
        &policy,
        None,
        &serde_json::json!({ "command": "ls | grep x" }),
    );
    assert!(is_approval(&complex.decision));
    assert!(complex.requires_shell);
    assert!(complex.argv.is_none());
}

#[test]
fn a_destructive_command_is_flagged_as_destructive() {
    let (dir, db) = temp_db("destructive");
    for command in [
        "rm -rf /",
        "dd if=/dev/zero of=/dev/sda",
        "mkfs.ext4 /dev/sda1",
    ] {
        let decision = decide(
            &db,
            "execute_shell",
            serde_json::json!({ "command": command }),
        );
        match decision {
            AuthDecision::RequireApproval { risk, .. } => {
                assert_eq!(risk, kyro_lib::execution::RiskLevel::Destructive);
            }
            other => panic!(
                "expected a destructive approval for '{}', got {:?}",
                command, other
            ),
        }
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_ask_always_policy_requires_an_approval_for_everything() {
    let (dir, db) = temp_db("ask_always");
    let policy = GovernancePolicy {
        shell_mode: "AskAlways".to_string(),
        ..GovernancePolicy::default()
    };
    db.set_setting(
        "governance_policy",
        &serde_json::to_string(&policy).unwrap(),
    )
    .unwrap();
    for command in ["ls", "echo hello", "cat README.md"] {
        let decision = decide(
            &db,
            "execute_shell",
            serde_json::json!({ "command": command }),
        );
        assert!(is_approval(&decision), "'{}' must be approved", command);
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_sandbox_only_policy_fails_closed_when_the_engine_is_missing() {
    let (dir, db) = temp_db("sandbox_only");
    let policy = GovernancePolicy {
        shell_mode: "SandboxOnly".to_string(),
        ..GovernancePolicy::default()
    };
    db.set_setting(
        "governance_policy",
        &serde_json::to_string(&policy).unwrap(),
    )
    .unwrap();
    let decision = decide(&db, "execute_shell", serde_json::json!({ "command": "ls" }));
    // Either the sandbox is available (AllowSandboxOnly) or the request is
    // denied; it must never fall through to a host execution.
    match decision {
        AuthDecision::AllowSandboxOnly { .. } => {}
        AuthDecision::Deny { .. } => {}
        other => panic!("unexpected decision: {:?}", other),
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_explicit_sandbox_request_never_runs_on_the_host() {
    let (dir, db) = temp_db("sandbox_request");
    let decision = decide(
        &db,
        "execute_shell",
        serde_json::json!({ "command": "ls", "use_sandbox": true }),
    );
    match decision {
        AuthDecision::AllowSandboxOnly { .. } => {}
        AuthDecision::Deny { .. } => {}
        other => panic!("unexpected decision: {:?}", other),
    }
    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------
// Filesystem boundaries
// ---------------------------------------------------------------------

#[test]
fn a_path_traversal_out_of_the_workspace_is_denied() {
    let (dir, db) = temp_db("traversal");
    let workspace = dir.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let mut ctx = context();
    ctx.workspace_root = Some(workspace);

    for path in [
        "../../etc/shadow",
        "/etc/shadow",
        "/etc/passwd",
        "subdir/../../outside.txt",
    ] {
        let decision = AuthorizationEngine::evaluate(
            &db,
            &ctx,
            "read_file",
            &serde_json::json!({ "path": path }),
        );
        assert!(is_deny(&decision), "'{}' must be denied", path);
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_write_outside_the_workspace_is_denied() {
    let (dir, db) = temp_db("write_outside");
    let workspace = dir.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let mut ctx = context();
    ctx.workspace_root = Some(workspace);

    for path in ["/etc/hosts", "../outside.txt", "/tmp/kyro-escape.txt"] {
        let decision = AuthorizationEngine::evaluate(
            &db,
            &ctx,
            "write_file",
            &serde_json::json!({ "path": path, "content": "x" }),
        );
        assert!(is_deny(&decision), "'{}' must be denied", path);
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_write_inside_the_workspace_is_allowed() {
    let (dir, db) = temp_db("write_inside");
    let workspace = dir.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let mut ctx = context();
    ctx.workspace_root = Some(workspace);

    let decision = AuthorizationEngine::evaluate(
        &db,
        &ctx,
        "write_file",
        &serde_json::json!({ "path": "notes.md", "content": "x" }),
    );
    assert!(is_allow(&decision), "{:?}", decision);
    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------
// Browser navigation boundaries
// ---------------------------------------------------------------------

#[test]
fn a_browser_navigation_to_a_blocked_domain_is_denied() {
    let (dir, db) = temp_db("browser_blocked");
    let decision = decide(
        &db,
        "browser_navigate",
        serde_json::json!({ "session_id": "s", "url": "https://www.paypal.com/checkout" }),
    );
    assert!(is_deny(&decision), "{:?}", decision);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_browser_navigation_outside_the_origin_scope_is_refused() {
    let policy =
        NavigationPolicy::default().with_origin_scope(Some("http://localhost:3000".to_string()));
    let err = policy
        .check("https://example.com/")
        .expect_err("an external destination must be refused");
    assert!(err.to_string().contains("origin scope"));

    // Inside the scope it is allowed.
    assert!(policy.check("http://localhost:3000/dashboard").is_ok());
}

#[test]
fn dangerous_url_schemes_are_refused() {
    let policy = NavigationPolicy::default();
    for url in [
        "javascript:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "file:///etc/passwd",
        "blob:https://example.com/abc",
        "chrome://settings",
    ] {
        assert!(policy.check(url).is_err(), "'{}' must be refused", url);
    }
}

#[test]
fn a_consequential_browser_click_requires_an_approval() {
    let policy = GovernancePolicy::default();
    let ctx = context();
    for label in [
        "Delete account",
        "Purchase now",
        "Pay invoice",
        "Send message",
        "Deploy to production",
        "Confirm order",
    ] {
        let args = serde_json::json!({ "name": label });
        let decision = tools::evaluate_tool(&policy, &ctx, "browser_click", &args)
            .expect("a consequential click must be gated");
        assert!(
            matches!(decision, AuthDecision::RequireApproval { .. }),
            "'{}' must require an approval",
            label
        );
    }
}

#[test]
fn an_ordinary_browser_click_is_not_gated() {
    let policy = GovernancePolicy::default();
    let ctx = context();
    let args = serde_json::json!({ "name": "Open settings" });
    assert!(tools::evaluate_tool(&policy, &ctx, "browser_click", &args).is_none());
}

#[test]
fn typing_a_credential_requires_a_human_takeover() {
    let policy = GovernancePolicy::default();
    let ctx = context();
    let args = serde_json::json!({ "value": "my credit card is 4111 1111 1111 1111" });
    let decision = tools::evaluate_tool(&policy, &ctx, "browser_fill", &args)
        .expect("a credential must be gated");
    assert!(matches!(decision, AuthDecision::RequireTakeover { .. }));
}

#[test]
fn a_browser_other_than_firefox_is_refused() {
    let policy = GovernancePolicy::default();
    let ctx = context();
    let args = serde_json::json!({ "browser": "chromium" });
    let decision = tools::evaluate_tool(&policy, &ctx, "browser_open", &args)
        .expect("another browser must be refused");
    assert!(matches!(decision, AuthDecision::Deny { .. }));
}

#[test]
fn the_audit_tool_requires_an_approval_before_it_executes_anything() {
    let policy = GovernancePolicy::default();
    let ctx = context();
    let decision = tools::evaluate_tool(
        &policy,
        &ctx,
        "audit_ui",
        &serde_json::json!({ "project_name": "Runstead" }),
    )
    .expect("the audit must be gated");
    assert!(matches!(decision, AuthDecision::RequireApproval { .. }));
}

// ---------------------------------------------------------------------
// Emergency stop
// ---------------------------------------------------------------------

#[test]
fn an_unrelated_task_cannot_clear_another_tasks_emergency_stop() {
    use kyro_lib::computer_use::InputController;
    let scope_a = format!("run:security-{}", uuid::Uuid::new_v4());
    let scope_b = format!("run:security-{}", uuid::Uuid::new_v4());

    InputController::trigger_abort_for(&scope_a);
    assert!(InputController::is_aborted_scope(&scope_a));

    // An unrelated task clearing its own scope must not clear scope A.
    InputController::reset_abort_for(&scope_b);
    assert!(InputController::is_aborted_scope(&scope_a));

    // Only the owning scope can clear its own stop.
    InputController::reset_abort_for(&scope_a);
    assert!(!InputController::is_aborted_scope(&scope_a));
}
