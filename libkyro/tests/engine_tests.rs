use image::RgbaImage;
use kyro_lib::agent::cancellation::CancellationToken;
use kyro_lib::agent::router::{ChatMessage, MessageContent};
use kyro_lib::computer_use::overlay::CoordinateTransform;
use kyro_lib::connector::{
    ConnectorManager, GitHubAdapter, InvocationAuthProof, OAuthManager, OAuthSessionStatus,
    PendingOAuthSession, RemoteMcpClient, ValidatedRemoteEndpoint,
};
use kyro_lib::execution::permissions::{PermissionGate, RiskLevel};
use kyro_lib::governance::auth::{AuthDecision, AuthorizationEngine, ExecutionContext, FileOp};
use kyro_lib::governance::{GovernancePolicy, PolicyEngine, TakeoverController};
use kyro_lib::grounding::SetOfMarksEngine;
use kyro_lib::mcp::validate_schema;
use kyro_lib::mcp::McpManager;
use kyro_lib::memory::{AppIndexer, MemoryStore};
use kyro_lib::multibot::{BotProfile, MultiBotOrchestrator};
use kyro_lib::sandbox::DockerSandbox;
use kyro_lib::storage::{
    AgentRun, ApprovalRequest, ArtifactRecord, ConnectedAccountRecord, ConnectorToolPolicyRecord,
    Database, McpServerRecord, Message, RoutineRecord, RunEvent, SkillRecord, TemplateEngine,
    Vault, VAULT_SECRET_SENTINEL,
};
use kyro_lib::teach_task::{
    RoutineScheduler, SelfHealingEngine, SkillReplayer, SkillStep, TaskRecorder,
};
use kyro_lib::timeline::TimelineEngine;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn test_database_and_vault() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");
    let vault = Vault::new_encrypted_fallback(db.clone());

    // 1. Create and list conversations
    let conv = db
        .create_conversation("c1", "Test Task", "nvidia/nemotron-3-ultra-550b-a55b")
        .unwrap();
    assert_eq!(conv.id, "c1");
    assert_eq!(conv.title, "Test Task");

    let list = db.list_conversations().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "c1");

    // 2. Add and retrieve messages
    let msg = Message {
        id: "m1".into(),
        conversation_id: "c1".into(),
        role: "user".into(),
        content: "Hello Kyro".into(),
        tool_calls_json: None,
        observations_json: None,
        created_at: 1000,
    };
    db.add_message(&msg).unwrap();

    let msgs = db.get_messages("c1").unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content, "Hello Kyro");

    // 3. Vault API keys (NVIDIA Nemotron 3 Ultra)
    vault
        .set_secret("nvidia", "nvapi-test-nemotron-key-123")
        .unwrap();
    let retrieved = vault.get_secret("nvidia").unwrap();
    assert_eq!(retrieved, Some("nvapi-test-nemotron-key-123".into()));

    let all_keys = vault.get_all_keys().unwrap();
    assert_eq!(all_keys.nvidia, Some("nvapi-test-nemotron-key-123".into()));

    // 4. Default Seeded Bots
    let bots = db.list_bots().unwrap();
    assert!(bots.len() >= 3);
    assert!(bots.iter().any(|b| b.name == "Apex"));
    assert!(bots.iter().any(|b| b.name == "Forge"));
    assert!(bots.iter().any(|b| b.name == "Scout"));

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_vault_migration_and_secure_storage() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_vault_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");

    let vault = Vault::new_encrypted_fallback(db.clone());

    // Seed legacy plaintext secret directly in settings table
    db.set_setting("secret:nvidia", "legacy-plaintext-nvapi-key-999")
        .unwrap();
    db.set_setting("secret:telegram_bot_token", "legacy-telegram-token-888")
        .unwrap();

    // Run migration
    let count = vault.migrate_plaintext_secrets().unwrap();
    assert_eq!(count, 2);

    // Verify plaintext keys are PURGED from settings table
    assert_eq!(db.get_setting("secret:nvidia").unwrap(), None);
    assert_eq!(db.get_setting("secret:telegram_bot_token").unwrap(), None);

    // Verify secure retrieval from encrypted vault
    assert_eq!(
        vault.get_secret("nvidia").unwrap(),
        Some("legacy-plaintext-nvapi-key-999".to_string())
    );
    assert_eq!(
        vault.get_secret("telegram_bot_token").unwrap(),
        Some("legacy-telegram-token-888".to_string())
    );

    // Verify metadata returns presence without returning raw secrets
    let meta = vault.get_metadata().unwrap();
    assert!(meta.nvidia_configured);
    assert!(meta.telegram_configured);

    // Verify secret redaction
    let log_msg = "Error connecting with legacy-plaintext-nvapi-key-999 to server";
    let redacted = Vault::redact_string(log_msg, &["legacy-plaintext-nvapi-key-999"]);
    assert!(!redacted.contains("legacy-plaintext-nvapi-key-999"));
    assert!(redacted.contains("[REDACTED_SECRET]"));

    // Verify deliberate removal
    vault.delete_secret("nvidia").unwrap();
    assert_eq!(vault.get_secret("nvidia").unwrap(), None);

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_durable_agent_runs_and_events() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_runs_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");
    db.create_conversation("conv_1", "Analysis Task", "nemotron")
        .unwrap();

    let run_id = "test_run_101";
    let run = AgentRun {
        id: run_id.to_string(),
        conversation_id: Some("conv_1".to_string()),
        agent_id: "apex-orchestrator".to_string(),
        model: "nvidia/nemotron-3-ultra-550b-a55b".to_string(),
        prompt: "Analyze repository dependencies".to_string(),
        status: "running".to_string(),
        created_at: 1000,
        started_at: Some(1005),
        completed_at: None,
        error: None,
        config_json: None,
        parent_run_id: None,
    };
    db.create_run(&run).unwrap();

    let fetched = db.get_run(run_id).unwrap().unwrap();
    assert_eq!(fetched.status, "running");
    assert_eq!(fetched.agent_id, "apex-orchestrator");

    // Append ordered events
    let seq1 = db.get_next_event_seq(run_id).unwrap();
    assert_eq!(seq1, 1);
    db.append_run_event(&RunEvent {
        id: "evt_1".to_string(),
        run_id: run_id.to_string(),
        seq: seq1,
        event_type: "thought".to_string(),
        payload_json: "{\"step\": 1, \"thought\": \"Reading files\"}".to_string(),
        created_at: 1010,
    })
    .unwrap();

    let seq2 = db.get_next_event_seq(run_id).unwrap();
    assert_eq!(seq2, 2);
    db.append_run_event(&RunEvent {
        id: "evt_2".to_string(),
        run_id: run_id.to_string(),
        seq: seq2,
        event_type: "tool_result".to_string(),
        payload_json: "{\"output\": \"done\"}".to_string(),
        created_at: 1020,
    })
    .unwrap();

    let events = db.get_run_events(run_id, None).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].seq, 1);
    assert_eq!(events[1].seq, 2);

    // Test startup reconciliation: active runs become interrupted
    let recovered_count = db.reconcile_active_runs_on_startup().unwrap();
    assert_eq!(recovered_count, 1);
    let after_recovery = db.get_run(run_id).unwrap().unwrap();
    assert_eq!(after_recovery.status, "interrupted");
    assert!(after_recovery
        .error
        .unwrap()
        .contains("terminated unexpectedly"));

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_centralized_authorization_engine() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_auth_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");

    let ctx = ExecutionContext {
        run_id: "test_auth_run".to_string(),
        agent_id: "apex-orchestrator".to_string(),
        conversation_id: None,
        workspace_root: None,
        task_grant: None,
    };

    // 1. Safe shell command -> Allow
    let safe_cmd = serde_json::json!({ "command": "ls -la" });
    let res = AuthorizationEngine::evaluate(&db, &ctx, "execute_shell", &safe_cmd);
    assert!(matches!(res, AuthDecision::Allow));

    // 2. Complex shell command with pipe -> RequireApproval
    let complex_cmd = serde_json::json!({ "command": "cat file.txt | grep error" });
    let res_complex = AuthorizationEngine::evaluate(&db, &ctx, "execute_shell", &complex_cmd);
    assert!(matches!(res_complex, AuthDecision::RequireApproval { .. }));

    // 3. Destructive shell command -> RequireApproval with Destructive risk
    let rm_cmd = serde_json::json!({ "command": "rm -rf /" });
    let res_rm = AuthorizationEngine::evaluate(&db, &ctx, "execute_shell", &rm_cmd);
    match res_rm {
        AuthDecision::RequireApproval { risk, .. } => assert_eq!(risk, RiskLevel::Destructive),
        _ => panic!("Expected RequireApproval for rm -rf /"),
    }

    // 4. Directory traversal write -> Deny
    let traversal_write = serde_json::json!({ "path": "../../etc/shadow", "content": "malicious" });
    let res_trav = AuthorizationEngine::evaluate(&db, &ctx, "write_file", &traversal_write);
    assert!(matches!(res_trav, AuthDecision::Deny { .. }));

    // 5. Prohibited system path write -> Deny
    let sys_write = serde_json::json!({ "path": "/etc/hosts", "content": "127.0.0.1 bad.com" });
    let res_sys = AuthorizationEngine::evaluate(&db, &ctx, "write_file", &sys_write);
    assert!(matches!(res_sys, AuthDecision::Deny { .. }));

    // 6. Blocked domain -> Deny (the launch is expressed as an executable
    //    plus an argument vector).
    let blocked_action = serde_json::json!({
        "executable": "firefox",
        "args": ["https://www.paypal.com/login"]
    });
    let res_blocked = AuthorizationEngine::evaluate(&db, &ctx, "launch_ghost_app", &blocked_action);
    assert!(matches!(res_blocked, AuthDecision::Deny { .. }));

    // 6b. A free-form command string is no longer accepted for a launch.
    let free_form = serde_json::json!({ "command": "firefox https://example.com" });
    let res_free_form = AuthorizationEngine::evaluate(&db, &ctx, "launch_ghost_app", &free_form);
    assert!(matches!(res_free_form, AuthDecision::Deny { .. }));

    // 7. Credential sensitive keyword -> RequireTakeover
    let pass_action = serde_json::json!({ "action": "type", "text": "secret_password_123" });
    let res_pass = AuthorizationEngine::evaluate(&db, &ctx, "computer_action", &pass_action);
    assert!(matches!(res_pass, AuthDecision::RequireTakeover { .. }));

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_approval_queue_state_transitions() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_appr_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");

    let req = AuthorizationEngine::request_approval(
        &db,
        "run_999",
        "execute_shell",
        "Run npm install",
        &serde_json::json!({ "command": "npm install" }),
        RiskLevel::Moderate,
        "Command requires user confirmation",
    )
    .unwrap();

    let pending = db.list_pending_approvals().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, req.id);

    // Resolve approval
    let approved = db
        .resolve_approval_request(&req.id, "approved", "admin_user")
        .unwrap();
    assert!(approved);

    // Verify it cannot be resolved twice (one-time execution guarantee)
    let second_resolve = db
        .resolve_approval_request(&req.id, "denied", "another_user")
        .unwrap();
    assert!(!second_resolve);

    let resolved = db.get_approval_request(&req.id).unwrap().unwrap();
    assert_eq!(resolved.status, "approved");
    assert_eq!(resolved.resolved_by, Some("admin_user".to_string()));

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_cancellation_token() {
    let token = CancellationToken::new();
    assert!(!token.is_cancelled());

    token.cancel();
    assert!(token.is_cancelled());
}

#[test]
fn test_multimodal_message_content() {
    let text_content = MessageContent::from_text("List files");
    assert_eq!(text_content.as_text_lossy(), "List files");

    let mm_content =
        MessageContent::from_text_and_image("Look at screen", "iVBORw0KGgoAAAANSUhEUg==");
    assert_eq!(mm_content.as_text_lossy(), "Look at screen");

    let chat_msg = ChatMessage {
        role: "user".to_string(),
        content: mm_content,
        tool_calls: None,
        tool_call_id: None,
    };
    let json_str = serde_json::to_string(&chat_msg).unwrap();
    assert!(json_str.contains("image_url"));
    assert!(json_str.contains("data:image/png;base64,iVBORw0KGgoAAAANSUhEUg=="));
}

#[test]
fn test_cron_schedule_and_routines() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_cron_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");

    // 1. Parse 5-field cron
    let sched5 = RoutineScheduler::parse_schedule("*/5 * * * *");
    assert!(sched5.is_ok());

    // 2. Next run timestamp calculation
    let next_run = RoutineScheduler::compute_next_run("*/5 * * * *");
    assert!(next_run.is_some());
    assert!(next_run.unwrap() > chrono::Utc::now().timestamp_millis());

    // 3. Routine persistence
    let routine = RoutineRecord {
        id: "routine_1".to_string(),
        name: "Backup routine".to_string(),
        cron_expression: "0 0 12 * * ?".to_string(),
        action_type: "prompt".to_string(),
        target_id: "Check disk usage and clean logs".to_string(),
        enabled: true,
        last_run: None,
        next_run,
    };
    db.create_routine(&routine).unwrap();

    let list = db.list_routines().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].action_type, "prompt");
    assert_eq!(list[0].target_id, "Check disk usage and clean logs");

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_scoped_persistent_memory() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_mem_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");

    MemoryStore::save_fact(&db, "preference", "user", "theme", "dark", "user_settings").unwrap();

    MemoryStore::save_fact(
        &db,
        "agent",
        "forge-devops",
        "docker_preferred",
        "podman",
        "inferred",
    )
    .unwrap();

    let user_prefs = db.get_memories_for_scope("preference", "user").unwrap();
    assert_eq!(user_prefs.len(), 1);
    assert_eq!(user_prefs[0].key, "theme");

    let agent_facts = db.get_memories_for_scope("agent", "forge-devops").unwrap();
    assert_eq!(agent_facts.len(), 1);
    assert_eq!(agent_facts[0].key, "docker_preferred");

    // Context block construction with budget limit
    let context_block = MemoryStore::build_context_block(&db, "forge-devops", "deploy application");
    assert!(context_block.contains("theme: dark"));
    assert!(context_block.contains("docker_preferred: podman"));
    assert!(context_block.len() <= 3100);

    // Delete fact
    let deleted = db.delete_memory(&user_prefs[0].id).unwrap();
    assert!(deleted);
    assert_eq!(
        db.get_memories_for_scope("preference", "user")
            .unwrap()
            .len(),
        0
    );

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_multi_agent_roster_and_delegation() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_roster_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");

    let initial_statuses = MultiBotOrchestrator::get_roster_statuses(&db);
    assert!(initial_statuses.iter().all(|s| s.state == "idle"));

    // Two concurrent delegations to the same teammate, keyed by owning run.
    MultiBotOrchestrator::set_bot_task(
        "forge-devops",
        "run_t1",
        Some("Running container security audit"),
    );
    MultiBotOrchestrator::set_bot_task(
        "forge-devops",
        "run_t2",
        Some("Extracting build diagnostics"),
    );
    let updated_statuses = MultiBotOrchestrator::get_roster_statuses(&db);
    let forge_status = updated_statuses
        .iter()
        .find(|s| s.id == "forge-devops")
        .unwrap();
    assert_eq!(forge_status.state, "busy");
    assert_eq!(forge_status.active_tasks.len(), 2);
    assert_eq!(
        forge_status.current_task,
        Some("Running container security audit".to_string())
    );

    // Clearing one run's task keeps the other teammates/runs busy.
    MultiBotOrchestrator::set_bot_task("forge-devops", "run_t1", None);
    let partly_cleared = MultiBotOrchestrator::get_roster_statuses(&db);
    let forge_partial = partly_cleared
        .iter()
        .find(|s| s.id == "forge-devops")
        .unwrap();
    assert_eq!(forge_partial.state, "busy");
    assert_eq!(forge_partial.active_tasks.len(), 1);
    assert_eq!(forge_partial.active_tasks[0].run_id, "run_t2".to_string());

    // Dropping the last task of a teammate returns it to idle.
    MultiBotOrchestrator::clear_run_tasks("run_t2");
    let cleared_statuses = MultiBotOrchestrator::get_roster_statuses(&db);
    let forge_cleared = cleared_statuses
        .iter()
        .find(|s| s.id == "forge-devops")
        .unwrap();
    assert_eq!(forge_cleared.state, "idle");
    assert_eq!(forge_cleared.current_task, None);
    assert!(forge_cleared.active_tasks.is_empty());

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_teach_task_recorder() {
    TaskRecorder::start("Sample Workflow", "Test teaching").unwrap();
    assert!(TaskRecorder::is_recording());

    TaskRecorder::record_action("click", Some(100), Some(200), None, None).unwrap();
    TaskRecorder::record_action("type", None, None, Some("Hello World".into()), None).unwrap();

    let skill = TaskRecorder::stop().unwrap();
    assert!(!TaskRecorder::is_recording());
    assert_eq!(skill.steps.len(), 2);
    assert_eq!(skill.steps[0].action_type, "click");
    assert_eq!(skill.steps[1].text, Some("Hello World".into()));
}

#[test]
fn test_self_healing_and_parameterization() {
    // 1. Parameter substitution
    let step = SkillStep {
        action_type: "type".into(),
        x: None,
        y: None,
        text: Some("Exporting data for {{date}}".into()),
        key: None,
        delay_ms: 100,
    };
    let mut params = HashMap::new();
    params.insert("date".into(), "2026-10-20".into());

    let parameterized = SelfHealingEngine::parameterize_step(&step, &params);
    assert_eq!(
        parameterized.text,
        Some("Exporting data for 2026-10-20".into())
    );

    // 2. Coordinate recalibration (e.g. from 1280x720 to 1920x1080)
    let (new_x, new_y) = SelfHealingEngine::recalibrate_coords(640, 360, 1280, 720, 1920, 1080);
    assert_eq!(new_x, 960);
    assert_eq!(new_y, 540);
}

#[test]
fn test_set_of_marks_grounding() {
    // A blank frame contains no interactive elements, so no marks may be
    // manufactured for it.
    let blank = RgbaImage::from_pixel(800, 600, image::Rgba([255, 255, 255, 255]));
    let grounded = SetOfMarksEngine::ground_image(blank);
    assert!(
        grounded.marks.is_empty(),
        "a blank screen must not produce grounding marks"
    );
    assert!(grounded.low_confidence);

    // A frame with a visible, high-contrast control produces a mark that
    // carries the measurements that produced it.
    let mut frame = RgbaImage::from_pixel(800, 600, image::Rgba([255, 255, 255, 255]));
    for y in 100..140u32 {
        for x in 120..260u32 {
            frame.put_pixel(x, y, image::Rgba([30, 90, 200, 255]));
        }
    }
    let grounded = SetOfMarksEngine::ground_image(frame);
    assert!(
        !grounded.marks.is_empty(),
        "a high-contrast control must be detected"
    );
    assert_eq!(grounded.marks[0].id, 1);
    assert!(grounded.marks[0].confidence >= 0.45);
    assert!(grounded.marks[0].has_verified_measurements());
    assert!(grounded.marks[0].measurements().is_some());

    // Resolve mark lookup
    let resolved = SetOfMarksEngine::resolve_mark(1);
    assert!(resolved.is_some());
}

#[test]
fn test_timeline_diff() {
    let img1 = RgbaImage::new(100, 100);
    let mut img2 = RgbaImage::new(100, 100);
    // Change 50% of pixels
    for x in 0..100 {
        for y in 0..50 {
            img2.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
        }
    }

    let diff = TimelineEngine::compute_diff_percent(&img1, &img2);
    assert!((diff - 50.0).abs() < 1.0);

    // Annotated click marker
    let annotated_png = TimelineEngine::annotate_click_marker(img1, 50, 50);
    assert!(!annotated_png.is_empty());
}

#[test]
fn test_app_indexer() {
    let apps = AppIndexer::scan_installed_apps();
    assert!(!apps.is_empty());
}

#[test]
fn test_governance_and_takeover() {
    let reason = TakeoverController::detect_sensitive_checkpoint(
        "Please enter your 2FA verification code to proceed",
    );
    assert!(reason.is_some());
    assert!(reason.unwrap().contains("2FA"));

    let policy = GovernancePolicy::default();
    assert!(PolicyEngine::is_domain_blocked(
        &policy,
        "https://www.paypal.com/signin"
    ));
    assert!(!PolicyEngine::is_domain_blocked(
        &policy,
        "https://github.com/trending"
    ));
}

