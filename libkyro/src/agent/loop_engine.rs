use super::cancellation::CancellationToken;
use super::router::{ChatMessage, MessageContent, ModelRouter, ToolCall, ToolDefinition};
use crate::agent::tools::{self, ToolOutcome};
use crate::browser::BrowserAutomationManager;
use crate::computer_use::{capture_screen, ComputerAction, InputController};
use crate::ghost::GhostDisplayManager;
use crate::governance::{AuthDecision, AuthorizationEngine, ExecutionContext, TakeoverController};
use crate::grounding::SetOfMarksEngine;
use crate::mcp::McpManager;
use crate::memory::MemoryStore;
use crate::multibot::MultiBotOrchestrator;
use crate::project::devserver::DevServerManager;
use crate::sandbox::DockerSandbox;
use crate::storage::{AgentRun, Database, RunEvent, TimelineStepRecord};
use crate::teach_task::{Skill, SkillReplayer};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Mutex as TokioMutex;
use tokio::time::{sleep, timeout};

/// The event emitted for each agent step.
///
/// The event carries the identity of the run and the conversation it belongs
/// to, so that the frontend can route it to the right timeline even when
/// several runs are active at the same time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStepEvent {
    /// The identifier of the run that produced the event.
    pub run_id: String,
    /// The conversation the run belongs to, when it is known.
    pub conversation_id: Option<String>,
    /// The agent that produced the event, when it is known.
    pub agent_id: Option<String>,
    /// The run this event was delegated from, for child-agent output.
    pub parent_run_id: Option<String>,
    /// `true` when the event was produced by a delegated child run.
    pub is_child_run: bool,
    pub step: usize,
    pub thought: String,
    pub tool_calls: Vec<ToolCall>,
    pub tool_results: Vec<ToolResult>,
    pub is_finished: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub name: String,
    pub output: String,
    pub is_error: bool,
}

#[derive(Clone)]
pub struct ActiveRunContext {
    pub token: CancellationToken,
    pub pids: Arc<std::sync::Mutex<Vec<u32>>>,
}

pub struct AgentLoopEngine {
    router: ModelRouter,
    active_runs: Arc<TokioMutex<HashMap<String, ActiveRunContext>>>,
    /// The managed browser automation subsystem.
    pub browser: Arc<BrowserAutomationManager>,
    /// The managed development-server subsystem.
    pub dev_servers: Arc<DevServerManager>,
}

impl Default for AgentLoopEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentLoopEngine {
    pub fn new() -> Self {
        Self::with_resources(
            Arc::new(BrowserAutomationManager::new()),
            Arc::new(DevServerManager::new()),
        )
    }

    /// Creates an engine that shares the browser and development-server
    /// managers with the rest of the application, so that the Tauri commands
    /// and the agent operate on the same resources.
    pub fn with_resources(
        browser: Arc<BrowserAutomationManager>,
        dev_servers: Arc<DevServerManager>,
    ) -> Self {
        Self {
            router: ModelRouter::new(),
            active_runs: Arc::new(TokioMutex::new(HashMap::new())),
            browser,
            dev_servers,
        }
    }

    /// Register a cancellation token for an active run
    pub async fn register_run_token(&self, run_id: &str, token: CancellationToken) {
        let mut map = self.active_runs.lock().await;
        map.insert(
            run_id.to_string(),
            ActiveRunContext {
                token,
                pids: Arc::new(std::sync::Mutex::new(Vec::new())),
            },
        );
    }

    /// Register a child process PID for process tree cleanup on cancellation
    pub async fn register_run_pid(&self, run_id: &str, pid: u32) {
        let map = self.active_runs.lock().await;
        if let Some(ctx) = map.get(run_id) {
            let mut pids = ctx.pids.lock().unwrap();
            pids.push(pid);
        }
    }

    /// Unregister a finished child process PID
    pub async fn unregister_run_pid(&self, run_id: &str, pid: u32) {
        let map = self.active_runs.lock().await;
        if let Some(ctx) = map.get(run_id) {
            let mut pids = ctx.pids.lock().unwrap();
            pids.retain(|&p| p != pid);
        }
    }

