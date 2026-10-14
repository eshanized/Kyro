// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use kyro_lib::{
    agent::AgentLoopEngine,
    commands::*,
    connector::ConnectorManager,
    mcp::McpManager,
    storage::{Database, Vault},
    teach_task::RoutineScheduler,
    telegram::TelegramBridge,
    AppState,
};
use std::sync::{Arc, Mutex};

fn main() {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("kyro");

    let db = Database::init(config_dir).expect("Failed to initialize Kyro SQLite database");
    let vault = Vault::new(db.clone());
    let browser = Arc::new(kyro_lib::browser::BrowserAutomationManager::new().with_database(db.clone()));
    let dev_servers = Arc::new(kyro_lib::project::DevServerManager::new());
    let engine = Arc::new(AgentLoopEngine::with_resources(
        browser.clone(),
        dev_servers.clone(),
    ));
    let mcp_manager = McpManager::new(db.clone(), vault.clone());
    let connector_manager = Arc::new(ConnectorManager::new(
        db.clone(),
        vault.clone(),
        mcp_manager.clone(),
    ));

    let state = AppState {
        db: db.clone(),
        vault: vault.clone(),
        engine: engine.clone(),
        mcp_manager: mcp_manager.clone(),
        connector_manager: connector_manager.clone(),
        pty_session: Mutex::new(None),
        browser: browser.clone(),
        dev_servers: dev_servers.clone(),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .manage(state)
        .setup(move |_app| {
            RoutineScheduler::start_worker(
                db.clone(),
                vault.clone(),
                engine.clone(),
                Some(mcp_manager.clone()),
            );
            TelegramBridge::start_worker(vault, db, engine, Some(mcp_manager));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_conversations,
            create_conversation,
            delete_conversation,
            get_messages,
            add_message,
            get_api_keys,
            save_api_keys,
            delete_secret_key,
            get_open_windows,
            take_screenshot,
            take_region_screenshot,
            perform_computer_action,
            abort_computer_use,
            check_command_permission,
            list_pending_approvals,
            resolve_approval,
            get_sandbox_status,
            execute_in_sandbox,
            spawn_terminal,
            write_terminal_input,
            resize_terminal,
            toggle_hud_window,
            toggle_overlay_window,
            start_teach_task,
            record_teach_step,
            stop_teach_task,
            list_skills,
            replay_skill,
            delete_skill,
            list_routines,
            create_routine,
            delete_routine,
            list_bots,
            create_bot,
            delete_bot,
            get_roster_statuses,
            get_takeover_status,
            request_takeover,
            resume_takeover,
            get_governance_policy,
            save_governance_policy,
            list_mcp_servers,
            create_mcp_server,
            delete_mcp_server,
            get_mcp_server_statuses,
            list_agent_runs,
            get_agent_run,
            get_run_events,
            cancel_agent_run,
            get_ghost_status,
            toggle_ghost_mode,
            capture_grounded_screen,
            get_timeline_steps,
            clear_timeline_steps,
            get_installed_apps,
            list_memories,
            save_memory_fact,
            delete_memory_fact,
            run_agent_turn,
            list_artifacts,
            get_artifact,
            create_artifact,
            delete_artifact,
            read_artifact_content,
            export_artifact,
            search_workspace,
            export_workspace_templates,
            import_workspace_templates,
            get_starter_templates,
            get_system_health,
            list_connector_catalog,
            list_connected_accounts,
            connect_account_token,
            start_oauth_flow,
            get_oauth_status,
            complete_oauth_flow,
            disconnect_account,
            test_connector_connection,
            get_connector_tools,
            get_account_tools,
            list_account_tool_policies,
            save_account_tool_policy,
            get_browser_capabilities,
            get_browser_settings,
            save_browser_settings,
            open_browser_session,
            close_browser_session,
            list_browser_sessions,
            discover_projects,
            list_dev_servers,
            get_dev_server,
            get_dev_server_logs,
            stop_dev_server
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kyro application");
}