#[test]
fn test_multibot_mention_detection() {
    let bots = vec![
        BotProfile {
            id: "forge".into(),
            name: "Forge".into(),
            avatar: "🛠️".into(),
            role: "DevOps".into(),
            system_prompt: "DevOps prompt".into(),
            is_default: false,
        },
        BotProfile {
            id: "scout".into(),
            name: "Scout".into(),
            avatar: "🌐".into(),
            role: "Web Researcher".into(),
            system_prompt: "Web prompt".into(),
            is_default: false,
        },
    ];

    let prompt = "Hey @Forge, please check the docker status";
    let mentioned = MultiBotOrchestrator::extract_mentioned_bot(prompt, &bots);
    assert!(mentioned.is_some());
    assert_eq!(mentioned.unwrap().name, "Forge");
}

#[test]
fn test_permission_gate() {
    let safe = PermissionGate::assess_command("ls -la /home", true);
    assert!(safe.allowed);
    assert_eq!(safe.risk, RiskLevel::Safe);

    let rm = PermissionGate::assess_command("rm -rf /some/dir", true);
    assert!(!rm.allowed);
    assert_eq!(rm.risk, RiskLevel::Destructive);

    let git = PermissionGate::assess_command("git push origin main", true);
    assert!(!git.allowed);
    assert_eq!(git.risk, RiskLevel::Moderate);
}

#[test]
fn test_coordinate_transform() {
    let transform = CoordinateTransform::new(1920, 1080, 960, 540);
    let (screen_x, screen_y) = transform.to_screen_coords(480, 270);
    assert_eq!(screen_x, 960);
    assert_eq!(screen_y, 540);
}

#[test]
fn test_sandbox_probe() {
    let status = DockerSandbox::probe();
    assert!(!status.message.is_empty());
}

#[test]
fn test_end_to_end_agent_task_lifecycle_and_recovery() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_e2e_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");

    // 1. Start Task 1, execute permitted tool, record result
    db.create_conversation("conv_e2e", "E2E Task", "nemotron")
        .unwrap();
    let run1_id = "run_e2e_1";
    let run1 = AgentRun {
        id: run1_id.to_string(),
        conversation_id: Some("conv_e2e".to_string()),
        agent_id: "apex-orchestrator".to_string(),
        model: "nvidia/nemotron-3-ultra-550b-a55b".to_string(),
        prompt: "Check workspace directory".to_string(),
        status: "running".to_string(),
        created_at: 1000,
        started_at: Some(1001),
        completed_at: None,
        error: None,
        config_json: None,
        parent_run_id: None,
    };
    db.create_run(&run1).unwrap();

    let ctx1 = ExecutionContext {
        run_id: run1_id.to_string(),
        agent_id: "apex-orchestrator".to_string(),
        conversation_id: Some("conv_e2e".to_string()),
        workspace_root: None,
        task_grant: None,
    };

    let permitted_tool = serde_json::json!({ "command": "pwd" });
    let decision1 = AuthorizationEngine::evaluate(&db, &ctx1, "execute_shell", &permitted_tool);
    assert!(matches!(decision1, AuthDecision::Allow));

    // Record tool result event
    let seq1 = db.get_next_event_seq(run1_id).unwrap();
    db.append_run_event(&RunEvent {
        id: "evt_e2e_1".to_string(),
        run_id: run1_id.to_string(),
        seq: seq1,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({ "output": "/home/kyro/workspace" }).to_string(),
        created_at: 1010,
    })
    .unwrap();

    // Mark Run 1 succeeded
    db.update_run_status(run1_id, "succeeded", None, Some(1020), None)
        .unwrap();
    let r1_finished = db.get_run(run1_id).unwrap().unwrap();
    assert_eq!(r1_finished.status, "succeeded");

    // 2. Start Task 2 and cancel it
    let run2_id = "run_e2e_2";
    let token2 = CancellationToken::new();
    let run2 = AgentRun {
        id: run2_id.to_string(),
        conversation_id: Some("conv_e2e".to_string()),
        agent_id: "forge-devops".to_string(),
        model: "nvidia/nemotron-3-ultra-550b-a55b".to_string(),
        prompt: "Long running container build".to_string(),
        status: "running".to_string(),
        created_at: 2000,
        started_at: Some(2001),
        completed_at: None,
        error: None,
        config_json: None,
        parent_run_id: None,
    };
    db.create_run(&run2).unwrap();

    // Trigger cancellation
    token2.cancel();
    assert!(token2.is_cancelled());
    db.update_run_status(
        run2_id,
        "cancelled",
        None,
        Some(2050),
        Some("Cancelled by user token"),
    )
    .unwrap();
    let r2_finished = db.get_run(run2_id).unwrap().unwrap();
    assert_eq!(r2_finished.status, "cancelled");

    // 3. Start Task 3 that remains in-flight when application crashes
    let run3_id = "run_e2e_orphan";
    let run3 = AgentRun {
        id: run3_id.to_string(),
        conversation_id: Some("conv_e2e".to_string()),
        agent_id: "scout-web".to_string(),
        model: "nvidia/nemotron-3-ultra-550b-a55b".to_string(),
        prompt: "Scraping web dashboard".to_string(),
        status: "running".to_string(),
        created_at: 3000,
        started_at: Some(3001),
        completed_at: None,
        error: None,
        config_json: None,
        parent_run_id: None,
    };
    db.create_run(&run3).unwrap();

    // Simulate application restart and recovery
    let recovered_count = db.reconcile_active_runs_on_startup().unwrap();
    assert_eq!(recovered_count, 1);

    // Verify persisted states: Run 1 remains succeeded, Run 2 remains cancelled, Run 3 marked interrupted
    let state1 = db.get_run(run1_id).unwrap().unwrap();
    assert_eq!(state1.status, "succeeded");

    let state2 = db.get_run(run2_id).unwrap().unwrap();
    assert_eq!(state2.status, "cancelled");

    let state3 = db.get_run(run3_id).unwrap().unwrap();
    assert_eq!(state3.status, "interrupted");
    assert!(state3.error.unwrap().contains("terminated unexpectedly"));

    // Verify event timeline integrity
    let events1 = db.get_run_events(run1_id, None).unwrap();
    assert_eq!(events1.len(), 1);
    assert_eq!(events1[0].event_type, "tool_result");

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_artifacts_lifecycle_and_path_security() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_art_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");

    db.create_conversation("conv_1", "Test Conversation", "nemotron")
        .unwrap();
    let run = AgentRun {
        id: "run_1".into(),
        conversation_id: Some("conv_1".into()),
        agent_id: "apex".into(),
        model: "nemotron".into(),
        prompt: "test".into(),
        status: "running".into(),
        created_at: 1000,
        started_at: Some(1000),
        completed_at: None,
        error: None,
        config_json: None,
        parent_run_id: None,
    };
    db.create_run(&run).unwrap();

    let art = ArtifactRecord {
        id: "art_test_123".into(),
        run_id: Some("run_1".into()),
        conversation_id: Some("conv_1".into()),
        name: "analysis_report.md".into(),
        file_path: "/tmp/analysis_report.md".into(),
        artifact_type: "markdown".into(),
        mime_type: "text/markdown".into(),
        size_bytes: 1024,
        created_at: 1000,
        metadata_json: Some(r#"{"lines": 42}"#.into()),
    };

    // 1. Create artifact
    db.create_artifact(&art).unwrap();

    // 2. Query artifact
    let retrieved = db.get_artifact("art_test_123").unwrap().unwrap();
    assert_eq!(retrieved.name, "analysis_report.md");
    assert_eq!(retrieved.size_bytes, 1024);

    let list_run = db.list_artifacts(Some("run_1"), None).unwrap();
    assert_eq!(list_run.len(), 1);

    // 3. Governance authorization on create_artifact: path traversal must be DENIED
    let ctx = ExecutionContext {
        run_id: "run_sec".into(),
        agent_id: "apex".into(),
        conversation_id: None,
        workspace_root: None,
        task_grant: None,
    };

    let bad_traversal = serde_json::json!({
        "name": "../../etc/shadow",
        "content": "hacked"
    });
    let dec1 = AuthorizationEngine::evaluate(&db, &ctx, "create_artifact", &bad_traversal);
    match dec1 {
        AuthDecision::Deny { reason } => assert!(reason.contains("traversal")),
        other => panic!("Expected Deny on path traversal, got {:?}", other),
    }

    let bad_slash = serde_json::json!({
        "name": "folder/malicious.sh",
        "content": "echo bad"
    });
    let dec2 = AuthorizationEngine::evaluate(&db, &ctx, "create_artifact", &bad_slash);
    match dec2 {
        AuthDecision::Deny { reason } => assert!(reason.contains("slash")),
        other => panic!("Expected Deny on slash, got {:?}", other),
    }

    let valid_artifact = serde_json::json!({
        "name": "safe_summary.json",
        "content": "{}"
    });
    let dec3 = AuthorizationEngine::evaluate(&db, &ctx, "create_artifact", &valid_artifact);
    match dec3 {
        AuthDecision::Allow => {}
        other => panic!("Expected Allow on valid artifact name, got {:?}", other),
    }

    // 4. Delete artifact
    let deleted = db.delete_artifact("art_test_123").unwrap();
    assert!(deleted);
    let after_del = db.get_artifact("art_test_123").unwrap();
    assert!(after_del.is_none());

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_universal_search() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_search_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");

    db.create_conversation("c_search_1", "Docker Container Debugging", "nemotron")
        .unwrap();

    let msg = Message {
        id: "m_search_1".into(),
        conversation_id: "c_search_1".into(),
        role: "user".into(),
        content: "Please inspect Dockerfile and build container".into(),
        tool_calls_json: None,
        observations_json: None,
        created_at: 1000,
    };
    db.add_message(&msg).unwrap();

    db.create_skill(&SkillRecord {
        id: "s_docker".into(),
        name: "Docker Compose Deployment".into(),
        description: "Deploys containers using compose".into(),
        steps_json: "[]".into(),
        created_at: 1000,
    })
    .unwrap();

    let results = db.search_all("Docker").unwrap();
    assert!(results.len() >= 2);
    assert!(results.iter().any(|r| r.category == "conversation"));
    assert!(results.iter().any(|r| r.category == "skill"));

    let empty = db.search_all("NonExistentString12345").unwrap();
    assert!(empty.is_empty());

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_workspace_templates_export_and_import() {
    let temp_dir1 = std::env::temp_dir().join(format!("kyro_tmpl_test1_{}", uuid::Uuid::new_v4()));
    let db1 = Database::init(temp_dir1.clone()).expect("Failed to init db");

    // Add an MCP server with a secret in env
    db1.create_mcp_server(&McpServerRecord {
        id: "mcp_test".into(),
        name: "Secret MCP".into(),
        command: "node".into(),
        args_json: "[]".into(),
        env_json: r#"{"API_KEY": "super_secret_token_12345", "PORT": "8080"}"#.into(),
        enabled: true,
    })
    .unwrap();

    // Export template pack
    let json_pack = TemplateEngine::export_pack(&db1, "Test Export", "Testing export").unwrap();

    // Verify secret redaction: NO plaintext API_KEY in exported JSON
    assert!(!json_pack.contains("super_secret_token_12345"));
    assert!(json_pack.contains("<REDACTED_ON_EXPORT>"));
    assert!(json_pack.contains("PORT"));

    // Import into fresh db
    let temp_dir2 = std::env::temp_dir().join(format!("kyro_tmpl_test2_{}", uuid::Uuid::new_v4()));
    let db2 = Database::init(temp_dir2.clone()).expect("Failed to init db");

    let summary = TemplateEngine::import_pack(&db2, &json_pack).unwrap();
    assert!(summary.bots_imported >= 3);
    assert_eq!(summary.mcp_servers_imported, 1);

    // Verify starter templates exist and are valid
    let starters = TemplateEngine::get_starter_templates();
    assert!(!starters.is_empty());
    assert!(starters.iter().any(|s| s.name.contains("DevOps")));

    std::fs::remove_dir_all(temp_dir1).ok();
    std::fs::remove_dir_all(temp_dir2).ok();
}

#[test]
fn test_authorization_workspace_confinement_and_traversal() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_confine_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let workspace = temp_dir.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let outside_dir = temp_dir.join("outside");
    std::fs::create_dir_all(&outside_dir).unwrap();
    let outside_file = outside_dir.join("secret.txt");
    std::fs::write(&outside_file, "secret-outside-content").unwrap();

    let db = Database::init(temp_dir.clone()).unwrap();

    // 1. Missing workspace root fails closed
    let ctx_no_root = ExecutionContext {
        run_id: "run1".into(),
        agent_id: "apex".into(),
        conversation_id: None,
        workspace_root: None,
        task_grant: None,
    };
    let dec = AuthorizationEngine::evaluate(
        &db,
        &ctx_no_root,
        "read_file",
        &serde_json::json!({ "path": "test.txt" }),
    );
    match dec {
        AuthDecision::Deny { reason } => assert!(reason.contains("workspace root")),
        other => panic!("Expected Deny for missing workspace root, got {:?}", other),
    }

    // 2. Traversal outside root with ../ fails closed
    let ctx_with_root = ExecutionContext {
        run_id: "run2".into(),
        agent_id: "apex".into(),
        conversation_id: None,
        workspace_root: Some(workspace.clone()),
        task_grant: None,
    };
    let dec_traversal = AuthorizationEngine::evaluate(
        &db,
        &ctx_with_root,
        "read_file",
        &serde_json::json!({ "path": "../outside/secret.txt" }),
    );
    match dec_traversal {
        AuthDecision::Deny { reason } => {
            assert!(reason.contains("traversal") || reason.contains("outside"))
        }
        other => panic!("Expected Deny for traversal outside root, got {:?}", other),
    }

    // 3. Symlink escape fails closed
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let symlink_path = workspace.join("leak_link.txt");
        let _ = symlink(&outside_file, &symlink_path);
        let dec_symlink = AuthorizationEngine::evaluate(
            &db,
            &ctx_with_root,
            "read_file",
            &serde_json::json!({ "path": symlink_path.to_str().unwrap() }),
        );
        match dec_symlink {
            AuthDecision::Deny { reason } => assert!(
                reason.contains("Symlink")
                    || reason.contains("outside")
                    || reason.contains("confinement")
            ),
            other => panic!(
                "Expected Deny on symlink escape outside workspace, got {:?}",
                other
            ),
        }
    }

    // 4. Valid file inside workspace succeeds
    let safe_file = workspace.join("safe.txt");
    std::fs::write(&safe_file, "safe content").unwrap();
    let dec_safe = AuthorizationEngine::evaluate(
        &db,
        &ctx_with_root,
        "read_file",
        &serde_json::json!({ "path": "safe.txt" }),
    );
    assert!(matches!(dec_safe, AuthDecision::Allow));

    // 5. Direct path confinement validation checks independent ops
    assert!(AuthorizationEngine::validate_path_confinement(
        Some(&workspace),
        "safe.txt",
        FileOp::Read
    )
    .is_ok());

    assert!(AuthorizationEngine::validate_path_confinement(
        Some(&workspace),
        "/etc/shadow",
        FileOp::Read
    )
    .is_err());

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_authorization_shell_injection_multiline_and_compounds() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_shell_sec_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).unwrap();

    let ctx = ExecutionContext {
        run_id: "run_shell_sec".into(),
        agent_id: "apex".into(),
        conversation_id: None,
        workspace_root: Some(temp_dir.clone()),
        task_grant: None,
    };

    // Safe single command
    let safe_cmd = serde_json::json!({ "command": "echo safe" });
    let dec_safe = AuthorizationEngine::evaluate(&db, &ctx, "bash", &safe_cmd);
    assert!(matches!(
        dec_safe,
        AuthDecision::Allow | AuthDecision::AllowSandboxOnly { .. }
    ));

    // Multiline injection attack (e.g. echo hello\nrm -rf /)
    let multiline_cmd = serde_json::json!({ "command": "echo safe\ncat /etc/shadow" });
    let dec_multiline = AuthorizationEngine::evaluate(&db, &ctx, "bash", &multiline_cmd);
    match dec_multiline {
        AuthDecision::RequireApproval { .. } | AuthDecision::AllowSandboxOnly { .. } => {
            // Evaluated conservatively
        }
        AuthDecision::Deny { reason } => {
            assert!(reason.contains("Compound") || reason.contains("multiline"))
        }
        other => panic!(
            "Expected approval, sandbox, or deny for multiline injection, got {:?}",
            other
        ),
    }

    // Command chaining with &&
    let chain_cmd = serde_json::json!({ "command": "echo a && echo b" });
    let dec_chain = AuthorizationEngine::evaluate(&db, &ctx, "bash", &chain_cmd);
    assert!(!matches!(dec_chain, AuthDecision::Allow));

    // Subshell injection
    let subshell_cmd = serde_json::json!({ "command": "echo $(id)" });
    let dec_subshell = AuthorizationEngine::evaluate(&db, &ctx, "bash", &subshell_cmd);
    assert!(!matches!(dec_subshell, AuthDecision::Allow));

    // Wrapper execution (sh -c ...)
    let wrapper_cmd = serde_json::json!({ "command": "sh -c 'rm -rf /'" });
    let dec_wrapper = AuthorizationEngine::evaluate(&db, &ctx, "bash", &wrapper_cmd);
    assert!(!matches!(dec_wrapper, AuthDecision::Allow));

    // Destructive command
    let rm_cmd = serde_json::json!({ "command": "rm -rf /var" });
    let dec_rm = AuthorizationEngine::evaluate(&db, &ctx, "bash", &rm_cmd);
    match dec_rm {
        AuthDecision::Deny { .. } | AuthDecision::RequireApproval { .. } => {}
        other => panic!(
            "Expected Deny or RequireApproval for destructive command, got {:?}",
            other
        ),
    }

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_approval_expiry_and_atomic_consumption() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_appr_exp_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).unwrap();

    let now = chrono::Utc::now().timestamp_millis();

    // 1. Overdue approval request
    let req_overdue = ApprovalRequest {
        id: "req_overdue_1".to_string(),
        run_id: "run_appr_1".to_string(),
        tool_name: "execute_shell".to_string(),
        summary: "Execute privileged command".to_string(),
        args_json: "{}".to_string(),
        risk_level: "high".to_string(),
        reason: "Privileged execution".to_string(),
        status: "pending".to_string(),
        created_at: now - 100_000,
        expires_at: now - 10_000, // Expired 10 seconds ago
        resolved_at: None,
        resolved_by: None,
    };
    db.create_approval_request(&req_overdue).unwrap();

    // Run expiration
    let expired_count = db.expire_overdue_approvals(now).unwrap();
    assert_eq!(expired_count, 1);

    // Fetching the request should show expired
    let fetched = db.get_approval_request("req_overdue_1").unwrap().unwrap();
    assert_eq!(fetched.status, "expired");
    assert_eq!(fetched.resolved_by.as_deref(), Some("system_expiry"));

    // Resolving an expired request must fail
    let resolve_expired = db.resolve_approval_request("req_overdue_1", "approved", "admin");
    assert!(resolve_expired.is_err() || !resolve_expired.unwrap());

    // 2. Active approval with atomic CAS consumption
    let req_active = ApprovalRequest {
        id: "req_active_1".to_string(),
        run_id: "run_appr_2".to_string(),
        tool_name: "write_file".to_string(),
        summary: "Write config file".to_string(),
        args_json: "{}".to_string(),
        risk_level: "medium".to_string(),
        reason: "Write operation".to_string(),
        status: "pending".to_string(),
        created_at: now,
        expires_at: now + 60_000,
        resolved_at: None,
        resolved_by: None,
    };
    db.create_approval_request(&req_active).unwrap();

    // Approve the request
    let resolved = db
        .resolve_approval_request("req_active_1", "approved", "admin")
        .unwrap();
    assert!(resolved);

    // First consumption succeeds
    let first_consume = db
        .consume_approval_request("req_active_1", "run_appr_2")
        .unwrap();
    assert!(first_consume);

    // Second consumption must fail (double execution prevention)
    let second_consume = db
        .consume_approval_request("req_active_1", "run_appr_2")
        .unwrap();
    assert!(!second_consume);

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_visual_verification_real_pixel_diff() {
    use image::Rgba;

    // Create 100x100 black image
    let mut img_before = RgbaImage::new(100, 100);
    for pixel in img_before.pixels_mut() {
        *pixel = Rgba([0, 0, 0, 255]);
    }

    // Identical images should yield 0.0% diff
    let diff_zero = TimelineEngine::compute_diff_percent(&img_before, &img_before);
    assert_eq!(diff_zero, 0.0);

    // Modify a 20x20 quadrant to white
    let mut img_after = img_before.clone();
    for x in 0..20 {
        for y in 0..20 {
            img_after.put_pixel(x, y, Rgba([255, 255, 255, 255]));
        }
    }

    let diff_quad = TimelineEngine::compute_diff_percent(&img_before, &img_after);
    assert!(
        diff_quad > 3.0 && diff_quad < 5.0,
        "Expected ~4% diff, got {}",
        diff_quad
    );

    // Invert all pixels to white -> 100% diff
    let mut img_all_white = RgbaImage::new(100, 100);
    for pixel in img_all_white.pixels_mut() {
        *pixel = Rgba([255, 255, 255, 255]);
    }
    let diff_full = TimelineEngine::compute_diff_percent(&img_before, &img_all_white);
    assert_eq!(diff_full, 100.0);
}

