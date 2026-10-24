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

#[test]
fn secure_write_contained_rejects_parent_symlink_escape_attack() {
    let (dir, _db) = temp_db("symlink_parent_attack");
    let workspace = dir.join("workspace");
    let outside = dir.join("outside_target");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&outside).unwrap();

    #[cfg(unix)]
    {
        // Adversarial setup: symlink inside workspace pointing to an outside directory
        let symlink_dir = workspace.join("sub_link");
        std::os::unix::fs::symlink(&outside, &symlink_dir).unwrap();

        // Attempting to write into sub_link/payload.txt must fail closed
        let res = AuthorizationEngine::secure_write_contained(
            Some(&workspace),
            "sub_link/payload.txt",
            b"exploit",
        );
        assert!(
            res.is_err(),
            "Must reject writing through symlink that escapes workspace"
        );
        let err = res.unwrap_err();
        assert!(
            err.contains("escapes workspace root") || err.contains("Security denial"),
            "Got: {}",
            err
        );
        assert!(
            !outside.join("payload.txt").exists(),
            "Payload must not have been written outside"
        );
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn secure_write_contained_rejects_destination_symlink_escape_attack() {
    let (dir, _db) = temp_db("symlink_dest_attack");
    let workspace = dir.join("workspace");
    let outside = dir.join("outside_target");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&outside).unwrap();

    #[cfg(unix)]
    {
        let outside_file = outside.join("victim.txt");
        std::fs::write(&outside_file, "original content").unwrap();

        let symlink_file = workspace.join("target.txt");
        std::os::unix::fs::symlink(&outside_file, &symlink_file).unwrap();

        let res = AuthorizationEngine::secure_write_contained(
            Some(&workspace),
            "target.txt",
            b"overwritten",
        );
        assert!(
            res.is_err(),
            "Must reject writing to symlink targeting outside workspace"
        );
        assert_eq!(
            std::fs::read_to_string(&outside_file).unwrap(),
            "original content",
            "Victim file outside workspace must not be overwritten"
        );
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn secure_write_contained_propagates_parent_creation_failure() {
    let (dir, _db) = temp_db("parent_fail");
    let workspace = dir.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    // Create a regular file where a directory component would be
    let obstacle = workspace.join("not_a_dir");
    std::fs::write(&obstacle, "file content").unwrap();

    // Attempting to write to not_a_dir/child.txt must explicitly propagate parent creation error
    let res = AuthorizationEngine::secure_write_contained(
        Some(&workspace),
        "not_a_dir/child.txt",
        b"content",
    );
    assert!(
        res.is_err(),
        "Must fail when parent directory creation fails"
    );
    let err = res.unwrap_err();
    assert!(
        err.contains("Failed to create parent directories"),
        "Got: {}",
        err
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn secure_write_contained_succeeds_for_legitimate_nested_path() {
    let (dir, _db) = temp_db("legit_nested");
    let workspace = dir.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    let res = AuthorizationEngine::secure_write_contained(
        Some(&workspace),
        "src/components/Button.tsx",
        b"export const Button = () => null;",
    );
    assert!(
        res.is_ok(),
        "Legitimate nested write must succeed: {:?}",
        res
    );
    let written = res.unwrap();
    assert!(written.exists());
    assert_eq!(
        std::fs::read_to_string(&written).unwrap(),
        "export const Button = () => null;"
    );

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
fn supported_browsers_are_permitted_for_browser_open() {
    let policy = GovernancePolicy::default();
    let ctx = context();
    let args = serde_json::json!({ "browser": "chromium" });
    let decision = tools::evaluate_tool(&policy, &ctx, "browser_open", &args);
    assert!(
        decision.is_none(),
        "supported browsers should be allowed: {:?}",
        decision
    );
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

// ---------------------------------------------------------------------
// Content Security Policy
// ---------------------------------------------------------------------

#[test]
fn test_tauri_csp_hardening_verification() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let conf_path = manifest_dir.join("tauri.conf.json");
    let content = std::fs::read_to_string(&conf_path).expect("read tauri.conf.json");
    let val: serde_json::Value = serde_json::from_str(&content).expect("parse tauri.conf.json");

    let csp = val["app"]["security"]["csp"]
        .as_str()
        .expect("app.security.csp should be configured as a string");

    // Must not allow unsafe-eval
    assert!(
        !csp.contains("'unsafe-eval'"),
        "CSP must NOT permit 'unsafe-eval'"
    );

    // Must enforce restrictive frame-src and object-src
    assert!(
        csp.contains("frame-src 'none'"),
        "CSP must set frame-src 'none'"
    );
    assert!(
        csp.contains("object-src 'none'"),
        "CSP must set object-src 'none'"
    );

    // Must restrict default-src and script-src to 'self'
    assert!(
        csp.contains("default-src 'self'"),
        "CSP must set default-src 'self'"
    );
    assert!(
        csp.contains("script-src 'self'"),
        "CSP must set script-src 'self'"
    );
}

// ---------------------------------------------------------------------
// File Organizer Governance and Filesystem Hardening
// ---------------------------------------------------------------------

#[test]
fn organize_files_governance_requires_human_approval_regardless_of_persisted_permission() {
    let (_dir, db) = temp_db("organizer_gov");

    // 1. By default, organize_files_preview is allowed if policy is default/allow
    let preview_args = serde_json::json!({
        "directory": "/home/user/Downloads"
    });
    let dec_preview = decide(&db, "organize_files_preview", preview_args);
    assert!(is_allow(&dec_preview));

    // 2. Mutating tools (organize_files_execute and organize_files_undo) require approval by default
    let exec_args = serde_json::json!({
        "plan": {
            "plan_id": "test-plan",
            "target_directory": "/home/user/Downloads",
            "operations": []
        }
    });
    let dec_exec = decide(&db, "organize_files_execute", exec_args.clone());
    assert!(is_approval(&dec_exec), "organize_files_execute must require approval");

    let undo_args = serde_json::json!({
        "transaction_id": "test-tx"
    });
    let dec_undo = decide(&db, "organize_files_undo", undo_args.clone());
    assert!(is_approval(&dec_undo), "organize_files_undo must require approval");

    // 3. Persisting "allow" permission must NEVER bypass required approval for mutating organizer tools
    db.set_setting("mcp_perm:organize_files_execute", "allow")
        .expect("save setting");
    db.set_setting("mcp_perm:organize_files_undo", "allow")
        .expect("save setting");

    let dec_exec_persisted = decide(&db, "organize_files_execute", exec_args);
    assert!(
        is_approval(&dec_exec_persisted),
        "persisted 'allow' must NOT bypass approval for organize_files_execute"
    );

    let dec_undo_persisted = decide(&db, "organize_files_undo", undo_args);
    assert!(
        is_approval(&dec_undo_persisted),
        "persisted 'allow' must NOT bypass approval for organize_files_undo"
    );

    // 4. Deny policy or explicit block must result in Deny
    db.set_setting("mcp_perm:organize_files_execute", "deny")
        .expect("save deny");
    let dec_exec_blocked = decide(&db, "organize_files_execute", serde_json::json!({}));
    assert!(is_deny(&dec_exec_blocked), "blocked tool must be denied");
}

#[test]
fn organize_files_rejects_unauthorized_paths_and_fails_closed() {
    use kyro_lib::storage::organizer::FileOrganizer;
    let (dir, db) = temp_db("organizer_unauth");
    let test_folder = dir.join("target_dir");
    std::fs::create_dir_all(&test_folder).unwrap();

    // No authorized folders configured -> fails closed
    let preview_err = FileOrganizer::preview_authorized(&db, &test_folder.to_string_lossy(), None);
    assert!(
        preview_err.is_err(),
        "preview must fail when no folders are authorized"
    );

    // Add an authorized folder that is DIFFERENT from test_folder
    let auth_dir = dir.join("other_dir");
    std::fs::create_dir_all(&auth_dir).unwrap();
    let canonical_auth = auth_dir.canonicalize().unwrap();
    let folder_rec = kyro_lib::storage::AuthorizedFolderRecord {
        id: "auth-1".to_string(),
        path: canonical_auth.to_string_lossy().to_string(),
        display_name: "Auth Folder".to_string(),
        added_at: 1000,
    };
    db.add_authorized_folder(&folder_rec).unwrap();

    // Target folder is outside authorized folder -> rejected
    let preview_outside = FileOrganizer::preview_authorized(&db, &test_folder.to_string_lossy(), None);
    assert!(
        preview_outside.is_err(),
        "preview must reject target outside authorized folders"
    );

    // Path traversal payload -> rejected
    let traversal = format!("{}/../target_dir", canonical_auth.to_string_lossy());
    let preview_traversal = FileOrganizer::preview_authorized(&db, &traversal, None);
    assert!(
        preview_traversal.is_err(),
        "preview must reject path traversal escaping authorized root"
    );
}

#[test]
fn organize_files_server_authoritative_anti_tampering_and_freshness() {
    use kyro_lib::storage::organizer::FileOrganizer;
    let (dir, db) = temp_db("organizer_integrity");
    let target = dir.join("target_dir");
    std::fs::create_dir_all(&target).unwrap();
    let canonical_target = target.canonicalize().unwrap();

    // Authorize target folder
    let folder_rec = kyro_lib::storage::AuthorizedFolderRecord {
        id: "auth-target".to_string(),
        path: canonical_target.to_string_lossy().to_string(),
        display_name: "Target".to_string(),
        added_at: 1000,
    };
    db.add_authorized_folder(&folder_rec).unwrap();

    // Create a test file
    let file_path = canonical_target.join("document.pdf");
    std::fs::write(&file_path, "original-pdf-content").unwrap();

    // 1. Generate preview plan
    let plan = FileOrganizer::preview_authorized(&db, &canonical_target.to_string_lossy(), None).unwrap();
    assert_eq!(plan.operations.len(), 1);

    // 2. Tampered plan with modified destination path outside approved scope
    let mut tampered_dest_plan = plan.clone();
    tampered_dest_plan.operations[0].destination_path = PathBuf::from("/tmp/evil_location.pdf");
    let tamper_res = FileOrganizer::execute_authorized(&db, &tampered_dest_plan);
    assert!(
        tamper_res.is_err(),
        "executing tampered plan with modified destination must fail"
    );

    // 3. Tampered plan with injected operation
    let mut tampered_injected_plan = plan.clone();
    tampered_injected_plan.operations.push(kyro_lib::storage::organizer::PlannedFileOp {
        source_path: PathBuf::from("/etc/passwd"),
        destination_path: canonical_target.join("passwd"),
        category: "Documents".to_string(),
        reason: "injected".to_string(),
        file_size: 100,
        is_duplicate: false,
        duplicate_of: None,
        file_hash: None,
        conflict: None,
    });
    let inject_res = FileOrganizer::execute_authorized(&db, &tampered_injected_plan);
    assert!(
        inject_res.is_err(),
        "executing plan with injected operations must fail"
    );

    // 4. Freshness check: modify the source file content before execution
    std::fs::write(&file_path, "modified-pdf-content-after-preview").unwrap();
    let exec_stale = FileOrganizer::execute_authorized(&db, &plan).unwrap();
    // The operation should not have moved the file because hash mismatch indicates staleness
    assert_eq!(exec_stale.operations[0].status, "stale");
    assert!(
        file_path.exists(),
        "stale source file must not be moved from original path"
    );
    assert_eq!(
        std::fs::read_to_string(&file_path).unwrap(),
        "modified-pdf-content-after-preview"
    );

    // 5. Replay protection: cannot execute a plan that has already been executed
    let replay_res = FileOrganizer::execute_authorized(&db, &plan);
    assert!(
        replay_res.is_err(),
        "re-executing an already executed plan must be rejected"
    );
}

#[test]
fn organize_files_symlink_exclusion_and_persistence_recovery() {
    use kyro_lib::storage::organizer::FileOrganizer;
    let (dir, db) = temp_db("organizer_symlink");
    let target = dir.join("target_dir");
    std::fs::create_dir_all(&target).unwrap();
    let canonical_target = target.canonicalize().unwrap();

    let folder_rec = kyro_lib::storage::AuthorizedFolderRecord {
        id: "auth-symlink".to_string(),
        path: canonical_target.to_string_lossy().to_string(),
        display_name: "SymlinkTarget".to_string(),
        added_at: 1000,
    };
    db.add_authorized_folder(&folder_rec).unwrap();

    // Create a regular file and a symlink
    let reg_file = canonical_target.join("regular.txt");
    std::fs::write(&reg_file, "regular text").unwrap();

    let external_file = dir.join("external_secret.txt");
    std::fs::write(&external_file, "secret").unwrap();

    let symlink_file = canonical_target.join("symlink_escape.txt");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&external_file, &symlink_file).unwrap();

    // Preview
    let plan = FileOrganizer::preview_authorized(&db, &canonical_target.to_string_lossy(), None).unwrap();
    // Symlink must be excluded / recorded in scan warnings and conflicts
    assert!(
        plan.scan_warnings.iter().any(|w| w.contains("Symlink")),
        "scan warnings must note symlink"
    );
    assert_eq!(plan.conflicts_count, 1, "symlink counted as conflict");
    assert_eq!(plan.planned_count, 1, "only 1 file planned to move");

    // Execute
    let tx = FileOrganizer::execute_authorized(&db, &plan).unwrap();
    assert_eq!(tx.status, "partially_failed", "1 moved, 1 skipped symlink");
    let completed_op = tx.operations.iter().find(|o| o.status == "completed").unwrap();
    assert_eq!(completed_op.source_path, reg_file);

    // Symlink still exists untouched
    assert!(symlink_file.exists());
    assert!(std::fs::symlink_metadata(&symlink_file).unwrap().is_symlink());

    // Persistence recovery: query DB directly for transaction
    let retrieved_tx = db.get_file_transaction(&tx.transaction_id).unwrap().unwrap();
    assert_eq!(retrieved_tx.transaction_id, tx.transaction_id);
    assert_eq!(retrieved_tx.status, "partially_failed");

    // Undo authorized
    let undone_tx = FileOrganizer::undo_authorized(&db, &tx.transaction_id).unwrap();
    assert_eq!(undone_tx.status, "undone");
    assert!(reg_file.exists(), "regular file must be restored");

    // Status in DB must now be 'undone'
    let updated_tx = db.get_file_transaction(&tx.transaction_id).unwrap().unwrap();
    assert_eq!(updated_tx.status, "undone");
}