    /// Cancel a specific run by its run_id and terminate its active process tree
    pub async fn cancel_run(&self, run_id: &str) -> bool {
        let mut map = self.active_runs.lock().await;
        if let Some(ctx) = map.remove(run_id) {
            ctx.token.cancel();
            let pids = {
                let lock = ctx.pids.lock().unwrap();
                lock.clone()
            };
            for pid in pids {
                #[cfg(unix)]
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
            true
        } else {
            false
        }
    }

    pub fn standard_tools() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "execute_shell".to_string(),
                description: "Execute a shell command on the host terminal or in an isolated sandbox.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "The shell command line string to run" },
                        "use_sandbox": { "type": "boolean", "description": "Whether to run in an ephemeral container sandbox" }
                    },
                    "required": ["command"]
                }),
            },
            ToolDefinition {
                name: "read_file".to_string(),
                description: "Read the full contents of a file on the local filesystem.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Absolute or relative path to file" }
                    },
                    "required": ["path"]
                }),
            },
            ToolDefinition {
                name: "write_file".to_string(),
                description: "Create or overwrite a file on the local filesystem with specified contents.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Absolute or relative path to file" },
                        "content": { "type": "string", "description": "The full text content to write" }
                    },
                    "required": ["path", "content"]
                }),
            },
            ToolDefinition {
                name: "create_artifact".to_string(),
                description: "Persist a named artifact file (code, markdown document, structured data, or report) in the local artifact registry.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "The human-readable filename (e.g. 'summary.md', 'script.py', 'chart.json')" },
                        "content": { "type": "string", "description": "The full text content of the artifact" },
                        "artifact_type": {
                            "type": "string",
                            "enum": ["code", "markdown", "image", "json", "log", "document"],
                            "description": "Category of artifact"
                        }
                    },
                    "required": ["name", "content"]
                }),
            },
            ToolDefinition {
                name: "capture_screen".to_string(),
                description: "Capture the current desktop display to inspect visual state.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "monitor_index": { "type": "integer", "description": "Index of monitor to capture (default 0)" }
                    }
                }),
            },
            ToolDefinition {
                name: "capture_grounded_screen".to_string(),
                description: "Capture screen with Set-of-Marks visual tags ([1], [2]...) overlaid on interactive elements.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolDefinition {
                name: "click_mark".to_string(),
                description: "Click a numbered UI tag directly from a grounded screen capture.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "mark_id": { "type": "integer", "description": "The numeric ID of the mark to click" }
                    },
                    "required": ["mark_id"]
                }),
            },
            ToolDefinition {
                name: "computer_action".to_string(),
                description: "Execute a mouse or keyboard action on the desktop.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["mouse_move", "left_click", "right_click", "double_click", "type", "key", "wait"],
                            "description": "The Computer Use action type"
                        },
                        "x": { "type": "integer", "description": "X coordinate on screen" },
                        "y": { "type": "integer", "description": "Y coordinate on screen" },
                        "text": { "type": "string", "description": "Text to type or key name to press" }
                    },
                    "required": ["action"]
                }),
            },
            ToolDefinition {
                name: "launch_ghost_app".to_string(),
                description: "Launch an application inside the isolated virtual Ghost Display (:99). \
The program is started as an explicit executable plus argument vector; free-form shell commands are rejected."
                    .to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "executable": { "type": "string", "description": "Program name or absolute path of the application to launch (e.g. 'firefox' or '/usr/bin/firefox')" },
                        "args": { "type": "array", "items": { "type": "string" }, "description": "Argument vector passed verbatim to the executable" }
                    },
                    "required": ["executable"]
                }),
            },
            ToolDefinition {
                name: "invoke_skill".to_string(),
                description: "Replay a pre-recorded 'Teach-a-Task' skill workflow by its unique skill ID.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "skill_id": { "type": "string", "description": "The unique ID of the saved skill to replay" }
                    },
                    "required": ["skill_id"]
                }),
            },
            ToolDefinition {
                name: "delegate_task".to_string(),
                description: "Delegate a sub-objective to a specialized teammate agent (e.g. @Forge for DevOps/Shell, @Scout for Web).".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "target_bot_id": { "type": "string", "description": "The teammate bot ID to delegate to (e.g. 'forge-devops', 'scout-web')" },
                        "objective": { "type": "string", "description": "The specific task objective for the teammate" },
                        "context_summary": { "type": "string", "description": "Relevant context and constraints for the teammate" }
                    },
                    "required": ["target_bot_id", "objective"]
                }),
            },
        ]
        .into_iter()
        .chain(tools::tool_definitions())
        .collect()
    }

    /// Execute an autonomous agent run step with cancellation, centralized authorization, and timeline persistence
    #[allow(clippy::too_many_arguments)]
    pub async fn run_step<F>(
        &self,
        run_id: &str,
        model: &str,
        api_key: &str,
        base_url: Option<&str>,
        system_prompt: &str,
        history: Vec<ChatMessage>,
        db: Database,
        mcp_manager: Option<McpManager>,
        cancel_token: Option<CancellationToken>,
        on_event: F,
    ) -> Result<String, String>
    where
        F: FnMut(AgentStepEvent) + Send + 'static,
    {
        self.run_step_with_connectors(
            run_id,
            model,
            api_key,
            base_url,
            system_prompt,
            history,
            db,
            mcp_manager,
            None,
            cancel_token,
            on_event,
        )
        .await
    }

    /// Execute an autonomous agent run step with both connector manager and local MCP support
    #[allow(clippy::too_many_arguments)]
    pub async fn run_step_with_connectors<F>(
        &self,
        run_id: &str,
        model: &str,
        api_key: &str,
        base_url: Option<&str>,
        system_prompt: &str,
        mut history: Vec<ChatMessage>,
        db: Database,
        mcp_manager: Option<McpManager>,
        connector_manager: Option<Arc<crate::connector::ConnectorManager>>,
        cancel_token: Option<CancellationToken>,
        mut on_event: F,
    ) -> Result<String, String>
    where
        F: FnMut(AgentStepEvent) + Send + 'static,
    {
        let token = cancel_token.unwrap_or_default();
        self.register_run_token(run_id, token.clone()).await;

        let mut tools = Self::standard_tools();
        if let Some(ref conn_mgr) = connector_manager {
            let conn_tools = conn_mgr.get_all_connector_tools().await;
            tools.extend(conn_tools);
        } else if let Some(ref mcp) = mcp_manager {
            let external_tools = mcp.get_all_mcp_tools().await;
            tools.extend(external_tools);
        }

        // Determine active agent ID from run or default to apex
        let (active_agent_id, run_conversation_id, run_parent_id) =
            if let Ok(Some(r)) = db.get_run(run_id) {
                (r.agent_id, r.conversation_id, r.parent_run_id)
            } else {
                ("apex-orchestrator".to_string(), None, None)
            };
        let is_child_run = run_parent_id.is_some();

        // Extract last user prompt for relevance matching
        let last_prompt = history
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.as_text_lossy())
            .unwrap_or_default();

        let memory_context = MemoryStore::build_context_block(&db, &active_agent_id, &last_prompt);
        let enriched_prompt = format!("{}{}", system_prompt, memory_context);

        let max_steps = 25;
        let mut final_content = String::new();

        for step in 1..=max_steps {
            if token.is_cancelled() {
                let _ = db.update_run_status(
                    run_id,
                    "cancelled",
                    None,
                    Some(chrono::Utc::now().timestamp_millis()),
                    Some("Run cancelled by user"),
                );
                return Err("Agent run cancelled by user".to_string());
            }

            // Wait if human takeover is active
            while TakeoverController::is_active() {
                if token.is_cancelled() {
                    return Err("Agent run cancelled during human takeover".to_string());
                }
                sleep(Duration::from_millis(400)).await;
            }

            let resp = self
                .router
                .complete(
                    model,
                    api_key,
                    base_url,
                    &enriched_prompt,
                    &history,
                    &tools,
                    Some(&token),
                )
                .await?;

            // Record thought in event timeline
            let seq = db.get_next_event_seq(run_id).unwrap_or(1);
            let thought_event = RunEvent {
                id: format!("evt_{}_{}", run_id, seq),
                run_id: run_id.to_string(),
                seq,
                event_type: "thought".to_string(),
                payload_json: serde_json::json!({
                    "step": step,
                    "thought": resp.content,
                    "tool_calls_count": resp.tool_calls.len(),
                })
                .to_string(),
                created_at: chrono::Utc::now().timestamp_millis(),
            };
            let _ = db.append_run_event(&thought_event);

            if resp.tool_calls.is_empty() {
                final_content = resp.content.clone();
                on_event(AgentStepEvent {
                    run_id: run_id.to_string(),
                    conversation_id: run_conversation_id.clone(),
                    agent_id: Some(active_agent_id.clone()),
                    parent_run_id: run_parent_id.clone(),
                    is_child_run,
                    step,
                    thought: resp.content,
                    tool_calls: vec![],
                    tool_results: vec![],
                    is_finished: true,
                });
                break;
            }

            // Check if model thought detects sensitive credential checkpoints
            if let Some(reason) = TakeoverController::detect_sensitive_checkpoint(&resp.content) {
                TakeoverController::request(&reason);
                while TakeoverController::is_active() {
                    if token.is_cancelled() {
                        return Err("Run cancelled during credential takeover".to_string());
                    }
                    sleep(Duration::from_millis(400)).await;
                }
            }

            // Append assistant response to history
            history.push(ChatMessage {
                role: "assistant".to_string(),
                content: MessageContent::from_text(&resp.content),
                tool_calls: Some(resp.tool_calls.clone()),
                tool_call_id: None,
            });

            let mut tool_results = Vec::new();

            for tc in &resp.tool_calls {
                if token.is_cancelled() {
                    let _ = db.update_run_status(
                        run_id,
                        "cancelled",
                        None,
                        Some(chrono::Utc::now().timestamp_millis()),
                        Some("Run cancelled"),
                    );
                    return Err("Agent run cancelled during tool execution".to_string());
                }

                let (output, is_error, visual_data) = self
                    .execute_tool_governed(
                        run_id,
                        &active_agent_id,
                        tc,
                        &db,
                        connector_manager.as_ref(),
                        mcp_manager.as_ref(),
                        &token,
                    )
                    .await;

                tool_results.push(ToolResult {
                    tool_call_id: tc.id.clone(),
                    name: tc.name.clone(),
                    output: output.clone(),
                    is_error,
                });

                // Persist tool execution event
                let tool_seq = db.get_next_event_seq(run_id).unwrap_or(seq + 1);
                let tool_event = RunEvent {
                    id: format!("evt_{}_{}", run_id, tool_seq),
                    run_id: run_id.to_string(),
                    seq: tool_seq,
                    event_type: "tool_result".to_string(),
                    payload_json: serde_json::json!({
                        "tool_call_id": tc.id,
                        "name": tc.name,
                        "arguments": tc.arguments,
                        "output": output,
                        "is_error": is_error,
                    })
                    .to_string(),
                    created_at: chrono::Utc::now().timestamp_millis(),
                };
                let _ = db.append_run_event(&tool_event);

                // If vision model and visual screenshot data is available, attach multimodal message
                let message_content = if ModelRouter::is_vision_model(model) {
                    if let Some(ref b64) = visual_data {
                        MessageContent::from_text_and_image(&output, b64)
                    } else {
                        MessageContent::from_text(&output)
                    }
                } else {
                    MessageContent::from_text(&output)
                };

                history.push(ChatMessage {
                    role: "tool".to_string(),
                    content: message_content,
                    tool_calls: None,
                    tool_call_id: Some(tc.id.clone()),
                });
            }

            on_event(AgentStepEvent {
                run_id: run_id.to_string(),
                conversation_id: run_conversation_id.clone(),
                agent_id: Some(active_agent_id.clone()),
                parent_run_id: run_parent_id.clone(),
                is_child_run,
                step,
                thought: resp.content,
                tool_calls: resp.tool_calls,
                tool_results,
                is_finished: false,
            });
        }

        // Clean up active run context
        {
            let mut map = self.active_runs.lock().await;
            map.remove(run_id);
        }

        Ok(final_content)
    }

    /// Execute a tool call strictly through the centralized authorization engine
    #[allow(clippy::too_many_arguments)]
    async fn execute_tool_governed(
        &self,
        run_id: &str,
        agent_id: &str,
        tc: &ToolCall,
        db: &Database,
        connector_manager: Option<&Arc<crate::connector::ConnectorManager>>,
        mcp_manager: Option<&McpManager>,
        token: &CancellationToken,
    ) -> (String, bool, Option<String>) {
        let workspace_root = if let Ok(Some(r)) = db.get_run(run_id) {
            r.config_json.as_ref().and_then(|cfg| {
                serde_json::from_str::<serde_json::Value>(cfg)
                    .ok()
                    .and_then(|v| {
                        v.get("workspace_root")
                            .and_then(|s| s.as_str())
                            .map(PathBuf::from)
                    })
            })
        } else {
            None
        }
        .or_else(|| {
            db.get_setting("workspace_root")
                .ok()
                .flatten()
                .map(PathBuf::from)
        })
        .or_else(|| dirs::data_dir().map(|d| d.join("kyro").join("workspace")));

        if let Some(ref root) = workspace_root {
            let _ = std::fs::create_dir_all(root);
        }

        let ctx = ExecutionContext {
            run_id: run_id.to_string(),
            agent_id: agent_id.to_string(),
            conversation_id: db
                .get_run(run_id)
                .ok()
                .flatten()
                .and_then(|r| r.conversation_id),
            workspace_root: workspace_root.clone(),
        };

        // The shell analysis is computed once, before authorisation, so that the
        // exact argument vector that was inspected is also the argument vector
        // that is executed.  Authorisation and execution can therefore never
        // disagree about what runs.
        let shell_analysis = if matches!(tc.name.as_str(), "execute_shell" | "bash" | "terminal") {
            let policy = crate::governance::PolicyEngine::load(db);
            Some(AuthorizationEngine::analyze_shell_command(
                &policy,
                workspace_root.as_deref(),
                &tc.arguments,
            ))
        } else {
            None
        };

        // 1. Evaluate centralized policy decision
        let decision = if let Some(conn_mgr) = connector_manager {
            if let Some(dec) = conn_mgr
                .evaluate_tool_authorization(&tc.name, &tc.arguments)
                .await
            {
                dec
            } else if let Some(sa) = &shell_analysis {
                sa.decision.clone()
            } else {
                AuthorizationEngine::evaluate(db, &ctx, &tc.name, &tc.arguments)
            }
        } else if let Some(sa) = &shell_analysis {
            sa.decision.clone()
        } else {
            AuthorizationEngine::evaluate(db, &ctx, &tc.name, &tc.arguments)
        };

        let authorized_argv: Option<Vec<String>> =
            shell_analysis.as_ref().and_then(|s| s.argv.clone());

        match decision {
            AuthDecision::Deny { reason } => {
                log::warn!("[Governance] Tool '{}' DENIED: {}", tc.name, reason);
                (
                    format!("Security policy denied action: {}", reason),
                    true,
                    None,
                )
            }
            AuthDecision::RequireTakeover { reason } => {
                log::info!(
                    "[Governance] Tool '{}' triggered takeover: {}",
                    tc.name,
                    reason
                );
                TakeoverController::request(&reason);
                while TakeoverController::is_active() {
                    if token.is_cancelled() {
                        return (
                            "Action cancelled during human takeover".to_string(),
                            true,
                            None,
                        );
                    }
                    sleep(Duration::from_millis(400)).await;
                }
                self.execute_tool_core(
                    run_id,
                    tc,
                    false,
                    None,
                    workspace_root.as_deref(),
                    db,
                    connector_manager,
                    mcp_manager,
                    token,
                    None,
                    authorized_argv.as_deref(),
                )
                .await
            }
            AuthDecision::RequireApproval {
                reason,
                risk,
                summary,
            } => {
                log::info!(
                    "[Governance] Tool '{}' requires approval: {}",
                    tc.name,
                    reason
                );
                let app_req = match AuthorizationEngine::request_approval(
                    db,
                    run_id,
                    &tc.name,
                    &summary,
                    &tc.arguments,
                    risk,
                    &reason,
                ) {
                    Ok(r) => r,
                    Err(e) => {
                        return (
                            format!("Failed to queue approval request: {}", e),
                            true,
                            None,
                        )
                    }
                };

                // Persist run state as waiting_approval
                let _ = db.update_run_status(run_id, "waiting_approval", None, None, None);
                let wait_seq = db.get_next_event_seq(run_id).unwrap_or(1);
                let _ = db.append_run_event(&RunEvent {
                    id: format!("evt_{}_{}", run_id, wait_seq),
                    run_id: run_id.to_string(),
                    seq: wait_seq,
                    event_type: "waiting_approval".to_string(),
                    payload_json: serde_json::json!({
                        "approval_id": app_req.id,
                        "tool_name": tc.name,
                        "reason": reason,
                    })
                    .to_string(),
                    created_at: chrono::Utc::now().timestamp_millis(),
                });

                // Wait for user approval in UI or expiration/cancellation
                let req_id = app_req.id;

                loop {
                    let now = chrono::Utc::now().timestamp_millis();
                    if token.is_cancelled() {
                        let _ =
                            db.resolve_approval_request(&req_id, "cancelled", "user_cancellation");
                        let _ = db.update_run_status(
                            run_id,
                            "cancelled",
                            None,
                            Some(now),
                            Some("Cancelled while waiting for approval"),
                        );
                        return (
                            "Action cancelled while awaiting approval".to_string(),
                            true,
                            None,
                        );
                    }

                    if let Ok(Some(status)) = db.get_approval_request(&req_id) {
                        if status.status == "approved" {
                            let _ = db.update_run_status(run_id, "running", None, None, None);
                            let proof = crate::connector::InvocationAuthProof {
                                approval_id: req_id.clone(),
                                run_id: run_id.to_string(),
                                tool_name: tc.name.clone(),
                                args_hash: crate::connector::InvocationAuthProof::compute_args_hash(
                                    &tc.arguments,
                                ),
                                timestamp: now,
                            };
                            return self
                                .execute_tool_core(
                                    run_id,
                                    tc,
                                    false,
                                    None,
                                    workspace_root.as_deref(),
                                    db,
                                    connector_manager,
                                    mcp_manager,
                                    token,
                                    Some(&proof),
                                    authorized_argv.as_deref(),
                                )
                                .await;
                        } else if status.status == "denied" {
                            let _ = db.update_run_status(run_id, "running", None, None, None);
                            return (
                                format!("Action '{}' was denied by user", tc.name),
                                true,
                                None,
                            );
                        } else if status.status == "expired" || status.expires_at <= now {
                            let _ = db.expire_overdue_approvals(now);
                            let _ = db.update_run_status(run_id, "running", None, None, None);
                            return (
                                format!("Approval request for '{}' expired", tc.name),
                                true,
                                None,
                            );
                        } else if status.status == "cancelled" {
                            let _ = db.update_run_status(run_id, "running", None, None, None);
                            return (
                                format!("Approval request for '{}' was cancelled", tc.name),
                                true,
                                None,
                            );
                        }
                    }

                    sleep(Duration::from_millis(300)).await;
                }
            }
            AuthDecision::AllowSandboxOnly { image } => {
                self.execute_tool_core(
                    run_id,
                    tc,
                    true,
                    Some(image),
                    workspace_root.as_deref(),
                    db,
                    connector_manager,
                    mcp_manager,
                    token,
                    None,
                    authorized_argv.as_deref(),
                )
                .await
            }
            AuthDecision::Allow => {
                self.execute_tool_core(
                    run_id,
                    tc,
                    false,
                    None,
                    workspace_root.as_deref(),
                    db,
                    connector_manager,
                    mcp_manager,
                    token,
                    None,
                    authorized_argv.as_deref(),
                )
                .await
            }
        }
    }

    /// Core tool dispatch after security authorization has succeeded
    #[allow(clippy::too_many_arguments)]
    async fn execute_tool_core(
        &self,
        run_id: &str,
        tc: &ToolCall,
        force_sandbox: bool,
        sandbox_image: Option<String>,
        workspace_root: Option<&Path>,
        db: &Database,
        connector_manager: Option<&Arc<crate::connector::ConnectorManager>>,
        mcp_manager: Option<&McpManager>,
        token: &CancellationToken,
        auth_proof: Option<&crate::connector::InvocationAuthProof>,
        authorized_argv: Option<&[String]>,
    ) -> (String, bool, Option<String>) {
        match tc.name.as_str() {
            "execute_shell" => {
                let cmd = tc.arguments["command"].as_str().unwrap_or_default();
                let use_sandbox =
                    force_sandbox || tc.arguments["use_sandbox"].as_bool().unwrap_or(false);

                if use_sandbox {
                    let image = sandbox_image.unwrap_or_else(|| "alpine:latest".to_string());
                    match DockerSandbox::run_in_sandbox(&image, cmd) {
                        Ok(res) => (
                            format!(
                                "Sandbox ({}) exit code {}:\nSTDOUT:\n{}\nSTDERR:\n{}",
                                image, res.exit_code, res.stdout, res.stderr
                            ),
                            res.exit_code != 0,
                            None,
                        ),
                        Err(e) => (format!("Sandbox execution failed: {}", e), true, None),
                    }
                } else {
                    // Execute the *authorised* argument vector directly whenever
                    // the command was classified as simple.  Only commands that
                    // genuinely require shell semantics (and therefore went
                    // through the approval flow) are handed to `sh -c`.
                    let (program, args): (String, Vec<String>) = match authorized_argv {
                        Some(argv) if !argv.is_empty() => (argv[0].clone(), argv[1..].to_vec()),
                        _ => ("sh".to_string(), vec!["-c".to_string(), cmd.to_string()]),
                    };

                    let mut cmd_builder = Command::new(&program);
                    cmd_builder
                        .args(&args)
                        .stdin(Stdio::null())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .kill_on_drop(true);

                    #[cfg(unix)]
                    {
                        cmd_builder.process_group(0);
                    }
                    cmd_builder.kill_on_drop(true);

                    let child = match cmd_builder.spawn() {
                        Ok(c) => c,
                        Err(e) => {
                            return (
                                format!("Failed to spawn process '{}': {}", program, e),
                                true,
                                None,
                            )
                        }
                    };

                    let pid = child.id();
                    if let Some(p) = pid {
                        self.register_run_pid(run_id, p).await;
                    }

                    let wait_fut = child.wait_with_output();
                    tokio::pin!(wait_fut);

                    let wait_res = tokio::select! {
                        _ = token.cancelled() => {
                            if let Some(p) = pid {
                                #[cfg(unix)]
                                unsafe {
                                    libc::kill(-(p as i32), libc::SIGKILL);
                                    libc::kill(p as i32, libc::SIGKILL);
                                }
                            }
                            if let Some(p) = pid {
                                self.unregister_run_pid(run_id, p).await;
                            }
                            return ("Shell command execution cancelled by user".to_string(), true, None);
                        }
                        out = timeout(Duration::from_secs(60), &mut wait_fut) => {
                            if let Some(p) = pid {
                                self.unregister_run_pid(run_id, p).await;
                            }
                            out
                        }
                    };

                    match wait_res {
                        Ok(Ok(out)) => {
                            let mut stdout = String::from_utf8_lossy(&out.stdout).to_string();
                            let mut stderr = String::from_utf8_lossy(&out.stderr).to_string();

                            const MAX_BYTES: usize = 256 * 1024;
                            if stdout.len() > MAX_BYTES {
                                stdout.truncate(MAX_BYTES);
                                stdout.push_str("\n...[STDOUT truncated at 256KB]");
                            }
                            if stderr.len() > MAX_BYTES {
                                stderr.truncate(MAX_BYTES);
                                stderr.push_str("\n...[STDERR truncated at 256KB]");
                            }

                            (
                                format!(
                                    "EXIT_CODE: {}\nCOMMAND: {}\nSTDOUT:\n{}\nSTDERR:\n{}",
                                    out.status.code().unwrap_or(-1),
                                    program,
                                    stdout,
                                    stderr
                                ),
                                !out.status.success(),
                                None,
                            )
                        }
                        Ok(Err(e)) => (format!("Subprocess execution error: {}", e), true, None),
                        Err(_) => (
                            "Shell command timed out after 60 seconds and was terminated"
                                .to_string(),
                            true,
                            None,
                        ),
                    }
                }
            }
            "read_file" => {
                let path = tc.arguments["path"].as_str().unwrap_or_default();
                let safe_path = match AuthorizationEngine::validate_path_confinement(
                    workspace_root,
                    path,
                    crate::governance::FileOp::Read,
                ) {
                    Ok(p) => p,
                    Err(e) => return (format!("Filesystem access denied: {}", e), true, None),
                };

                match std::fs::read_to_string(&safe_path) {
                    Ok(mut content) => {
                        const MAX_READ_BYTES: usize = 512 * 1024;
                        if content.len() > MAX_READ_BYTES {
                            content.truncate(MAX_READ_BYTES);
                            content.push_str("\n...[Content truncated at 512KB limit]");
                        }
                        (content, false, None)
                    }
                    Err(e) => (format!("Failed to read file '{}': {}", path, e), true, None),
                }
            }
            "write_file" => {
                let path = tc.arguments["path"].as_str().unwrap_or_default();
                let content = tc.arguments["content"].as_str().unwrap_or_default();
                let safe_path = match AuthorizationEngine::validate_path_confinement(
                    workspace_root,
                    path,
                    crate::governance::FileOp::Write,
                ) {
                    Ok(p) => p,
                    Err(e) => return (format!("Filesystem access denied: {}", e), true, None),
                };

                if let Some(parent) = safe_path.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                match std::fs::write(&safe_path, content) {
                    Ok(_) => {
                        // Register write_file into artifacts registry
                        let file_name = safe_path
                            .file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_else(|| "file.txt".to_string());
                        let conv_id = db
                            .get_run(run_id)
                            .ok()
                            .flatten()
                            .and_then(|r| r.conversation_id);
                        let ext = safe_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                        let art_type = match ext {
                            "rs" | "ts" | "tsx" | "js" | "py" | "sh" | "c" | "cpp" | "go" => "code",
                            "md" | "txt" => "markdown",
                            "json" => "json",
                            "png" | "jpg" | "jpeg" => "image",
                            _ => "document",
                        };
                        let rec = crate::storage::ArtifactRecord {
                            id: format!("art_{}", uuid::Uuid::new_v4()),
                            run_id: Some(run_id.to_string()),
                            conversation_id: conv_id,
                            name: file_name,
                            file_path: safe_path.to_string_lossy().to_string(),
                            artifact_type: art_type.to_string(),
                            mime_type: "text/plain".to_string(),
                            size_bytes: content.len() as i64,
                            created_at: chrono::Utc::now().timestamp_millis(),
                            metadata_json: None,
                        };
                        let _ = db.create_artifact(&rec);

                        (
                            format!(
                                "Successfully wrote {} bytes to '{}'",
                                content.len(),
                                safe_path.display()
                            ),
                            false,
                            None,
                        )
                    }
                    Err(e) => (
                        format!("Failed to write file '{}': {}", safe_path.display(), e),
                        true,
                        None,
                    ),
                }
            }
            "create_artifact" => {
                let name = tc.arguments["name"].as_str().unwrap_or_default();
                let content = tc.arguments["content"].as_str().unwrap_or_default();
                let artifact_type = tc.arguments["artifact_type"].as_str().unwrap_or("document");

                if name.contains("..")
                    || name.contains('/')
                    || name.contains('\\')
                    || name.contains('\0')
                {
                    return (
                        "Artifact name cannot contain directory traversal, slash, or null characters"
                            .into(),
                        true,
                        None,
                    );
                }

                let artifact_id = format!("art_{}", uuid::Uuid::new_v4());
                let artifacts_dir = dirs::config_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("kyro")
                    .join("artifacts");

                if let Err(e) = std::fs::create_dir_all(&artifacts_dir) {
                    return (
                        format!("Failed to create artifacts directory: {}", e),
                        true,
                        None,
                    );
                }

                let safe_filename = format!("{}_{}", &artifact_id[4..12], name);
                let file_path = artifacts_dir.join(safe_filename);

                match std::fs::write(&file_path, content) {
                    Ok(_) => {
                        let path_str = file_path.to_string_lossy().to_string();
                        let size_bytes = content.len() as i64;
                        let now = chrono::Utc::now().timestamp_millis();
                        let conv_id = db
                            .get_run(run_id)
                            .ok()
                            .flatten()
                            .and_then(|r| r.conversation_id);

                        let mime = match artifact_type {
                            "code" => "text/plain",
                            "markdown" => "text/markdown",
                            "json" => "application/json",
                            "image" => "image/png",
                            _ => "text/plain",
                        };

                        let rec = crate::storage::ArtifactRecord {
                            id: artifact_id.clone(),
                            run_id: Some(run_id.to_string()),
                            conversation_id: conv_id,
                            name: name.to_string(),
                            file_path: path_str.clone(),
                            artifact_type: artifact_type.to_string(),
                            mime_type: mime.to_string(),
                            size_bytes,
                            created_at: now,
                            metadata_json: Some(
                                serde_json::json!({
                                    "preview": content.chars().take(200).collect::<String>()
                                })
                                .to_string(),
                            ),
                        };
                        let _ = db.create_artifact(&rec);

                        (
                            format!(
                                "Created artifact '{}' (ID: {}, {} bytes) at '{}'",
                                name, artifact_id, size_bytes, path_str
                            ),
                            false,
                            None,
                        )
                    }
                    Err(e) => (format!("Failed to write artifact file: {}", e), true, None),
                }
            }
            "capture_screen" => {
                let idx = tc.arguments["monitor_index"].as_u64().unwrap_or(0) as usize;
                match capture_screen(idx) {
                    Ok(sc) => (
                        format!(
                            "Captured screen monitor {}: {}x{} px",
                            idx, sc.width, sc.height
                        ),
                        false,
                        Some(sc.base64_png),
                    ),
                    Err(e) => (format!("Screen capture error: {}", e), true, None),
                }
            }
            "capture_grounded_screen" => {
                let monitors = xcap::Monitor::all().unwrap_or_default();
                if let Some(m) = monitors.first() {
                    if let Ok(img) = m.capture_image() {
                        let grounded = SetOfMarksEngine::ground_image(img);
                        let marks_summary = grounded
                            .marks
                            .iter()
                            .take(30)
                            .map(|m| format!("#{}: ({}, {})", m.id, m.x, m.y))
                            .collect::<Vec<_>>()
                            .join(", ");

                        (
                            format!(
                                "Screen captured with Set-of-Marks tags (Total: {} marks).\nActive Marks: {}",
                                grounded.marks.len(),
                                marks_summary
                            ),
                            false,
                            Some(grounded.base64_png),
                        )
                    } else {
                        (
                            "Failed to capture monitor buffer for grounding".into(),
                            true,
                            None,
                        )
                    }
                } else {
                    ("No active display monitors found".into(), true, None)
                }
            }
            "click_mark" => {
                let mark_id = tc.arguments["mark_id"].as_u64().unwrap_or(0) as usize;
                if let Some((x, y)) = SetOfMarksEngine::resolve_mark(mark_id) {
                    let before_shot = capture_screen(0).ok();
                    let before_png = before_shot
                        .as_ref()
                        .map(|s| s.base64_png.clone())
                        .unwrap_or_default();

                    if GhostDisplayManager::is_enabled() {
                        let _ = GhostDisplayManager::click(x, y);
                    } else {
                        let mut ctrl = match InputController::new() {
                            Ok(c) => c,
                            Err(e) => {
                                return (
                                    format!("Failed to init input controller: {}", e),
                                    true,
                                    None,
                                )
                            }
                        };
                        let act = ComputerAction::LeftClick {
                            x: Some(x),
                            y: Some(y),
                        };
                        if let Err(e) = ctrl.execute(&act) {
                            return (format!("Failed to click mark: {}", e), true, None);
                        }
                    }

                    // Post-action verification frame
                    sleep(Duration::from_millis(150)).await;
                    let after_shot = capture_screen(0).ok();
                    let after_png = after_shot
                        .as_ref()
                        .map(|s| s.base64_png.clone())
                        .unwrap_or_default();

                    // Calculate real image diff percentage
                    let diff_percent = if let (Some(ref b_shot), Some(ref a_shot)) =
                        (&before_shot, &after_shot)
                    {
                        use base64::Engine;
                        if let (Ok(b_bytes), Ok(a_bytes)) = (
                            base64::engine::general_purpose::STANDARD.decode(&b_shot.base64_png),
                            base64::engine::general_purpose::STANDARD.decode(&a_shot.base64_png),
                        ) {
                            if let (Ok(b_img), Ok(a_img)) = (
                                image::load_from_memory(&b_bytes),
                                image::load_from_memory(&a_bytes),
                            ) {
                                crate::timeline::TimelineEngine::compute_diff_percent(
                                    &b_img.to_rgba8(),
                                    &a_img.to_rgba8(),
                                )
                            } else {
                                0.0
                            }
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    };

                    // Record in timeline persistence
                    let timeline_rec = TimelineStepRecord {
                        id: format!("tl_{}_{}", run_id, mark_id),
                        run_id: Some(run_id.to_string()),
                        step: mark_id,
                        action_name: format!("click_mark #{}", mark_id),
                        click_x: Some(x),
                        click_y: Some(y),
                        before_base64: before_png,
                        after_base64: after_png.clone(),
                        diff_percent,
                        timestamp: chrono::Utc::now().timestamp_millis(),
                    };
                    let _ = db.save_timeline_step(&timeline_rec);

                    (
                        format!(
                            "Successfully clicked Mark #{} at coordinate ({}, {}) (Visual state change: {:.1}%)",
                            mark_id, x, y, diff_percent
                        ),
                        false,
                        Some(after_png),
                    )
                } else {
                    (
                        format!("Mark #{} not found in active screen session. Call capture_grounded_screen to generate fresh marks.", mark_id),
                        true,
                        None,
                    )
                }
            }
            "computer_action" => {
                let action_str = tc.arguments["action"].as_str().unwrap_or_default();
                let x = tc.arguments["x"].as_i64().map(|v| v as i32);
                let y = tc.arguments["y"].as_i64().map(|v| v as i32);
                let text = tc.arguments["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();

                // The action vocabulary is closed: an unknown action is an
                // explicit failure, never a generic success.
                const SUPPORTED: &[&str] = &[
                    "mouse_move",
                    "left_click",
                    "right_click",
                    "double_click",
                    "type",
                    "key",
                    "wait",
                ];
                if !SUPPORTED.contains(&action_str) {
                    return (
                        format!(
                            "Unsupported computer action '{}'. Supported actions: {}",
                            action_str,
                            SUPPORTED.join(", ")
                        ),
                        true,
                        None,
                    );
                }

                if GhostDisplayManager::is_enabled() {
                    let outcome = match action_str {
                        "mouse_move" => {
                            let (cx, cy) = match (x, y) {
                                (Some(cx), Some(cy)) => (cx, cy),
                                _ => return (
                                    "Ghost Mode mouse_move requires both 'x' and 'y' coordinates"
                                        .to_string(),
                                    true,
                                    None,
                                ),
                            };
                            GhostDisplayManager::move_mouse(cx, cy)
                                .map(|_| "moved the pointer".to_string())
                        }
                        "left_click" => {
                            let (cx, cy) = match (x, y) {
                                (Some(cx), Some(cy)) => (cx, cy),
                                _ => return (
                                    "Ghost Mode left_click requires both 'x' and 'y' coordinates"
                                        .to_string(),
                                    true,
                                    None,
                                ),
                            };
                            GhostDisplayManager::click(cx, cy).map(|_| "left-clicked".to_string())
                        }
                        "right_click" => {
                            let (cx, cy) = match (x, y) {
                                (Some(cx), Some(cy)) => (cx, cy),
                                _ => return (
                                    "Ghost Mode right_click requires both 'x' and 'y' coordinates"
                                        .to_string(),
                                    true,
                                    None,
                                ),
                            };
                            GhostDisplayManager::right_click(cx, cy)
                                .map(|_| "right-clicked".to_string())
                        }
                        "double_click" => {
                            let (cx, cy) = match (x, y) {
                                (Some(cx), Some(cy)) => (cx, cy),
                                _ => return (
                                    "Ghost Mode double_click requires both 'x' and 'y' coordinates"
                                        .to_string(),
                                    true,
                                    None,
                                ),
                            };
                            GhostDisplayManager::double_click(cx, cy)
                                .map(|_| "double-clicked".to_string())
                        }
                        "type" => {
                            if text.is_empty() {
                                return (
                                    "Ghost Mode type requires a non-empty 'text' value".to_string(),
                                    true,
                                    None,
                                );
                            }
                            GhostDisplayManager::type_text(&text)
                                .map(|_| format!("typed {} characters", text.chars().count()))
                        }
                        "key" => {
                            if text.is_empty() {
                                return (
                                    "Ghost Mode key requires a non-empty 'text' value".to_string(),
                                    true,
                                    None,
                                );
                            }
                            GhostDisplayManager::key_press(&text)
                                .map(|_| format!("pressed key '{}'", text))
                        }
                        "wait" => {
                            sleep(Duration::from_millis(500)).await;
                            Ok("waited 500ms".to_string())
                        }
                        other => Err(format!("Unsupported computer action '{}'", other)),
                    };

                    return match outcome {
                        Ok(performed) => (
                            serde_json::json!({
                                "status": "success",
                                "display": "ghost",
                                "action": action_str,
                                "performed": performed,
                                "x": x,
                                "y": y,
                            })
                            .to_string(),
                            false,
                            None,
                        ),
                        Err(e) => (
                            serde_json::json!({
                                "status": "failed",
                                "display": "ghost",
                                "action": action_str,
                                "error": e,
                                "x": x,
                                "y": y,
                            })
                            .to_string(),
                            true,
                            None,
                        ),
                    };
                }

                let action = match action_str {
                    "mouse_move" => ComputerAction::MouseMove {
                        x: x.unwrap_or(0),
                        y: y.unwrap_or(0),
                    },
                    "left_click" => ComputerAction::LeftClick { x, y },
                    "right_click" => ComputerAction::RightClick { x, y },
                    "double_click" => ComputerAction::DoubleClick { x, y },
                    "type" => ComputerAction::Type { text: text.clone() },
                    "key" => ComputerAction::Key { text: text.clone() },
                    "wait" => ComputerAction::Wait { ms: 500 },
                    other => return (format!("Unknown computer action: {}", other), true, None),
                };

                let before_shot = capture_screen(0).ok();

                match InputController::new() {
                    Ok(mut controller) => match controller.execute(&action) {
                        Ok(_) => {
                            // Record action to Teach-a-Task recorder if recording session is active
                            if crate::teach_task::TaskRecorder::is_recording() {
                                let (act_type, ax, ay, atxt, akey) = match &action {
                                    ComputerAction::MouseMove { x, y } => {
                                        ("move", Some(*x), Some(*y), None, None)
                                    }
                                    ComputerAction::LeftClick { x, y } => {
                                        ("click", *x, *y, None, None)
                                    }
                                    ComputerAction::RightClick { x, y } => {
                                        ("right_click", *x, *y, None, None)
                                    }
                                    ComputerAction::DoubleClick { x, y } => {
                                        ("double_click", *x, *y, None, None)
                                    }
                                    ComputerAction::LeftClickDrag { x, y } => {
                                        ("drag", Some(*x), Some(*y), None, None)
                                    }
                                    ComputerAction::Type { text } => {
                                        ("type", None, None, Some(text.clone()), None)
                                    }
                                    ComputerAction::Key { text } => {
                                        ("key", None, None, None, Some(text.clone()))
                                    }
                                    ComputerAction::Hotkey { keys } => {
                                        ("key", None, None, None, Some(keys.join("+")))
                                    }
                                    ComputerAction::Wait { .. } => {
                                        ("delay", None, None, None, None)
                                    }
                                };
                                let _ = crate::teach_task::TaskRecorder::record_action(
                                    act_type, ax, ay, atxt, akey,
                                );
                            }

                            // Record timeline step with real diff calculation
                            let mut diff_percent = 0.0f32;
                            let mut after_png = String::new();
                            if let (Some(cx), Some(cy)) = (x, y) {
                                sleep(Duration::from_millis(150)).await;
                                let after_shot = capture_screen(0).ok();
                                after_png = after_shot
                                    .as_ref()
                                    .map(|s| s.base64_png.clone())
                                    .unwrap_or_default();
                                diff_percent = if let (Some(ref b_shot), Some(ref a_shot)) =
                                    (&before_shot, &after_shot)
                                {
                                    use base64::Engine;
                                    if let (Ok(b_bytes), Ok(a_bytes)) = (
                                        base64::engine::general_purpose::STANDARD
                                            .decode(&b_shot.base64_png),
                                        base64::engine::general_purpose::STANDARD
                                            .decode(&a_shot.base64_png),
                                    ) {
                                        if let (Ok(b_img), Ok(a_img)) = (
                                            image::load_from_memory(&b_bytes),
                                            image::load_from_memory(&a_bytes),
                                        ) {
                                            crate::timeline::TimelineEngine::compute_diff_percent(
                                                &b_img.to_rgba8(),
                                                &a_img.to_rgba8(),
                                            )
                                        } else {
                                            0.0
                                        }
                                    } else {
                                        0.0
                                    }
                                } else {
                                    0.0
                                };

                                let step_rec = TimelineStepRecord {
                                    id: format!("tl_ca_{}", uuid::Uuid::new_v4()),
                                    run_id: Some(run_id.to_string()),
                                    step: 1,
                                    action_name: action_str.to_string(),
                                    click_x: Some(cx),
                                    click_y: Some(cy),
                                    before_base64: before_shot
                                        .map(|s| s.base64_png)
                                        .unwrap_or_default(),
                                    after_base64: after_png.clone(),
                                    diff_percent,
                                    timestamp: chrono::Utc::now().timestamp_millis(),
                                };
                                let _ = db.save_timeline_step(&step_rec);
                            }

                            // The action was dispatched successfully.  Whether
                            // the *intended* transition happened is reported
                            // separately so the model does not mistake a
                            // successful dispatch for a confirmed effect.
                            let status = if x.is_some() && y.is_some() {
                                if diff_percent > 0.05 {
                                    "success"
                                } else {
                                    "uncertain"
                                }
                            } else {
                                "success"
                            };
                            (
                                serde_json::json!({
                                    "status": status,
                                    "display": "physical",
                                    "action": action_str,
                                    "performed": format!("dispatched {:?}", action),
                                    "x": x,
                                    "y": y,
                                    "visual_change_percent": diff_percent,
                                    "note": if status == "uncertain" {
                                        "The input was delivered but no visual change was detected; the target may not have changed state."
                                    } else {
                                        ""
                                    },
                                })
                                .to_string(),
                                false,
                                if after_png.is_empty() {
                                    None
                                } else {
                                    Some(after_png)
                                },
                            )
                        }
                        Err(e) => (
                            serde_json::json!({
                                "status": "failed",
                                "display": "physical",
                                "action": action_str,
                                "error": e.to_string(),
                                "x": x,
                                "y": y,
                            })
                            .to_string(),
                            true,
                            None,
                        ),
                    },
                    Err(e) => (
                        format!("Failed to init input controller: {}", e),
                        true,
                        None,
                    ),
                }
            }
            "launch_ghost_app" => {
                // The launch is expressed as an explicit executable plus an
                // argument vector.  No free-form command string is accepted
                // and nothing is ever passed through a shell.
                let executable = tc.arguments["executable"].as_str().unwrap_or_default();
                let args: Vec<String> = tc.arguments["args"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                if executable.trim().is_empty() {
                    return (
                        "launch_ghost_app requires an 'executable' field naming the program to start"
                            .to_string(),
                        true,
                        None,
                    );
                }

                let spec = crate::execution::launcher::ProgramSpec {
                    executable: executable.to_string(),
                    args: args.clone(),
                    cwd: None,
                    env: Vec::new(),
                };

                match GhostDisplayManager::launch_in_ghost(&spec) {
                    Ok(res) => (
                        serde_json::json!({
                            "status": "success",
                            "executable": res.executable,
                            "args": res.args,
                            "pid": res.pid,
                            "display": res.display,
                        })
                        .to_string(),
                        false,
                        None,
                    ),
                    Err(e) => (
                        serde_json::json!({
                            "status": "failed",
                            "executable": executable,
                            "error": e,
                        })
                        .to_string(),
                        true,
                        None,
                    ),
                }
            }
            "invoke_skill" => {
                let skill_id = tc.arguments["skill_id"].as_str().unwrap_or_default();
                match db.get_skill(skill_id) {
                    Ok(Some(rec)) => {
                        match serde_json::from_str::<Vec<crate::teach_task::SkillStep>>(
                            &rec.steps_json,
                        ) {
                            Ok(steps) => {
                                let skill = Skill {
                                    id: rec.id,
                                    name: rec.name,
                                    description: rec.description,
                                    steps,
                                    created_at: rec.created_at,
                                };
                                match SkillReplayer::execute(
                                    &skill,
                                    Some(db),
                                    Some(token),
                                    |step, total, desc| {
                                        log::debug!("[Agent Skill] {}/{}: {}", step, total, desc);
                                    },
                                )
                                .await
                                {
                                    Ok(_) => (
                                        format!("Successfully replayed skill '{}'", skill.name),
                                        false,
                                        None,
                                    ),
                                    Err(e) => (format!("Skill replay error: {}", e), true, None),
                                }
                            }
                            Err(e) => (format!("Failed to parse skill steps: {}", e), true, None),
                        }
                    }
                    Ok(None) => (
                        format!("Skill ID '{}' not found in database", skill_id),
                        true,
                        None,
                    ),
                    Err(e) => (format!("Database error reading skill: {}", e), true, None),
                }
            }
            "delegate_task" => {
                let target_bot_id = tc.arguments["target_bot_id"].as_str().unwrap_or_default();
                let objective = tc.arguments["objective"].as_str().unwrap_or_default();
                let context_summary = tc.arguments["context_summary"].as_str().unwrap_or_default();

                // Recursion check: Disallow deep delegation chains (max depth 3)
                if run_id.contains("_child_") && run_id.matches("_child_").count() >= 3 {
                    return (
                        "Max delegation recursion depth (3) exceeded".to_string(),
                        true,
                        None,
                    );
                }

                // Verify target bot exists
                let bots = db.list_bots().unwrap_or_default();
                let target_bot = match bots.iter().find(|b| b.id == target_bot_id) {
                    Some(b) => b.clone(),
                    None => {
                        return (
                            format!("Target bot '{}' not found in roster", target_bot_id),
                            true,
                            None,
                        )
                    }
                };

                // Track bot status in roster
                MultiBotOrchestrator::set_bot_task(target_bot_id, Some(objective));
                struct BotRosterGuard(String);
                impl Drop for BotRosterGuard {
                    fn drop(&mut self) {
                        MultiBotOrchestrator::set_bot_task(&self.0, None);
                    }
                }
                let _roster_guard = BotRosterGuard(target_bot_id.to_string());

                // Create child run record with unique ID
                let child_run_id = format!(
                    "{}_child_{}_{}",
                    run_id,
                    target_bot_id,
                    &uuid::Uuid::new_v4().to_string()[..8]
                );
                let now = chrono::Utc::now().timestamp_millis();
                let parent_model = db
                    .get_run(run_id)
                    .ok()
                    .flatten()
                    .map(|r| r.model)
                    .unwrap_or_else(|| "nvidia/nemotron-3-ultra-550b-a55b".to_string());
                let child_model = parent_model;

                let child_run = AgentRun {
                    id: child_run_id.clone(),
                    conversation_id: db
                        .get_run(run_id)
                        .ok()
                        .flatten()
                        .and_then(|r| r.conversation_id),
                    agent_id: target_bot_id.to_string(),
                    model: child_model.clone(),
                    prompt: objective.to_string(),
                    status: "running".to_string(),
                    created_at: now,
                    started_at: Some(now),
                    completed_at: None,
                    error: None,
                    config_json: Some(
                        serde_json::json!({
                            "context": context_summary,
                            "parent_run_id": run_id,
                            "workspace_root": workspace_root.map(|p| p.to_string_lossy().to_string()),
                        })
                        .to_string(),
                    ),
                    parent_run_id: Some(run_id.to_string()),
                };
                let _ = db.create_run(&child_run);

                // Build specialized prompt for child agent
                let child_sys_prompt = format!(
                    "{}\nYou have been delegated the following sub-objective: {}\nContext: {}\nComplete this specialized task and report back clearly.",
                    target_bot.system_prompt, objective, context_summary
                );

                let child_history = vec![ChatMessage {
                    role: "user".to_string(),
                    content: MessageContent::from_text(objective),
                    tool_calls: None,
                    tool_call_id: None,
                }];

                // Execute child agent turn with linked child cancellation token
                let child_token = token.child_token();
                let api_key = match crate::storage::Vault::new(db.clone()).get_all_keys() {
                    Ok(k) => k.nvidia.unwrap_or_default(),
                    Err(_) => String::new(),
                };

                let res = Box::pin(self.run_step_with_connectors(
                    &child_run_id,
                    &child_model,
                    &api_key,
                    None,
                    &child_sys_prompt,
                    child_history,
                    db.clone(),
                    mcp_manager.cloned(),
                    connector_manager.cloned(),
                    Some(child_token),
                    |_| {},
                ))
                .await;

                match res {
                    Ok(out) => {
                        let comp = chrono::Utc::now().timestamp_millis();
                        let _ = db.update_run_status(
                            &child_run_id,
                            "succeeded",
                            None,
                            Some(comp),
                            None,
                        );
                        (
                            format!(
                                "Teammate @{} completed delegated objective:\n{}",
                                target_bot.name, out
                            ),
                            false,
                            None,
                        )
                    }
                    Err(e) => {
                        let comp = chrono::Utc::now().timestamp_millis();
                        let _ = db.update_run_status(
                            &child_run_id,
                            "failed",
                            None,
                            Some(comp),
                            Some(&e),
                        );
                        (
                            format!(
                                "Teammate @{} encountered an error during delegation: {}",
                                target_bot.name, e
                            ),
                            true,
                            None,
                        )
                    }
                }
            }
            // ---- Project, development server and browser tools ----------
            "discover_project" => {
                let outcome = tools::discover_project(&tc.arguments).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "inspect_project" => {
                let outcome = tools::inspect_project(&tc.arguments).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "start_dev_server" => {
                let outcome =
                    tools::start_dev_server(&tc.arguments, &self.dev_servers, run_id).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "dev_server_status" => {
                let outcome = tools::dev_server_status(&tc.arguments, &self.dev_servers).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "dev_server_logs" => {
                let outcome = tools::dev_server_logs(&tc.arguments, &self.dev_servers).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "stop_dev_server" => {
                let outcome = tools::stop_dev_server(&tc.arguments, &self.dev_servers).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "browser_capabilities" => {
                let outcome = tools::browser_capabilities(&self.browser).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "browser_open" => {
                let policy = crate::governance::PolicyEngine::load(db);
                let outcome = tools::browser_open(
                    &tc.arguments,
                    &self.browser,
                    run_id,
                    &policy.blocked_domains,
                )
                .await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "browser_navigate" => {
                let outcome = tools::browser_navigate(&tc.arguments, &self.browser).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "browser_snapshot" => {
                let outcome = tools::browser_snapshot(&tc.arguments, &self.browser).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "browser_click" => {
                let outcome = tools::browser_click(&tc.arguments, &self.browser).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "browser_fill" => {
                let outcome = tools::browser_fill(&tc.arguments, &self.browser).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "browser_press" => {
                let outcome = tools::browser_press(&tc.arguments, &self.browser).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "browser_screenshot" => {
                let outcome = tools::browser_screenshot(&tc.arguments, &self.browser).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "browser_console" => {
                let outcome = tools::browser_console(&tc.arguments, &self.browser).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "browser_network" => {
                let outcome = tools::browser_network(&tc.arguments, &self.browser).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "browser_tabs" => {
                let outcome = tools::browser_tabs(&tc.arguments, &self.browser).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "browser_close" => {
                let outcome = tools::browser_close(&tc.arguments, &self.browser).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "audit_ui" => {
                let outcome = self.run_ui_audit(run_id, &tc.arguments, db, token).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            custom => {
                // 1. Try connector runtime tool execution (Native, Remote MCP, or unified)
                if let Some(conn_mgr) = connector_manager {
                    if let Some(res) = conn_mgr
                        .call_tool_with_auth(custom, tc.arguments.clone(), auth_proof)
                        .await
                    {
                        return match res {
                            Ok(out) => (out, false, None),
                            Err(e) => (format!("Connector Tool error: {}", e), true, None),
                        };
                    }
                }
                // 2. Fall back to legacy local MCP manager if unhandled
                if let Some(mcp) = mcp_manager {
                    if let Some(res) = mcp.call_mcp_tool(custom, tc.arguments.clone()).await {
                        return match res {
                            Ok(out) => (out, false, None),
                            Err(e) => (format!("MCP Tool error: {}", e), true, None),
                        };
                    }
                }
                (format!("Unknown or unhandled tool: {}", custom), true, None)
            }
        }
    }

    /// Runs the complete UI audit workflow.
    ///
    /// The workflow asks for approval for the *exact* development-server
    /// command before executing any repository-controlled code, then drives the
    /// browser and produces a Markdown report with evidence.
    async fn run_ui_audit(
        &self,
        run_id: &str,
        args: &serde_json::Value,
        db: &Database,
        token: &CancellationToken,
    ) -> ToolOutcome {
        let project_name = args["project_name"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_string();
        if project_name.is_empty() {
            return ToolOutcome::fail("audit_ui requires a 'project_name' parameter.");
        }

        let search_root = match tools::resolve_search_root(args) {
            Ok(r) => r,
            Err(e) => return ToolOutcome::fail(e),
        };

        let browser = args["browser"]
            .as_str()
            .unwrap_or("firefox")
            .trim()
            .to_string();
        if !browser.eq_ignore_ascii_case("firefox") {
            return ToolOutcome::fail(format!(
                "Kyro can only drive Firefox. '{}' was requested, and Kyro will not silently \
substitute a different browser.",
                browser
            ));
        }

        let options = crate::audit::workflow::AuditOptions {
            project_name,
            search_root,
            browser,
            headless: args["headless"].as_bool().unwrap_or(true),
            keep_server_running: args["keep_server_running"].as_bool().unwrap_or(false),
            ..Default::default()
        };

        let workflow = crate::audit::workflow::UiAuditWorkflow::new(
            self.browser.clone(),
            self.dev_servers.clone(),
        );

        let mut progress_log: Vec<String> = Vec::new();
        let mut on_progress = |p: crate::audit::workflow::AuditProgress| {
            progress_log.push(format!("[{}] {}", p.stage.as_str(), p.message));
        };

        // Build the plan first, so that the approval names the exact project
        // directory and command that will be executed.
        let plan = match workflow.plan(&options).await {
            Ok(p) => p,
            Err(e) => {
                return ToolOutcome::fail(
                    serde_json::json!({
                        "status": "failed",
                        "error": e,
                    })
                    .to_string(),
                )
            }
        };

        let approved = self
            .await_approval(
                db,
                run_id,
                "start_dev_server",
                &format!(
                    "Start `{}` in `{}`",
                    plan.command.display(),
                    plan.command.working_directory.display()
                ),
                &serde_json::json!({
                    "command": plan.command.display(),
                    "working_directory": plan.command.working_directory,
                    "script": plan.command.script,
                    "project_path": plan.project.path,
                }),
                crate::execution::permissions::RiskLevel::Moderate,
                "Running a project's development script executes repository-controlled code",
                token,
            )
            .await;

        match approved {
            Ok(true) => {}
            Ok(false) => {
                return ToolOutcome::fail(
                    serde_json::json!({
                        "status": "cancelled",
                        "error": "The development server was not started: the request was not approved. Nothing was executed.",
                    })
                    .to_string(),
                )
            }
            Err(e) => {
                return ToolOutcome::fail(
                    serde_json::json!({
                        "status": "failed",
                        "error": e,
                    })
                    .to_string(),
                )
            }
        }

        let outcome = match workflow.execute(&options, plan, &mut on_progress).await {
            Ok(o) => o,
            Err(e) => {
                return ToolOutcome::fail(
                    serde_json::json!({
                        "status": "failed",
                        "error": e,
                        "progress": progress_log,
                    })
                    .to_string(),
                )
            }
        };

        // Store the report through Kyro's artifact registry so that the
        // evidence does not end up scattered on the Desktop.
        let report_name = format!("ui-audit-{}.md", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
        let artifacts_dir = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("kyro")
            .join("artifacts");
        if let Err(e) = std::fs::create_dir_all(&artifacts_dir) {
            return ToolOutcome::fail(format!("Could not create the artifact directory: {}", e));
        }
        let artifact_id = format!("art_{}", uuid::Uuid::new_v4());
        let report_path = artifacts_dir.join(format!("{}_{}", &artifact_id[4..12], report_name));
        if let Err(e) = std::fs::write(&report_path, &outcome.report_markdown) {
            return ToolOutcome::fail(format!("Could not write the bug report: {}", e));
        }

        let conv_id = db
            .get_run(run_id)
            .ok()
            .flatten()
            .and_then(|r| r.conversation_id);
        let rec = crate::storage::ArtifactRecord {
            id: artifact_id.clone(),
            run_id: Some(run_id.to_string()),
            conversation_id: conv_id,
            name: report_name.clone(),
            file_path: report_path.to_string_lossy().to_string(),
            artifact_type: "markdown".to_string(),
            mime_type: "text/markdown".to_string(),
            size_bytes: outcome.report_markdown.len() as i64,
            created_at: chrono::Utc::now().timestamp_millis(),
            metadata_json: Some(
                serde_json::json!({
                    "preview": outcome.report_markdown.chars().take(400).collect::<String>(),
                    "findings": outcome.findings.len(),
                    "routes_discovered": outcome.context.routes_discovered,
                    "routes_inspected": outcome.context.routes_inspected,
                })
                .to_string(),
            ),
        };
        let _ = db.create_artifact(&rec);

        let payload = serde_json::json!({
            "status": "success",
            "report_artifact_id": artifact_id,
            "report_path": report_path.to_string_lossy(),
            "progress": progress_log,
            "context": outcome.context,
            "finding_count": outcome.findings.len(),
            "findings": outcome.findings,
            "route_inventory": outcome.route_inventory,
            "dev_server": outcome.dev_server,
        });
        ToolOutcome::ok(payload.to_string())
    }

    /// Requests an approval and waits for it to be resolved.
    #[allow(clippy::too_many_arguments)]
    async fn await_approval(
        &self,
        db: &Database,
        run_id: &str,
        tool_name: &str,
        summary: &str,
        args: &serde_json::Value,
        risk: crate::execution::permissions::RiskLevel,
        reason: &str,
        token: &CancellationToken,
    ) -> Result<bool, String> {
        let request = AuthorizationEngine::request_approval(
            db, run_id, tool_name, summary, args, risk, reason,
        )
        .map_err(|e| format!("Could not queue the approval request: {}", e))?;

        loop {
            if token.is_cancelled() {
                let _ = db.resolve_approval_request(&request.id, "cancelled", "user_cancellation");
                return Err("The request was cancelled while waiting for approval.".to_string());
            }
            if let Ok(Some(status)) = db.get_approval_request(&request.id) {
                match status.status.as_str() {
                    "approved" => return Ok(true),
                    "denied" => return Ok(false),
                    "cancelled" => return Ok(false),
                    "expired" => {
                        let _ = db.expire_overdue_approvals(chrono::Utc::now().timestamp_millis());
                        return Ok(false);
                    }
                    _ => {}
                }
            }
            sleep(Duration::from_millis(300)).await;
        }
    }
}