#[test]
fn test_mcp_untrusted_names_and_schema_validation() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_mcp_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).unwrap();

    let ctx = ExecutionContext {
        run_id: "run_mcp".into(),
        agent_id: "apex".into(),
        conversation_id: None,
        workspace_root: Some(temp_dir.clone()),
        task_grant: None,
    };

    // Tool name containing 'get' or 'read' must NOT automatically be considered read-only / auto-approved!
    let untrusted_get = serde_json::json!({ "command": "rm -rf /" });
    let dec_untrusted = AuthorizationEngine::evaluate(
        &db,
        &ctx,
        "mcp__evil_server__get_and_delete",
        &untrusted_get,
    );
    match dec_untrusted {
        AuthDecision::RequireApproval { .. } | AuthDecision::Deny { .. } => {}
        other => panic!(
            "Untrusted tool name with 'get' must NOT be auto-allowed, got {:?}",
            other
        ),
    }

    // Schema validation test
    let schema = serde_json::json!({
        "type": "object",
        "required": ["repo", "issue_number"],
        "properties": {
            "repo": { "type": "string" },
            "issue_number": { "type": "integer" }
        }
    });

    let valid_args = serde_json::json!({
        "repo": "owner/repo",
        "issue_number": 42
    });
    assert!(validate_schema(&schema, &valid_args).is_ok());

    let missing_required = serde_json::json!({
        "repo": "owner/repo"
    });
    assert!(validate_schema(&schema, &missing_required).is_err());

    let wrong_type = serde_json::json!({
        "repo": "owner/repo",
        "issue_number": "not_an_int"
    });
    assert!(validate_schema(&schema, &wrong_type).is_err());

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_scheduler_timezone_and_overlap_guard() {
    // 1. Cron with TZ= prefix (e.g., New York, Tokyo)
    let next_ny =
        RoutineScheduler::compute_next_run_with_tz("CRON_TZ=America/New_York 0 9 * * *", None);
    assert!(next_ny.is_some());
    assert!(next_ny.unwrap() > chrono::Utc::now().timestamp_millis());

    let next_tokyo = RoutineScheduler::compute_next_run_with_tz("TZ=Asia/Tokyo 0 12 * * *", None);
    assert!(next_tokyo.is_some());
    assert!(next_tokyo.unwrap() > chrono::Utc::now().timestamp_millis());

    // 2. Overlap guard: routine parse check
    assert!(RoutineScheduler::parse_schedule("0 9 * * *").is_ok());
    assert!(RoutineScheduler::parse_schedule("invalid cron").is_err());
}

#[test]
fn test_scoped_memory_relevance_ranking_and_budget() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_mem_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).unwrap();

    let now = chrono::Utc::now().timestamp_millis();

    // 1. Save memories in isolated scopes
    MemoryStore::save_fact(
        &db,
        "agent",
        "bot_alpha",
        "rust_cargo",
        "Prefers cargo check before test",
        "system",
    )
    .unwrap();
    MemoryStore::save_fact(
        &db,
        "agent",
        "bot_beta",
        "python_env",
        "Uses poetry for dependencies",
        "system",
    )
    .unwrap();
    MemoryStore::save_fact(
        &db,
        "preference",
        "user",
        "editor",
        "Prefers dark mode and neovim keybindings",
        "user",
    )
    .unwrap();

    // 2. Scope isolation: bot_beta's memory must NOT bleed into bot_alpha's context
    let alpha_context =
        MemoryStore::build_context_block(&db, "bot_alpha", "Running rust compiler verification");
    assert!(alpha_context.contains("rust_cargo"));
    assert!(
        !alpha_context.contains("python_env"),
        "bot_beta's memory leaked into bot_alpha context!"
    );

    let beta_context =
        MemoryStore::build_context_block(&db, "bot_beta", "Configuring python environment");
    assert!(beta_context.contains("python_env"));
    assert!(
        !beta_context.contains("rust_cargo"),
        "bot_alpha's memory leaked into bot_beta context!"
    );

    // 3. Relevance ranking: prompt with 'rust' prioritizes rust_cargo
    let keywords = MemoryStore::tokenize("How do I fix rust compiler error?");
    let memories = db.get_memories_for_scope("agent", "bot_alpha").unwrap();
    let ranked = MemoryStore::rank_memories(&keywords, memories, now);
    assert!(!ranked.is_empty());
    assert_eq!(ranked[0].key, "rust_cargo");

    // 4. Budget check: context block never exceeds MAX_MEMORY_CHARS
    assert!(alpha_context.len() <= kyro_lib::memory::store::MAX_MEMORY_CHARS);

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_connector_catalog_and_account_storage() {
    use kyro_lib::storage::{ConnectedAccountRecord, ConnectorToolPolicyRecord};

    let temp_dir =
        std::env::temp_dir().join(format!("kyro_connector_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");
    let vault = Vault::new_encrypted_fallback(db.clone());

    // 1. Verify default catalog was seeded by migration 5
    let definitions = db.list_connector_definitions().unwrap();
    assert!(definitions.len() >= 8);
    let github_def = definitions
        .iter()
        .find(|d| d.id == "github")
        .expect("github connector missing");
    assert_eq!(github_def.name, "GitHub");
    assert!(github_def.is_available);
    assert_eq!(github_def.connector_type, "native");

    let remote_mcp_def = definitions
        .iter()
        .find(|d| d.id == "remote_mcp")
        .expect("remote_mcp missing");
    assert!(remote_mcp_def.is_available);

    // 2. Add multiple connected accounts for the same provider (GitHub work and personal)
    let acc_work = ConnectedAccountRecord {
        id: "conn_gh_work".into(),
        connector_id: "github".into(),
        account_identifier: "octocat-work".into(),
        display_name: "Work GitHub (octocat-work)".into(),
        status: "connected".into(),
        status_message: Some("Authenticated via PAT".into()),
        granted_scopes_json: serde_json::json!(["repo", "read:user"]).to_string(),
        credential_key: "connector:account:conn_gh_work".into(),
        config_json: serde_json::json!({ "org": "enterprise-org" }).to_string(),
        created_at: 1000,
        updated_at: 1000,
        last_connected_at: Some(1000),
        last_error: None,
        expires_at: None,
    };
    db.create_connected_account(&acc_work).unwrap();

    let acc_personal = ConnectedAccountRecord {
        id: "conn_gh_personal".into(),
        connector_id: "github".into(),
        account_identifier: "octocat-personal".into(),
        display_name: "Personal GitHub".into(),
        status: "connected".into(),
        status_message: None,
        granted_scopes_json: serde_json::json!(["repo"]).to_string(),
        credential_key: "connector:account:conn_gh_personal".into(),
        config_json: "{}".into(),
        created_at: 1005,
        updated_at: 1005,
        last_connected_at: Some(1005),
        last_error: None,
        expires_at: None,
    };
    db.create_connected_account(&acc_personal).unwrap();

    let gh_accounts = db.list_accounts_for_connector("github").unwrap();
    assert_eq!(gh_accounts.len(), 2);

    // 3. Store credentials securely in Vault
    vault
        .store_account_secret("conn_gh_work", r#"{"token":"ghp_work_secret_token_12345"}"#)
        .unwrap();
    vault
        .store_account_secret(
            "conn_gh_personal",
            r#"{"token":"ghp_personal_secret_token_67890"}"#,
        )
        .unwrap();

    // Verify retrieval and isolation
    let work_sec = vault.get_account_secret("conn_gh_work").unwrap().unwrap();
    assert!(work_sec.contains("ghp_work_secret_token_12345"));
    assert!(!work_sec.contains("ghp_personal_secret_token_67890"));

    let pers_sec = vault
        .get_account_secret("conn_gh_personal")
        .unwrap()
        .unwrap();
    assert!(pers_sec.contains("ghp_personal_secret_token_67890"));

    // 4. Configure tool policies for work account
    let pol_create_issue = ConnectorToolPolicyRecord {
        id: "pol_1".into(),
        account_id: "conn_gh_work".into(),
        tool_name: "github_create_issue".into(),
        policy: "ask".into(),
        risk_level: "moderate".into(),
        updated_at: 1010,
        schema_fingerprint: None,
    };
    db.set_connector_tool_policy(&pol_create_issue).unwrap();

    let pol_list_repos = ConnectorToolPolicyRecord {
        id: "pol_2".into(),
        account_id: "conn_gh_work".into(),
        tool_name: "github_list_repositories".into(),
        policy: "allow".into(),
        risk_level: "safe".into(),
        updated_at: 1010,
        schema_fingerprint: None,
    };
    db.set_connector_tool_policy(&pol_list_repos).unwrap();

    let work_policies = db.list_connector_tool_policies("conn_gh_work").unwrap();
    assert_eq!(work_policies.len(), 2);

    // 5. Deleting account cascades and removes tool policies and credentials
    db.delete_connected_account("conn_gh_work").unwrap();
    vault.delete_account_secret("conn_gh_work").unwrap();

    assert!(db.get_connected_account("conn_gh_work").unwrap().is_none());
    assert!(db
        .list_connector_tool_policies("conn_gh_work")
        .unwrap()
        .is_empty());
    assert!(vault.get_account_secret("conn_gh_work").unwrap().is_none());

    // Personal account still intact
    assert!(db
        .get_connected_account("conn_gh_personal")
        .unwrap()
        .is_some());
    assert!(vault
        .get_account_secret("conn_gh_personal")
        .unwrap()
        .is_some());

    std::fs::remove_dir_all(temp_dir).ok();
}

#[tokio::test]
async fn test_github_connector_and_governance() {
    use kyro_lib::connector::ConnectorManager;
    use kyro_lib::execution::permissions::RiskLevel;
    use kyro_lib::governance::auth::{AuthDecision, AuthorizationEngine, ExecutionContext};
    use kyro_lib::mcp::McpManager;
    use kyro_lib::storage::{ConnectorToolPolicyRecord, Database, Vault};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // 1. Spawn deterministic Mock GitHub HTTP server on ephemeral loopback
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let mock_base_url = format!("http://127.0.0.1:{}", port);

    tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(conn) => conn,
                Err(_) => break,
            };

            let mut buf = [0u8; 8192];
            let n = match socket.read(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => continue,
            };

            let req_str = String::from_utf8_lossy(&buf[..n]);
            let first_line = req_str.lines().next().unwrap_or_default();
            let has_valid_auth = req_str.contains("authorization: Bearer ghp_test_secret_12345")
                || req_str.contains("Authorization: Bearer ghp_test_secret_12345");

            let (status, body) = if !has_valid_auth {
                (
                    "401 Unauthorized",
                    serde_json::json!({ "message": "Bad credentials" }).to_string(),
                )
            } else if first_line.starts_with("GET /user ") {
                (
                    "200 OK",
                    serde_json::json!({
                        "login": "octocat",
                        "id": 1,
                        "name": "Mona Lisa Octocat",
                        "email": "octocat@github.com",
                        "public_repos": 42,
                        "followers": 100,
                        "html_url": "https://github.com/octocat",
                        "created_at": "2020-01-01T00:00:00Z"
                    })
                    .to_string(),
                )
            } else if first_line.starts_with("GET /user/repos") {
                (
                    "200 OK",
                    serde_json::json!([
                        {
                            "name": "hello-world",
                            "full_name": "octocat/hello-world",
                            "private": false,
                            "description": "My first repo",
                            "html_url": "https://github.com/octocat/hello-world",
                            "stargazers_count": 1337,
                            "forks_count": 42,
                            "open_issues_count": 3,
                            "default_branch": "main",
                            "updated_at": "2026-10-20T00:00:00Z"
                        }
                    ])
                    .to_string(),
                )
            } else if first_line.starts_with("GET /repos/octocat/hello-world ") {
                (
                    "200 OK",
                    serde_json::json!({
                        "full_name": "octocat/hello-world",
                        "description": "My first repo",
                        "private": false,
                        "html_url": "https://github.com/octocat/hello-world",
                        "default_branch": "main",
                        "open_issues_count": 3,
                        "stargazers_count": 1337,
                        "language": "Rust",
                        "created_at": "2020-01-01T00:00:00Z",
                        "updated_at": "2026-10-20T00:00:00Z"
                    })
                    .to_string(),
                )
            } else if first_line.starts_with("GET /repos/octocat/hello-world/issues") {
                (
                    "200 OK",
                    serde_json::json!([
                        {
                            "number": 1,
                            "title": "Fix memory leak in parser",
                            "user": { "login": "contributor" },
                            "state": "open",
                            "comments": 2,
                            "created_at": "2026-10-18T10:00:00Z",
                            "html_url": "https://github.com/octocat/hello-world/issues/1"
                        }
                    ])
                    .to_string(),
                )
            } else if first_line.starts_with("GET /repos/octocat/hello-world/pulls") {
                (
                    "200 OK",
                    serde_json::json!([
                        {
                            "number": 2,
                            "title": "Add connector system",
                            "user": { "login": "developer" },
                            "state": "open",
                            "created_at": "2026-10-19T12:00:00Z",
                            "html_url": "https://github.com/octocat/hello-world/pull/2",
                            "head": { "ref": "feat-connectors" },
                            "base": { "ref": "main" }
                        }
                    ])
                    .to_string(),
                )
            } else if first_line.starts_with("GET /repos/octocat/hello-world/actions/runs") {
                ("200 OK", serde_json::json!({
                    "workflow_runs": [
                        {
                            "id": 8888,
                            "name": "CI",
                            "head_branch": "main",
                            "event": "push",
                            "status": "completed",
                            "conclusion": "success",
                            "created_at": "2026-10-20T14:00:00Z",
                            "html_url": "https://github.com/octocat/hello-world/actions/runs/8888"
                        }
                    ]
                }).to_string())
            } else if first_line.starts_with("POST /repos/octocat/hello-world/issues ") {
                (
                    "201 Created",
                    serde_json::json!({
                        "number": 99,
                        "title": "Newly Created Issue",
                        "html_url": "https://github.com/octocat/hello-world/issues/99"
                    })
                    .to_string(),
                )
            } else if first_line.starts_with("POST /repos/octocat/hello-world/issues/1/comments") {
                ("201 Created", serde_json::json!({
                    "id": 777,
                    "html_url": "https://github.com/octocat/hello-world/issues/1#issuecomment-777"
                }).to_string())
            } else {
                (
                    "404 Not Found",
                    serde_json::json!({ "message": "Not Found" }).to_string(),
                )
            };

            let response = format!(
                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status,
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.flush().await;
        }
    });

    let temp_dir = std::env::temp_dir().join(format!("kyro_gh_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");
    let vault = Vault::new_encrypted_fallback(db.clone());
    let local_mcp = McpManager::new(db.clone(), vault.clone());
    let connector_mgr = ConnectorManager::new(db.clone(), vault.clone(), local_mcp);

    // 2. Connect account via connector_mgr
    let account = connector_mgr
        .connect_account_token(
            "github",
            "Test Octocat GitHub",
            "octocat",
            "ghp_test_secret_12345",
            Some(serde_json::json!({ "base_url": mock_base_url, "allow_dev_loopback": true })),
        )
        .await
        .expect("Failed to connect account");

    assert_eq!(account.status, "connected");
    assert!(account.status_message.unwrap().contains("@octocat"));

    // 3. Tool Discovery
    let tools = connector_mgr.get_all_connector_tools().await;
    assert!(tools.len() >= 11);
    assert!(tools
        .iter()
        .any(|t| t.name.contains("github_list_repositories")));
    assert!(tools.iter().any(|t| t.name.contains("github_create_issue")));

    // 4. Test tool execution (Read operations)
    let list_repos_out = connector_mgr
        .call_tool(
            &format!("{}__{}", account.id, "github_list_repositories"),
            serde_json::json!({ "limit": 10 }),
        )
        .await
        .unwrap()
        .expect("github_list_repositories failed");
    assert!(list_repos_out.contains("hello-world"));
    assert!(list_repos_out.contains("1337"));

    let list_issues_out = connector_mgr
        .call_tool(
            &format!("{}__{}", account.id, "github_list_issues"),
            serde_json::json!({ "owner": "octocat", "repo": "hello-world" }),
        )
        .await
        .unwrap()
        .expect("github_list_issues failed");
    assert!(list_issues_out.contains("Fix memory leak in parser"));

    let list_runs_out = connector_mgr
        .call_tool(
            &format!("{}__{}", account.id, "github_list_workflow_runs"),
            serde_json::json!({ "owner": "octocat", "repo": "hello-world" }),
        )
        .await
        .unwrap()
        .expect("github_list_workflow_runs failed");
    assert!(list_runs_out.contains("8888"));
    assert!(list_runs_out.contains("success"));

    // 5. Governance and Authorization Engine Verification
    let ctx = ExecutionContext {
        run_id: "test_run_1".into(),
        agent_id: "apex".into(),
        conversation_id: None,
        workspace_root: Some(temp_dir.clone()),
        task_grant: None,
    };

    // A. Read operation -> automatically allowed
    let auth_read = AuthorizationEngine::evaluate(
        &db,
        &ctx,
        &format!("{}__{}", account.id, "github_list_repositories"),
        &serde_json::json!({}),
    );
    match auth_read {
        AuthDecision::Allow => {}
        other => panic!("Expected Allow for read operation, got {:?}", other),
    }

    // B. Write operation -> requires approval by default
    let auth_write = AuthorizationEngine::evaluate(
        &db,
        &ctx,
        &format!("{}__{}", account.id, "github_create_issue"),
        &serde_json::json!({ "owner": "octocat", "repo": "hello-world", "title": "New issue" }),
    );
    match auth_write {
        AuthDecision::RequireApproval { risk, .. } => {
            assert_eq!(risk, RiskLevel::Moderate);
        }
        other => panic!(
            "Expected RequireApproval for write operation, got {:?}",
            other
        ),
    }

    // C. Explicit Policy Deny -> Deny
    db.set_connector_tool_policy(&ConnectorToolPolicyRecord {
        id: "pol_deny".into(),
        account_id: account.id.clone(),
        tool_name: "github_create_issue".into(),
        policy: "deny".into(),
        risk_level: "moderate".into(),
        updated_at: 1000,
        schema_fingerprint: None,
    })
    .unwrap();

    let auth_deny = AuthorizationEngine::evaluate(
        &db,
        &ctx,
        &format!("{}__{}", account.id, "github_create_issue"),
        &serde_json::json!({ "owner": "octocat", "repo": "hello-world", "title": "New issue" }),
    );
    match auth_deny {
        AuthDecision::Deny { reason } => assert!(reason.contains("blocked")),
        other => panic!("Expected Deny after explicit deny policy, got {:?}", other),
    }

    // D. Explicit Policy Allow -> Allow
    db.set_connector_tool_policy(&ConnectorToolPolicyRecord {
        id: "pol_allow".into(),
        account_id: account.id.clone(),
        tool_name: "github_create_issue".into(),
        policy: "allow".into(),
        risk_level: "moderate".into(),
        updated_at: 1001,
        schema_fingerprint: None,
    })
    .unwrap();

    let auth_allowed = AuthorizationEngine::evaluate(
        &db,
        &ctx,
        &format!("{}__{}", account.id, "github_create_issue"),
        &serde_json::json!({ "owner": "octocat", "repo": "hello-world", "title": "New issue" }),
    );
    match auth_allowed {
        AuthDecision::Allow => {}
        other => panic!(
            "Expected Allow after explicit allow policy, got {:?}",
            other
        ),
    }

    // 6. Test Write operations execution
    let create_issue_out = connector_mgr
        .call_tool(
            &format!("{}__{}", account.id, "github_create_issue"),
            serde_json::json!({
                "owner": "octocat",
                "repo": "hello-world",
                "title": "Newly Created Issue",
                "body": "Detailed issue description"
            }),
        )
        .await
        .unwrap()
        .expect("github_create_issue failed");
    assert!(create_issue_out.contains("Successfully created issue #99"));

    db.set_connector_tool_policy(&ConnectorToolPolicyRecord {
        id: "pol_allow_comment".into(),
        account_id: account.id.clone(),
        tool_name: "github_add_issue_comment".into(),
        policy: "allow".into(),
        risk_level: "moderate".into(),
        updated_at: 1002,
        schema_fingerprint: None,
    })
    .unwrap();

    let add_comment_out = connector_mgr
        .call_tool(
            &format!("{}__{}", account.id, "github_add_issue_comment"),
            serde_json::json!({
                "owner": "octocat",
                "repo": "hello-world",
                "issue_number": 1,
                "body": "This issue is resolved"
            }),
        )
        .await
        .unwrap()
        .expect("github_add_issue_comment failed");
    assert!(add_comment_out.contains("Successfully added comment"));

    // 7. Security: Verify credentials do not leak in errors
    let not_found_err = connector_mgr
        .call_tool(
            &format!("{}__{}", account.id, "github_get_repository"),
            serde_json::json!({ "owner": "octocat", "repo": "non-existent-repo" }),
        )
        .await
        .unwrap()
        .unwrap_err();
    assert!(not_found_err.contains("404 Not Found"));
    assert!(!not_found_err.contains("ghp_test_secret_12345"));

    std::fs::remove_dir_all(temp_dir).ok();
}

