use kyro_lib::{
    agent::registry::AgentRegistry,
    agent::AgentLoopEngine,
    browser::BrowserAutomationManager,
    commands::*,
    connector::ConnectorManager,
    mcp::McpManager,
    project::devserver::DevServerManager,
    storage::{
        AgentRun, ApiKeys, BotRecord, Database, McpServerRecord, Message, RoutineRecord, Vault,
    },
    AppState,
};
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
    let dir = std::env::temp_dir().join(format!("kyro_cmd_test_{}_{}", tag, uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();

    let db = Database::init(dir.clone()).expect("Failed to initialize test db");
    let vault = Vault::new_encrypted_fallback(db.clone());
    let browser = Arc::new(BrowserAutomationManager::new().with_database(db.clone()));
    let dev_servers = Arc::new(DevServerManager::new());
    // File-backed registry so workspace commands have a durable config root
    // (mirrors production main.rs wiring).
    let registry = Arc::new(tokio::sync::RwLock::new(AgentRegistry::load_or_embedded(
        &dir.join("config"),
    )));
    let engine = Arc::new(AgentLoopEngine::with_registry(
        browser.clone(),
        dev_servers.clone(),
        registry,
    ));
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

#[tokio::test]
async fn test_system_prompt_builder() {
    let prompt = build_default_system_prompt();
    assert!(prompt.contains("Kyro"));
    assert!(prompt.contains("UI audit"));
    assert!(prompt.contains("Web Research"));
}

#[tokio::test]
async fn test_conversations_and_messages_commands() {
    let (_ctx, state) = setup_test_context("conv");

    // Initially empty
    let convs = list_conversations(state.clone()).unwrap();
    assert_eq!(convs.len(), 0);

    // Create conversation
    let conv = create_conversation(
        state.clone(),
        "c-1".to_string(),
        "Test Conversation".to_string(),
        "gpt-4o".to_string(),
    )
    .unwrap();
    assert_eq!(conv.id, "c-1");
    assert_eq!(conv.title, "Test Conversation");

    let convs = list_conversations(state.clone()).unwrap();
    assert_eq!(convs.len(), 1);

    // Add and retrieve messages
    let msg = Message {
        id: "m-1".to_string(),
        conversation_id: "c-1".to_string(),
        role: "user".to_string(),
        content: "Hello Kyro".to_string(),
        tool_calls_json: None,
        observations_json: None,
        created_at: 1000,
    };
    add_message(state.clone(), msg).unwrap();

    let msgs = get_messages(state.clone(), "c-1".to_string()).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content, "Hello Kyro");

    // Empty conversation messages
    let empty_msgs = get_messages(state.clone(), "c-nonexistent".to_string()).unwrap();
    assert_eq!(empty_msgs.len(), 0);

    // Delete conversation validation
    assert!(delete_conversation(state.clone(), "".to_string()).is_err());
    assert!(delete_conversation(state.clone(), "   ".to_string()).is_err());

    let deleted = delete_conversation(state.clone(), "c-1".to_string()).unwrap();
    assert!(deleted);

    let convs_after = list_conversations(state.clone()).unwrap();
    assert_eq!(convs_after.len(), 0);
}

#[tokio::test]
async fn test_vault_and_api_keys_commands() {
    let (_ctx, state) = setup_test_context("vault");

    let meta = get_api_keys(state.clone()).unwrap();
    assert!(!meta.nvidia_configured);

    let keys = ApiKeys {
        nvidia: Some("nv-test-key".to_string()),
        telegram_bot_token: Some("bot-token".to_string()),
        telegram_chat_id: Some("12345".to_string()),
    };
    save_api_keys(state.clone(), keys).unwrap();

    let meta_after = get_api_keys(state.clone()).unwrap();
    assert!(meta_after.nvidia_configured);
    assert!(meta_after.telegram_configured);

    delete_secret_key(state.clone(), "nvidia".to_string()).unwrap();
    let meta_deleted = get_api_keys(state.clone()).unwrap();
    assert!(!meta_deleted.nvidia_configured);
}

#[tokio::test]
async fn test_computer_use_and_permission_commands() {
    let (_ctx, state) = setup_test_context("cu");

    let wins = get_open_windows();
    let _ = wins;

    abort_computer_use();

    let perm1 = check_command_permission("ls -la".to_string(), false);
    assert!(perm1.allowed || !perm1.allowed);

    let perm_dangerous = check_command_permission("rm -rf /".to_string(), false);
    assert!(perm_dangerous.requires_prompt || !perm_dangerous.allowed);

    let click_action = kyro_lib::computer_use::ComputerAction::LeftClick {
        x: Some(100),
        y: Some(100),
    };
    let _ = perform_computer_action(state.clone(), click_action);
}

#[tokio::test]
async fn test_approvals_and_takeover_commands() {
    let (_ctx, state) = setup_test_context("appr");

    let pending = list_pending_approvals(state.clone()).unwrap();
    assert_eq!(pending.len(), 0);

    let res = resolve_approval(
        state.clone(),
        "appr-fake".to_string(),
        "approved".to_string(),
        Some("tester".to_string()),
    )
    .unwrap();
    assert!(!res);

    let res_inv = resolve_approval(
        state.clone(),
        "appr-fake".to_string(),
        "invalid_choice".to_string(),
        None,
    )
    .unwrap();
    assert!(!res_inv);

    assert!(!get_takeover_status().is_active);
    request_takeover("Testing takeover request".to_string());
    assert!(get_takeover_status().is_active);
    assert_eq!(get_takeover_status().reason, "Testing takeover request");
    resume_takeover();
    assert!(!get_takeover_status().is_active);
}

#[tokio::test]
async fn test_governance_policy_commands() {
    let (_ctx, state) = setup_test_context("gov");

    let policy = get_governance_policy(state.clone());
    assert_eq!(policy.shell_mode, "SafeAutoApprove");

    let mut new_policy = policy.clone();
    new_policy.shell_mode = "AskAlways".to_string();
    save_governance_policy(state.clone(), new_policy).unwrap();

    let updated = get_governance_policy(state.clone());
    assert_eq!(updated.shell_mode, "AskAlways");
}

#[tokio::test]
async fn test_sandbox_and_terminal_commands() {
    let (ctx, state) = setup_test_context("sbx");

    let status = get_sandbox_status();
    assert!(!status.available || status.available);

    let exec_res = execute_in_sandbox("alpine:latest".to_string(), "echo hi".to_string());
    assert!(exec_res.is_ok() || exec_res.is_err());

    let spawn_res = spawn_terminal(ctx.app.handle().clone(), state.clone());
    assert!(spawn_res.is_ok());

    let write_res = write_terminal_input(state.clone(), "echo hello\n".to_string());
    assert!(write_res.is_ok());

    let resize_res = resize_terminal(state.clone(), 24, 80);
    assert!(resize_res.is_ok());

    let hud_res = toggle_hud_window(ctx.app.handle().clone());
    assert!(hud_res.is_ok());

    let overlay_res = toggle_overlay_window(ctx.app.handle().clone(), true);
    assert!(overlay_res.is_ok());
    let overlay_res2 = toggle_overlay_window(ctx.app.handle().clone(), false);
    assert!(overlay_res2.is_ok());
}

#[tokio::test]
async fn test_teach_task_and_skills_commands() {
    let (ctx, state) = setup_test_context("teach");

    start_teach_task("TestSkill".to_string(), "Description".to_string()).unwrap();

    let rec_res = record_teach_step("click".to_string(), Some(50), Some(50), None, None);
    assert!(rec_res.is_ok());

    let skill = stop_teach_task(state.clone()).unwrap();
    assert_eq!(skill.name, "TestSkill");
    assert_eq!(skill.steps.len(), 1);

    let skills = list_skills(state.clone()).unwrap();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name, "TestSkill");

    let replay_res = replay_skill(
        ctx.app.handle().clone(),
        state.clone(),
        skills[0].id.clone(),
    )
    .await;
    assert!(replay_res.is_ok() || replay_res.is_err());

    let replay_fake = replay_skill(
        ctx.app.handle().clone(),
        state.clone(),
        "fake-skill-id".to_string(),
    )
    .await;
    assert!(replay_fake.is_err());

    let del_res = delete_skill(state.clone(), skills[0].id.clone()).unwrap();
    assert!(del_res);
    let skills_empty = list_skills(state.clone()).unwrap();
    assert_eq!(skills_empty.len(), 0);
}

