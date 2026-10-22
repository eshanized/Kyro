use kyro_lib::{
    agent::{
        loop_engine::AgentLoopEngine,
        router::{ChatMessage, MessageContent},
        tools,
    },
    browser::BrowserAutomationManager,
    commands::*,
    computer_use::input::{ComputerAction, InputController},
    connector::{
        manager::{canonicalize_json_value, classify_external_tool, compute_schema_fingerprint, ConnectorManager, InvocationAuthProof},
        native::google::GoogleAdapter,
        native::github::GitHubAdapter,
    },
    governance::{
        auth::{AuthorizationEngine, FileOp},
        shell_lexer::{lex, ShellConstruct},
    },
    mcp::McpManager,
    multibot::workspace::SpecialistRole,
    project::{
        checkpoint::CheckpointManager,
        devserver::DevServerManager,
    },
    storage::{
        ApiKeys, BotRecord, Database, RoutineRecord, Vault,
        file_assistant::FileAssistant,
        organizer::FileOrganizer,
    },
    teach_task::{
        recorder::TaskRecorder,
        replay::SkillReplayer,
        scheduler::RoutineScheduler,
    },
    AppState,
};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::Manager;

struct TestContext {
    dir: PathBuf,
    app: tauri::App<tauri::test::MockRuntime>,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn setup_test_context(tag: &str) -> (TestContext, tauri::State<'static, AppState>) {
    let dir = std::env::temp_dir().join(format!("kyro_cov95_{}_{}", tag, uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();

    let db = Database::init(dir.clone()).expect("Failed to initialize test db");
    let vault = Vault::new_encrypted_fallback(db.clone());
    let browser = Arc::new(BrowserAutomationManager::new().with_database(db.clone()));
    let dev_servers = Arc::new(DevServerManager::new());
    let engine = Arc::new(AgentLoopEngine::with_resources(browser.clone(), dev_servers.clone()));
    let mcp_manager = McpManager::new(db.clone(), vault.clone());
    let connector_manager = Arc::new(ConnectorManager::new(
        db.clone(),
        vault.clone(),
        mcp_manager.clone(),
    ));

    let state = AppState {
        db,
        vault,
        engine,
        mcp_manager,
        connector_manager,
        pty_session: Mutex::new(None),
        browser,
        dev_servers,
    };

    let app = tauri::test::mock_app();
    app.manage(state);

    let state_ref: tauri::State<'_, AppState> = app.state::<AppState>();
    let state_static: tauri::State<'static, AppState> = unsafe { std::mem::transmute(state_ref) };

    (TestContext { dir, app }, state_static)
}

#[test]
fn test_natural_language_to_routine_all_branches() {
    // 1. Empty description
    assert!(natural_language_to_routine("   ".to_string()).is_err());

    // 2. Times of day variations
    let t1 = natural_language_to_routine("Check invoice at 9am".to_string()).unwrap();
    assert_eq!(t1.cron_expression, "0 9 * * *");
    assert!(t1.schedule_confidence.contains("inferred"));

    let t2 = natural_language_to_routine("Check invoice at 9:30 am".to_string()).unwrap();
    assert_eq!(t2.cron_expression, "30 9 * * *");

    let t3 = natural_language_to_routine("Run check at 5pm".to_string()).unwrap();
    assert_eq!(t3.cron_expression, "0 17 * * *");

    let t4 = natural_language_to_routine("Run check at 12am".to_string()).unwrap();
    assert_eq!(t4.cron_expression, "0 0 * * *");

    let t5 = natural_language_to_routine("Run check in the morning".to_string()).unwrap();
    assert_eq!(t5.cron_expression, "0 9 * * *");

    let t6 = natural_language_to_routine("Run check in the evening".to_string()).unwrap();
    assert_eq!(t6.cron_expression, "0 18 * * *");

    let t7 = natural_language_to_routine("Run check at night".to_string()).unwrap();
    assert_eq!(t7.cron_expression, "0 20 * * *");

    let t8 = natural_language_to_routine("Run check at midnight".to_string()).unwrap();
    assert_eq!(t8.cron_expression, "0 0 * * *");

    let t9 = natural_language_to_routine("Run check at noon".to_string()).unwrap();
    assert_eq!(t9.cron_expression, "0 12 * * *");

    // 3. Recurrence expressions
    let r15 = natural_language_to_routine("Check api every 15 min".to_string()).unwrap();
    assert_eq!(r15.cron_expression, "*/15 * * * *");

    let r15b = natural_language_to_routine("Check api every 15 minutes".to_string()).unwrap();
    assert_eq!(r15b.cron_expression, "*/15 * * * *");

    let r30 = natural_language_to_routine("Check health every 30 min".to_string()).unwrap();
    assert_eq!(r30.cron_expression, "*/30 * * * *");

    let r30b = natural_language_to_routine("Check health every 30 minutes".to_string()).unwrap();
    assert_eq!(r30b.cron_expression, "*/30 * * * *");

    let rh = natural_language_to_routine("Sync data every hour".to_string()).unwrap();
    assert_eq!(rh.cron_expression, "0 * * * *");

    let rhb = natural_language_to_routine("Sync data hourly".to_string()).unwrap();
    assert_eq!(rhb.cron_expression, "0 * * * *");

    let r2h = natural_language_to_routine("Sync data every 2 hours".to_string()).unwrap();
    assert_eq!(r2h.cron_expression, "0 */2 * * *");

    // Days of the week
    let r_fri = natural_language_to_routine("Send report every friday at 4pm".to_string()).unwrap();
    assert_eq!(r_fri.cron_expression, "0 16 * * 5");

    let r_mon = natural_language_to_routine("Team standup each monday at 10am".to_string()).unwrap();
    assert_eq!(r_mon.cron_expression, "0 10 * * 1");

    let r_tue = natural_language_to_routine("Deploy staging every tuesday at 2pm".to_string()).unwrap();
    assert_eq!(r_tue.cron_expression, "0 14 * * 2");

    let r_wed = natural_language_to_routine("Review metrics each wednesday at 11am".to_string()).unwrap();
    assert_eq!(r_wed.cron_expression, "0 11 * * 3");

    let r_thu = natural_language_to_routine("Review logs every thursday at 3pm".to_string()).unwrap();
    assert_eq!(r_thu.cron_expression, "0 15 * * 4");

    let r_sat = natural_language_to_routine("Maintenance every saturday at 1am".to_string()).unwrap();
    assert_eq!(r_sat.cron_expression, "0 1 * * 6");

    let r_sun = natural_language_to_routine("Backup data each sunday at 2am".to_string()).unwrap();
    assert_eq!(r_sun.cron_expression, "0 2 * * 0");

    let r_wkday = natural_language_to_routine("Scrape headlines every weekday at 8am".to_string()).unwrap();
    assert_eq!(r_wkday.cron_expression, "0 8 * * 1-5");

    let r_wkday2 = natural_language_to_routine("Scrape headlines monday through friday at 8am".to_string()).unwrap();
    assert_eq!(r_wkday2.cron_expression, "0 8 * * 1-5");

    let r_wkend = natural_language_to_routine("Clean database every weekend at 5am".to_string()).unwrap();
    assert_eq!(r_wkend.cron_expression, "0 5 * * 6,0");

    let r_daily = natural_language_to_routine("Daily summary at 9am".to_string()).unwrap();
    assert_eq!(r_daily.cron_expression, "0 9 * * *");

    let r_m1 = natural_language_to_routine("Send invoices on first day of the month at 9am".to_string()).unwrap();
    assert_eq!(r_m1.cron_expression, "0 9 1 * *");

    let r_m2 = natural_language_to_routine("Archive logs monthly at 9am".to_string()).unwrap();
    assert!(r_m2.needs_clarification);

    let r_none = natural_language_to_routine("Do something whenever".to_string()).unwrap();
    assert!(r_none.needs_clarification);

    // 4. Resource categories
    let res_rec = natural_language_to_routine("Process receipts daily".to_string()).unwrap();
    assert!(res_rec.required_resources.iter().any(|r| r.contains("Local document")));

    let res_org = natural_language_to_routine("Organize downloads clean sort files daily".to_string()).unwrap();
    assert!(res_org.requires_confirmation);

    let res_mail = natural_language_to_routine("Check unread gmail emails daily".to_string()).unwrap();
    assert!(res_mail.required_resources.iter().any(|r| r.contains("Gmail")));

    let res_cal = natural_language_to_routine("Check meeting calendar schedule daily".to_string()).unwrap();
    assert!(res_cal.required_resources.iter().any(|r| r.contains("Calendar")));

    let res_cmp = natural_language_to_routine("Compare research web pricing daily".to_string()).unwrap();
    assert!(res_cmp.required_resources.iter().any(|r| r.contains("Web Automation")));
}

#[tokio::test]
async fn test_commands_perform_computer_action_and_recording() {
    let (_ctx, state) = setup_test_context("comp_act");

    let _ = TaskRecorder::start("test_workflow", "recording actions");
    assert!(TaskRecorder::is_recording());

    let actions = vec![
        ComputerAction::MouseMove { x: 100, y: 100 },
        ComputerAction::LeftClick { x: Some(100), y: Some(100) },
        ComputerAction::RightClick { x: Some(100), y: Some(100) },
        ComputerAction::DoubleClick { x: Some(100), y: Some(100) },
        ComputerAction::LeftClickDrag { x: 200, y: 200 },
        ComputerAction::Type { text: "hello".to_string() },
        ComputerAction::Key { text: "Enter".to_string() },
        ComputerAction::Hotkey { keys: vec!["Ctrl".to_string(), "C".to_string()] },
        ComputerAction::Wait { ms: 50 },
    ];

    for act in actions {
        let _ = perform_computer_action(state.clone(), act);
    }

    let _ = TaskRecorder::stop();
    assert!(!TaskRecorder::is_recording());

    abort_computer_use();
    assert!(InputController::is_aborted());
    InputController::reset_abort();
    assert!(!InputController::is_aborted());
}

#[tokio::test]
async fn test_commands_agent_turn_and_runs_lifecycle() {
    let (ctx, state) = setup_test_context("turn_life");

    let turn_err = run_agent_turn(
        ctx.app.handle().clone(),
        state.clone(),
        None,
        None,
        None,
        None,
        vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::from_text("List files"),
            tool_calls: None,
            tool_call_id: None,
        }],
        None,
        None,
    )
    .await;
    assert!(turn_err.is_err());
    assert!(turn_err.unwrap_err().contains("NVIDIA API Key not configured"));

    state.vault.save_all_keys(&ApiKeys {
        nvidia: Some("nvapi-test-key".to_string()),
        ..Default::default()
    }).unwrap();

    // Create conversation first for foreign key integrity
    let _ = state.db.create_conversation("conv-1", "Test Conv", "gpt-4o");

    let run = kyro_lib::storage::AgentRun {
        id: "run-test-cmd-1".to_string(),
        conversation_id: Some("conv-1".to_string()),
        agent_id: "apex-orchestrator".to_string(),
        model: "nvidia/nemotron-4-340b-instruct".to_string(),
        prompt: "Check system health".to_string(),
        status: "running".to_string(),
        created_at: chrono::Utc::now().timestamp_millis(),
        started_at: Some(chrono::Utc::now().timestamp_millis()),
        completed_at: None,
        error: None,
        config_json: None,
        parent_run_id: None,
    };
    state.db.create_run(&run).unwrap();

    let fetched = get_agent_run(state.clone(), "run-test-cmd-1".to_string()).unwrap();
    assert!(fetched.is_some());

    let runs_list = list_agent_runs(state.clone(), Some(10)).unwrap();
    assert!(!runs_list.is_empty());

    let events = get_run_events(state.clone(), "run-test-cmd-1".to_string(), None).unwrap();
    assert!(events.is_empty());

    let cancelled = cancel_agent_run(state.clone(), "run-test-cmd-1".to_string()).await.unwrap();
    assert!(cancelled);
}

#[tokio::test]
async fn test_commands_today_dashboard_and_briefing() {
    let (_ctx, state) = setup_test_context("today_dash");

    let dash_res = get_today_dashboard_data(state.clone()).await;
    assert!(dash_res.is_ok());
    let data = dash_res.unwrap();
    assert!(!data.date_display.is_empty());
    assert!(data.greeting == "Good morning" || data.greeting == "Good afternoon" || data.greeting == "Good evening");
    assert!(!data.email_summary.is_connected);
}

#[tokio::test]
async fn test_commands_safe_file_ops_and_duplicates() {
    let (ctx, state) = setup_test_context("safe_files");
    let test_dir = ctx.dir.join("safe_sandbox");
    std::fs::create_dir_all(&test_dir).unwrap();

    // Authorize folder
    add_authorized_folder(state.clone(), test_dir.to_string_lossy().to_string(), Some("SafeSandbox".to_string()), None).unwrap();

    let file_a = test_dir.join("orig.txt");
    let file_b = test_dir.join("copy.txt");
    std::fs::write(&file_a, "Hello safe operations 12345").unwrap();
    std::fs::write(&file_b, "Hello safe operations 12345").unwrap();

    let dups = detect_folder_duplicates(state.clone(), test_dir.to_string_lossy().to_string()).unwrap();
    assert_eq!(dups.len(), 1);
    assert_eq!(dups[0].files.len(), 2);

    let copy_dry = execute_safe_file_op(
        state.clone(),
        "copy".to_string(),
        file_a.to_string_lossy().to_string(),
        Some(test_dir.join("target_dry.txt").to_string_lossy().to_string()),
        true,
    ).unwrap();
    assert!(copy_dry.requires_confirmation || copy_dry.success);

    let copy_real = execute_safe_file_op(
        state.clone(),
        "copy".to_string(),
        file_a.to_string_lossy().to_string(),
        Some(test_dir.join("target_real.txt").to_string_lossy().to_string()),
        false,
    ).unwrap();
    assert!(copy_real.success);

    let move_real = execute_safe_file_op(
        state.clone(),
        "move".to_string(),
        test_dir.join("target_real.txt").to_string_lossy().to_string(),
        Some(test_dir.join("target_moved.txt").to_string_lossy().to_string()),
        false,
    ).unwrap();
    assert!(move_real.success);

    let rename_real = execute_safe_file_op(
        state.clone(),
        "rename".to_string(),
        test_dir.join("target_moved.txt").to_string_lossy().to_string(),
        Some(test_dir.join("target_renamed.txt").to_string_lossy().to_string()),
        false,
    ).unwrap();
    assert!(rename_real.success);

    let trash_real = execute_safe_file_op(
        state.clone(),
        "trash".to_string(),
        test_dir.join("target_renamed.txt").to_string_lossy().to_string(),
        None,
        false,
    ).unwrap();
    assert!(trash_real.success);

    let del_real = execute_safe_file_op(
        state.clone(),
        "delete".to_string(),
        file_b.to_string_lossy().to_string(),
        None,
        false,
    ).unwrap();
    assert!(del_real.success);

    let txs = list_file_transactions(state.clone(), Some(10)).unwrap();
    assert!(!txs.is_empty());
}

#[tokio::test]
async fn test_commands_study_notes_and_passage_explanation() {
    let (ctx, state) = setup_test_context("study_pass");
    add_authorized_folder(state.clone(), ctx.dir.to_string_lossy().to_string(), Some("TestDir".to_string()), None).unwrap();

    let doc_file = ctx.dir.join("textbook.md");
    std::fs::write(&doc_file, "# Chapter 1: Operating Systems\n\nOperating systems manage hardware resources and provide a stable virtual execution environment for programs.").unwrap();

    let notes = generate_study_notes(state.clone(), doc_file.to_string_lossy().to_string(), Some("Operating Systems".to_string())).unwrap();
    assert_eq!(notes.doc_name, "textbook.md");
    assert!(!notes.overview.is_empty());

    let passage_expl = explain_document_passage(state.clone(), doc_file.to_string_lossy().to_string(), "stable virtual execution environment".to_string()).unwrap();
    assert!(passage_expl.contains("stable virtual execution environment"));
}

#[test]
fn test_commands_system_health_and_ghost_status() {
    let (_ctx, state) = setup_test_context("sys_health");

    let health = get_system_health(state.clone()).unwrap();
    assert!(health.db_connected);

    let ghost_stat = get_ghost_status();
    assert!(!ghost_stat.is_enabled);

    let toggle_res = toggle_ghost_mode(false);
    assert!(toggle_res.is_ok());

    let grounded_shot = capture_grounded_screen();
    assert!(grounded_shot.is_ok());
}

#[tokio::test]
async fn test_commands_routines_and_skills_lifecycle() {
    let (_ctx, state) = setup_test_context("rout_skill");

    start_teach_task("Daily Cleanup".to_string(), "Cleans downloads".to_string()).unwrap();
    record_teach_step("click".to_string(), Some(100), Some(200), None, None).unwrap();
    let skill = stop_teach_task(state.clone()).unwrap();
    assert_eq!(skill.name, "Daily Cleanup");

    let skills = list_skills(state.clone()).unwrap();
    assert!(!skills.is_empty());

    let routine = RoutineRecord {
        id: "routine-cmd-1".to_string(),
        name: "Morning Sync".to_string(),
        cron_expression: "0 9 * * *".to_string(),
        action_type: "prompt".to_string(),
        target_id: "Summarize calendar".to_string(),
        enabled: true,
        last_run: None,
        next_run: None,
    };
    create_routine(state.clone(), routine.clone()).unwrap();

    let routines = list_routines(state.clone()).unwrap();
    assert_eq!(routines.len(), 1);

    let mut updated = routine.clone();
    updated.name = "Morning Briefing Sync".to_string();
    update_routine(state.clone(), updated).unwrap();

    let runs = list_routine_runs(state.clone(), Some(10)).unwrap();
    assert!(runs.is_empty());

    delete_routine(state.clone(), "routine-cmd-1".to_string()).unwrap();
    assert_eq!(list_routines(state.clone()).unwrap().len(), 0);

    delete_skill(state.clone(), skill.id).unwrap();
}

#[tokio::test]
async fn test_commands_bots_and_governance() {
    let (_ctx, state) = setup_test_context("bots_gov");

    let bot = BotRecord {
        id: "bot-sec-1".to_string(),
        name: "SecAuditBot".to_string(),
        avatar: "avatar.png".to_string(),
        role: "security_auditor".to_string(),
        system_prompt: "Audit everything".to_string(),
        is_default: false,
    };
    create_bot(state.clone(), bot).unwrap();

    let bots = list_bots(state.clone()).unwrap();
    assert!(bots.len() >= 4);

    let statuses = get_roster_statuses(state.clone());
    assert!(!statuses.is_empty());

    delete_bot(state.clone(), "bot-sec-1".to_string()).unwrap();

    request_takeover("Need 2FA PIN".to_string());
    let t_status = get_takeover_status();
    assert!(t_status.is_active);
    assert_eq!(t_status.reason, "Need 2FA PIN");
    resume_takeover();
    assert!(!get_takeover_status().is_active);

    let mut pol = get_governance_policy(state.clone());
    pol.require_takeover_for_credentials = true;
    save_governance_policy(state.clone(), pol.clone()).unwrap();
    assert!(get_governance_policy(state.clone()).require_takeover_for_credentials);
}

#[tokio::test]
async fn test_commands_mcp_servers_and_models() {
    let (_ctx, state) = setup_test_context("mcp_cmd");

    let srv = kyro_lib::storage::McpServerRecord {
        id: "mcp-test-1".to_string(),
        name: "Local FS Tools".to_string(),
        command: "node".to_string(),
        args_json: "[\"server.js\"]".to_string(),
        env_json: "{}".to_string(),
        enabled: true,
    };
    create_mcp_server(state.clone(), srv).await.unwrap();

    let list = list_mcp_servers(state.clone()).unwrap();
    assert_eq!(list.len(), 1);

    let statuses = get_mcp_server_statuses(state.clone()).await.unwrap();
    assert!(statuses.contains_key("mcp-test-1"));

    delete_mcp_server(state.clone(), "mcp-test-1".to_string()).await.unwrap();
    assert!(list_mcp_servers(state.clone()).unwrap().is_empty());

    let models = list_available_models().await.unwrap();
    assert!(!models.is_empty());

    let caps = get_model_capabilities("nvidia/nemotron-4-340b-instruct".to_string()).await.unwrap();
    assert_eq!(caps.model_id, "nvidia/nemotron-4-340b-instruct");
}

#[tokio::test]
async fn test_commands_projects_and_dev_servers() {
    let (ctx, state) = setup_test_context("proj_dev");
    let proj_dir = ctx.dir.join("sample_web_app");
    std::fs::create_dir_all(&proj_dir).unwrap();
    std::fs::write(proj_dir.join("package.json"), r#"{"name":"sample","scripts":{"dev":"vite"}}"#).unwrap();

    let discovered = discover_projects(ctx.dir.to_string_lossy().to_string(), None).unwrap();
    assert_eq!(discovered.candidates.len(), 1);
    assert!(!discovered.candidates[0].name.is_empty());

    let servers = list_dev_servers(state.clone()).unwrap();
    assert!(servers.is_empty());

    let server_res = get_dev_server(state.clone(), "srv-none".to_string());
    assert!(server_res.is_err());

    let logs = get_dev_server_logs(state.clone(), "srv-none".to_string(), Some(50)).unwrap();
    assert!(logs.is_empty());

    let stopped = stop_dev_server(state.clone(), "srv-none".to_string()).unwrap();
    assert!(!stopped);
}

#[tokio::test]
async fn test_agent_tools_module_functions() {
    let manager = Arc::new(BrowserAutomationManager::new());

    let defs = tools::tool_definitions();
    assert!(!defs.is_empty());

    let valid_root = tools::resolve_search_root(&json!({ "search_root": "/tmp" }));
    assert!(valid_root.is_ok());

    let default_root = tools::resolve_search_root(&json!({}));
    assert!(default_root.is_ok());

    let disc = tools::discover_project(&json!({ "search_root": "/tmp" })).await;
    assert!(!disc.message.is_empty());

    let insp = tools::inspect_project(&json!({ "project_path": "/tmp" })).await;
    assert!(!insp.message.is_empty());

    let caps = tools::browser_capabilities(&manager).await;
    assert!(!caps.is_error);

    let targ_ref = tools::target_from_args(&json!({ "ref": "elem-123" })).unwrap();
    assert_eq!(targ_ref.ref_id.unwrap(), "elem-123");

    let targ_role = tools::target_from_args(&json!({ "role": "button", "name": "Submit" })).unwrap();
    assert_eq!(targ_role.role.unwrap(), "button");

    let targ_css = tools::target_from_args(&json!({ "css": ".submit-btn" })).unwrap();
    assert_eq!(targ_css.css.unwrap(), ".submit-btn");

    let targ_coords = tools::target_from_args(&json!({ "x": 100, "y": 200 })).unwrap();
    assert_eq!(targ_coords.x, Some(100));
    assert_eq!(targ_coords.y, Some(200));

    let targ_err = tools::target_from_args(&json!({}));
    assert!(targ_err.is_err());

    let nav = tools::browser_navigate(&json!({ "url": "https://example.com" }), &manager).await;
    assert!(nav.is_error);

    let snap = tools::browser_snapshot(&json!({}), &manager).await;
    assert!(snap.is_error);

    let click = tools::browser_click(&json!({ "ref": "e1" }), &manager).await;
    assert!(click.is_error);

    let fill = tools::browser_fill(&json!({ "ref": "e1", "value": "text" }), &manager).await;
    assert!(fill.is_error);

    let press = tools::browser_press(&json!({ "key": "Enter" }), &manager).await;
    assert!(press.is_error);

    let shot = tools::browser_screenshot(&json!({}), &manager).await;
    assert!(shot.is_error);

    let cons = tools::browser_console(&json!({}), &manager).await;
    assert!(cons.is_error);

    let net = tools::browser_network(&json!({}), &manager).await;
    assert!(net.is_error);

    let tabs = tools::browser_tabs(&json!({}), &manager).await;
    assert!(tabs.is_error);

    let close = tools::browser_close(&json!({}), &manager).await;
    assert!(!close.is_error);

    let org_prev = tools::organize_files_preview(&json!({ "directory": "/tmp" })).await;
    assert!(!org_prev.message.is_empty());

    let org_exec = tools::organize_files_execute(&json!({ "plan": {} })).await;
    assert!(org_exec.is_error);

    let org_undo = tools::organize_files_undo(&json!({ "transaction": {} })).await;
    assert!(org_undo.is_error);

    let diag_build = tools::diagnose_build_failure(&json!({ "error_output": "cargo error: could not compile" })).await;
    assert!(!diag_build.message.is_empty());
}

#[tokio::test]
async fn test_connector_manager_functions_and_tools() {
    let (_ctx, state) = setup_test_context("conn_mgt");

    let v1 = json!({ "b": 2, "a": 1 });
    let v2 = json!({ "a": 1, "b": 2 });
    assert_eq!(canonicalize_json_value(&v1), canonicalize_json_value(&v2));
    assert_eq!(InvocationAuthProof::compute_args_hash(&v1), InvocationAuthProof::compute_args_hash(&v2));

    let schema = json!({
        "type": "object",
        "properties": { "name": { "type": "string" } },
        "required": ["name"]
    });
    let fp = compute_schema_fingerprint(&schema);
    assert!(!fp.is_empty());

    let cls_del = classify_external_tool("delete_file", "delete", &json!({}));
    assert_eq!(format!("{:?}", cls_del.0), "Destructive");

    let cls_read = classify_external_tool("get_user", "get", &json!({}));
    assert_eq!(format!("{:?}", cls_read.0), "Safe");

    assert!(ConnectorManager::is_permanent_refresh_error("invalid_grant: token revoked"));
    assert!(!ConnectorManager::is_permanent_refresh_error("timed out"));

    let _google_adapter = GoogleAdapter::new();
    let _github_adapter = GitHubAdapter::new();

    let acc = kyro_lib::storage::ConnectedAccountRecord {
        id: "acc-github-test".to_string(),
        connector_id: "github".to_string(),
        account_identifier: "octocat".to_string(),
        display_name: "Octocat Account".to_string(),
        status: "connected".to_string(),
        status_message: None,
        granted_scopes_json: "[\"repo\", \"issues\"]".to_string(),
        credential_key: "vault_gh_token".to_string(),
        config_json: "{}".to_string(),
        created_at: 1000,
        updated_at: 1000,
        last_connected_at: Some(1000),
        last_error: None,
        expires_at: None,
    };
    state.db.create_connected_account(&acc).unwrap();

    let catalog = state.connector_manager.list_catalog().unwrap();
    assert!(!catalog.is_empty());

    let accs = state.connector_manager.list_connected_accounts().unwrap();
    assert_eq!(accs.len(), 1);

    let all_tools = state.connector_manager.get_all_connector_tools().await;
    assert!(!all_tools.is_empty());

    let resolved = state.connector_manager.resolve_tool("github_list_repos").await;
    assert!(resolved.is_some());
}

#[test]
fn test_governance_shell_lexer_and_policy() {
    let lex1 = lex("echo hello | grep h");
    assert!(lex1.has_construct(&ShellConstruct::Operator));

    let lex2 = lex("cat file > out.txt");
    assert!(lex2.has_construct(&ShellConstruct::Redirection));

    let lex3 = lex("echo $(whoami)");
    assert!(lex3.has_construct(&ShellConstruct::Substitution));

    let lex4 = lex("echo $HOME");
    assert!(lex4.has_construct(&ShellConstruct::Variable));

    let lex5 = lex("echo 'literal string'");
    assert_eq!(lex5.executable(), Some("echo"));

    let ws = std::env::temp_dir().join(format!("test_gov_ws_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(ws.join("sub")).unwrap();
    let sub_file = ws.join("sub").join("file.txt");
    std::fs::write(&sub_file, "data").unwrap();

    let p_valid = AuthorizationEngine::validate_path_confinement(Some(&ws), &sub_file.to_string_lossy(), FileOp::Read);
    assert!(p_valid.is_ok());

    let p_traversal = AuthorizationEngine::validate_path_confinement(Some(&ws), &format!("{}/../../etc/passwd", ws.to_string_lossy()), FileOp::Read);
    assert!(p_traversal.is_err());

    let _ = std::fs::remove_dir_all(&ws);
}

#[test]
fn test_task_scheduler_and_replay_functions() {
    let sched_res = RoutineScheduler::parse_schedule("*/5 * * * *");
    assert!(sched_res.is_ok());

    let (clean_expr, tz) = RoutineScheduler::extract_timezone("CRON_TZ=America/New_York 0 9 * * *");
    assert_eq!(clean_expr, "0 9 * * *");
    assert_eq!(tz, Some("America/New_York".to_string()));

    let next_run = RoutineScheduler::compute_next_run("0 9 * * *");
    assert!(next_run.is_some());

    RoutineScheduler::set_routine_running_for_test("routine-test-flag", true);
    assert!(RoutineScheduler::is_routine_running("routine-test-flag"));
    RoutineScheduler::set_routine_running_for_test("routine-test-flag", false);
    assert!(!RoutineScheduler::is_routine_running("routine-test-flag"));

    SkillReplayer::set_skill_running_for_test("skill-test-flag", true);
    assert!(SkillReplayer::is_skill_running("skill-test-flag"));
    SkillReplayer::set_skill_running_for_test("skill-test-flag", false);
    assert!(!SkillReplayer::is_skill_running("skill-test-flag"));
}

#[test]
fn test_project_checkpoints_and_multibot_workspace() {
    let temp_ws = std::env::temp_dir().join(format!("test_git_ws_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_ws).unwrap();

    let git_state = CheckpointManager::inspect_git_state(&temp_ws);
    assert!(!git_state.is_git_repo);

    let mut cp = CheckpointManager::create_checkpoint(&temp_ws, "Initial clean baseline");
    assert_eq!(cp.description, "Initial clean baseline");

    let risk_high = CheckpointManager::assess_risk(".env", "SECRET=123");
    assert_eq!(risk_high, "high");

    let diff = CheckpointManager::compute_unified_diff("line1\n", Some("line1\nline2\n"), "test.txt");
    assert!(diff.2.contains("+line2"));

    let rollback_res = CheckpointManager::rollback(&mut cp);
    assert!(rollback_res.is_ok());

    let role = SpecialistRole::ImplementationEngineer;
    assert_eq!(role.display_name(), "Implementation Engineer");

    let _ = std::fs::remove_dir_all(&temp_ws);
}

#[test]
fn test_file_assistant_and_organizer_functions() {
    let q1 = FileAssistant::parse_natural_language_query("Find pdf invoices from October 2024 with budget");
    assert_eq!(q1.format_hint, Some("pdf".to_string()));

    let facts = FileAssistant::extract_facts_from_text("Invoice number INV-9982 issued on 2024-10-15 total $4,500.00");
    assert!(!facts.is_empty());

    let unicode_idx = FileAssistant::find_unicode_passage("Hello beautiful world", "beautiful");
    assert_eq!(unicode_idx, Some((6, 15)));

    let mut custom_map = HashMap::new();
    custom_map.insert("xyz".to_string(), "CustomType".to_string());
    assert_eq!(FileOrganizer::categorize_extension("xyz", &custom_map), "CustomType");
    assert_eq!(FileOrganizer::categorize_extension("pdf", &custom_map), "Documents");
    assert_eq!(FileOrganizer::categorize_extension("png", &custom_map), "Images");
    assert_eq!(FileOrganizer::categorize_extension("mp4", &custom_map), "Video");
}