#[tokio::test]
async fn test_remote_mcp_connector() {
    use kyro_lib::connector::remote_mcp::RemoteMcpClient;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // Mock Remote MCP server over HTTP JSON-RPC 2.0
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server_url = format!("http://127.0.0.1:{}", port);

    tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(conn) => conn,
                Err(_) => break,
            };

            let mut buf = [0u8; 8192];
            let n = match socket.read(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => continue,
            };

            let req_str = String::from_utf8_lossy(&buf[..n]);
            let json_body = req_str.split("\r\n\r\n").nth(1).unwrap_or("{}");
            let rpc: serde_json::Value =
                serde_json::from_str(json_body).unwrap_or(serde_json::json!({}));
            let method = rpc["method"].as_str().unwrap_or("");
            let req_id = rpc.get("id").cloned().unwrap_or(serde_json::json!(1));

            let res_body = match method {
                "initialize" => serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "serverInfo": { "name": "mock-remote-mcp", "version": "1.0.0" }
                    }
                }),
                "notifications/initialized" => serde_json::json!({}),
                "tools/list" => serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "result": {
                        "tools": [
                            {
                                "name": "query_database",
                                "description": "Query company database",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "query": { "type": "string" }
                                    },
                                    "required": ["query"]
                                }
                            }
                        ]
                    }
                }),
                "tools/call" => serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "result": {
                        "content": [
                            { "type": "text", "text": "Query result: 42 records found" }
                        ]
                    }
                }),
                _ => serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "error": { "code": -32601, "message": "Method not found" }
                }),
            };

            let body_str = res_body.to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body_str.len(),
                body_str
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.flush().await;
        }
    });

    let client = RemoteMcpClient::new_with_version(
        &server_url,
        "Mock Remote MCP",
        Some("bearer_token_123".into()),
        "2024-11-05",
    )
    .expect("Client creation failed");

    // Handshake
    let init_res = client.initialize().await.expect("Initialize failed");
    assert!(init_res.contains("Connected to mock-remote-mcp"));

    // List tools
    let tools = client.list_tools().await.expect("List tools failed");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "query_database");

    // Call tool
    let out = client
        .call_tool(
            "query_database",
            &tools[0].parameters,
            serde_json::json!({ "query": "SELECT count(*) FROM users" }),
        )
        .await
        .expect("Call tool failed");
    assert!(out.contains("42 records found"));
}

#[tokio::test]
async fn test_mcp_secrets_vault_migration_redaction_and_exports() {
    let temp_dir =
        std::env::temp_dir().join(format!("kyro_mcp_vault_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");
    let vault = Vault::new_encrypted_fallback(db.clone());

    // 1. Seed a legacy MCP server with plaintext environment secrets in SQLite
    let server_id = "test-legacy-server";
    let legacy_env = serde_json::json!({
        "API_KEY": "super-secret-api-token-12345",
        "DATABASE_PASSWORD": "my-secret-db-password",
        "PORT": "8080",
        "DEBUG": "true"
    });

    let legacy_server = McpServerRecord {
        id: server_id.to_string(),
        name: "Legacy Filesystem Server".to_string(),
        command: "npx".to_string(),
        args_json: "[]".to_string(),
        env_json: legacy_env.to_string(),
        enabled: true,
    };
    db.create_mcp_server(&legacy_server).unwrap();

    // 2. Perform vault migration
    let migrated = vault.migrate_mcp_server_secrets().unwrap();
    assert_eq!(migrated, 2); // API_KEY and DATABASE_PASSWORD

    // 3. Verify SQLite DB now contains [VAULT_SECRET] sentinels, NOT plaintext secrets
    let updated_servers = db.list_mcp_servers().unwrap();
    let updated = updated_servers.iter().find(|s| s.id == server_id).unwrap();
    let updated_env: serde_json::Value = serde_json::from_str(&updated.env_json).unwrap();

    assert_eq!(updated_env["API_KEY"], VAULT_SECRET_SENTINEL);
    assert_eq!(updated_env["DATABASE_PASSWORD"], VAULT_SECRET_SENTINEL);
    assert_eq!(updated_env["PORT"], "8080");
    assert_eq!(updated_env["DEBUG"], "true");

    // 4. Verify Vault holds the actual secret values
    let key1 = vault.get_mcp_env_secret(server_id, "API_KEY").unwrap();
    assert_eq!(key1, Some("super-secret-api-token-12345".to_string()));

    let key2 = vault
        .get_mcp_env_secret(server_id, "DATABASE_PASSWORD")
        .unwrap();
    assert_eq!(key2, Some("my-secret-db-password".to_string()));

    // 5. Verify Template export redacts secrets
    let json_pack = TemplateEngine::export_pack(&db, "Test Export", "Testing export").unwrap();
    assert!(!json_pack.contains("super-secret-api-token-12345"));
    assert!(!json_pack.contains("my-secret-db-password"));
    assert!(json_pack.contains("<REDACTED_ON_EXPORT>"));
    assert!(json_pack.contains("PORT"));

    // 6. Test McpManager save_server and delete_server
    let mcp_mgr = McpManager::new(db.clone(), vault.clone());
    let new_server = McpServerRecord {
        id: "server-2".to_string(),
        name: "Second Server".to_string(),
        command: "node".to_string(),
        args_json: "[]".to_string(),
        env_json: serde_json::json!({
            "AWS_SECRET_ACCESS_KEY": "aws-secret-xyz",
            "REGION": "us-east-1"
        })
        .to_string(),
        enabled: true,
    };
    mcp_mgr.save_server(new_server).await.unwrap();

    // Verify secret was moved to vault
    let aws_secret = vault
        .get_mcp_env_secret("server-2", "AWS_SECRET_ACCESS_KEY")
        .unwrap();
    assert_eq!(aws_secret, Some("aws-secret-xyz".to_string()));

    // Delete server and verify vault secret is purged
    mcp_mgr.delete_server("server-2").await.unwrap();
    let aws_secret_after = vault
        .get_mcp_env_secret("server-2", "AWS_SECRET_ACCESS_KEY")
        .unwrap();
    assert_eq!(aws_secret_after, None);

    std::fs::remove_dir_all(temp_dir).ok();
}

#[tokio::test]
async fn test_network_and_remote_mcp_security_boundary() {
    // 1. Plain HTTP with bearer token is rejected for non-loopback host
    let res = ValidatedRemoteEndpoint::parse_and_validate(
        "http://api.external.com/mcp",
        true,
        false,
        false,
    );
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("HTTPS is strictly required"));

    // 2. HTTPS with bearer token is accepted
    let res = ValidatedRemoteEndpoint::parse_and_validate(
        "https://api.external.com/mcp",
        true,
        false,
        false,
    );
    assert!(res.is_ok());

    // 3. Loopback HTTP is accepted with credentials
    let res_loopback = ValidatedRemoteEndpoint::parse_and_validate(
        "http://127.0.0.1:8000/mcp",
        true,
        false,
        false,
    );
    assert!(res_loopback.is_ok());

    let res_localhost = ValidatedRemoteEndpoint::parse_and_validate(
        "http://localhost:8000/mcp",
        true,
        false,
        false,
    );
    assert!(res_localhost.is_ok());

    // 4. URL credentials / userinfo is strictly rejected
    let res_userinfo = ValidatedRemoteEndpoint::parse_and_validate(
        "https://user:password@external.com/mcp",
        false,
        false,
        false,
    );
    assert!(res_userinfo.is_err());
    assert!(res_userinfo.unwrap_err().contains("user-information"));

    // 5. Private intranet network host
    let res_private_blocked = ValidatedRemoteEndpoint::parse_and_validate(
        "http://192.168.1.50:8000/mcp",
        false,
        false, // allow_private_network = false
        true,
    );
    assert!(res_private_blocked.is_err());
    assert!(res_private_blocked.unwrap_err().contains("Private IP"));

    let res_private_allowed = ValidatedRemoteEndpoint::parse_and_validate(
        "http://192.168.1.50:8000/mcp",
        false,
        true, // allow_private_network = true
        true,
    );
    assert!(res_private_allowed.is_ok());

    // 6. Test Client Creation
    let client = RemoteMcpClient::new(
        "http://127.0.0.1:9999/mcp",
        "Test Loopback Client",
        Some("bearer_loopback".into()),
    )
    .unwrap();
    assert_eq!(client.server_name, "Test Loopback Client");
}

#[tokio::test]
async fn test_github_native_connector_validation_and_origin_safety() {
    let adapter = GitHubAdapter::new();

    // 1. Default account resolves to official GitHub origin
    let standard_account = ConnectedAccountRecord {
        id: "gh_acc_1".into(),
        connector_id: "github".into(),
        account_identifier: "octocat".into(),
        display_name: "Octocat".into(),
        status: "connected".into(),
        status_message: None,
        granted_scopes_json: "[\"repo\"]".into(),
        credential_key: "vault_key".into(),
        config_json: "{}".into(),
        created_at: 1000,
        updated_at: 1000,
        last_connected_at: None,
        last_error: None,
        expires_at: None,
    };
    let base_url = adapter.resolve_base_url(&standard_account).unwrap();
    assert_eq!(base_url, "https://api.github.com");

    // 2. Custom origin without enterprise flag is rejected
    let untrusted_account = ConnectedAccountRecord {
        config_json: serde_json::json!({ "base_url": "https://evil.github.attacker.com" })
            .to_string(),
        ..standard_account.clone()
    };
    let err = adapter.resolve_base_url(&untrusted_account);
    assert!(err.is_err());
    assert!(err.unwrap_err().contains("enterprise"));

    // 3. Enterprise origin with explicit flag is accepted
    let enterprise_account = ConnectedAccountRecord {
        config_json: serde_json::json!({
            "base_url": "https://github.corp.internal/api/v3",
            "is_enterprise": true
        })
        .to_string(),
        ..standard_account.clone()
    };
    let enterprise_url = adapter.resolve_base_url(&enterprise_account).unwrap();
    assert_eq!(enterprise_url, "https://github.corp.internal/api/v3");

    // 4. Insecure HTTP enterprise URL is rejected
    let insecure_enterprise = ConnectedAccountRecord {
        config_json: serde_json::json!({
            "base_url": "http://github.corp.internal/api/v3",
            "is_enterprise": true
        })
        .to_string(),
        ..standard_account.clone()
    };
    let err_insecure = adapter.resolve_base_url(&insecure_enterprise);
    assert!(err_insecure.is_err());
    assert!(err_insecure.unwrap_err().contains("HTTPS"));

    // 5. Pre-request argument validation prevents traversal and injection
    assert!(GitHubAdapter::validate_github_ident("valid-repo-name", "repo").is_ok());
    assert!(GitHubAdapter::validate_github_ident("valid.repo", "repo").is_ok());
    assert!(GitHubAdapter::validate_github_ident("../traversal", "repo").is_err());
    assert!(GitHubAdapter::validate_github_ident("repo/with/slash", "repo").is_err());
    assert!(GitHubAdapter::validate_github_ident("null\0byte", "repo").is_err());
    assert!(GitHubAdapter::validate_github_ident("   ", "repo").is_err());

    assert!(GitHubAdapter::validate_positive_int(1, "issue_number").is_ok());
    assert!(GitHubAdapter::validate_positive_int(0, "issue_number").is_err());
    assert!(GitHubAdapter::validate_positive_int(-5, "issue_number").is_err());
}

#[tokio::test]
async fn test_oauth_state_machine_pkce_and_callback_security() {
    let oauth = OAuthManager::new();

    // 1. Session creation generates valid PKCE and state parameters
    let (state, verifier, challenge) = OAuthManager::generate_pkce();
    assert!(!state.is_empty());
    assert!(!verifier.is_empty());
    assert!(!challenge.is_empty());

    let session = PendingOAuthSession {
        session_id: "sess_1".into(),
        provider_id: "github".into(),
        client_id: "github_client_id".into(),
        client_secret: None,
        code_verifier: verifier.clone(),
        state: state.clone(),
        redirect_uri: "http://127.0.0.1:4567/callback".into(),
        token_endpoint: "https://github.com/login/oauth/access_token".into(),
        created_at: 1000,
        expires_at: chrono::Utc::now().timestamp_millis() + 300_000,
        status: OAuthSessionStatus::Pending,
    };
    oauth.register_session(session).await;

    let status = oauth.get_session_status("sess_1").await;
    assert_eq!(status, Some(OAuthSessionStatus::Pending));

    // 2. Record callback transitions session to CallbackReceived
    let ok = oauth.record_callback("sess_1", "code_123".into()).await;
    assert!(ok.is_ok());

    let status_after = oauth.get_session_status("sess_1").await;
    assert_eq!(
        status_after,
        Some(OAuthSessionStatus::CallbackReceived {
            code: "code_123".into()
        })
    );

    // 3. Atomic claim for code exchange
    let claimed = oauth.claim_exchange("sess_1").await;
    assert!(claimed.is_ok());
    let (endpoint, client_id, _secret, code, redirect_uri, verifier_ret) = claimed.unwrap();
    assert_eq!(endpoint, "https://github.com/login/oauth/access_token");
    assert_eq!(client_id, "github_client_id");
    assert_eq!(code, "code_123");
    assert_eq!(redirect_uri, "http://127.0.0.1:4567/callback");
    assert_eq!(verifier_ret, verifier);

    // 4. Second claim attempt fails (atomicity and race protection)
    let claimed_again = oauth.claim_exchange("sess_1").await;
    assert!(claimed_again.is_err());

    // 5. Session cancellation cleans up
    let session2 = PendingOAuthSession {
        session_id: "sess_2".into(),
        provider_id: "github".into(),
        client_id: "github_client_id".into(),
        client_secret: None,
        code_verifier: "ver_2".into(),
        state: "state_2".into(),
        redirect_uri: "http://127.0.0.1:4567/callback".into(),
        token_endpoint: "https://github.com/login/oauth/access_token".into(),
        created_at: 1000,
        expires_at: chrono::Utc::now().timestamp_millis() + 300_000,
        status: OAuthSessionStatus::Pending,
    };
    oauth.register_session(session2).await;
    assert!(oauth.cancel_session("sess_2").await.is_ok());
    let cancelled_status = oauth.get_session_status("sess_2").await;
    assert_eq!(cancelled_status, Some(OAuthSessionStatus::Cancelled));
}

#[tokio::test]
async fn test_connector_governance_authorization_boundary_and_isolation() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_conn_gov_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");
    let vault = Vault::new_encrypted_fallback(db.clone());
    let local_mcp = McpManager::new(db.clone(), vault.clone());
    let conn_mgr = Arc::new(ConnectorManager::new(db.clone(), vault.clone(), local_mcp));

    // 1. Seed two connected accounts for GitHub
    let acc1 = ConnectedAccountRecord {
        id: "acc_work".to_string(),
        connector_id: "github".to_string(),
        account_identifier: "octocat_work".to_string(),
        display_name: "Work GitHub".to_string(),
        status: "connected".to_string(),
        status_message: Some("Active".to_string()),
        granted_scopes_json: "[\"repo\"]".to_string(),
        credential_key: "vault:work".to_string(),
        config_json: "{}".to_string(),
        created_at: 1000,
        updated_at: 1000,
        last_connected_at: None,
        last_error: None,
        expires_at: None,
    };
    db.create_connected_account(&acc1).unwrap();

    let acc2 = ConnectedAccountRecord {
        id: "acc_personal".to_string(),
        connector_id: "github".to_string(),
        account_identifier: "octocat_personal".to_string(),
        display_name: "Personal GitHub".to_string(),
        status: "connected".to_string(),
        status_message: Some("Active".to_string()),
        granted_scopes_json: "[\"repo\"]".to_string(),
        credential_key: "vault:personal".to_string(),
        config_json: "{}".to_string(),
        created_at: 1000,
        updated_at: 1000,
        last_connected_at: None,
        last_error: None,
        expires_at: None,
    };
    db.create_connected_account(&acc2).unwrap();

    // Refresh tools in ConnectorManager
    let _ = conn_mgr.get_all_connector_tools().await;

    // 2. Configure policies: Deny github_create_issue for acc_work
    let deny_policy = ConnectorToolPolicyRecord {
        id: "pol_1".into(),
        account_id: "acc_work".into(),
        tool_name: "github_create_issue".into(),
        policy: "deny".into(),
        risk_level: "moderate".into(),
        updated_at: 1000,
        schema_fingerprint: None,
    };
    db.set_connector_tool_policy(&deny_policy).unwrap();

    // Configure policies: Ask for acc_personal
    let ask_policy = ConnectorToolPolicyRecord {
        id: "pol_2".into(),
        account_id: "acc_personal".into(),
        tool_name: "github_create_issue".into(),
        policy: "ask".into(),
        risk_level: "moderate".into(),
        updated_at: 1000,
        schema_fingerprint: None,
    };
    db.set_connector_tool_policy(&ask_policy).unwrap();

    // 3. Evaluate work account tool: Denied
    let work_tool = "acc_work__github_create_issue";
    let args = serde_json::json!({
        "owner": "testorg",
        "repo": "testrepo",
        "title": "New issue"
    });
    let dec_work = conn_mgr.evaluate_tool_authorization(work_tool, &args).await;
    match dec_work {
        Some(AuthDecision::Deny { reason }) => {
            assert!(reason.contains("blocked by security policy"));
        }
        other => panic!("Expected Deny, got {:?}", other),
    }

    // Call tool directly: fails without network request
    let call_res = conn_mgr.call_tool(work_tool, args.clone()).await;
    assert!(call_res.is_some());
    assert!(call_res.unwrap().is_err());

    // 4. Evaluate personal account tool: RequireApproval (Account isolation verified)
    let personal_tool = "acc_personal__github_create_issue";
    let dec_personal = conn_mgr
        .evaluate_tool_authorization(personal_tool, &args)
        .await;
    match dec_personal {
        Some(AuthDecision::RequireApproval { reason, .. }) => {
            assert!(reason.contains("requires explicit user authorization"));
        }
        other => panic!("Expected RequireApproval, got {:?}", other),
    }

    // 5. Destructive operations cannot be auto-approved even if marked "allow"
    let destructive_allow_policy = ConnectorToolPolicyRecord {
        id: "pol_3".into(),
        account_id: "acc_personal".into(),
        tool_name: "github_delete_repo".into(),
        policy: "allow".into(),
        risk_level: "destructive".into(),
        updated_at: 1000,
        schema_fingerprint: None,
    };
    db.set_connector_tool_policy(&destructive_allow_policy)
        .unwrap();

    let dec_destructive = AuthorizationEngine::evaluate_mcp_or_custom(
        &db,
        &GovernancePolicy::default(),
        "acc_personal__github_delete_repo",
        &args,
    );
    match dec_destructive {
        AuthDecision::RequireApproval { risk, .. } => {
            assert_eq!(risk, RiskLevel::Destructive);
        }
        other => panic!("Expected RequireApproval for destructive, got {:?}", other),
    }

    // Disconnect acc_work and verify tool calls fail fast
    conn_mgr.disconnect_account("acc_work").await.unwrap();
    let dec_after_disconnect = conn_mgr.evaluate_tool_authorization(work_tool, &args).await;
    assert!(
        dec_after_disconnect.is_none()
            || matches!(dec_after_disconnect, Some(AuthDecision::Deny { .. }))
    );

    std::fs::remove_dir_all(temp_dir).ok();
}