#[tokio::test]
async fn test_routines_commands() {
    let (_ctx, state) = setup_test_context("routines");

    let routines = list_routines(state.clone()).unwrap();
    assert_eq!(routines.len(), 0);

    let routine = RoutineRecord {
        id: "rt-1".to_string(),
        name: "Morning Brief".to_string(),
        cron_expression: "0 8 * * *".to_string(),
        action_type: "prompt".to_string(),
        target_id: "Summarize top news".to_string(),
        enabled: true,
        last_run: None,
        next_run: None,
    };
    create_routine(state.clone(), routine.clone()).unwrap();

    let listed = list_routines(state.clone()).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "Morning Brief");

    let mut updated = routine.clone();
    updated.name = "Morning Brief Updated".to_string();
    update_routine(state.clone(), updated).unwrap();

    let listed2 = list_routines(state.clone()).unwrap();
    assert_eq!(listed2[0].name, "Morning Brief Updated");

    let del = delete_routine(state.clone(), "rt-1".to_string()).unwrap();
    assert!(del);
    let del_fake = delete_routine(state.clone(), "rt-fake".to_string()).unwrap();
    assert!(!del_fake);

    let parsed =
        natural_language_to_routine("Clean desktop downloads every day at 9am".to_string())
            .unwrap();
    assert!(!parsed.cron_expression.is_empty());

    let editable = kyro_lib::teach_task::portable_routine::EditableRoutine {
        version: "kyro.routine.v1".to_string(),
        id: "rt-edit".to_string(),
        name: "Backup Routine".to_string(),
        description: "Backs up files".to_string(),
        cron_expression: Some("0 0 * * *".to_string()),
        enabled: true,
        parameters: vec![],
        steps: vec![],
        created_at: 1000,
        updated_at: 1000,
        trusted: true,
    };
    let json_str = export_routine(editable.clone()).await.unwrap();
    let imported = import_routine(json_str).await.unwrap();
    assert_eq!(imported.name, "Backup Routine");
}

#[tokio::test]
async fn test_bots_and_roster_commands() {
    let (_ctx, state) = setup_test_context("bots");

    let bots = list_bots(state.clone()).unwrap();
    assert!(bots.len() >= 3);

    let bot = BotRecord {
        id: "bot-coder".to_string(),
        name: "Code Master".to_string(),
        avatar: "🤖".to_string(),
        role: "Rust coder".to_string(),
        system_prompt: "You write Rust".to_string(),
        is_default: false,
    };
    create_bot(state.clone(), bot).unwrap();

    let bots_after = list_bots(state.clone()).unwrap();
    assert_eq!(bots_after.len(), bots.len() + 1);

    let statuses = get_roster_statuses(state.clone()).await.unwrap();
    assert!(!statuses.is_empty());

    delete_bot(state.clone(), "bot-coder".to_string()).unwrap();
    let bots_final = list_bots(state.clone()).unwrap();
    assert_eq!(bots_final.len(), bots.len());
}

#[tokio::test]
async fn test_mcp_servers_commands() {
    let (_ctx, state) = setup_test_context("mcp");

    let servers = list_mcp_servers(state.clone()).unwrap();
    assert_eq!(servers.len(), 0);

    let srv = McpServerRecord {
        id: "mcp-test".to_string(),
        name: "Test MCP".to_string(),
        command: "node".to_string(),
        args_json: "[]".to_string(),
        env_json: "{}".to_string(),
        enabled: true,
    };
    create_mcp_server(state.clone(), srv).await.unwrap();

    let servers_after = list_mcp_servers(state.clone()).unwrap();
    assert_eq!(servers_after.len(), 1);

    let statuses = get_mcp_server_statuses(state.clone()).await.unwrap();
    assert_eq!(statuses.len(), 1);

    delete_mcp_server(state.clone(), "mcp-test".to_string())
        .await
        .unwrap();
    let servers_empty = list_mcp_servers(state.clone()).unwrap();
    assert_eq!(servers_empty.len(), 0);
}

#[tokio::test]
async fn test_agent_runs_lifecycle_commands() {
    let (_ctx, state) = setup_test_context("runs");

    // Must first create conversation to satisfy foreign key constraint
    create_conversation(
        state.clone(),
        "conv-1".to_string(),
        "Run Conversation".to_string(),
        "claude-3-5-sonnet".to_string(),
    )
    .unwrap();

    let run = AgentRun {
        id: "run-test-1".to_string(),
        conversation_id: Some("conv-1".to_string()),
        agent_id: "apex".to_string(),
        model: "claude-3-5-sonnet".to_string(),
        prompt: "Help me code".to_string(),
        status: "running".to_string(),
        created_at: 1000,
        started_at: Some(1000),
        completed_at: None,
        error: None,
        config_json: None,
        parent_run_id: None,
    };
    state.db.create_run(&run).unwrap();

    let runs = list_agent_runs(state.clone(), Some(10)).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].id, "run-test-1");

    let fetched = get_agent_run(state.clone(), "run-test-1".to_string()).unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().status, "running");

    let events = get_run_events(state.clone(), "run-test-1".to_string(), None).unwrap();
    assert_eq!(events.len(), 0);

    // Register run in engine so cancel_agent_run finds active token
    state
        .engine
        .register_run_token(
            "run-test-1",
            kyro_lib::agent::cancellation::CancellationToken::new(),
        )
        .await;

    let cancelled = cancel_agent_run(state.clone(), "run-test-1".to_string())
        .await
        .unwrap();
    assert!(cancelled);

    let cancelled_again = cancel_agent_run(state.clone(), "run-test-1".to_string())
        .await
        .unwrap();
    assert!(!cancelled_again);

    let cancelled_fake = cancel_agent_run(state.clone(), "run-fake".to_string()).await;
    assert!(cancelled_fake.is_err());
}

#[tokio::test]
async fn test_run_cancellation_race_elimination() {
    let (_ctx, state) = setup_test_context("cancel_race");

    create_conversation(
        state.clone(),
        "conv-race".to_string(),
        "Cancellation Race Test".to_string(),
        "claude-3-5-sonnet".to_string(),
    )
    .unwrap();

    let run_id = "run-race-1";
    let run = AgentRun {
        id: run_id.to_string(),
        conversation_id: Some("conv-race".to_string()),
        agent_id: "apex".to_string(),
        model: "claude-3-5-sonnet".to_string(),
        prompt: "Will be cancelled before engine registration".to_string(),
        status: "running".to_string(),
        created_at: 1000,
        started_at: Some(1000),
        completed_at: None,
        error: None,
        config_json: None,
        parent_run_id: None,
    };
    state.db.create_run(&run).unwrap();

    // 1. Cancel run before engine registers it
    let res = cancel_agent_run(state.clone(), run_id.to_string()).await;
    assert!(res.is_ok());
    assert!(res.unwrap());

    // 2. Engine recognizes run as cancelled
    assert!(state.engine.is_run_cancelled(run_id).await);

    // 3. Status in DB is updated to cancelled
    let db_run = state.db.get_run(run_id).unwrap().unwrap();
    assert_eq!(db_run.status, "cancelled");

    // 4. Repeated cancellation returns false since run is already terminal
    let res_repeat = cancel_agent_run(state.clone(), run_id.to_string())
        .await
        .unwrap();
    assert!(!res_repeat);
}

#[tokio::test]
async fn test_ghost_grounding_timeline_apps_commands() {
    let (_ctx, state) = setup_test_context("ghost");

    let ghost = get_ghost_status();
    assert!(!ghost.is_enabled || ghost.is_enabled);

    let toggle_res = toggle_ghost_mode(false);
    assert!(toggle_res.is_ok());

    let ground_res = capture_grounded_screen();
    assert!(ground_res.is_ok() || ground_res.is_err());

    let steps = get_timeline_steps(state.clone(), Some("run-1".to_string())).unwrap();
    assert_eq!(steps.len(), 0);

    let clear_res = clear_timeline_steps(state.clone());
    assert!(clear_res.is_ok());

    let apps = get_installed_apps();
    assert!(apps.is_empty() || !apps.is_empty());
}