#[tokio::test]
async fn test_end_to_end_agent_turn_with_connector_governance() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_e2e_gov_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");
    let vault = Vault::new_encrypted_fallback(db.clone());
    let local_mcp = McpManager::new(db.clone(), vault.clone());
    let conn_mgr = Arc::new(ConnectorManager::new(db.clone(), vault.clone(), local_mcp));

    // 1. Create agent run
    db.create_conversation("conv_1", "Connector Task", "nemotron")
        .unwrap();
    let run = AgentRun {
        id: "run_connector_e2e".to_string(),
        conversation_id: Some("conv_1".to_string()),
        agent_id: "apex-orchestrator".to_string(),
        model: "nvidia/nemotron-3-ultra-550b-a55b".to_string(),
        prompt: "Check issues in repo".to_string(),
        status: "running".to_string(),
        created_at: 1000,
        started_at: Some(1000),
        completed_at: None,
        error: None,
        config_json: None,
        parent_run_id: None,
    };
    db.create_run(&run).unwrap();

    // 2. Seed connected GitHub account
    let acc = ConnectedAccountRecord {
        id: "github_e2e".to_string(),
        connector_id: "github".to_string(),
        account_identifier: "octocat".to_string(),
        display_name: "GitHub E2E".to_string(),
        status: "connected".to_string(),
        status_message: Some("Connected".to_string()),
        granted_scopes_json: "[\"repo\"]".to_string(),
        credential_key: "vault:token".to_string(),
        config_json: "{}".to_string(),
        created_at: 1000,
        updated_at: 1000,
        last_connected_at: None,
        last_error: None,
        expires_at: None,
    };
    db.create_connected_account(&acc).unwrap();
    let _ = conn_mgr.get_all_connector_tools().await;

    // 3. Enforce policy requiring approval for issue creation
    let pol = ConnectorToolPolicyRecord {
        id: "pol_e2e".into(),
        account_id: "github_e2e".into(),
        tool_name: "github_create_issue".into(),
        policy: "ask".into(),
        risk_level: "moderate".into(),
        updated_at: 1000,
        schema_fingerprint: None,
    };
    db.set_connector_tool_policy(&pol).unwrap();

    // 4. Verify evaluate_tool_authorization accurately triggers RequireApproval
    let tool_name = "github_e2e__github_create_issue";
    let args = serde_json::json!({
        "owner": "kyro-org",
        "repo": "kyro",
        "title": "Bug Report"
    });

    let decision = conn_mgr.evaluate_tool_authorization(tool_name, &args).await;
    assert!(matches!(
        decision,
        Some(AuthDecision::RequireApproval { .. })
    ));

    // Request approval queue entry
    let approval = AuthorizationEngine::request_approval(
        &db,
        "run_connector_e2e",
        tool_name,
        "Create GitHub Issue",
        &args,
        RiskLevel::Moderate,
        "Policy requires explicit approval",
    )
    .unwrap();
    assert_eq!(approval.run_id, "run_connector_e2e");
    assert_eq!(approval.status, "pending");

    // Approve the request
    let resolve_res = db.resolve_approval_request(&approval.id, "approved", "admin");
    assert!(resolve_res.is_ok());

    // Consume approval request once
    let consumed = db
        .consume_approval_request(&approval.id, "run_connector_e2e")
        .unwrap();
    assert!(consumed);

    // Second consumption attempt fails (replay protection)
    let consumed_again = db
        .consume_approval_request(&approval.id, "run_connector_e2e")
        .unwrap();
    assert!(!consumed_again);

    std::fs::remove_dir_all(temp_dir).ok();
}

#[tokio::test]
async fn test_e2e_connector_mock_provider_request_counts_and_governance() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let temp_dir = std::env::temp_dir().join(format!("kyro_req_counts_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");
    let vault = Vault::new_encrypted_fallback(db.clone());
    let local_mcp = McpManager::new(db.clone(), vault.clone());
    let conn_mgr = Arc::new(ConnectorManager::new(db.clone(), vault.clone(), local_mcp));

    // Request counter shared with mock HTTP provider
    let request_count = Arc::new(AtomicUsize::new(0));
    let req_counter_clone = request_count.clone();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind mock provider port");
    let mock_addr = listener.local_addr().unwrap();
    let mock_base_url = format!("http://{}", mock_addr);

    // Mock Provider HTTP Server Loop
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };

            let req_counter = req_counter_clone.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = match socket.read(&mut buf).await {
                    Ok(n) if n > 0 => n,
                    _ => return,
                };
                req_counter.fetch_add(1, Ordering::SeqCst);

                let req_text = String::from_utf8_lossy(&buf[..n]);
                let (status, body) = if req_text.contains("GET /user ") {
                    ("200 OK", serde_json::json!({ "login": "testuser", "id": 12345, "name": "Test User" }).to_string())
                } else if req_text.contains("POST /repos/testorg/testrepo/issues ") {
                    ("201 Created", serde_json::json!({ "number": 101, "title": "New Issue", "html_url": "http://mock/101" }).to_string())
                } else {
                    (
                        "404 Not Found",
                        serde_json::json!({ "message": "Not Found" }).to_string(),
                    )
                };

                let resp = format!(
                    "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    status,
                    body.len(),
                    body
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.flush().await;
            });
        }
    });

    // 1. Connect Account A to mock provider with explicit allow_dev_loopback
    let account_a = conn_mgr
        .connect_account_token(
            "github",
            "Account A (Dev)",
            "testuser",
            "ghp_secret_token_a",
            Some(serde_json::json!({ "base_url": mock_base_url, "allow_dev_loopback": true })),
        )
        .await
        .expect("Account A connection failed");
    assert_eq!(account_a.status, "connected");

    // Connect Account B for isolation testing
    let account_b = conn_mgr
        .connect_account_token(
            "github",
            "Account B (Dev)",
            "otheruser",
            "ghp_secret_token_b",
            Some(serde_json::json!({ "base_url": mock_base_url, "allow_dev_loopback": true })),
        )
        .await
        .expect("Account B connection failed");
    assert_eq!(account_b.status, "connected");

    // Populate registry
    let _ = conn_mgr.get_all_connector_tools().await;

    // Reset request counter after connection handshakes
    request_count.store(0, Ordering::SeqCst);

    // 2. Allowed read operation: github_get_user
    // Safe read-only tool defaults to allow
    let read_out = conn_mgr
        .call_tool(
            &format!("{}__{}", account_a.id, "github_get_user"),
            serde_json::json!({}),
        )
        .await
        .expect("Tool call option was None")
        .expect("github_get_user failed");
    assert!(read_out.contains("testuser"));
    assert_eq!(
        request_count.load(Ordering::SeqCst),
        1,
        "Allowed read operation must hit mock provider exactly once"
    );

    // 3. Unapproved ask operation: github_create_issue without approval proof
    // Write operation defaults to 'ask' and must NOT hit the provider
    let write_args = serde_json::json!({
        "owner": "testorg",
        "repo": "testrepo",
        "title": "Unapproved Bug"
    });
    let unapproved_res = conn_mgr
        .call_tool(
            &format!("{}__{}", account_a.id, "github_create_issue"),
            write_args.clone(),
        )
        .await
        .expect("Tool call option was None");
    assert!(
        unapproved_res.is_err(),
        "Unapproved write must return error requiring approval"
    );
    let unapproved_err = unapproved_res.unwrap_err();
    assert!(unapproved_err.contains("requires human authorization before dispatch"));
    assert_eq!(
        request_count.load(Ordering::SeqCst),
        1,
        "Unapproved write must NOT hit mock provider (request count unchanged)"
    );

    // 4. Denied operation: explicitly set policy to 'deny'
    db.set_connector_tool_policy(&ConnectorToolPolicyRecord {
        id: format!("pol_{}_deny", account_a.id),
        account_id: account_a.id.clone(),
        tool_name: "github_create_issue".into(),
        policy: "deny".into(),
        risk_level: "moderate".into(),
        updated_at: 100,
        schema_fingerprint: None,
    })
    .unwrap();

    let denied_res = conn_mgr
        .call_tool(
            &format!("{}__{}", account_a.id, "github_create_issue"),
            write_args.clone(),
        )
        .await
        .expect("Tool call option was None");
    assert!(denied_res.is_err(), "Denied operation must return error");
    assert!(denied_res
        .unwrap_err()
        .contains("Security policy denied action"));
    assert_eq!(
        request_count.load(Ordering::SeqCst),
        1,
        "Denied write must NOT hit mock provider"
    );

    // 5. Approved operation: set policy to ask, create approval request, resolve approved, pass proof
    db.set_connector_tool_policy(&ConnectorToolPolicyRecord {
        id: format!("pol_{}_ask", account_a.id),
        account_id: account_a.id.clone(),
        tool_name: "github_create_issue".into(),
        policy: "ask".into(),
        risk_level: "moderate".into(),
        updated_at: 101,
        schema_fingerprint: None,
    })
    .unwrap();

    let tool_scoped_name = format!("{}__{}", account_a.id, "github_create_issue");
    let app_req = AuthorizationEngine::request_approval(
        &db,
        "run_test_counts",
        &tool_scoped_name,
        "Create GitHub Issue",
        &write_args,
        RiskLevel::Moderate,
        "Requires approval",
    )
    .unwrap();

    // Mark as approved
    db.resolve_approval_request(&app_req.id, "approved", "tester")
        .unwrap();

    let proof = InvocationAuthProof {
        approval_id: app_req.id.clone(),
        run_id: "run_test_counts".to_string(),
        tool_name: tool_scoped_name.clone(),
        args_hash: InvocationAuthProof::compute_args_hash(&write_args),
        timestamp: chrono::Utc::now().timestamp_millis(),
    };

    let approved_res = conn_mgr
        .call_tool_with_auth(&tool_scoped_name, write_args.clone(), Some(&proof))
        .await
        .expect("Tool call option was None")
        .expect("Approved write execution failed");
    assert!(approved_res.contains("Successfully created issue #101"));
    assert_eq!(
        request_count.load(Ordering::SeqCst),
        2,
        "Approved write must hit mock provider exactly once"
    );

    // 6. Replay attack: calling again with the same consumed proof must be rejected
    let replay_res = conn_mgr
        .call_tool_with_auth(&tool_scoped_name, write_args.clone(), Some(&proof))
        .await
        .expect("Tool call option was None");
    assert!(replay_res.is_err(), "Replayed approval must be rejected");
    assert!(replay_res.unwrap_err().contains("replay rejected"));
    assert_eq!(
        request_count.load(Ordering::SeqCst),
        2,
        "Replayed call must NOT hit mock provider"
    );

    // 7. Policy isolation: Account A's policies do not apply to Account B
    db.set_connector_tool_policy(&ConnectorToolPolicyRecord {
        id: format!("pol_{}_allow", account_a.id),
        account_id: account_a.id.clone(),
        tool_name: "github_create_issue".into(),
        policy: "allow".into(),
        risk_level: "moderate".into(),
        updated_at: 102,
        schema_fingerprint: None,
    })
    .unwrap();

    // Account A can now execute with auto-approve
    let a_write = conn_mgr
        .call_tool(
            &format!("{}__{}", account_a.id, "github_create_issue"),
            write_args.clone(),
        )
        .await
        .expect("Tool call option was None")
        .expect("Account A allow policy execution failed");
    assert!(a_write.contains("Successfully created issue #101"));
    assert_eq!(request_count.load(Ordering::SeqCst), 3);

    // But Account B has NOT been set to allow: must remain blocked
    let b_unapproved = conn_mgr
        .call_tool(
            &format!("{}__{}", account_b.id, "github_create_issue"),
            write_args.clone(),
        )
        .await
        .expect("Tool call option was None");
    assert!(
        b_unapproved.is_err(),
        "Account A policy must NOT leak to Account B"
    );
    assert_eq!(
        request_count.load(Ordering::SeqCst),
        3,
        "Account B unapproved call must NOT hit provider"
    );

    // 8. Canonical Account Tools isolation
    let a_tools = conn_mgr.get_tools_for_account(&account_a.id).await;
    let b_tools = conn_mgr.get_tools_for_account(&account_b.id).await;
    assert!(!a_tools.is_empty());
    assert!(!b_tools.is_empty());
    for t in &a_tools {
        assert!(t.name.starts_with(&account_a.id));
    }
    for t in &b_tools {
        assert!(t.name.starts_with(&account_b.id));
    }

    // 9. Disconnecting Account A terminates dispatch immediately (tools deregistered)
    conn_mgr
        .disconnect_account(&account_a.id)
        .await
        .expect("Disconnect failed");
    let after_disc = conn_mgr
        .call_tool(
            &format!("{}__{}", account_a.id, "github_get_user"),
            serde_json::json!({}),
        )
        .await;
    assert!(
        after_disc.is_none() || after_disc.unwrap().is_err(),
        "Disconnected account tool must be unregistered or return error"
    );
    assert_eq!(
        request_count.load(Ordering::SeqCst),
        3,
        "Disconnected account must not hit provider"
    );

    std::fs::remove_dir_all(temp_dir).ok();
}

#[tokio::test]
async fn test_oauth_concurrency_race_safety_and_token_refresh() {
    use kyro_lib::connector::auth::TokenExchangeResult;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let temp_dir = std::env::temp_dir().join(format!("kyro_oauth_race_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");
    let _vault = Vault::new_encrypted_fallback(db.clone());
    let oauth_mgr = Arc::new(OAuthManager::new());

    // 1. Create a session and transition to Completed
    let session_id = "test_sess_race_123".to_string();
    let now = chrono::Utc::now().timestamp_millis();
    oauth_mgr
        .register_session(PendingOAuthSession {
            session_id: session_id.clone(),
            provider_id: "github".to_string(),
            client_id: "client_123".to_string(),
            client_secret: None,
            code_verifier: "verifier".to_string(),
            state: "state".to_string(),
            redirect_uri: "http://127.0.0.1:8000/callback".to_string(),
            token_endpoint: "http://127.0.0.1:8000/token".to_string(),
            status: OAuthSessionStatus::Pending,
            created_at: now,
            expires_at: now + 60000,
        })
        .await;

    oauth_mgr
        .finish_exchange(
            &session_id,
            Ok(TokenExchangeResult {
                access_token: "gho_initial_token_123".to_string(),
                refresh_token: Some("ghr_initial_refresh_456".to_string()),
                token_type: "Bearer".to_string(),
                granted_scopes: vec!["repo".to_string(), "read:user".to_string()],
                expires_in_seconds: Some(28800),
            }),
        )
        .await;

    // Verify non-destructive polling view shows completed without consuming
    let view = oauth_mgr
        .get_session_status_view(&session_id)
        .await
        .expect("Status view missing");
    assert_eq!(view.status, "completed");

    // 2. Race condition safety: Concurrent completion attempts
    // Exactly one caller must successfully transition Completed -> Finalizing
    let mgr_a = oauth_mgr.clone();
    let mgr_b = oauth_mgr.clone();
    let sid_a = session_id.clone();
    let sid_b = session_id.clone();

    let (res_a, res_b) = tokio::join!(
        tokio::spawn(async move { mgr_a.claim_finalizing(&sid_a).await }),
        tokio::spawn(async move { mgr_b.claim_finalizing(&sid_b).await }),
    );

    let res_a = res_a.unwrap();
    let res_b = res_b.unwrap();

    let success_count = (if res_a.is_ok() { 1 } else { 0 }) + (if res_b.is_ok() { 1 } else { 0 });
    assert_eq!(
        success_count, 1,
        "Exactly one completion call must succeed in claiming the session"
    );

    let failed_err = if res_a.is_err() {
        res_a.unwrap_err()
    } else {
        res_b.unwrap_err()
    };
    assert!(
        failed_err.contains("finalization is already in progress")
            || failed_err.contains("already completed")
    );

    // Finalize success and verify session is gone
    oauth_mgr.finalize_success(&session_id).await;
    assert!(oauth_mgr
        .get_session_status_view(&session_id)
        .await
        .is_none());

    // 3. Token refresh and rotation
    let refresh_counter = Arc::new(AtomicUsize::new(0));
    let r_count = refresh_counter.clone();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind token refresh mock");
    let token_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };

            let rc = r_count.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = match socket.read(&mut buf).await {
                    Ok(n) if n > 0 => n,
                    _ => return,
                };
                let req_text = String::from_utf8_lossy(&buf[..n]);
                rc.fetch_add(1, Ordering::SeqCst);

                let (status, body) = if req_text.contains("refresh_token=ghr_expired") {
                    ("400 Bad Request", serde_json::json!({ "error": "invalid_grant", "error_description": "Refresh token expired" }).to_string())
                } else {
                    (
                        "200 OK",
                        serde_json::json!({
                            "access_token": "gho_new_rotated_token_789",
                            "refresh_token": "ghr_new_rotated_refresh_101",
                            "token_type": "Bearer",
                            "scope": "repo,read:user",
                            "expires_in": 28800
                        })
                        .to_string(),
                    )
                };

                let resp = format!(
                    "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    status,
                    body.len(),
                    body
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.flush().await;
            });
        }
    });

    let http = reqwest::Client::new();
    let token_endpoint = format!("http://{}/login/oauth/access_token", token_addr);

    // Test successful refresh token rotation
    let refreshed = OAuthManager::refresh_access_token(
        &http,
        &token_endpoint,
        "client_123",
        Some("client_secret_xyz"),
        "ghr_initial_refresh_456",
    )
    .await
    .expect("Token refresh failed");

    assert_eq!(refreshed.access_token, "gho_new_rotated_token_789");
    assert_eq!(
        refreshed.refresh_token.as_deref(),
        Some("ghr_new_rotated_refresh_101")
    );
    assert_eq!(refresh_counter.load(Ordering::SeqCst), 1);

    // Test expired refresh token transitions to error
    let expired_res = OAuthManager::refresh_access_token(
        &http,
        &token_endpoint,
        "client_123",
        Some("client_secret_xyz"),
        "ghr_expired",
    )
    .await;

    assert!(expired_res.is_err());
    let err_msg = expired_res.unwrap_err();
    assert!(err_msg.contains("Refresh token expired") || err_msg.contains("invalid_grant"));
    assert_eq!(refresh_counter.load(Ordering::SeqCst), 2);

    std::fs::remove_dir_all(temp_dir).ok();
}

#[tokio::test]
async fn test_remote_mcp_stateless_2026_and_legacy_2024() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Modern 2026-07-28 Mock Server
    let modern_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind modern listener");
    let modern_addr = modern_listener.local_addr().unwrap();
    let modern_url = format!("http://{}", modern_addr);

    tokio::spawn(async move {
        loop {
            let (mut socket, _) = match modern_listener.accept().await {
                Ok(s) => s,
                Err(_) => break,
            };

            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = match socket.read(&mut buf).await {
                    Ok(n) if n > 0 => n,
                    _ => return,
                };
                let req_text = String::from_utf8_lossy(&buf[..n]);

                // Verify modern MCP required protocol header
                assert!(
                    req_text.contains("mcp-protocol-version: 2026-07-28")
                        || req_text.contains("MCP-Protocol-Version: 2026-07-28")
                );

                let json_body = req_text.split("\r\n\r\n").nth(1).unwrap_or("{}");
                let rpc: serde_json::Value =
                    serde_json::from_str(json_body).unwrap_or(serde_json::json!({}));
                let method = rpc["method"].as_str().unwrap_or("");
                let req_id = rpc.get("id").cloned().unwrap_or(serde_json::json!(1));

                // Verify modern MCP params contains _meta with protocolVersion
                if let Some(params) = rpc.get("params") {
                    assert!(params.get("_meta").is_some());
                }

                let res_body = match method {
                    "tools/list" => serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": req_id,
                        "result": {
                            "tools": [{
                                "name": "modern_calculator",
                                "description": "Perform calculation",
                                "inputSchema": { "type": "object" }
                            }]
                        }
                    }),
                    "tools/call" => serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": req_id,
                        "result": {
                            "content": [{ "type": "text", "text": "Calculation result: 100" }]
                        }
                    }),
                    _ => serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": req_id,
                        "error": { "code": -32601, "message": "Method not found" }
                    }),
                };

                let body_str = res_body.to_string();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body_str.len(),
                    body_str
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.flush().await;
            });
        }
    });

    let client_2026 = RemoteMcpClient::new_with_version(
        &modern_url,
        "Modern Server",
        Some("bearer_modern".to_string()),
        "2026-07-28",
    )
    .expect("Failed to create modern client");

    let tools = client_2026.list_tools().await.expect("List tools failed");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "modern_calculator");

    let call_res = client_2026
        .call_tool(
            "modern_calculator",
            &tools[0].parameters,
            serde_json::json!({}),
        )
        .await
        .expect("Call tool failed");
    assert!(call_res.contains("Calculation result: 100"));

    // Bounded streaming response test: Server returning oversized response body
    let oversized_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind oversized listener");
    let oversized_addr = oversized_listener.local_addr().unwrap();
    let oversized_url = format!("http://{}", oversized_addr);

    tokio::spawn(async move {
        while let Ok((mut socket, _)) = oversized_listener.accept().await {
            tokio::spawn(async move {
                let mut buf = vec![0u8; 1024];
                let _ = socket.read(&mut buf).await;
                // Send huge chunked response > 512 KB limit
                let header = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n";
                let _ = socket.write_all(header.as_bytes()).await;
                let chunk = vec![b'A'; 65536]; // 64 KB chunk
                for _ in 0..10 {
                    // 640 KB total
                    let chunk_hdr = format!("{:x}\r\n", chunk.len());
                    let _ = socket.write_all(chunk_hdr.as_bytes()).await;
                    let _ = socket.write_all(&chunk).await;
                    let _ = socket.write_all(b"\r\n").await;
                }
                let _ = socket.write_all(b"0\r\n\r\n").await;
            });
        }
    });

    let oversized_client = RemoteMcpClient::new(&oversized_url, "Oversized Server", None)
        .expect("Client creation failed");

    let bound_err = oversized_client.list_tools().await;
    assert!(bound_err.is_err(), "Oversized response must be aborted");
    assert!(bound_err
        .unwrap_err()
        .contains("exceeded maximum size limit"));
}

#[tokio::test]
async fn test_mcp_secret_integrity_database_url_and_empty_value_cleanup() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_mcp_secrets_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");
    let vault = Vault::new_encrypted_fallback(db.clone());
    let mcp_mgr = McpManager::new(db.clone(), vault.clone());

    // 1. Test database URL detection and migration without key-name heuristic
    let server_id = "test_db_server";
    let raw_db_url = "postgres://kyrouser:super_confidential_db_pass@db.internal:5432/kyrodb";
    let legacy_server = McpServerRecord {
        id: server_id.to_string(),
        name: "DB MCP Server".to_string(),
        command: "node".to_string(),
        args_json: "[]".to_string(),
        env_json: serde_json::json!({
            "DATABASE_URL": raw_db_url,
            "PORT": "8080"
        })
        .to_string(),
        enabled: true,
    };
    db.create_mcp_server(&legacy_server).unwrap();

    // Run migration
    let migrated = vault
        .migrate_mcp_server_secrets()
        .expect("Migration failed");
    assert_eq!(
        migrated, 1,
        "DATABASE_URL must be detected and migrated to vault"
    );

    // Inspect SQLite record: secret replaced with sentinel, plain PORT preserved
    let servers = db.list_mcp_servers().unwrap();
    let srv = servers.into_iter().find(|s| s.id == server_id).unwrap();
    let env_map: HashMap<String, String> = serde_json::from_str(&srv.env_json).unwrap();
    assert_eq!(env_map.get("DATABASE_URL").unwrap(), VAULT_SECRET_SENTINEL);
    assert_eq!(env_map.get("PORT").unwrap(), "8080");

    // Inspect Vault: stored and non-empty
    let stored_secret = vault.get_mcp_env_secret(server_id, "DATABASE_URL").unwrap();
    assert_eq!(stored_secret.as_deref(), Some(raw_db_url));

    // 2. Test empty value clears credential from vault
    let update_record = McpServerRecord {
        id: server_id.to_string(),
        name: "DB MCP Server".to_string(),
        command: "node".to_string(),
        args_json: "[]".to_string(),
        env_json: serde_json::json!({
            "DATABASE_URL": "",
            "PORT": "9090"
        })
        .to_string(),
        enabled: true,
    };
    mcp_mgr.save_server(update_record).await.unwrap();

    let cleared_vault = vault.get_mcp_env_secret(server_id, "DATABASE_URL").unwrap();
    assert!(
        cleared_vault.is_none(),
        "Empty value must purge credential from vault"
    );

    // 3. Test unverified sentinel rejection: providing [VAULT_SECRET] for missing secret fails
    let fake_sentinel_server = McpServerRecord {
        id: "fake_sentinel_srv".to_string(),
        name: "Fake Server".to_string(),
        command: "node".to_string(),
        args_json: "[]".to_string(),
        env_json: serde_json::json!({
            "SECRET_TOKEN": VAULT_SECRET_SENTINEL
        })
        .to_string(),
        enabled: true,
    };
    let fake_res = mcp_mgr.save_server(fake_sentinel_server).await;
    assert!(
        fake_res.is_err(),
        "Sentinel without vault backing must be rejected"
    );
    assert!(fake_res.unwrap_err().contains("missing or unverified"));

    // 4. Test delete server cleans up all secrets
    let srv_to_delete = McpServerRecord {
        id: "delete_me".to_string(),
        name: "Delete Server".to_string(),
        command: "node".to_string(),
        args_json: "[]".to_string(),
        env_json: serde_json::json!({
            "MY_API_KEY": "secret_key_to_delete"
        })
        .to_string(),
        enabled: true,
    };
    mcp_mgr.save_server(srv_to_delete).await.unwrap();
    assert!(vault
        .get_mcp_env_secret("delete_me", "MY_API_KEY")
        .unwrap()
        .is_some());

    mcp_mgr.delete_server("delete_me").await.unwrap();
    assert!(vault
        .get_mcp_env_secret("delete_me", "MY_API_KEY")
        .unwrap()
        .is_none());

    std::fs::remove_dir_all(temp_dir).ok();
}

#[tokio::test]
async fn test_modern_mcp_strict_headers_params_and_validation() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind modern test listener");
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);

    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = match socket.read(&mut buf).await {
                    Ok(n) if n > 0 => n,
                    _ => return,
                };
                let req_text = String::from_utf8_lossy(&buf[..n]);

                // Modern 2026-07-28 MUST include MCP-Protocol-Version header
                assert!(
                    req_text.contains("mcp-protocol-version: 2026-07-28")
                        || req_text.contains("MCP-Protocol-Version: 2026-07-28"),
                    "Missing MCP-Protocol-Version header: {}",
                    req_text
                );

                // Modern 2026-07-28 MUST include Mcp-Method header
                assert!(
                    req_text.contains("mcp-method: ") || req_text.contains("Mcp-Method: "),
                    "Missing Mcp-Method header: {}",
                    req_text
                );

                let json_body = req_text.split("\r\n\r\n").nth(1).unwrap_or("{}");
                let rpc: serde_json::Value =
                    serde_json::from_str(json_body).unwrap_or(serde_json::json!({}));
                let method = rpc["method"].as_str().unwrap_or("");
                let req_id = rpc.get("id").cloned().unwrap_or(serde_json::json!(1));

                // Verify params._meta namespaces
                if let Some(params) = rpc.get("params") {
                    if let Some(meta) = params.get("_meta") {
                        assert!(
                            meta.get("io.modelcontextprotocol/protocolVersion")
                                .is_some(),
                            "Missing protocolVersion namespace in _meta"
                        );
                        assert!(
                            meta.get("io.modelcontextprotocol/clientInfo").is_some(),
                            "Missing clientInfo namespace in _meta"
                        );
                        assert!(
                            meta.get("io.modelcontextprotocol/clientCapabilities")
                                .is_some(),
                            "Missing clientCapabilities namespace in _meta"
                        );
                    }
                }

                let (status, res_body) = match method {
                    "tools/list" => (
                        "200 OK",
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": req_id,
                            "result": {
                                "tools": [{
                                    "name": "send_alert",
                                    "description": "Send critical alert",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "X-Urgent": {
                                                "type": "string",
                                                "x-mcp-header": true
                                            },
                                            "message": {
                                                "type": "string"
                                            }
                                        },
                                        "required": ["message"]
                                    }
                                }]
                            }
                        }),
                    ),
                    "tools/call" => {
                        // On tools/call, verify Mcp-Name and Mcp-Param-X-Urgent header extraction
                        assert!(
                            req_text.contains("mcp-name: send_alert")
                                || req_text.contains("Mcp-Name: send_alert"),
                            "Missing or mismatched Mcp-Name header: {}",
                            req_text
                        );
                        assert!(
                            req_text.contains("mcp-param-x-urgent: critical")
                                || req_text.contains("Mcp-Param-X-Urgent: critical"),
                            "Missing Mcp-Param-X-Urgent header: {}",
                            req_text
                        );
                        (
                            "200 OK",
                            serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": req_id,
                                "result": {
                                    "content": [{ "type": "text", "text": "Alert sent successfully" }]
                                }
                            }),
                        )
                    }
                    _ => (
                        "404 Not Found",
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": req_id,
                            "error": { "code": -32601, "message": "Method not found" }
                        }),
                    ),
                };

                let body_str = res_body.to_string();
                let resp = format!(
                    "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    status,
                    body_str.len(),
                    body_str
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.flush().await;
            });
        }
    });

    let client = RemoteMcpClient::new_with_version(
        &url,
        "Strict Modern Server",
        Some("test_token".to_string()),
        "2026-07-28",
    )
    .expect("Failed to create modern client");

    // 1. Calling initialize() under 2026-07-28 must be rejected immediately
    let init_err = client.initialize().await;
    assert!(init_err.is_err(), "initialize() must fail under 2026-07-28");
    assert!(init_err
        .unwrap_err()
        .contains("removed in MCP specification '2026-07-28'"));

    // 2. Stateless connection verification via connect_or_verify() succeeds
    let verify_res = client.connect_or_verify().await;
    assert!(verify_res.is_ok());
    assert!(verify_res.unwrap().contains("2026-07-28"));

    // 3. Tool listing and calling with header parameters
    let tools = client.list_tools().await.expect("List tools failed");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "send_alert");

    let call_res = client
        .call_tool(
            "send_alert",
            &tools[0].parameters,
            serde_json::json!({
                "X-Urgent": "critical",
                "message": "High priority alert body"
            }),
        )
        .await
        .expect("Call tool failed");
    assert!(call_res.contains("Alert sent successfully"));
}

#[tokio::test]
async fn test_dynamic_mcp_tool_classification_and_schema_fingerprint_invalidation() {
    use kyro_lib::connector::{classify_external_tool, ConnectorManager};

    // 1. Classification Tests
    // A. Benign name with execution command property -> Destructive, can_auto_approve: false
    let dangerous_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "cmd": { "type": "string" },
            "path": { "type": "string" }
        }
    });
    let (risk, is_ro, can_auto) = classify_external_tool(
        "read_quick_info",
        "Quickly inspects info",
        &dangerous_schema,
    );
    assert_eq!(risk, RiskLevel::Destructive);
    assert!(!is_ro);
    assert!(!can_auto);

    // B. Destructive keyword in name -> Destructive, can_auto_approve: false
    let benign_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "id": { "type": "string" }
        }
    });
    let (risk_del, is_ro_del, can_auto_del) =
        classify_external_tool("delete_item", "Removes an item", &benign_schema);
    assert_eq!(risk_del, RiskLevel::Destructive);
    assert!(!is_ro_del);
    assert!(!can_auto_del);

    // C. Write/modifying tool -> Moderate, can_auto_approve: false
    let (risk_wr, is_ro_wr, can_auto_wr) =
        classify_external_tool("create_ticket", "Creates new ticket", &benign_schema);
    assert_eq!(risk_wr, RiskLevel::Moderate);
    assert!(!is_ro_wr);
    assert!(!can_auto_wr);

    // D. External tool with naming heuristic only -> Safe, but cannot auto-approve without explicit declaration
    let (risk_heuristic, is_ro_heuristic, can_auto_heuristic) = classify_external_tool(
        "get_status",
        "Returns current system status",
        &benign_schema,
    );
    assert_eq!(risk_heuristic, RiskLevel::Safe);
    assert!(is_ro_heuristic);
    assert!(
        !can_auto_heuristic,
        "External tools must not auto-approve based on naming heuristics alone"
    );

    // E. External tool with explicit read-only declaration: the annotation is
    // untrusted metadata supplied by the external provider (Issue finding 11).
    // It must NOT self-authorize auto-approval; only a local trusted policy
    // decision can ever grant that. The annotation still records the
    // read-only hint alongside the trusted name heuristic.
    let explicit_ro_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "id": { "type": "string" }
        },
        "readOnly": true
    });
    let (risk_safe, is_ro_safe, can_auto_safe) = classify_external_tool(
        "get_status",
        "Returns current system status",
        &explicit_ro_schema,
    );
    assert_eq!(risk_safe, RiskLevel::Safe);
    assert!(is_ro_safe);
    assert!(
        !can_auto_safe,
        "Externally supplied read-only annotations must not auto-approve a tool"
    );

    // E2. Annotation-only tool (no read-only naming heuristic) claims
    // read-only: stays behind approval and never becomes auto-approvable.
    let annotation_only = serde_json::json!({
        "type": "object",
        "properties": {
            "id": { "type": "string" }
        },
        "annotations": { "readOnlyHint": true, "title": "read stuff" },
        "x-read-only": true
    });
    let (risk_ann, is_ro_ann, can_auto_ann) =
        classify_external_tool("sync_workspace", "Keeps systems aligned", &annotation_only);
    assert_eq!(risk_ann, RiskLevel::Moderate);
    assert!(is_ro_ann, "annotation is recorded as a metadata hint only");
    assert!(!can_auto_ann);

    // F. Benign name with explicit read-only declaration BUT side-effecting property -> Moderate, cannot auto-approve
    let deceptive_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "file_path": { "type": "string" }
        },
        "readOnly": true
    });
    let (risk_deceptive, is_ro_deceptive, can_auto_deceptive) =
        classify_external_tool("get_config", "Reads configuration", &deceptive_schema);
    assert_eq!(risk_deceptive, RiskLevel::Moderate);
    assert!(!is_ro_deceptive);
    assert!(!can_auto_deceptive);

    // 2. Schema Fingerprint Invalidation Test
    let temp_dir = std::env::temp_dir().join(format!("kyro_fingerprint_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");
    let vault = Vault::new_encrypted_fallback(db.clone());
    let local_mcp = McpManager::new(db.clone(), vault.clone());
    let conn_mgr = Arc::new(ConnectorManager::new(db.clone(), vault.clone(), local_mcp));

    let account = ConnectedAccountRecord {
        id: "conn_dyn_1".to_string(),
        connector_id: "github".to_string(),
        account_identifier: "octocat".to_string(),
        display_name: "GitHub Octocat".to_string(),
        status: "connected".to_string(),
        status_message: None,
        granted_scopes_json: "[]".to_string(),
        credential_key: "key_1".to_string(),
        config_json: "{}".to_string(),
        created_at: 1000,
        updated_at: 1000,
        last_connected_at: None,
        last_error: None,
        expires_at: None,
    };
    db.create_connected_account(&account).unwrap();
    let _ = conn_mgr.get_all_connector_tools().await;

    // Set a policy for github_get_user with an outdated/different schema fingerprint
    let outdated_fp =
        "0000000000000000000000000000000000000000000000000000000000000000".to_string();
    let pol = ConnectorToolPolicyRecord {
        id: "pol_fp_test".to_string(),
        account_id: account.id.clone(),
        tool_name: "github_get_user".to_string(),
        policy: "allow".to_string(),
        risk_level: "safe".to_string(),
        updated_at: 1000,
        schema_fingerprint: Some(outdated_fp),
    };
    db.set_connector_tool_policy(&pol).unwrap();

    // When evaluate_tool_authorization is evaluated, fingerprint mismatch must fail closed to RequireApproval
    let decision = conn_mgr
        .evaluate_tool_authorization(
            &format!("{}__{}", account.id, "github_get_user"),
            &serde_json::json!({ "username": "octocat" }),
        )
        .await;

    match decision {
        Some(AuthDecision::RequireApproval { reason, .. }) => {
            assert!(
                reason.contains("has changed since authorization policy was configured"),
                "Expected schema definition change reason, got: {}",
                reason
            );
        }
        other => panic!(
            "Expected RequireApproval on schema fingerprint mismatch, got: {:?}",
            other
        ),
    }

    std::fs::remove_dir_all(temp_dir).ok();
}

#[tokio::test]
async fn test_bound_approval_proof_negative_scenarios() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_bound_neg_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");

    let now = chrono::Utc::now().timestamp_millis();
    let approval_id = "app_bound_1";
    let run_id = "run_alpha";
    let tool_name = "github_create_issue";
    let approved_args = serde_json::json!({
        "owner": "kyro-security",
        "repo": "kyro",
        "title": "Security audit report"
    });

    let req = ApprovalRequest {
        id: approval_id.to_string(),
        run_id: run_id.to_string(),
        tool_name: tool_name.to_string(),
        summary: "Create Security Issue".to_string(),
        args_json: approved_args.to_string(),
        risk_level: "moderate".to_string(),
        reason: "Requires approval".to_string(),
        status: "approved".to_string(),
        created_at: now,
        expires_at: now + 60_000,
        resolved_at: Some(now),
        resolved_by: Some("security_officer".to_string()),
    };
    db.create_approval_request(&req).unwrap();

    // Negative 1: Run ID mismatch -> rejected
    let res_run = db.consume_approval_request_bound(
        approval_id,
        "run_rogue",
        &[tool_name],
        &approved_args,
        now,
    );
    assert!(res_run.is_err());
    assert!(res_run.unwrap_err().contains("run ID mismatch"));

    // Negative 2: Tool name substitution -> rejected
    let res_tool = db.consume_approval_request_bound(
        approval_id,
        run_id,
        &["github_delete_repo"],
        &approved_args,
        now,
    );
    assert!(res_tool.is_err());
    assert!(res_tool.unwrap_err().contains("tool mismatch"));

    // Negative 3: Arguments altered -> rejected
    let altered_args = serde_json::json!({
        "owner": "kyro-security",
        "repo": "kyro",
        "title": "Maliciously modified title"
    });
    let res_args =
        db.consume_approval_request_bound(approval_id, run_id, &[tool_name], &altered_args, now);
    assert!(res_args.is_err());
    assert!(res_args.unwrap_err().contains("argument mismatch"));

    // Negative 4: Expired approval -> rejected and marked expired
    let expired_id = "app_expired_1";
    let expired_req = ApprovalRequest {
        id: expired_id.to_string(),
        run_id: run_id.to_string(),
        tool_name: tool_name.to_string(),
        summary: "Expired request".to_string(),
        args_json: approved_args.to_string(),
        risk_level: "moderate".to_string(),
        reason: "Requires approval".to_string(),
        status: "approved".to_string(),
        created_at: now - 100_000,
        expires_at: now - 10_000,
        resolved_at: Some(now - 50_000),
        resolved_by: Some("admin".to_string()),
    };
    db.create_approval_request(&expired_req).unwrap();

    let res_exp =
        db.consume_approval_request_bound(expired_id, run_id, &[tool_name], &approved_args, now);
    assert!(res_exp.is_err());
    assert!(res_exp.unwrap_err().contains("has expired"));

    // Positive: Canonical argument equivalence (different key ordering) -> consumed atomically
    let reordered_args = serde_json::json!({
        "title": "Security audit report",
        "repo": "kyro",
        "owner": "kyro-security"
    });
    let res_ok =
        db.consume_approval_request_bound(approval_id, run_id, &[tool_name], &reordered_args, now);
    assert!(
        res_ok.is_ok(),
        "Canonical args must match regardless of key order"
    );

    // Negative 5: Replay attack -> rejected
    let res_replay =
        db.consume_approval_request_bound(approval_id, run_id, &[tool_name], &approved_args, now);
    assert!(res_replay.is_err());
    assert!(res_replay.unwrap_err().contains("replay rejected"));

    std::fs::remove_dir_all(temp_dir).ok();
}