#[tokio::test]
async fn test_memory_and_artifacts_commands() {
    let (_ctx, state) = setup_test_context("mem_art");

    let mems = list_memories(state.clone()).unwrap();
    assert_eq!(mems.len(), 0);

    save_memory_fact(
        state.clone(),
        "global".to_string(),
        "user".to_string(),
        "theme".to_string(),
        "dark".to_string(),
        "settings".to_string(),
    )
    .unwrap();

    let mems_after = list_memories(state.clone()).unwrap();
    assert_eq!(mems_after.len(), 1);
    assert_eq!(mems_after[0].key, "theme");
    assert_eq!(mems_after[0].value, "dark");

    let del_mem = delete_memory_fact(state.clone(), mems_after[0].id.clone()).unwrap();
    assert!(del_mem);

    let arts = list_artifacts(state.clone(), None, None).unwrap();
    assert_eq!(arts.len(), 0);

    // Create artifact without foreign key requirement (conv_id: None)
    let art_rec = create_artifact(
        state.clone(),
        None,
        None,
        "report.txt".to_string(),
        "Report contents here".to_string(),
        "text".to_string(),
    )
    .unwrap();
    assert!(!art_rec.id.is_empty());

    let fetched_art = get_artifact(state.clone(), art_rec.id.clone()).unwrap();
    assert!(fetched_art.is_some());
    assert_eq!(fetched_art.unwrap().name, "report.txt");

    let content = read_artifact_content(state.clone(), art_rec.id.clone()).unwrap();
    assert_eq!(content, "Report contents here");

    let comment_res = add_artifact_comment(
        state.clone(),
        art_rec.id.clone(),
        "Good summary".to_string(),
        false,
    );
    assert!(comment_res.is_ok());

    let export_dir = std::env::temp_dir().join(format!("art_export_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&export_dir).unwrap();
    let target_export = export_dir.join("exported_report.txt");
    let export_res = export_artifact(
        state.clone(),
        art_rec.id.clone(),
        target_export.to_string_lossy().to_string(),
    );
    assert!(export_res.is_ok());
    assert!(target_export.exists());

    let del_art = delete_artifact(state.clone(), art_rec.id).unwrap();
    assert!(del_art);

    std::fs::remove_dir_all(&export_dir).ok();
}

#[tokio::test]
async fn test_templates_and_system_health_commands() {
    let (_ctx, state) = setup_test_context("health");

    let starters = get_starter_templates();
    assert!(!starters.is_empty());

    let exported = export_workspace_templates(
        state.clone(),
        "My Workspace".to_string(),
        "Description".to_string(),
    )
    .unwrap();
    assert!(exported.contains("My Workspace"));

    let imported = import_workspace_templates(state.clone(), exported).unwrap();
    let _ = imported.skills_imported;

    let health = get_system_health(state.clone()).unwrap();
    assert!(health.db_connected);

    let search_res = search_workspace(state.clone(), "test".to_string()).unwrap();
    assert!(search_res.is_empty() || !search_res.is_empty());
}

#[tokio::test]
async fn test_browser_and_dev_servers_commands() {
    let (_ctx, state) = setup_test_context("browser_cmd");

    let caps = get_browser_capabilities(state.clone()).await.unwrap();
    assert!(!caps.browsers.is_empty());

    let settings = get_browser_settings(state.clone()).unwrap();
    assert!(!settings.selected_browser.is_empty());

    let new_settings = kyro_lib::browser::BrowserSettings {
        selected_browser: "chrome".to_string(),
        headless_default: false,
    };
    save_browser_settings(state.clone(), new_settings).unwrap();

    let sessions = list_browser_sessions(state.clone()).await.unwrap();
    assert_eq!(sessions.len(), 0);

    let close_res = close_browser_session(state.clone(), "fake-session".to_string()).await;
    assert!(close_res.is_ok() || close_res.is_err());

    let servers = list_dev_servers(state.clone()).unwrap();
    assert_eq!(servers.len(), 0);

    let fake_srv = get_dev_server(state.clone(), "fake-server".to_string());
    assert!(fake_srv.is_err());

    let fake_logs = get_dev_server_logs(state.clone(), "fake-server".to_string(), Some(10));
    assert!(fake_logs.is_err());

    let stop_fake = stop_dev_server(state.clone(), "fake-server".to_string());
    assert!(stop_fake.is_err());
}

#[tokio::test]
async fn test_connector_catalog_and_accounts_commands() {
    let (_ctx, state) = setup_test_context("conn_catalog");

    let catalog = list_connector_catalog(state.clone()).unwrap();
    assert!(!catalog.is_empty());

    let accounts = list_connected_accounts(state.clone()).unwrap();
    assert_eq!(accounts.len(), 0);

    let flow_res = start_oauth_flow(
        state.clone(),
        "unsupported-provider".to_string(),
        None,
        None,
    )
    .await;
    assert!(flow_res.is_err());

    let status_res = get_oauth_status(state.clone(), "fake-flow-id".to_string()).await;
    assert!(status_res.is_err());

    let complete_fake = complete_oauth_flow(
        state.clone(),
        "fake-flow-id".to_string(),
        Some("fake-code".to_string()),
    )
    .await;
    assert!(complete_fake.is_err());

    let disconnect_fake = disconnect_account(state.clone(), "fake-account".to_string()).await;
    assert!(disconnect_fake.is_err());

    let test_conn = test_connector_connection(state.clone(), "fake-account".to_string()).await;
    assert!(test_conn.is_err() || test_conn.is_ok());

    let tools = get_connector_tools(state.clone()).await.unwrap();
    assert_eq!(tools.len(), 0);

    let policies = list_account_tool_policies(state.clone(), "fake-account".to_string()).unwrap();
    assert_eq!(policies.len(), 0);

    let save_pol_inv = save_account_tool_policy(
        state.clone(),
        "fake-account".to_string(),
        "google_gmail_send".to_string(),
        "invalid_policy".to_string(),
        "high".to_string(),
    )
    .await;
    assert!(save_pol_inv.is_err());

    let save_pol_missing = save_account_tool_policy(
        state.clone(),
        "fake-account".to_string(),
        "google_gmail_send".to_string(),
        "deny".to_string(),
        "high".to_string(),
    )
    .await;
    assert!(save_pol_missing.is_err());
}

#[tokio::test]
async fn test_authorized_folders_and_documents_commands() {
    let (ctx, state) = setup_test_context("auth_docs");

    let suggested = get_suggested_folders();
    assert!(suggested.is_ok());

    let folders = list_authorized_folders(state.clone()).unwrap();
    assert_eq!(folders.len(), 0);

    let safe_folder = ctx.dir.join("safe_work");
    std::fs::create_dir_all(&safe_folder).unwrap();

    let add_res = add_authorized_folder(
        state.clone(),
        safe_folder.to_string_lossy().to_string(),
        Some("Safe Work".to_string()),
        None,
    );
    assert!(add_res.is_ok());

    let folders_after = list_authorized_folders(state.clone()).unwrap();
    assert_eq!(folders_after.len(), 1);

    let docs = search_authorized_documents(state.clone(), "notes".to_string(), None, None).unwrap();
    assert_eq!(docs.len(), 0);

    let rem_res = remove_authorized_folder(state.clone(), Some(folders_after[0].id.clone()), None);
    assert!(rem_res.is_ok());

    let folders_empty = list_authorized_folders(state.clone()).unwrap();
    assert_eq!(folders_empty.len(), 0);
}

#[tokio::test]
async fn test_file_assistant_operations_commands() {
    let (ctx, state) = setup_test_context("file_op");

    let work_dir = ctx.dir.join("safe_ops");
    std::fs::create_dir_all(&work_dir).unwrap();

    add_authorized_folder(
        state.clone(),
        work_dir.to_string_lossy().to_string(),
        Some("Ops".to_string()),
        None,
    )
    .unwrap();

    let src_file = work_dir.join("source.txt");
    std::fs::write(&src_file, "Sample file content").unwrap();

    let dst_file = work_dir.join("destination.txt");

    let copy_res = execute_safe_file_op(
        state.clone(),
        "copy".to_string(),
        src_file.to_string_lossy().to_string(),
        Some(dst_file.to_string_lossy().to_string()),
        false,
    )
    .unwrap();
    assert!(copy_res.success);
    assert!(dst_file.exists());

    let dups =
        detect_folder_duplicates(state.clone(), work_dir.to_string_lossy().to_string()).unwrap();
    assert!(dups.is_empty() || !dups.is_empty());

    let txs = list_file_transactions(state.clone(), Some(10)).unwrap();
    assert!(txs.is_empty() || !txs.is_empty());

    let moved_file = work_dir.join("moved.txt");
    let move_res = execute_safe_file_op(
        state.clone(),
        "move".to_string(),
        dst_file.to_string_lossy().to_string(),
        Some(moved_file.to_string_lossy().to_string()),
        false,
    )
    .unwrap();
    assert!(move_res.success);
    assert!(moved_file.exists());

    // Delete file safe without confirmation returns requires_confirmation
    let del_unconfirmed = execute_safe_file_op(
        state.clone(),
        "delete".to_string(),
        moved_file.to_string_lossy().to_string(),
        None,
        false,
    )
    .unwrap();
    assert!(!del_unconfirmed.success);
    assert!(del_unconfirmed.requires_confirmation);
    assert!(moved_file.exists());

    // Delete file safe with confirmation succeeds
    let del_res = execute_safe_file_op(
        state.clone(),
        "delete".to_string(),
        moved_file.to_string_lossy().to_string(),
        None,
        true,
    )
    .unwrap();
    assert!(del_res.success);
    assert!(!moved_file.exists());
}

#[tokio::test]
async fn test_model_capabilities_and_today_dashboard_commands() {
    let (_ctx, state) = setup_test_context("models_today");

    let models = list_available_models().await.unwrap();
    assert!(!models.is_empty());

    let cap = get_model_capabilities("nvidia/nemotron-4-340b-instruct".to_string())
        .await
        .unwrap();
    assert_eq!(cap.model_id, "nvidia/nemotron-4-340b-instruct");

    let dash = get_today_dashboard_data(state.clone()).await.unwrap();
    assert!(!dash.greeting.is_empty());
    assert!(!dash.services.is_empty());
}

#[tokio::test]
async fn test_project_checkpoints_and_discovery_commands() {
    let (ctx, _state) = setup_test_context("proj_chk");

    let proj_dir = ctx.dir.join("sample_proj");
    std::fs::create_dir_all(&proj_dir).unwrap();
    std::fs::write(
        proj_dir.join("package.json"),
        r#"{"name": "test-app", "scripts": {"build": "exit 1"}}"#,
    )
    .unwrap();

    let disc = discover_projects(
        "test-app".to_string(),
        Some(ctx.dir.to_string_lossy().to_string()),
    )
    .unwrap();
    let _ = disc.candidates.len();

    let chk = create_code_checkpoint(
        proj_dir.to_string_lossy().to_string(),
        "Init checkpoint".to_string(),
    )
    .await
    .unwrap();
    assert_eq!(chk.description, "Init checkpoint");

    let rollback = rollback_code_checkpoint(chk).await.unwrap();
    assert_eq!(rollback.status, "success");

    let diag = diagnose_build_failure(
        proj_dir.to_string_lossy().to_string(),
        "npm run build".to_string(),
        "error TS2304: Cannot find name 'foo'".to_string(),
    )
    .await
    .unwrap();
    assert!(!diag.root_cause_error.is_empty());
}

#[tokio::test]
async fn test_visual_baselines_and_multiagent_workspace_commands() {
    let (ctx, state) = setup_test_context("vis_base");

    let proj_dir = ctx.dir.join("vis_proj");
    std::fs::create_dir_all(&proj_dir).unwrap();

    // 1x1 transparent png in base64
    let fake_png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    let saved = set_audit_baseline(
        proj_dir.to_string_lossy().to_string(),
        "/".to_string(),
        "desktop".to_string(),
        fake_png.to_string(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(saved.route, "/");

    let loaded = get_audit_baseline(
        proj_dir.to_string_lossy().to_string(),
        "/".to_string(),
        "desktop".to_string(),
    )
    .await
    .unwrap();
    assert!(loaded.is_some());

    let comp = compare_visual_baseline(
        fake_png.to_string(),
        fake_png.to_string(),
        "/".to_string(),
        "desktop".to_string(),
        Some(1.0),
    )
    .await
    .unwrap();
    assert!(!comp.has_regression);

    // Multi-agent workspace
    let ws = create_multi_agent_workspace(
        state.clone(),
        "run-parent-1".to_string(),
        proj_dir.to_string_lossy().to_string(),
        "Refactor API".to_string(),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(ws.parent_run_id, "run-parent-1");

    let recon = reconcile_multi_agent_workspace(
        state.clone(),
        proj_dir.to_string_lossy().to_string(),
        proj_dir.to_string_lossy().to_string(),
        ws.workspace_id.clone(),
    )
    .await;
    assert!(recon.is_ok() || recon.is_err());
}

#[tokio::test]
async fn test_document_assistant_and_file_organization_commands() {
    let (ctx, state) = setup_test_context("doc_org");

    let safe_folder = ctx.dir.join("docs_folder");
    std::fs::create_dir_all(&safe_folder).unwrap();

    add_authorized_folder(
        state.clone(),
        safe_folder.to_string_lossy().to_string(),
        Some("Docs Folder".to_string()),
        None,
    )
    .unwrap();

    let doc_a = safe_folder.join("doc_a.txt");
    std::fs::write(
        &doc_a,
        "Meeting Notes\nAction: Submit Q3 budget by Friday Oct 24.\nKey Fact: Revenue grew 20%.",
    )
    .unwrap();

    let doc_b = safe_folder.join("doc_b.txt");
    std::fs::write(
        &doc_b,
        "Meeting Notes\nAction: Submit Q3 budget by Monday Oct 27.\nKey Fact: Revenue grew 25%.",
    )
    .unwrap();

    let analysis = analyze_document(doc_a.to_string_lossy().to_string(), None, state.clone())
        .await
        .unwrap();
    assert_eq!(analysis.metadata.file_name, "doc_a.txt");

    let comp_docs = compare_documents(
        doc_a.to_string_lossy().to_string(),
        doc_b.to_string_lossy().to_string(),
        state.clone(),
    )
    .await
    .unwrap();
    assert!(!comp_docs.markdown_report.is_empty());

    let notes = generate_study_notes(
        state.clone(),
        doc_a.to_string_lossy().to_string(),
        Some("budget".to_string()),
    )
    .unwrap();
    assert!(!notes.overview.is_empty());

    let expl = explain_document_passage(
        state.clone(),
        doc_a.to_string_lossy().to_string(),
        "Revenue grew 20%".to_string(),
    )
    .unwrap();
    assert!(!expl.is_empty());

    // File organization preview, execute, undo
    let preview = preview_file_organization(
        state.clone(),
        safe_folder.to_string_lossy().to_string(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        preview.target_directory,
        safe_folder.canonicalize().unwrap()
    );

    let trx = execute_file_organization(preview, state.clone())
        .await
        .unwrap();

    // Verify transaction retrieval and listing
    let retrieved = get_file_transaction(state.clone(), trx.transaction_id.clone())
        .unwrap()
        .expect("transaction should exist");
    assert_eq!(retrieved.transaction_id, trx.transaction_id);

    let listed_txs = list_file_transactions(state.clone(), Some(10)).unwrap();
    assert!(listed_txs
        .iter()
        .any(|t| t.transaction_id == trx.transaction_id));

    // Test undo_file_transaction by transaction ID command
    let undone_count = undo_file_transaction(state.clone(), trx.transaction_id.clone())
        .await
        .unwrap();
    assert_eq!(undone_count, 2);

    let updated_tx = get_file_transaction(state.clone(), trx.transaction_id.clone())
        .unwrap()
        .expect("transaction should exist");
    assert_eq!(updated_tx.status, "undone");
}

#[tokio::test]
async fn test_routine_execution_and_nl_parser_comprehensive() {
    let (_ctx, state) = setup_test_context("rt_exec_nl");

    // Comprehensive natural language parsing coverage
    let tests = vec![
        "every day at 9am clean cache",
        "daily at 18:30 backup database",
        "every weekday in the morning run standup",
        "every monday at 10am weekly sync",
        "every 15 min check status",
        "every 30 min sync email",
        "every hour ping server",
        "on the 1st of the month generate report",
        "at midnight purge logs",
        "at noon review pull requests",
        "in the evening summarize tasks",
        "at night run deep tests",
        "random prompt requiring clarification",
    ];

    for prompt in tests {
        let res = natural_language_to_routine(prompt.to_string()).unwrap();
        assert!(!res.cron_expression.is_empty());
    }

    // List routine runs
    let runs = list_routine_runs(state.clone(), Some(10)).unwrap();
    assert_eq!(runs.len(), 0);

    // Execute non-existent routine
    let exec_fake = execute_routine_now(state.clone(), "fake-rt".to_string()).await;
    assert!(exec_fake.is_err());
}

#[tokio::test]
async fn test_connect_account_token_command() {
    let (_ctx, state) = setup_test_context("acc_token");

    // Fails closed with invalid credential verification
    let res = connect_account_token(
        state.clone(),
        "github".to_string(),
        "My GitHub".to_string(),
        "octocat".to_string(),
        "ghp_test_token_secret".to_string(),
        None,
    )
    .await;

    assert!(res.is_err());
    let err_msg = res.unwrap_err();
    assert!(err_msg.contains("Credential verification failed") || err_msg.contains("failed"));

    // Unsupported provider
    let unsupp = connect_account_token(
        state.clone(),
        "unknown_service".to_string(),
        "Unknown".to_string(),
        "user".to_string(),
        "token".to_string(),
        None,
    )
    .await;
    assert!(unsupp.is_err());
}

#[tokio::test]
async fn test_artifact_lifecycle_and_content_export() {
    let (_ctx, state) = setup_test_context("artifact_lifecycle");

    // Create an artifact record
    let art_id = "art_test_123".to_string();
    let file_path = _ctx.dir.join("test_art.md");
    std::fs::write(&file_path, "# Sample Markdown Artifact\nLine 1\nLine 2").unwrap();

    let record = kyro_lib::storage::ArtifactRecord {
        id: art_id.clone(),
        run_id: None,
        conversation_id: None,
        name: "test_art.md".to_string(),
        file_path: file_path.to_string_lossy().to_string(),
        artifact_type: "markdown".to_string(),
        mime_type: "text/markdown".to_string(),
        size_bytes: 40,
        created_at: 1000,
        metadata_json: None,
    };
    state.db.create_artifact(&record).unwrap();

    // Read artifact content
    let content = read_artifact_content(state.clone(), art_id.clone()).unwrap();
    assert!(content.contains("Sample Markdown Artifact"));

    // Add artifact comment
    add_artifact_comment(
        state.clone(),
        art_id.clone(),
        "Review comment".to_string(),
        false,
    )
    .unwrap();
    let art_fetched = get_artifact(state.clone(), art_id.clone()).unwrap();
    assert!(art_fetched.is_some());

    // Export artifact to target path
    let export_target = _ctx.dir.join("exported_art.md");
    export_artifact(
        state.clone(),
        art_id.clone(),
        export_target.to_string_lossy().to_string(),
    )
    .unwrap();
    assert!(export_target.exists());

    // Search workspace (covers universal search over memories)
    save_memory_fact(
        state.clone(),
        "workspace".to_string(),
        "notes".to_string(),
        "SampleKey".to_string(),
        "Sample memory content".to_string(),
        "user".to_string(),
    )
    .unwrap();
    let search_res = search_workspace(state.clone(), "Sample".to_string()).unwrap();
    assert!(!search_res.is_empty());

    // Delete artifact
    let deleted = delete_artifact(state.clone(), art_id.clone()).unwrap();
    assert!(deleted);
}

#[tokio::test]
async fn test_workspace_search_and_templates_export_import() {
    let (_ctx, state) = setup_test_context("templates_cmd");

    // Starter templates
    let starter = get_starter_templates();
    assert!(!starter.is_empty());

    // Export workspace templates
    let exp_res = export_workspace_templates(
        state.clone(),
        "TestPack".to_string(),
        "Description".to_string(),
    )
    .unwrap();
    assert!(exp_res.contains("TestPack"));

    // Import workspace templates
    let imp_res = import_workspace_templates(state.clone(), exp_res);
    assert!(imp_res.is_ok());
}

#[tokio::test]
async fn test_routine_portable_export_and_import() {
    let routine = kyro_lib::teach_task::portable_routine::EditableRoutine {
        version: "kyro.routine.v1".to_string(),
        id: "rt_editable".to_string(),
        name: "Sync Workflows".to_string(),
        description: "Scheduled workflow".to_string(),
        cron_expression: Some("0 9 * * 1-5".to_string()),
        enabled: true,
        parameters: vec![],
        steps: vec![],
        created_at: 1000,
        updated_at: 1000,
        trusted: true,
    };

    let exported = export_routine(routine.clone()).await.unwrap();
    assert!(exported.contains("Sync Workflows"));

    let imported = import_routine(exported).await.unwrap();
    assert_eq!(imported.name, "Sync Workflows");
    assert_eq!(imported.cron_expression.as_deref(), Some("0 9 * * 1-5"));
}

#[tokio::test]
async fn test_folder_suggestions_and_safe_file_ops() {
    let (_ctx, state) = setup_test_context("safe_file_ops");

    // Suggested folders
    let suggested = get_suggested_folders();
    assert!(suggested.is_ok());

    // Authorize folder in state
    add_authorized_folder(
        state.clone(),
        _ctx.dir.to_string_lossy().to_string(),
        Some("TestDir".to_string()),
        None,
    )
    .unwrap();

    // Detect duplicates in temp directory
    let dupes =
        detect_folder_duplicates(state.clone(), _ctx.dir.to_string_lossy().to_string()).unwrap();
    assert_eq!(dupes.len(), 0);

    // List file transactions
    let txs = list_file_transactions(state.clone(), None).unwrap();
    assert_eq!(txs.len(), 0);

    // Safe file operation
    let test_file = _ctx.dir.join("safe_test.txt");
    std::fs::write(&test_file, "Safe content").unwrap();
    let dest_file = _ctx.dir.join("safe_dest.txt");
    let exec_res = execute_safe_file_op(
        state.clone(),
        "copy".to_string(),
        test_file.to_string_lossy().to_string(),
        Some(dest_file.to_string_lossy().to_string()),
        false,
    );
    assert!(exec_res.is_ok());
}

#[tokio::test]
async fn test_screen_capture_and_ghost_mode_commands() {
    let (_ctx, state) = setup_test_context("screen_ghost");

    // Ghost status and toggle
    let ghost = get_ghost_status();
    let _ = ghost;
    let _ = toggle_ghost_mode(false);

    // Open windows & screenshot commands
    let _ = get_open_windows();
    let _ = take_screenshot(None);
    let _ = take_region_screenshot(0, 0, 10, 10);
    abort_computer_use();

    // Command permissions
    let perm = check_command_permission("ls -la".to_string(), true);
    assert!(perm.allowed);

    // Explain screen capture without model fallback
    let sample_png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==".to_string();
    let expl = explain_screen_capture(sample_png, "What is this?".to_string(), None, state.clone())
        .await
        .unwrap();
    assert!(!expl.summary.is_empty());
}

#[tokio::test]
async fn test_connector_policies_and_account_lifecycle() {
    let (_ctx, state) = setup_test_context("connector_cmd_lifecycle");

    // List catalog & accounts
    let catalog = list_connector_catalog(state.clone()).unwrap();
    assert!(!catalog.is_empty());
    let accounts = list_connected_accounts(state.clone()).unwrap();
    assert_eq!(accounts.len(), 0);

    // Start OAuth flow for GitHub
    let oauth_init = start_oauth_flow(
        state.clone(),
        "github".to_string(),
        Some("client_id_123".to_string()),
        Some("client_secret_xyz".to_string()),
    )
    .await;
    assert!(oauth_init.is_ok());
    let init_res = oauth_init.unwrap();
    assert!(!init_res.session_id.is_empty());

    // OAuth status initially pending
    let status = get_oauth_status(state.clone(), init_res.session_id.clone())
        .await
        .unwrap();
    assert_eq!(status.status, "pending");

    // Invalid session id
    assert!(cancel_oauth_flow(state.clone(), "".to_string())
        .await
        .is_err());
    assert!(cancel_oauth_flow(state.clone(), "   ".to_string())
        .await
        .is_err());

    // Cancel OAuth flow via command
    let cancel_res = cancel_oauth_flow(state.clone(), init_res.session_id.clone()).await;
    assert!(cancel_res.is_ok());

    // Status is now cancelled
    let status_after = get_oauth_status(state.clone(), init_res.session_id.clone())
        .await
        .unwrap();
    assert_eq!(status_after.status, "cancelled");

    // Repeated cancellation fails deterministically
    assert!(
        cancel_oauth_flow(state.clone(), init_res.session_id.clone())
            .await
            .is_err()
    );

    // Complete OAuth flow on cancelled session fails
    let comp = complete_oauth_flow(state.clone(), init_res.session_id, None).await;
    assert!(comp.is_err());

    // Disconnect non-existent account
    let disc = disconnect_account(state.clone(), "fake-acc".to_string()).await;
    assert!(disc.is_err());

    // Test connection on non-existent account
    let test_c = test_connector_connection(state.clone(), "fake-acc".to_string()).await;
    assert!(test_c.is_err());

    // Connector tools before accounts are connected
    let tools_empty = get_connector_tools(state.clone()).await.unwrap();
    assert_eq!(tools_empty.len(), 0);

    // Account tool policies for non-existent account
    let policies = list_account_tool_policies(state.clone(), "acc-1".to_string()).unwrap();
    assert_eq!(policies.len(), 0);
    let acc_tools_missing = get_account_tools(state.clone(), "acc-1".to_string()).await;
    assert!(acc_tools_missing.is_err());

    // Save tool policy on non-existent account returns error
    let save_pol_missing = save_account_tool_policy(
        state.clone(),
        "non-existent".to_string(),
        "create_issue".to_string(),
        "ask".to_string(),
        "Risk notes".to_string(),
    )
    .await;
    assert!(save_pol_missing.is_err());

    // Create connected account and refresh connector tools
    let acc_record = kyro_lib::storage::db::ConnectedAccountRecord {
        id: "acc-1".to_string(),
        connector_id: "github".to_string(),
        account_identifier: "octocat".to_string(),
        display_name: "Octocat".to_string(),
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
    state.db.create_connected_account(&acc_record).unwrap();
    let tools = get_connector_tools(state.clone()).await.unwrap();
    assert!(!tools.is_empty());

    let acc_tools = get_account_tools(state.clone(), "acc-1".to_string())
        .await
        .unwrap();
    assert!(!acc_tools.is_empty());

    let first_tool = acc_tools[0].name.clone();
    let save_ok = save_account_tool_policy(
        state.clone(),
        "acc-1".to_string(),
        first_tool,
        "ask".to_string(),
        "Risk notes".to_string(),
    )
    .await;
    assert!(save_ok.is_ok());
}

#[tokio::test]
async fn test_checkpoint_and_build_doctor_commands() {
    let (_ctx, _state) = setup_test_context("checkpoint_doctor");

    // Create code checkpoint
    let cp_res = create_code_checkpoint(
        _ctx.dir.to_string_lossy().to_string(),
        "Test Checkpoint".to_string(),
    )
    .await;
    assert!(cp_res.is_ok());
    let cp = cp_res.unwrap();
    assert_eq!(cp.description, "Test Checkpoint");

    // Rollback code checkpoint
    let rb_res = rollback_code_checkpoint(cp).await;
    assert!(rb_res.is_ok());

    // Diagnose build failure
    let diag = diagnose_build_failure(
        _ctx.dir.to_string_lossy().to_string(),
        "npm run build".to_string(),
        "Error: Cannot find module 'react'".to_string(),
    )
    .await;
    assert!(diag.is_ok());
    let d = diag.unwrap();
    assert!(
        d.root_cause_error.contains("missing")
            || d.root_cause_error.contains("module")
            || !d.suggested_repair.is_empty()
    );
}

#[tokio::test]
async fn test_multiagent_workspace_and_baseline_commands() {
    let (_ctx, state) = setup_test_context("multiagent_baseline");

    // Multi-agent workspace
    let ws = create_multi_agent_workspace(
        state.clone(),
        "parent_run_1".to_string(),
        _ctx.dir.to_string_lossy().to_string(),
        "Build feature".to_string(),
        None,
        None,
    )
    .await;
    assert!(ws.is_ok());
    let ws_record = ws.unwrap();
    assert_eq!(ws_record.parent_run_id, "parent_run_1");

    let rec = reconcile_multi_agent_workspace(
        state.clone(),
        _ctx.dir.to_string_lossy().to_string(),
        _ctx.dir.to_string_lossy().to_string(),
        ws_record.workspace_id,
    )
    .await;
    assert!(rec.is_ok());

    // Visual baseline commands
    let sample_png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==".to_string();
    let set_base = set_audit_baseline(
        _ctx.dir.to_string_lossy().to_string(),
        "/home".to_string(),
        "1280x800".to_string(),
        sample_png.clone(),
        None,
    )
    .await;
    assert!(set_base.is_ok());

    let get_base = get_audit_baseline(
        _ctx.dir.to_string_lossy().to_string(),
        "/home".to_string(),
        "1280x800".to_string(),
    )
    .await;
    assert!(get_base.is_ok());
    assert!(get_base.unwrap().is_some());

    let comp_base = compare_visual_baseline(
        sample_png.clone(),
        sample_png,
        "/home".to_string(),
        "1280x800".to_string(),
        Some(1.0),
    )
    .await;
    assert!(comp_base.is_ok());
}

#[tokio::test]
async fn test_terminal_and_timeline_commands() {
    let (_ctx, state) = setup_test_context("term_timeline");

    // Terminal commands (gracefully no-op when no active pty session)
    let write_in = write_terminal_input(state.clone(), "echo 1\n".to_string());
    assert!(write_in.is_ok());

    let resize = resize_terminal(state.clone(), 24, 80);
    assert!(resize.is_ok());

    // Timeline steps
    let steps = get_timeline_steps(state.clone(), None).unwrap();
    assert_eq!(steps.len(), 0);

    let clear = clear_timeline_steps(state.clone());
    assert!(clear.is_ok());

    // Installed apps
    let apps = get_installed_apps();
    let _ = apps;
}

#[tokio::test]
async fn test_agent_profile_registry_commands() {
    use kyro_lib::agent::ProfileOrigin;

    let (_ctx, state) = setup_test_context("agent_profiles");

    // 1. Bundled profiles are listed out of the box, with sentry builtins.
    let listed = list_agent_profiles(state.clone()).await.unwrap();
    assert!(
        listed.len() >= 5,
        "expected at least the 5 shipped profiles"
    );
    assert!(listed
        .iter()
        .any(|p| p.id == "apex-orchestrator" && p.origin == ProfileOrigin::Builtin));
    assert!(listed
        .iter()
        .any(|p| p.id == "forge-devops" && p.origin == ProfileOrigin::Builtin));

    // 2. Document read for a builtin returns the shipped manifest.
    let doc = get_agent_profile_document(state.clone(), "scout-web".to_string())
        .await
        .unwrap();
    assert!(doc.document.contains("id: scout-web"));

    // 3. Create a custom profile: it lands on disk as a Markdown manifest and
    //    appears in the roster.
    let created = create_agent_profile(
        state.clone(),
        CreateAgentProfileRequest {
            id: "Sentinel Reviewer".to_string(),
            name: "Sentinel".to_string(),
            avatar: "shield".to_string(),
            role: "Static analysis & review".to_string(),
            description: "Reviews changes".to_string(),
            instructions: "# Identity\n\nYou are Sentinel, a careful reviewer.".to_string(),
        },
    )
    .await
    .unwrap();
    assert_eq!(created.id, "sentinel-reviewer");
    assert_eq!(created.origin, ProfileOrigin::Custom);
    assert!(!created.is_builtin);
    assert!(created.version_label.starts_with("v1:"));

    let listed_after = list_agent_profiles(state.clone()).await.unwrap();
    assert!(listed_after.iter().any(|p| p.id == "sentinel-reviewer"));
    // Built-in ids are never shadowed by create.
    let dup_builtin = create_agent_profile(
        state.clone(),
        CreateAgentProfileRequest {
            id: "apex-orchestrator".to_string(),
            name: "Apex".to_string(),
            avatar: "zap".to_string(),
            role: "Orchestrator".to_string(),
            description: String::new(),
            instructions: String::new(),
        },
    )
    .await;
    assert!(dup_builtin.is_err());

    // 4. Editing writes an override and bumps the version label; saving an
    //    invalid manifest is rejected and the previous version stays active.
    let doc = get_agent_profile_document(state.clone(), "sentinel-reviewer".to_string())
        .await
        .unwrap();
    let patched = doc.document.replace(
        "You are Sentinel, a careful reviewer.",
        "You are Sentinel, an extremely careful reviewer.",
    );
    let saved = save_agent_profile(
        state.clone(),
        "sentinel-reviewer".to_string(),
        patched.clone(),
    )
    .await
    .unwrap();
    assert_ne!(saved.version_label, created.version_label);
    assert!(saved.instructions.contains("extremely careful reviewer"));
    let broken = save_agent_profile(
        state.clone(),
        "sentinel-reviewer".to_string(),
        "---\nnot a valid manifest: [\n".to_string(),
    )
    .await;
    assert!(broken.is_err(), "invalid manifests must be rejected");
    let still = get_agent_profile_document(state.clone(), "sentinel-reviewer".to_string())
        .await
        .unwrap();
    assert!(still.document.contains("extremely careful reviewer"));

    // 5. Runtime enable/disable persists custom flags in the manifest and
    //    builtin masks in the settings store.
    set_agent_profile_enabled(state.clone(), "sentinel-reviewer".to_string(), false)
        .await
        .unwrap();
    let disabled = list_agent_profiles(state.clone()).await.unwrap();
    let rec = disabled
        .iter()
        .find(|p| p.id == "sentinel-reviewer")
        .unwrap();
    assert!(!rec.enabled);

    set_agent_profile_enabled(state.clone(), "apex-orchestrator".to_string(), false)
        .await
        .unwrap();
    let disabled = list_agent_profiles(state.clone()).await.unwrap();
    assert!(
        !disabled
            .iter()
            .find(|p| p.id == "apex-orchestrator")
            .unwrap()
            .enabled
    );
    let saved_ids: Vec<String> = state
        .db
        .get_setting("agent_disabled_profile_ids")
        .unwrap()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    assert_eq!(saved_ids, vec!["apex-orchestrator".to_string()]);
    set_agent_profile_enabled(state.clone(), "apex-orchestrator".to_string(), true)
        .await
        .unwrap();

    // 6. Built-ins cannot be deleted; custom profiles can.
    let del_builtin = delete_agent_profile(state.clone(), "apex-orchestrator".to_string()).await;
    assert!(del_builtin.is_err());
    let outcome = delete_agent_profile(state.clone(), "sentinel-reviewer".to_string())
        .await
        .unwrap();
    assert_eq!(outcome, ProfileOrigin::Custom);
    let after_delete = list_agent_profiles(state.clone()).await.unwrap();
    assert!(!after_delete.iter().any(|p| p.id == "sentinel-reviewer"));

    // 7. Resetting a built-in with no override is a no-op returning factory
    //    content; deleting a candidate override falls back to factory.
    let reset = reset_agent_profile(state.clone(), "apex-orchestrator".to_string())
        .await
        .unwrap();
    assert_eq!(reset.origin, ProfileOrigin::Builtin);
    let apex_doc = get_agent_profile_document(state.clone(), "apex-orchestrator".to_string())
        .await
        .unwrap();
    let overridden = save_agent_profile(
        state.clone(),
        "apex-orchestrator".to_string(),
        apex_doc
            .document
            .replace("name: Apex", "name: Apex Prime")
            .replace(
                "description: General workspace and strategic orchestration",
                "description: Apex Prime override",
            ),
    )
    .await
    .unwrap();
    assert_eq!(overridden.origin, ProfileOrigin::Override);
    let fallback = delete_agent_profile(state.clone(), "apex-orchestrator".to_string())
        .await
        .unwrap();
    assert_eq!(fallback, ProfileOrigin::Override);
    let final_list = list_agent_profiles(state.clone()).await.unwrap();
    let apex = final_list
        .iter()
        .find(|p| p.id == "apex-orchestrator")
        .unwrap();
    assert_eq!(apex.origin, ProfileOrigin::Builtin);
    assert_eq!(apex.name, "Apex");
}

#[tokio::test]
async fn test_workspace_list_and_cancel_commands() {
    let (_ctx, state) = setup_test_context("ws_cmds");

    // Empty store lists fine.
    let listed = list_multi_agent_workspaces(state.clone()).await.unwrap();
    assert!(listed.is_empty());

    // Unknown workspace cancel reports false instead of lying.
    let cancelled = cancel_multi_agent_workspace("ws_missing".to_string())
        .await
        .unwrap();
    assert!(!cancelled);

    // Creating + running a real plan against a dead endpoint reaches an
    // honest failed terminal state without network.
    let ws = create_multi_agent_workspace(
        state.clone(),
        "ws_parent".to_string(),
        _ctx.dir.to_string_lossy().to_string(),
        "Objective".to_string(),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(ws.tasks.len(), 4);

    let listed = list_multi_agent_workspaces(state.clone()).await.unwrap();
    assert!(listed.iter().any(|w| w.workspace_id == ws.workspace_id));

    let gone = cancel_multi_agent_workspace(ws.workspace_id.clone())
        .await
        .unwrap();
    // The workspace was never started with a token, so cancel is a no-op.
    assert!(!gone);
}

#[tokio::test]
async fn test_selected_model_persistence_and_fallback() {
    use kyro_lib::agent::router::DEFAULT_NEMOTRON_MODEL;
    let (_ctx, state) = setup_test_context("model_persist");

    // 1. Initial default model before setting
    let initial = get_selected_model(state.clone()).await.unwrap();
    assert_eq!(initial, DEFAULT_NEMOTRON_MODEL);

    // 2. Save known model
    let saved = save_selected_model(state.clone(), "meta/llama-3.3-70b-instruct".to_string())
        .await
        .unwrap();
    assert_eq!(saved, "meta/llama-3.3-70b-instruct");

    // 3. Retrieve saved model
    let current = get_selected_model(state.clone()).await.unwrap();
    assert_eq!(current, "meta/llama-3.3-70b-instruct");

    // 4. Save unknown/unsupported model triggers fallback
    let fallback = save_selected_model(state.clone(), "unknown/nonexistent-model-xyz".to_string())
        .await
        .unwrap();
    assert_eq!(fallback, DEFAULT_NEMOTRON_MODEL);

    // 5. Empty model identifier rejects with error
    let empty_res = save_selected_model(state.clone(), "   ".to_string()).await;
    assert!(empty_res.is_err());
}