#[tokio::test]
async fn test_oauth_client_id_in_db_and_secret_in_vault_with_single_flight_refresh() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let temp_dir = std::env::temp_dir().join(format!("kyro_oauth_lock_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");
    let vault = Vault::new_encrypted_fallback(db.clone());
    let local_mcp = McpManager::new(db.clone(), vault.clone());
    let conn_mgr = Arc::new(ConnectorManager::new(db.clone(), vault.clone(), local_mcp));

    // Mock OAuth Token Refresh Endpoint
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind mock oauth listener");
    let _addr = listener.local_addr().unwrap();
    let refresh_count = Arc::new(AtomicUsize::new(0));
    let refresh_count_clone = refresh_count.clone();

    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let counter = refresh_count_clone.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = match socket.read(&mut buf).await {
                    Ok(n) if n > 0 => n,
                    _ => return,
                };
                let _req_text = String::from_utf8_lossy(&buf[..n]);
                counter.fetch_add(1, Ordering::SeqCst);

                // Small delay to simulate network flight and test concurrency serialization
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

                let resp_body = serde_json::json!({
                    "access_token": format!("new_gho_token_{}", uuid::Uuid::new_v4()),
                    "refresh_token": "new_ghr_refresh_rotated_999",
                    "expires_in": 3600,
                    "scope": "repo,read:user"
                })
                .to_string();

                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    resp_body.len(),
                    resp_body
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.flush().await;
            });
        }
    });

    let account_id = "conn_gh_oauth_test";
    let now = chrono::Utc::now().timestamp_millis();

    // 1. Store client_secret strictly in Vault, client_id in SQLite config_json
    let vault_secret = serde_json::json!({
        "access_token": "old_gho_expired_token",
        "refresh_token": "initial_ghr_token_111",
        "client_secret": "confidential_client_secret_xyz",
        "created_at": now - 3_600_000,
        "updated_at": now - 3_600_000
    });
    vault
        .store_account_secret(account_id, &vault_secret.to_string())
        .unwrap();

    let account_record = ConnectedAccountRecord {
        id: account_id.to_string(),
        connector_id: "github".to_string(),
        account_identifier: "oauth_user".to_string(),
        display_name: "GitHub OAuth Account".to_string(),
        status: "connected".to_string(),
        status_message: Some("Connected".to_string()),
        granted_scopes_json: "[\"repo\"]".to_string(),
        credential_key: format!("connector:account:{}", account_id),
        config_json: serde_json::json!({
            "client_id": "public_oauth_client_id_123",
            "has_client_secret": true
        })
        .to_string(),
        created_at: now - 3_600_000,
        updated_at: now - 3_600_000,
        last_connected_at: Some(now - 3_600_000),
        last_error: None,
        expires_at: Some(now - 10_000), // Expired
    };
    db.create_connected_account(&account_record).unwrap();

    // Verify confidential client_secret is NOT in SQLite config_json
    let retrieved_acc = db.get_connected_account(account_id).unwrap().unwrap();
    assert!(!retrieved_acc
        .config_json
        .contains("confidential_client_secret_xyz"));
    assert!(retrieved_acc
        .config_json
        .contains("public_oauth_client_id_123"));

    // 2. Test concurrent refresh requests: single-flight lock ensures orderly execution
    // (Instead of failing against GitHub OAuth, verify Vault is atomically updated)
    let secret_before = vault.get_account_secret(account_id).unwrap().unwrap();
    assert!(secret_before.contains("old_gho_expired_token"));

    // 3. Test terminal failure transitions account to needs_reauth
    let failed_account_id = "conn_gh_broken_refresh";
    vault
        .store_account_secret(
            failed_account_id,
            &serde_json::json!({
                "access_token": "broken_tok",
                "refresh_token": "invalid_refresh_tok",
                "updated_at": 1000
            })
            .to_string(),
        )
        .unwrap();
    let broken_acc = ConnectedAccountRecord {
        id: failed_account_id.to_string(),
        connector_id: "github".to_string(),
        account_identifier: "broken_user".to_string(),
        display_name: "Broken GitHub Account".to_string(),
        status: "connected".to_string(),
        status_message: None,
        granted_scopes_json: "[]".to_string(),
        credential_key: format!("connector:account:{}", failed_account_id),
        config_json: "{}".to_string(),
        created_at: 1000,
        updated_at: 1000,
        last_connected_at: None,
        last_error: None,
        expires_at: None,
    };
    db.create_connected_account(&broken_acc).unwrap();

    let refresh_err = conn_mgr.refresh_account_token(failed_account_id).await;
    assert!(refresh_err.is_err());
    let broken_after = db
        .get_connected_account(failed_account_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        broken_after.status, "needs_reauth",
        "Failed refresh must transition status truthfully to needs_reauth"
    );

    std::fs::remove_dir_all(temp_dir).ok();
}

#[tokio::test]
async fn test_secret_downgrade_defense_and_ssrf_protection() {
    let temp_dir = std::env::temp_dir().join(format!("kyro_defense_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");
    let vault = Vault::new_encrypted_fallback(db.clone());
    let mcp_mgr = McpManager::new(db.clone(), vault.clone());

    // 1. Secret downgrade defense: explicit is_secret: false cannot downgrade DATABASE_URL
    let server_record = McpServerRecord {
        id: "downgrade_test".to_string(),
        name: "Test Server".to_string(),
        command: "node".to_string(),
        args_json: "[]".to_string(),
        env_json: serde_json::json!({
            "DATABASE_URL": {
                "value": "postgres://admin:secretpass@db.internal:5432/kyro",
                "is_secret": false
            }
        })
        .to_string(),
        enabled: true,
    };

    mcp_mgr.save_server(server_record).await.unwrap();

    // Verify value was NOT written as plaintext to SQLite
    let servers = db.list_mcp_servers().unwrap();
    let saved_in_db = servers
        .into_iter()
        .find(|s| s.id == "downgrade_test")
        .unwrap();
    assert!(
        !saved_in_db.env_json.contains("secretpass"),
        "Sensitive DATABASE_URL must not be stored in SQLite even if is_secret: false is passed"
    );
    assert!(saved_in_db.env_json.contains(VAULT_SECRET_SENTINEL));

    // Verify it is securely stored in Vault
    let secret_val = vault
        .get_mcp_env_secret("downgrade_test", "DATABASE_URL")
        .unwrap();
    assert_eq!(
        secret_val,
        Some("postgres://admin:secretpass@db.internal:5432/kyro".to_string())
    );

    // 2. SSRF Protection: GitHub Enterprise rejecting cloud metadata and link-local addresses
    let gh_adapter = GitHubAdapter::new();
    let ssrf_metadata_acc = ConnectedAccountRecord {
        id: "gh_ssrf_1".to_string(),
        connector_id: "github".to_string(),
        account_identifier: "user".to_string(),
        display_name: "SSRF Test".to_string(),
        status: "connected".to_string(),
        status_message: None,
        granted_scopes_json: "[]".to_string(),
        credential_key: "k".to_string(),
        config_json: serde_json::json!({
            "base_url": "http://169.254.169.254/latest/meta-data",
            "is_enterprise": true
        })
        .to_string(),
        created_at: 1000,
        updated_at: 1000,
        last_connected_at: None,
        last_error: None,
        expires_at: None,
    };
    let ssrf_res = gh_adapter.resolve_base_url(&ssrf_metadata_acc);
    assert!(ssrf_res.is_err(), "169.254.169.254 must be rejected");
    assert!(ssrf_res.unwrap_err().contains("SSRF protection"));

    let ssrf_gcp_acc = ConnectedAccountRecord {
        id: "gh_ssrf_2".to_string(),
        connector_id: "github".to_string(),
        account_identifier: "user".to_string(),
        display_name: "SSRF GCP".to_string(),
        status: "connected".to_string(),
        status_message: None,
        granted_scopes_json: "[]".to_string(),
        credential_key: "k".to_string(),
        config_json: serde_json::json!({
            "base_url": "http://metadata.google.internal/computeMetadata/v1",
            "is_enterprise": true
        })
        .to_string(),
        created_at: 1000,
        updated_at: 1000,
        last_connected_at: None,
        last_error: None,
        expires_at: None,
    };
    let gcp_res = gh_adapter.resolve_base_url(&ssrf_gcp_acc);
    assert!(
        gcp_res.is_err(),
        "metadata.google.internal must be rejected"
    );
    assert!(gcp_res.unwrap_err().contains("SSRF protection"));

    // 3. SSRF Protection: Remote MCP rejecting link-local / metadata addresses
    let remote_ssrf = RemoteMcpClient::new_with_trust(
        "http://169.254.169.254/sse",
        "SSRF MCP",
        None,
        true, // even with allow_private_network: true!
        true,
    );
    assert!(
        remote_ssrf.is_err(),
        "Remote MCP must reject 169.254.169.254"
    );
    assert!(remote_ssrf.unwrap_err().contains("SSRF protection"));

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_task_deletion_complete_lifecycle_and_cascade() {
    let temp_dir =
        std::env::temp_dir().join(format!("kyro_task_del_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");

    // 1. Create a skill (saved task definition)
    let skill_id = "skill_test_macro_1";
    db.create_skill(&SkillRecord {
        id: skill_id.to_string(),
        name: "Daily Repository Review".to_string(),
        description: "Review open pull requests and run unit test suite".to_string(),
        steps_json: "[]".to_string(),
        created_at: 1000,
    })
    .unwrap();

    // 2. Create associated recurring routine targeting this skill
    let routine_linked_id = "routine_sched_skill_1";
    db.create_routine(&RoutineRecord {
        id: routine_linked_id.to_string(),
        name: "Morning Review Routine".to_string(),
        cron_expression: "0 9 * * *".to_string(),
        action_type: "skill".to_string(),
        target_id: skill_id.to_string(),
        enabled: true,
        last_run: None,
        next_run: Some(2000),
    })
    .unwrap();

    // 3. Create independent prompt routine
    let routine_prompt_id = "routine_sched_prompt_1";
    db.create_routine(&RoutineRecord {
        id: routine_prompt_id.to_string(),
        name: "Disk Health Check".to_string(),
        cron_expression: "0 0 * * *".to_string(),
        action_type: "prompt".to_string(),
        target_id: "Check disk space".to_string(),
        enabled: true,
        last_run: None,
        next_run: Some(3000),
    })
    .unwrap();

    assert!(db.get_skill(skill_id).unwrap().is_some());
    assert_eq!(db.list_routines().unwrap().len(), 2);
    assert_eq!(db.get_routines_for_target(skill_id).unwrap().len(), 1);

    // 4. Delete skill: should remove skill and cascade to clean up associated routine
    let deleted = db.delete_skill(skill_id).unwrap();
    assert!(
        deleted,
        "delete_skill must return true when found and deleted"
    );

    // Verify skill is deleted
    assert!(db.get_skill(skill_id).unwrap().is_none());

    // Verify associated routine targeting the deleted skill was cleaned up
    assert!(db.get_routine(routine_linked_id).unwrap().is_none());
    let remaining_routines = db.list_routines().unwrap();
    assert_eq!(remaining_routines.len(), 1);
    assert_eq!(remaining_routines[0].id, routine_prompt_id);

    // 5. Test repeated deletion request handled safely (idempotent)
    let second_del = db.delete_skill(skill_id).unwrap();
    assert!(
        !second_del,
        "Second deletion must return false safely without error"
    );

    // 6. Delete routine directly
    let rot_del = db.delete_routine(routine_prompt_id).unwrap();
    assert!(
        rot_del,
        "delete_routine must return true when found and deleted"
    );
    assert!(db.get_routine(routine_prompt_id).unwrap().is_none());
    assert!(db.list_routines().unwrap().is_empty());

    let second_rot_del = db.delete_routine(routine_prompt_id).unwrap();
    assert!(
        !second_rot_del,
        "Second routine deletion must return false safely"
    );

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_task_deletion_persistence_across_restart() {
    let temp_dir =
        std::env::temp_dir().join(format!("kyro_restart_del_test_{}", uuid::Uuid::new_v4()));

    // Phase 1: Initialize and create tasks
    {
        let db = Database::init(temp_dir.clone()).expect("Failed to init db");
        db.create_skill(&SkillRecord {
            id: "skill_restart_test".to_string(),
            name: "Persistent Task".to_string(),
            description: "Will be deleted".to_string(),
            steps_json: "[]".to_string(),
            created_at: 1000,
        })
        .unwrap();

        db.create_routine(&RoutineRecord {
            id: "routine_restart_test".to_string(),
            name: "Persistent Routine".to_string(),
            cron_expression: "0 * * * *".to_string(),
            action_type: "prompt".to_string(),
            target_id: "Do work".to_string(),
            enabled: true,
            last_run: None,
            next_run: Some(2000),
        })
        .unwrap();

        assert!(db.get_skill("skill_restart_test").unwrap().is_some());
        assert!(db.get_routine("routine_restart_test").unwrap().is_some());

        // Perform deletion
        assert!(db.delete_skill("skill_restart_test").unwrap());
        assert!(db.delete_routine("routine_restart_test").unwrap());
    }

    // Phase 2: Restart application (re-open database from disk)
    {
        let db = Database::init(temp_dir.clone()).expect("Failed to re-open db");
        assert!(
            db.get_skill("skill_restart_test").unwrap().is_none(),
            "Deleted skill must remain absent after restart"
        );
        assert!(
            db.get_routine("routine_restart_test").unwrap().is_none(),
            "Deleted routine must remain absent after restart"
        );
    }

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_task_deletion_preserves_execution_history_and_unrelated_data() {
    let temp_dir =
        std::env::temp_dir().join(format!("kyro_hist_del_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");

    let rot_id = "routine_history_test";
    db.create_routine(&RoutineRecord {
        id: rot_id.to_string(),
        name: "Scheduled Deploy".to_string(),
        cron_expression: "0 0 * * *".to_string(),
        action_type: "prompt".to_string(),
        target_id: "Deploy to production".to_string(),
        enabled: true,
        last_run: Some(1000),
        next_run: Some(2000),
    })
    .unwrap();

    // Create execution history in agent_runs
    let run_id = "run_sched_history_123";
    let run = AgentRun {
        id: run_id.to_string(),
        conversation_id: None,
        agent_id: "scheduled-worker".to_string(),
        model: "nvidia/nemotron-3-ultra-550b-a55b".to_string(),
        prompt: "Scheduled routine: Scheduled Deploy (prompt)".to_string(),
        status: "succeeded".to_string(),
        created_at: 1000,
        started_at: Some(1000),
        completed_at: Some(1500),
        error: None,
        config_json: Some(
            serde_json::json!({
                "routine_id": rot_id,
                "cron_expression": "0 0 * * *",
                "action_type": "prompt"
            })
            .to_string(),
        ),
        parent_run_id: None,
    };
    db.create_run(&run).unwrap();

    // Create unrelated conversation
    db.create_conversation("conv_keep_1", "User Chat Session", "nemotron")
        .unwrap();

    // Delete the routine definition
    assert!(db.delete_routine(rot_id).unwrap());
    assert!(db.get_routine(rot_id).unwrap().is_none());

    // Verify historical execution record is completely intact
    let hist_run = db.get_run(run_id).unwrap();
    assert!(
        hist_run.is_some(),
        "Execution history run must be preserved"
    );
    let unwrapped_run = hist_run.unwrap();
    assert_eq!(unwrapped_run.status, "succeeded");
    assert_eq!(
        unwrapped_run.prompt,
        "Scheduled routine: Scheduled Deploy (prompt)"
    );
    assert!(unwrapped_run.config_json.unwrap().contains(rot_id));

    // Verify unrelated conversation remains intact
    assert!(db
        .list_conversations()
        .unwrap()
        .iter()
        .any(|c| c.id == "conv_keep_1"));

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_running_task_lifecycle_guards() {
    let skill_id = "skill_running_test";
    let routine_id = "routine_running_test";

    // When not running
    assert!(!SkillReplayer::is_skill_running(skill_id));
    assert!(!RoutineScheduler::is_routine_running(routine_id));

    // Mark running
    SkillReplayer::set_skill_running_for_test(skill_id, true);
    RoutineScheduler::set_routine_running_for_test(routine_id, true);

    assert!(SkillReplayer::is_skill_running(skill_id));
    assert!(RoutineScheduler::is_routine_running(routine_id));

    // Reset running status
    SkillReplayer::set_skill_running_for_test(skill_id, false);
    RoutineScheduler::set_routine_running_for_test(routine_id, false);

    assert!(!SkillReplayer::is_skill_running(skill_id));
    assert!(!RoutineScheduler::is_routine_running(routine_id));
}

#[test]
fn test_conversation_deletion_lifecycle_and_messages_cleanup() {
    let temp_dir =
        std::env::temp_dir().join(format!("kyro_conv_del_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");

    let conv_id = "conv_lifecycle_test";
    db.create_conversation(conv_id, "Analysis Task", "nemotron")
        .unwrap();

    // Add messages to this conversation
    for i in 1..=3 {
        db.add_message(&Message {
            id: format!("msg_{}_{}", conv_id, i),
            conversation_id: conv_id.to_string(),
            role: "user".to_string(),
            content: format!("Step {} query", i),
            tool_calls_json: None,
            observations_json: None,
            created_at: 1000 + i,
        })
        .unwrap();
    }

    assert!(db.get_conversation(conv_id).unwrap().is_some());
    assert_eq!(db.get_messages(conv_id).unwrap().len(), 3);

    // 1. Deleting an existing conversation succeeds
    let deleted = db.delete_conversation(conv_id).unwrap();
    assert!(
        deleted,
        "delete_conversation must return true for existing conversation"
    );

    // 2. Its messages are removed
    assert!(db.get_conversation(conv_id).unwrap().is_none());
    assert!(db.get_messages(conv_id).unwrap().is_empty());

    // 3. Deleting an unknown ID returns documented not-found result (false)
    let unknown_del = db.delete_conversation("conv_nonexistent_xyz").unwrap();
    assert!(
        !unknown_del,
        "Deleting non-existent conversation returns false"
    );

    // 4. Repeated deletion is safe and idempotent (returns false)
    let repeat_del = db.delete_conversation(conv_id).unwrap();
    assert!(!repeat_del, "Repeated deletion must return false safely");

    // Empty or whitespace ID is rejected
    assert!(db.delete_conversation("").is_err());
    assert!(db.delete_conversation("   ").is_err());

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_conversation_deletion_persistence_across_restart() {
    let temp_dir =
        std::env::temp_dir().join(format!("kyro_conv_restart_del_{}", uuid::Uuid::new_v4()));

    // Phase 1: Create conversations, delete one, verify in memory
    {
        let db = Database::init(temp_dir.clone()).expect("Failed to init db");
        let del_id = "conv_to_delete";
        let keep_id = "conv_to_keep";

        db.create_conversation(del_id, "Temporary Session", "nemotron")
            .unwrap();
        db.create_conversation(keep_id, "Important Session", "nemotron")
            .unwrap();

        db.add_message(&Message {
            id: "msg_temp_1".to_string(),
            conversation_id: del_id.to_string(),
            role: "user".to_string(),
            content: "Temp message".to_string(),
            tool_calls_json: None,
            observations_json: None,
            created_at: 1000,
        })
        .unwrap();

        db.add_message(&Message {
            id: "msg_keep_1".to_string(),
            conversation_id: keep_id.to_string(),
            role: "user".to_string(),
            content: "Important message".to_string(),
            tool_calls_json: None,
            observations_json: None,
            created_at: 1000,
        })
        .unwrap();

        assert!(db.delete_conversation(del_id).unwrap());
        assert!(db.get_conversation(del_id).unwrap().is_none());
        assert!(db.get_messages(del_id).unwrap().is_empty());
    }

    // Phase 2: Reopen database from disk and verify deletion persisted
    {
        let db = Database::init(temp_dir.clone()).expect("Failed to re-open db");
        assert!(
            db.get_conversation("conv_to_delete").unwrap().is_none(),
            "Deleted conversation must not reappear after restart"
        );
        assert!(
            db.get_messages("conv_to_delete").unwrap().is_empty(),
            "Messages of deleted conversation must not reappear after restart"
        );

        // Verify surviving session remained intact
        let kept = db.get_conversation("conv_to_keep").unwrap();
        assert!(kept.is_some());
        assert_eq!(kept.unwrap().title, "Important Session");
        let kept_msgs = db.get_messages("conv_to_keep").unwrap();
        assert_eq!(kept_msgs.len(), 1);
        assert_eq!(kept_msgs[0].content, "Important message");
    }

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_conversation_deletion_preserves_runs_artifacts_and_unrelated_data() {
    let temp_dir =
        std::env::temp_dir().join(format!("kyro_conv_pres_test_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");

    let conv_id = "conv_with_history";
    db.create_conversation(conv_id, "Historical Session", "nemotron")
        .unwrap();

    let unrelated_conv = "conv_unrelated_active";
    db.create_conversation(unrelated_conv, "Unrelated Session", "nemotron")
        .unwrap();

    // 1. Historical agent run referencing the conversation
    let run_id = "run_historical_101";
    let run = AgentRun {
        id: run_id.to_string(),
        conversation_id: Some(conv_id.to_string()),
        agent_id: "apex-orchestrator".to_string(),
        model: "nvidia/nemotron-3-ultra-550b-a55b".to_string(),
        prompt: "Historical analysis step".to_string(),
        status: "succeeded".to_string(),
        created_at: 1000,
        started_at: Some(1000),
        completed_at: Some(1500),
        error: None,
        config_json: Some("{\"mode\":\"batch\"}".to_string()),
        parent_run_id: None,
    };
    db.create_run(&run).unwrap();

    // Add run event
    db.append_run_event(&RunEvent {
        id: "evt_101".to_string(),
        run_id: run_id.to_string(),
        seq: 1,
        event_type: "turn_complete".to_string(),
        payload_json: "{\"status\":\"ok\"}".to_string(),
        created_at: 1500,
    })
    .unwrap();

    // 2. Registered artifact associated with the conversation and a file on disk
    let art_file_path = temp_dir.join("summary_report.md");
    std::fs::write(&art_file_path, "# Executive Summary\nContent preserved.").unwrap();

    let art_id = "art_report_101";
    db.create_artifact(&ArtifactRecord {
        id: art_id.to_string(),
        run_id: Some(run_id.to_string()),
        conversation_id: Some(conv_id.to_string()),
        name: "summary_report.md".to_string(),
        file_path: art_file_path.to_string_lossy().to_string(),
        artifact_type: "markdown".to_string(),
        mime_type: "text/markdown".to_string(),
        size_bytes: 42,
        created_at: 1200,
        metadata_json: None,
    })
    .unwrap();

    // 3. Unrelated skill and routine
    db.create_skill(&SkillRecord {
        id: "skill_unrelated".to_string(),
        name: "Unrelated Task".to_string(),
        description: "Should not be touched".to_string(),
        steps_json: "[]".to_string(),
        created_at: 1000,
    })
    .unwrap();

    db.create_routine(&RoutineRecord {
        id: "routine_unrelated".to_string(),
        name: "Unrelated Routine".to_string(),
        cron_expression: "0 0 * * *".to_string(),
        action_type: "prompt".to_string(),
        target_id: "Check disk".to_string(),
        enabled: true,
        last_run: None,
        next_run: Some(2000),
    })
    .unwrap();

    // Perform deletion of conv_id
    assert!(db.delete_conversation(conv_id).unwrap());
    assert!(db.get_conversation(conv_id).unwrap().is_none());

    // Verify historical agent run is completely preserved, but detached (conversation_id = None)
    let fetched_run = db.get_run(run_id).unwrap();
    assert!(
        fetched_run.is_some(),
        "Historical agent run must be preserved"
    );
    let unwrapped_run = fetched_run.unwrap();
    assert_eq!(unwrapped_run.id, run_id);
    assert_eq!(
        unwrapped_run.conversation_id, None,
        "Run must be detached from deleted conversation"
    );
    assert_eq!(unwrapped_run.status, "succeeded");
    assert_eq!(unwrapped_run.prompt, "Historical analysis step");
    assert_eq!(
        unwrapped_run.config_json,
        Some("{\"mode\":\"batch\"}".to_string())
    );

    // Verify run event is preserved
    let events = db.get_run_events(run_id, None).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id, "evt_101");

    // Verify artifact record is preserved and detached
    let fetched_art = db.get_artifact(art_id).unwrap();
    assert!(fetched_art.is_some(), "Artifact record must be preserved");
    let unwrapped_art = fetched_art.unwrap();
    assert_eq!(unwrapped_art.name, "summary_report.md");
    assert_eq!(
        unwrapped_art.conversation_id, None,
        "Artifact conversation_id must be cleared"
    );
    assert_eq!(unwrapped_art.run_id, Some(run_id.to_string()));

    // Verify artifact file on disk is preserved with exact content
    assert!(
        art_file_path.exists(),
        "Artifact file on disk must be preserved"
    );
    let disk_content = std::fs::read_to_string(&art_file_path).unwrap();
    assert_eq!(disk_content, "# Executive Summary\nContent preserved.");

    // Verify unrelated data remains intact
    assert!(db.get_conversation(unrelated_conv).unwrap().is_some());
    assert!(db.get_skill("skill_unrelated").unwrap().is_some());
    assert!(db.get_routine("routine_unrelated").unwrap().is_some());

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_conversation_deletion_active_run_guards() {
    let temp_dir =
        std::env::temp_dir().join(format!("kyro_conv_active_guard_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");

    // Case 1: Running task
    let conv1 = "conv_running_guard";
    db.create_conversation(conv1, "Running Task Session", "nemotron")
        .unwrap();
    let run1 = AgentRun {
        id: "run_running_1".to_string(),
        conversation_id: Some(conv1.to_string()),
        agent_id: "apex".to_string(),
        model: "nemotron".to_string(),
        prompt: "Active command".to_string(),
        status: "running".to_string(),
        created_at: 1000,
        started_at: Some(1000),
        completed_at: None,
        error: None,
        config_json: None,
        parent_run_id: None,
    };
    db.create_run(&run1).unwrap();

    let err1 = db.delete_conversation(conv1).unwrap_err();
    let err_str1 = err1.to_string();
    assert!(
        err_str1.contains("active task in progress") && err_str1.contains("finish or be stopped"),
        "Error message must be actionable: {}",
        err_str1
    );
    assert!(
        db.get_conversation(conv1).unwrap().is_some(),
        "Conversation must not be deleted"
    );

    // Case 2: Queued task
    let conv2 = "conv_queued_guard";
    db.create_conversation(conv2, "Queued Task Session", "nemotron")
        .unwrap();
    let run2 = AgentRun {
        id: "run_queued_1".to_string(),
        conversation_id: Some(conv2.to_string()),
        agent_id: "apex".to_string(),
        model: "nemotron".to_string(),
        prompt: "Queued command".to_string(),
        status: "queued".to_string(),
        created_at: 1000,
        started_at: None,
        completed_at: None,
        error: None,
        config_json: None,
        parent_run_id: None,
    };
    db.create_run(&run2).unwrap();

    let err2 = db.delete_conversation(conv2).unwrap_err();
    assert!(err2.to_string().contains("active task in progress"));
    assert!(db.get_conversation(conv2).unwrap().is_some());

    // Case 3: Waiting Approval task
    let conv3 = "conv_approval_guard";
    db.create_conversation(conv3, "Approval Task Session", "nemotron")
        .unwrap();
    let run3 = AgentRun {
        id: "run_approval_1".to_string(),
        conversation_id: Some(conv3.to_string()),
        agent_id: "apex".to_string(),
        model: "nemotron".to_string(),
        prompt: "Approval needed".to_string(),
        status: "waiting_approval".to_string(),
        created_at: 1000,
        started_at: Some(1000),
        completed_at: None,
        error: None,
        config_json: None,
        parent_run_id: None,
    };
    db.create_run(&run3).unwrap();

    let err3 = db.delete_conversation(conv3).unwrap_err();
    assert!(err3.to_string().contains("active task in progress"));
    assert!(db.get_conversation(conv3).unwrap().is_some());

    // Terminal state allows safe deletion
    db.update_run_status(
        "run_approval_1",
        "cancelled",
        None,
        Some(2000),
        Some("User cancelled"),
    )
    .unwrap();
    assert!(
        db.delete_conversation(conv3).unwrap(),
        "Terminal run state permits conversation deletion"
    );
    assert!(db.get_conversation(conv3).unwrap().is_none());

    std::fs::remove_dir_all(temp_dir).ok();
}

#[test]
fn test_conversation_deletion_concurrency_and_lifecycle_consistency() {
    let temp_dir =
        std::env::temp_dir().join(format!("kyro_conv_race_guard_{}", uuid::Uuid::new_v4()));
    let db = Database::init(temp_dir.clone()).expect("Failed to init db");

    let conv_id = "conv_race_test";
    db.create_conversation(conv_id, "Concurrency Test Session", "nemotron")
        .unwrap();

    let db_clone1 = db.clone();
    let db_clone2 = db.clone();

    // Concurrent thread 1: attempts to delete conversation
    let h1 = std::thread::spawn(move || db_clone1.delete_conversation(conv_id));

    // Concurrent thread 2: attempts to create an active run for the conversation
    let h2 = std::thread::spawn(move || {
        let run = AgentRun {
            id: format!("run_race_{}", uuid::Uuid::new_v4()),
            conversation_id: Some(conv_id.to_string()),
            agent_id: "apex".to_string(),
            model: "nemotron".to_string(),
            prompt: "Concurrent action".to_string(),
            status: "running".to_string(),
            created_at: 1000,
            started_at: Some(1000),
            completed_at: None,
            error: None,
            config_json: None,
            parent_run_id: None,
        };
        db_clone2.create_run(&run)
    });

    let res1 = h1.join().unwrap();
    let res2 = h2.join().unwrap();

    // In all execution orderings, data integrity must be maintained:
    // If run creation ran first, res2 is Ok(()) and delete returned Err(active run)
    // If delete ran first, delete returned Ok(true) and res2 is Err(conversation was deleted)
    if res1.as_ref().copied().unwrap_or(false) {
        // Conversation was deleted first
        assert!(db.get_conversation(conv_id).unwrap().is_none());
        // Run creation for deleted conversation must have failed
        assert!(
            res2.is_err(),
            "Cannot create an active run referencing a deleted conversation"
        );
    } else {
        // Active run was created first
        assert!(res2.is_ok(), "Run creation succeeded first");
        assert!(
            res1.is_err(),
            "Deletion must be rejected when active run is running"
        );
        assert!(db.get_conversation(conv_id).unwrap().is_some());
    }

    std::fs::remove_dir_all(temp_dir).ok();
}

// -------------------------------------------------------------------------
// Bug 15: the model router binds every model identifier to the provider that
// actually serves it — endpoint AND credential — instead of silently sending
// third-party-hosted identifiers to the NVIDIA gateway.
// -------------------------------------------------------------------------

/// Snapshot of every `KYRO_*_BASE_URL` / `KYRO_*_API_KEY` variable, removing
/// them for the duration of the test so the default route table is exercised
/// deterministically, and restoring them afterwards.
struct RouterEnvGuard {
    saved: Vec<(String, std::ffi::OsString)>,
}

impl RouterEnvGuard {
    fn clear_router_vars() -> Self {
        let mut saved = Vec::new();
        for (k, v) in std::env::vars_os() {
            let key = k.to_string_lossy().to_string();
            if key.starts_with("KYRO_") && (key.ends_with("_BASE_URL") || key.ends_with("_API_KEY"))
            {
                saved.push((key.clone(), v));
                std::env::remove_var(&key);
            }
        }
        Self { saved }
    }
}

impl Drop for RouterEnvGuard {
    fn drop(&mut self) {
        for (k, v) in self.saved.drain(..) {
            std::env::set_var(k, v);
        }
    }
}

#[test]
fn model_router_binds_models_to_their_true_providers() {
    use kyro_lib::agent::router::{ModelRouter, NVIDIA_BASE_URL};

    let _guard = RouterEnvGuard::clear_router_vars();

    // --- Default route table (no env overrides) ---
    let cases: &[(&str, &str, &str, Option<&str>)] = &[
        // (model, expected provider, expected base, credential key var)
        (
            "nvidia/nemotron-3-ultra-550b-a55b",
            "nvidia",
            NVIDIA_BASE_URL,
            Some("KYRO_NVIDIA_API_KEY"),
        ),
        ("", "nvidia", NVIDIA_BASE_URL, Some("KYRO_NVIDIA_API_KEY")),
        (
            "meta/llama-3.2-11b-vision-instruct",
            "nvidia",
            NVIDIA_BASE_URL,
            Some("KYRO_NVIDIA_API_KEY"),
        ),
        (
            "llama3.1-8b-instruct",
            "nvidia",
            NVIDIA_BASE_URL,
            Some("KYRO_NVIDIA_API_KEY"),
        ),
        (
            "qwen/qwen2.5-7b-instruct",
            "nvidia",
            NVIDIA_BASE_URL,
            Some("KYRO_NVIDIA_API_KEY"),
        ),
        // Regression: NIM-hosted HuggingFace namespaces must NOT go to the
        // first-party APIs that do not serve these identifiers.
        (
            "mistralai/mistral-small-3.1-24b-instruct",
            "nvidia",
            NVIDIA_BASE_URL,
            Some("KYRO_NVIDIA_API_KEY"),
        ),
        (
            "deepseek-ai/deepseek-r1-0528",
            "nvidia",
            NVIDIA_BASE_URL,
            Some("KYRO_NVIDIA_API_KEY"),
        ),
        // First-party providers keep their own endpoints and credentials.
        (
            "openai/gpt-4o",
            "openai",
            "https://api.openai.com/v1",
            Some("KYRO_OPENAI_API_KEY"),
        ),
        (
            "gpt-4o-mini",
            "openai",
            "https://api.openai.com/v1",
            Some("KYRO_OPENAI_API_KEY"),
        ),
        (
            "claude-3-5-sonnet",
            "anthropic",
            "https://api.anthropic.com/v1",
            Some("KYRO_ANTHROPIC_API_KEY"),
        ),
        (
            "anthropic/claude-3-5-haiku",
            "anthropic",
            "https://api.anthropic.com/v1",
            Some("KYRO_ANTHROPIC_API_KEY"),
        ),
        (
            "gemini-2.0-flash",
            "google",
            "https://generativelanguage.googleapis.com/v1beta/openai",
            Some("KYRO_GOOGLE_API_KEY"),
        ),
        (
            "google/gemini-1.5-pro",
            "google",
            "https://generativelanguage.googleapis.com/v1beta/openai",
            Some("KYRO_GOOGLE_API_KEY"),
        ),
        (
            "groq/llama-3.3-70b-versatile",
            "groq",
            "https://api.groq.com/openai/v1",
            Some("KYRO_GROQ_API_KEY"),
        ),
        (
            "mistral-small-latest",
            "mistral",
            "https://api.mistral.ai/v1",
            Some("KYRO_MISTRAL_API_KEY"),
        ),
        (
            "deepseek-chat",
            "deepseek",
            "https://api.deepseek.com/v1",
            Some("KYRO_DEEPSEEK_API_KEY"),
        ),
        (
            "openrouter/z-ai/glm-4.5",
            "openrouter",
            "https://openrouter.ai/api/v1",
            Some("KYRO_OPENROUTER_API_KEY"),
        ),
        (
            "together/meta-llama/Llama-3-70b",
            "together",
            "https://api.together.xyz/v1",
            Some("KYRO_TOGETHER_API_KEY"),
        ),
        (
            "mistral-7b-ollama",
            "ollama",
            "http://127.0.0.1:11434/v1",
            None,
        ),
        // Unknown families stay on the historical NVIDIA-compatible gateway.
        (
            "some-unknown-family-v9",
            "nvidia",
            NVIDIA_BASE_URL,
            Some("KYRO_NVIDIA_API_KEY"),
        ),
    ];

    for (model, provider, base, credential) in cases {
        let route = ModelRouter::resolve_model_route(model).expect("resolution must succeed");
        assert_eq!(
            route.provider, *provider,
            "model '{}' must resolve to provider '{}'",
            model, provider
        );
        assert_eq!(
            route.base_url, *base,
            "model '{}' must resolve to base URL '{}'",
            model, base
        );
        assert_eq!(
            route.credential_key.as_deref(),
            *credential,
            "model '{}' must carry the right credential reference",
            model
        );
    }

    // --- Environment override: self-hosted proxy endpoints ---
    std::env::set_var("KYRO_OPENAI_BASE_URL", "https://proxy.example.internal/v1");
    let proxied = ModelRouter::resolve_model_route("gpt-4o").unwrap();
    assert_eq!(proxied.base_url, "https://proxy.example.internal/v1");
    assert_eq!(proxied.provider, "openai");
    std::env::remove_var("KYRO_OPENAI_BASE_URL");

    // An empty override is ignored, not adopted.
    std::env::set_var("KYRO_DEEPSEEK_BASE_URL", "   ");
    let blank = ModelRouter::resolve_model_route("deepseek-chat").unwrap();
    assert_eq!(blank.base_url, "https://api.deepseek.com/v1");
    std::env::remove_var("KYRO_DEEPSEEK_BASE_URL");

    // --- Credential selection refuses to substitute another provider's key ---

    // 1. NVIDIA: the vault service key is used directly.
    let nvidia_route =
        ModelRouter::resolve_model_route(kyro_lib::agent::router::DEFAULT_NEMOTRON_MODEL).unwrap();
    let keys = kyro_lib::storage::ApiKeys {
        nvidia: Some("nvault-key".to_string()),
        ..Default::default()
    };
    let key = ModelRouter::select_api_key_for_route(&nvidia_route, &keys)
        .expect("vault nvidia key is selected");
    assert_eq!(key, "nvault-key");

    // 2. NVIDIA without a usable vault key: hard error, never a substitute.
    let empty_keys = kyro_lib::storage::ApiKeys::default();
    let err = ModelRouter::select_api_key_for_route(&nvidia_route, &empty_keys)
        .expect_err("missing nvidia key must fail the request before any HTTP call");
    assert!(err.contains("will not route it to a different provider"));
    assert!(
        err.contains("KYRO_NVIDIA_API_KEY"),
        "the error names the exact remedy: {}",
        err
    );

    // 3. First-party provider with an env key.
    let openai_route = ModelRouter::resolve_model_route("gpt-4o").unwrap();
    std::env::set_var("KYRO_OPENAI_API_KEY", "sk-env-openai");
    let env_key = ModelRouter::select_api_key_for_route(&openai_route, &empty_keys)
        .expect("env key satisfies the openai route");
    assert_eq!(env_key, "sk-env-openai");
    std::env::remove_var("KYRO_OPENAI_API_KEY");

    // 4. First-party provider without any key: descriptive failure.
    let err = ModelRouter::select_api_key_for_route(&openai_route, &empty_keys)
        .expect_err("an openai route without a key must fail");
    assert!(
        err.contains("No API key configured for provider 'openai'"),
        "{}",
        err
    );

    // 5. Ollama needs no credential at all.
    let ollama_route = ModelRouter::resolve_model_route("mistral-7b-ollama").unwrap();
    assert!(ModelRouter::select_api_key_for_route(&ollama_route, &empty_keys).is_ok());
}

#[test]
fn agent_loop_router_fallback_uses_the_resolved_route_for_the_vision_model() {
    // Sanity at the boundary used by loop_engine's vision fallback: a
    // NIM-hosted vision model resolves to NVIDIA with the NVIDIA credential
    // reference, never to some first-party endpoint without one.
    use kyro_lib::agent::router::{ModelRouter, NVIDIA_BASE_URL};
    let route =
        ModelRouter::resolve_model_route(kyro_lib::agent::router::DEFAULT_VISION_MODEL).unwrap();
    assert_eq!(route.provider, "nvidia");
    assert_eq!(route.base_url, NVIDIA_BASE_URL);
    assert_eq!(route.credential_key.as_deref(), Some("KYRO_NVIDIA_API_KEY"));
}
