use super::cancellation::CancellationToken;
use super::registry::{
    canonical_tool_name, AgentRegistry, ProfileSnapshot, ResolveError, RunConfigSnapshot,
};
use super::router::{
    ChatMessage, MessageContent, ModelRouter, ToolCall, ToolDefinition, DEFAULT_VISION_MODEL,
};
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
use tokio::sync::{Mutex as TokioMutex, RwLock as TokioRwLock};
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

/// The incremental event emitted as model reasoning tokens arrive.
///
/// Keeps raw provider reasoning separate from ordinary assistant conversation
/// content and carries sufficient identity to route across concurrent and
/// delegated runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReasoningEvent {
    /// The identifier of the run producing the reasoning.
    pub run_id: String,
    /// The conversation the run belongs to, when known.
    pub conversation_id: Option<String>,
    /// The agent producing the reasoning, when known.
    pub agent_id: Option<String>,
    /// The run this event was delegated from, for child-agent reasoning.
    pub parent_run_id: Option<String>,
    /// `true` when produced by a delegated child run.
    pub is_child_run: bool,
    /// The current step identifier.
    pub step: usize,
    /// Incremental reasoning chunk received.
    pub delta: String,
    /// Accumulated reasoning content for the step.
    pub accumulated: String,
    /// Stream lifecycle state: "start", "delta", "completed", "cancelled".
    pub state: String,
}

/// Shared callback sink for live reasoning events across subtasks and steps.
pub type ReasoningSink = Arc<std::sync::Mutex<Box<dyn FnMut(AgentReasoningEvent) + Send>>>;

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
    pub model: String,
    pub api_key: String,
    pub base_url: Option<String>,
}

/// RAII Drop guard to ensure active run context cleanup on all exit paths
struct ActiveRunGuard {
    run_id: String,
    active_runs: Arc<TokioMutex<HashMap<String, ActiveRunContext>>>,
}

impl Drop for ActiveRunGuard {
    fn drop(&mut self) {
        if let Ok(mut map) = self.active_runs.try_lock() {
            map.remove(&self.run_id);
        } else {
            let active_runs = self.active_runs.clone();
            let run_id = self.run_id.clone();
            tokio::spawn(async move {
                let mut map = active_runs.lock().await;
                map.remove(&run_id);
            });
        }
    }
}

pub struct AgentLoopEngine {
    router: ModelRouter,
    active_runs: Arc<TokioMutex<HashMap<String, ActiveRunContext>>>,
    cancelled_runs: Arc<TokioMutex<std::collections::HashSet<String>>>,
    /// The managed browser automation subsystem.
    pub browser: Arc<BrowserAutomationManager>,
    /// The managed development-server subsystem.
    pub dev_servers: Arc<DevServerManager>,
    /// The canonical agent profile registry. Authoritative for identity,
    /// instructions, tool policy, delegation budgets and step limits; manifests
    /// on disk are the single editable source of truth.
    pub registry: Arc<TokioRwLock<AgentRegistry>>,
}

/// Hard safety bound on delegation chain walks (defense against corrupted
/// parent linkage creating pathological loops).
const MAX_DELEGATION_DEPTH_WALK: usize = 64;

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
        let registry = Arc::new(TokioRwLock::new(AgentRegistry::embedded_only()));
        Self::with_registry(browser, dev_servers, registry)
    }

    /// Creates an engine with the shipped subsystem managers and an
    /// externally maintained agent registry (the production wiring shares one
    /// registry between the engine and the command layer).
    pub fn with_registry(
        browser: Arc<BrowserAutomationManager>,
        dev_servers: Arc<DevServerManager>,
        registry: Arc<TokioRwLock<AgentRegistry>>,
    ) -> Self {
        Self {
            router: ModelRouter::new(),
            active_runs: Arc::new(TokioMutex::new(HashMap::new())),
            cancelled_runs: Arc::new(TokioMutex::new(std::collections::HashSet::new())),
            browser,
            dev_servers,
            registry,
        }
    }

    /// Resolve the canonical profile for a requested agent (id, alias, or
    /// `None` for the bundled default). Refreshes the registry when manifest
    /// files changed on disk. Production callers use this before creating a
    /// run record so persisted ids are always canonical.
    pub async fn resolve_agent(
        &self,
        id_or_alias: Option<&str>,
    ) -> Result<ProfileSnapshot, String> {
        let mut registry = self.registry.write().await;
        registry.ensure_fresh();
        match id_or_alias.filter(|s| !s.trim().is_empty()) {
            None => registry
                .resolve_default()
                .cloned()
                .map_err(|e| e.to_string()),
            Some(id) => registry.resolve_lenient(id).map_err(|e| e.to_string()),
        }
    }

    /// Configuration directory root backing the registry's manifests, when
    /// one exists (workspace progress persists there).
    pub async fn config_root(&self) -> Option<std::path::PathBuf> {
        let registry = self.registry.read().await;
        registry.config_root().map(|p| p.to_path_buf())
    }

    /// Register a cancellation token for an active run
    pub async fn register_run_token(&self, run_id: &str, token: CancellationToken) {
        self.register_run(run_id, token, "", "", None).await;
    }

    /// Register full run context including cancellation token, model, and credentials
    pub async fn register_run(
        &self,
        run_id: &str,
        token: CancellationToken,
        model: &str,
        api_key: &str,
        base_url: Option<&str>,
    ) {
        let is_already_cancelled = {
            let cancelled = self.cancelled_runs.lock().await;
            cancelled.contains(run_id)
        };
        if is_already_cancelled {
            token.cancel();
        }
        let mut map = self.active_runs.lock().await;
        map.insert(
            run_id.to_string(),
            ActiveRunContext {
                token,
                pids: Arc::new(std::sync::Mutex::new(Vec::new())),
                model: model.to_string(),
                api_key: api_key.to_string(),
                base_url: base_url.map(|s| s.to_string()),
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
        {
            let mut cancelled = self.cancelled_runs.lock().await;
            cancelled.insert(run_id.to_string());
        }
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
            true
        }
    }

    /// Returns whether a run has an active in-memory execution context
    pub async fn is_run_active(&self, run_id: &str) -> bool {
        let map = self.active_runs.lock().await;
        map.contains_key(run_id)
    }

    /// Returns whether a run has been requested to be cancelled
    pub async fn is_run_cancelled(&self, run_id: &str) -> bool {
        let cancelled = self.cancelled_runs.lock().await;
        cancelled.contains(run_id)
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
                name: "run_safe_experiment".to_string(),
                description: "Execute a controlled code experiment (script, prototype, or test) inside a secure, network-isolated container sandbox with strict CPU, memory, and timeout limits.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Short descriptive name for the experiment" },
                        "runtime": {
                            "type": "string",
                            "enum": ["python", "node", "rust", "bash", "debian"],
                            "description": "The isolated runtime environment"
                        },
                        "script": { "type": "string", "description": "The full script source code to execute inside the sandbox" },
                        "workspace_path": { "type": "string", "description": "Optional project directory to safely copy into the experiment workspace" },
                        "timeout_seconds": { "type": "integer", "description": "Timeout in seconds (default 60, max 300)" }
                    },
                    "required": ["name", "runtime", "script"]
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

    /// Execute an autonomous agent run step with cancellation, centralized authorization, and timeline persistence.
    ///
    /// Compatibility note: `system_prompt` is retained in the signature but is
    /// no longer authoritative — the run's system prompt is composed from the
    /// agent profile resolved in [`AgentRegistry`] plus the platform manual
    /// (see `run_step_with_connectors`).
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

    /// Execute an autonomous agent run step with both connector manager and local MCP support.
    ///
    /// The system prompt, tool set, delegation policy and step budget are all
    /// resolved from the run's agent profile in the [`AgentRegistry`]; the
    /// `system_prompt` parameter is accepted for signature compatibility but is
    /// ignored.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_step_with_connectors<F>(
        &self,
        run_id: &str,
        model: &str,
        api_key: &str,
        base_url: Option<&str>,
        // Ignored; the profile resolved from the registry is authoritative.
        system_prompt: &str,
        history: Vec<ChatMessage>,
        db: Database,
        mcp_manager: Option<McpManager>,
        connector_manager: Option<Arc<crate::connector::ConnectorManager>>,
        cancel_token: Option<CancellationToken>,
        on_event: F,
    ) -> Result<String, String>
    where
        F: FnMut(AgentStepEvent) + Send + 'static,
    {
        self.run_step_extended(
            run_id,
            model,
            api_key,
            base_url,
            system_prompt,
            history,
            db,
            mcp_manager,
            connector_manager,
            cancel_token,
            on_event,
            |_| {},
        )
        .await
    }

    /// Execute an autonomous agent run step with both connector manager, local MCP support,
    /// and live model reasoning event streaming.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_step_extended<F, R>(
        &self,
        run_id: &str,
        model: &str,
        api_key: &str,
        base_url: Option<&str>,
        // Ignored; the profile resolved from the registry is authoritative.
        _system_prompt: &str,
        mut history: Vec<ChatMessage>,
        db: Database,
        mcp_manager: Option<McpManager>,
        connector_manager: Option<Arc<crate::connector::ConnectorManager>>,
        cancel_token: Option<CancellationToken>,
        mut on_event: F,
        on_reasoning: R,
    ) -> Result<String, String>
    where
        F: FnMut(AgentStepEvent) + Send + 'static,
        R: FnMut(AgentReasoningEvent) + Send + 'static,
    {
        let token = cancel_token.unwrap_or_default();
        self.register_run(run_id, token.clone(), model, api_key, base_url)
            .await;
        let _run_guard = ActiveRunGuard {
            run_id: run_id.to_string(),
            active_runs: self.active_runs.clone(),
        };

        let reasoning_sender: ReasoningSink =
            Arc::new(std::sync::Mutex::new(Box::new(on_reasoning)));

        if token.is_cancelled() || self.is_run_cancelled(run_id).await {
            return Err("Agent run was cancelled by user before execution started".to_string());
        }

        if let Ok(Some(r)) = db.get_run(run_id) {
            if r.status == "cancelled" {
                return Err("Agent run was cancelled by user before execution started".to_string());
            }
        }

        let mut tools = Self::standard_tools();
        if let Some(ref conn_mgr) = connector_manager {
            let conn_tools = conn_mgr.get_all_connector_tools().await;
            tools.extend(conn_tools);
        } else if let Some(ref mcp) = mcp_manager {
            let external_tools = mcp.get_all_mcp_tools().await;
            tools.extend(external_tools);
        }

        // Determine active agent ID from run or default to apex
        let (recorded_agent_id, run_conversation_id, run_parent_id) =
            if let Ok(Some(r)) = db.get_run(run_id) {
                (r.agent_id, r.conversation_id, r.parent_run_id)
            } else {
                ("apex-orchestrator".to_string(), None, None)
            };
        let is_child_run = run_parent_id.is_some();

        // ---- Stage C: the registry, not the caller, decides who runs what ----
        // The profile is resolved by id/alias from the agent registry; the
        // legacy `system_prompt` parameter is accepted for signature
        // compatibility but is never authoritative.
        let profile: ProfileSnapshot = {
            let mut registry = self.registry.write().await;
            registry.ensure_fresh();
            match registry.resolve_lenient(&recorded_agent_id) {
                Ok(p) => p,
                Err(err) => {
                    let msg = match &err {
                        ResolveError::DisabledProfile(id) => {
                            format!("Agent '{}' is disabled and cannot run.", id)
                        }
                        ResolveError::InvalidProfile(id, reason) => {
                            format!("Agent '{}' has an invalid profile: {}", id, reason)
                        }
                        ResolveError::UnknownProfile(id) => format!(
                            "No agent profile could be resolved for '{}' (registry default unavailable)",
                            id
                        ),
                    };
                    let seq = db.get_next_event_seq(run_id).unwrap_or(1);
                    let _ = db.append_run_event(&RunEvent {
                        id: format!("evt_{}_{}", run_id, seq),
                        run_id: run_id.to_string(),
                        seq,
                        event_type: "error".to_string(),
                        payload_json: serde_json::json!({ "step": 0, "error": &msg }).to_string(),
                        created_at: chrono::Utc::now().timestamp_millis(),
                    });
                    let _ = db.update_run_status(
                        run_id,
                        "failed",
                        None,
                        Some(chrono::Utc::now().timestamp_millis()),
                        Some(&msg),
                    );
                    return Err(msg);
                }
            }
        };
        // Canonical profile id: memory scopes, timeline events and roster
        // status always key on it, even when the run was recorded with an
        // alias or a legacy pseudo id.
        let active_agent_id = profile.id.clone();

        // Extract last user prompt for relevance matching
        let last_prompt = history
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.as_text_lossy())
            .unwrap_or_default();

        let memory_context = MemoryStore::build_context_block_scoped(
            &db,
            &active_agent_id,
            &last_prompt,
            &profile.context.memory_scopes,
        );

        // Query deep local RAG over authorized documents if there is an active user prompt
        let rag_context = if !last_prompt.trim().is_empty() {
            let engine = crate::storage::retrieval::RetrievalEngine::new(db.clone());
            if let Ok(results) = engine.search(&last_prompt, 4).await {
                crate::storage::retrieval::RetrievalEngine::format_context_block(&results)
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Compose the authoritative system prompt: platform subsystem manual,
        // then the profile's own instructions (the last identity statement
        // wins), then memory and retrieved context.
        let composed_system = {
            let mut s = crate::commands::build_default_system_prompt();
            s.push_str("\n\n# Agent profile: ");
            s.push_str(&profile.name);
            s.push_str(" (");
            s.push_str(&profile.id);
            s.push_str(")\n\n");
            s.push_str(&profile.instructions);
            s
        };
        let enriched_prompt = format!("{}{}{}", composed_system, memory_context, rag_context);

        // Profile tool policy (Stage C): an allowlist restricts dispatch to
        // exactly the named tools (deny by default), and a profile without
        // delegation permission never sees delegate_task.
        tools.retain(|t| profile.tool_allowed(&t.name));
        if !profile.delegation.enabled {
            tools.retain(|t| t.name != "delegate_task");
        }

        // Persist the profile snapshot with the run timeline so provenance is
        // immutable for in-flight and historical runs even when the manifest is
        // later edited or deleted.
        {
            let seq = db.get_next_event_seq(run_id).unwrap_or(1);
            let snapshot_event = RunEvent {
                id: format!("evt_{}_{}", run_id, seq),
                run_id: run_id.to_string(),
                seq,
                event_type: "profile".to_string(),
                payload_json: serde_json::json!({
                    "profile_id": profile.id,
                    "name": profile.name,
                    "role": profile.role,
                    "origin": profile.origin,
                    "enabled": profile.enabled,
                    "version": profile.version_label(),
                    "content_hash": profile.content_hash,
                    "model": profile.model,
                    "tools": { "mode": profile.tools.mode, "allow": profile.tools.allow },
                    "delegation": profile.delegation,
                    "limits": profile.limits,
                    "instructions": profile.instructions,
                })
                .to_string(),
                created_at: chrono::Utc::now().timestamp_millis(),
            };
            let _ = db.append_run_event(&snapshot_event);
        }

        let max_steps = profile.limits.max_steps.max(1) as usize;
        let mut final_content = String::new();
        let mut is_completed = false;

        for step in 1..=max_steps {
            if token.is_cancelled() {
                {
                    let mut map = self.active_runs.lock().await;
                    map.remove(run_id);
                }
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
                    {
                        let mut map = self.active_runs.lock().await;
                        map.remove(run_id);
                    }
                    return Err("Agent run cancelled during human takeover".to_string());
                }
                sleep(Duration::from_millis(400)).await;
            }

            let mut accumulated_reasoning = String::new();
            let mut has_started_reasoning = false;
            let stream_sender = reasoning_sender.clone();

            let resp = match self
                .router
                .complete_streamed(
                    model,
                    api_key,
                    base_url,
                    &enriched_prompt,
                    &history,
                    &tools,
                    Some(&token),
                    |delta: &str| {
                        accumulated_reasoning.push_str(delta);
                        let state = if !has_started_reasoning {
                            has_started_reasoning = true;
                            "start".to_string()
                        } else {
                            "delta".to_string()
                        };
                        if let Ok(mut guard) = stream_sender.lock() {
                            guard(AgentReasoningEvent {
                                run_id: run_id.to_string(),
                                conversation_id: run_conversation_id.clone(),
                                agent_id: Some(active_agent_id.clone()),
                                parent_run_id: run_parent_id.clone(),
                                is_child_run,
                                step,
                                delta: delta.to_string(),
                                accumulated: accumulated_reasoning.clone(),
                                state,
                            });
                        }
                    },
                )
                .await
            {
                Ok(r) => {
                    if has_started_reasoning {
                        if let Ok(mut guard) = reasoning_sender.lock() {
                            guard(AgentReasoningEvent {
                                run_id: run_id.to_string(),
                                conversation_id: run_conversation_id.clone(),
                                agent_id: Some(active_agent_id.clone()),
                                parent_run_id: run_parent_id.clone(),
                                is_child_run,
                                step,
                                delta: String::new(),
                                accumulated: accumulated_reasoning.clone(),
                                state: "completed".to_string(),
                            });
                        }
                    }
                    r
                }
                Err(e) => {
                    {
                        let mut map = self.active_runs.lock().await;
                        map.remove(run_id);
                    }
                    let _ = db.update_run_status(
                        run_id,
                        "failed",
                        None,
                        Some(chrono::Utc::now().timestamp_millis()),
                        Some(&e),
                    );
                    let seq = db.get_next_event_seq(run_id).unwrap_or(step as i64);
                    let err_event = RunEvent {
                        id: format!("evt_{}_{}", run_id, seq),
                        run_id: run_id.to_string(),
                        seq,
                        event_type: "error".to_string(),
                        payload_json: serde_json::json!({
                            "step": step,
                            "error": &e,
                        })
                        .to_string(),
                        created_at: chrono::Utc::now().timestamp_millis(),
                    };
                    let _ = db.append_run_event(&err_event);
                    return Err(e);
                }
            };

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
                is_completed = true;
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
                        &profile,
                        base_url,
                        Some(reasoning_sender.clone()),
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

                // Handle visual data from tools: persist image artifact and process with vision/multimodal
                let message_content = if let Some(ref b64) = visual_data {
                    use base64::Engine;
                    let shot_art_id = format!("art_{}", uuid::Uuid::new_v4());
                    let shot_name =
                        format!("shot-{}.png", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
                    let artifacts_dir = dirs::config_dir()
                        .unwrap_or_else(|| std::path::PathBuf::from("."))
                        .join("kyro")
                        .join("artifacts");
                    let _ = std::fs::create_dir_all(&artifacts_dir);
                    let shot_path =
                        artifacts_dir.join(format!("{}_{}", &shot_art_id[4..12], shot_name));
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
                        let _ = std::fs::write(&shot_path, &bytes);
                        let conv_id = db
                            .get_run(run_id)
                            .ok()
                            .flatten()
                            .and_then(|r| r.conversation_id);
                        let rec = crate::storage::ArtifactRecord {
                            id: shot_art_id.clone(),
                            run_id: Some(run_id.to_string()),
                            conversation_id: conv_id,
                            name: shot_name.clone(),
                            file_path: shot_path.to_string_lossy().to_string(),
                            artifact_type: "image".to_string(),
                            mime_type: "image/png".to_string(),
                            size_bytes: bytes.len() as i64,
                            created_at: chrono::Utc::now().timestamp_millis(),
                            metadata_json: Some(
                                serde_json::json!({
                                    "tool": tc.name,
                                    "source": "browser_tool",
                                })
                                .to_string(),
                            ),
                        };
                        let _ = db.create_artifact(&rec);
                    }

                    if ModelRouter::is_vision_model(model) {
                        MessageContent::from_text_and_image(
                            &format!(
                                "{}\n[Visual artifact saved: {} ({})]",
                                output, shot_name, shot_art_id
                            ),
                            b64,
                        )
                    } else {
                        // Text-only model: Run visual analyzer fallback and return structured summary.
                        // (Bug 15) The fallback vision model may live on a
                        // different provider than the run model, so its
                        // endpoint and credential are resolved for the vision
                        // model itself; an unavailable credential degrades to
                        // the analyzer's honest "not analyzed" report.
                        let fallback_vision_model = DEFAULT_VISION_MODEL.to_string();
                        let vis_api_key = if let Ok(route) =
                            ModelRouter::resolve_model_route(&fallback_vision_model)
                        {
                            let vault = crate::storage::Vault::new(db.clone());
                            let vault_keys = vault.get_all_keys().unwrap_or_default();
                            match ModelRouter::select_api_key_for_route(&route, &vault_keys) {
                                Ok(k) => Some((k, route.base_url)),
                                Err(_) => None,
                            }
                        } else {
                            None
                        };
                        let (vis_api_key, vis_base_url) = vis_api_key
                            .map(|(k, b)| (k, Some(b)))
                            .unwrap_or((String::new(), base_url.map(|s| s.to_string())));
                        let analyzer = crate::agent::vision::VisualAnalyzer::new(
                            Arc::new(self.router.clone()),
                            vis_api_key,
                            vis_base_url,
                        );
                        let vis_ctx = crate::agent::vision::VisualAnalysisContext {
                            route: "".to_string(),
                            preview_url: "".to_string(),
                            viewport: "".to_string(),
                            dom_summary: None,
                            element_of_interest: None,
                        };
                        let vis_res = analyzer.analyze_screenshot(b64, model, &vis_ctx).await;
                        let defects_str = if vis_res.visible_defects.is_empty() {
                            "None detected".to_string()
                        } else {
                            vis_res
                                .visible_defects
                                .iter()
                                .map(|d| format!("{}: {}", d.defect_type, d.description))
                                .collect::<Vec<_>>()
                                .join("; ")
                        };
                        let augmented = format!(
                            "{}\n\n[Visual Analysis - Artifact: {} ({})]\nSummary: {}\nDefects: {}\nConfidence: {}",
                            output,
                            shot_name,
                            shot_art_id,
                            vis_res.summary,
                            defects_str,
                            vis_res.confidence
                        );
                        MessageContent::from_text(&augmented)
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

        if !is_completed {
            {
                let mut map = self.active_runs.lock().await;
                map.remove(run_id);
            }
            let err_msg = format!(
                "Agent reached maximum step limit ({}) without completing the task",
                max_steps
            );
            let _ = db.update_run_status(
                run_id,
                "failed",
                None,
                Some(chrono::Utc::now().timestamp_millis()),
                Some(&err_msg),
            );
            let seq = db
                .get_next_event_seq(run_id)
                .unwrap_or(max_steps as i64 + 1);
            let err_event = RunEvent {
                id: format!("evt_{}_{}", run_id, seq),
                run_id: run_id.to_string(),
                seq,
                event_type: "error".to_string(),
                payload_json: serde_json::json!({
                    "step": max_steps,
                    "error": &err_msg,
                    "reason": "step_limit_exceeded"
                })
                .to_string(),
                created_at: chrono::Utc::now().timestamp_millis(),
            };
            let _ = db.append_run_event(&err_event);
            return Err(err_msg);
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
        profile: &ProfileSnapshot,
        base_url: Option<&str>,
        reasoning_sink: Option<ReasoningSink>,
    ) -> (String, bool, Option<String>) {
        let canonical_name = canonical_tool_name(&tc.name);

        // Security boundary: enforce profile-level tool restrictions
        if !profile.tool_allowed(&canonical_name) {
            log::warn!(
                "[Security] Tool '{}' (canonical: '{}') rejected by agent '@{}' profile policy (mode: '{}')",
                tc.name,
                canonical_name,
                profile.name,
                profile.tools.mode
            );
            return (
                format!(
                    "Tool '{}' is not permitted by agent '@{}' profile policy.",
                    tc.name, profile.name
                ),
                true,
                None,
            );
        }

        if canonical_name == "delegate_task" && !profile.delegation.enabled {
            return (
                format!(
                    "Delegation rejected: agent '@{}' has delegation disabled in its profile.",
                    profile.name
                ),
                true,
                None,
            );
        }

        let (configured_workspace, requires_isolated_worktree, persisted_task_grant) =
            if let Ok(Some(r)) = db.get_run(run_id) {
                if let Some(ref cfg_str) = r.config_json {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(cfg_str) {
                        let iso_path = v
                            .get("isolated_worktree_path")
                            .and_then(|s| s.as_str())
                            .map(PathBuf::from);
                        let ws_path = v
                            .get("workspace_root")
                            .and_then(|s| s.as_str())
                            .map(PathBuf::from);
                        let is_isolated = v
                            .get("isolated_worktree")
                            .map(|b| b.as_bool().unwrap_or(true))
                            .unwrap_or(iso_path.is_some());
                        let grant: Option<crate::governance::TaskAuthorizationGrant> = v
                            .get("task_grant")
                            .and_then(|g| serde_json::from_value(g.clone()).ok());

                        if let Some(p) = iso_path {
                            (Some(p), true, grant)
                        } else if let Some(p) = ws_path {
                            (Some(p), is_isolated, grant)
                        } else {
                            (None, false, grant)
                        }
                    } else {
                        (None, false, None)
                    }
                } else {
                    (None, false, None)
                }
            } else {
                (None, false, None)
            };

        // Fail-closed workspace containment: if isolation was requested, path MUST exist.
        // Never silently fall back to host project root or default workspace!
        let workspace_root = if requires_isolated_worktree {
            match configured_workspace {
                Some(ref root) if root.exists() => configured_workspace,
                Some(ref root) => {
                    log::error!(
                        "[Security] Isolated workspace {:?} does not exist. Halting execution to prevent fallback.",
                        root
                    );
                    return (
                        format!(
                            "Execution failed: requested isolated workspace {:?} does not exist. Halting task to prevent unauthorized execution in project root.",
                            root
                        ),
                        true,
                        None,
                    );
                }
                None => {
                    return (
                        "Execution failed: task requires an isolated worktree, but no worktree path was configured.".to_string(),
                        true,
                        None,
                    );
                }
            }
        } else {
            configured_workspace
                .or_else(|| {
                    db.get_setting("workspace_root")
                        .ok()
                        .flatten()
                        .map(PathBuf::from)
                })
                .or_else(|| dirs::data_dir().map(|d| d.join("kyro").join("workspace")))
        };

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
            task_grant: persisted_task_grant,
        };

        // Bug 9: preserve original grant provenance on context so AuthorizationEngine
        // can distinguish between none issued, active, invalid/expired, and failed validation.

        // The shell analysis is computed once, before authorisation, so that the
        // exact argument vector that was inspected is also the argument vector
        // that is executed.  Authorisation and execution can therefore never
        // disagree about what runs.
        let shell_analysis = if matches!(tc.name.as_str(), "execute_shell" | "bash" | "terminal") {
            let policy = crate::governance::PolicyEngine::load(db);
            Some(AuthorizationEngine::analyze_shell_command_in_context(
                &policy,
                Some(&ctx),
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
                    profile,
                    base_url,
                    reasoning_sink.clone(),
                    Some(crate::governance::TaskAuthorizationGrant::execution_roots(
                        &ctx,
                    )),
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
                                    profile,
                                    base_url,
                                    reasoning_sink.clone(),
                                    Some(
                                        crate::governance::TaskAuthorizationGrant::execution_roots(
                                            &ctx,
                                        ),
                                    ),
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
                    profile,
                    base_url,
                    reasoning_sink.clone(),
                    Some(crate::governance::TaskAuthorizationGrant::execution_roots(
                        &ctx,
                    )),
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
                    profile,
                    base_url,
                    reasoning_sink.clone(),
                    Some(crate::governance::TaskAuthorizationGrant::execution_roots(
                        &ctx,
                    )),
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
        profile: &ProfileSnapshot,
        base_url: Option<&str>,
        reasoning_sink: Option<ReasoningSink>,
        authorized_roots: Option<Vec<std::path::PathBuf>>,
    ) -> (String, bool, Option<String>) {
        // Filesystem execution uses exactly the canonical roots that
        // authorization validated (Bug 14); with none available the
        // operation fails closed through the empty-roots validator.
        let file_operation_roots: Vec<PathBuf> = authorized_roots
            .or_else(|| workspace_root.map(|ws| vec![ws.to_path_buf()]))
            .unwrap_or_default();

        // Bug 9: Revalidate task grant immediately prior to executing privileged operations
        if matches!(
            tc.name.as_str(),
            "execute_shell"
                | "bash"
                | "terminal"
                | "read_file"
                | "write_file"
                | "organize_files_execute"
        ) && auth_proof.is_none()
        {
            let check_ctx = ExecutionContext {
                run_id: run_id.to_string(),
                agent_id: profile.id.clone(),
                conversation_id: None,
                workspace_root: workspace_root.map(|p| p.to_path_buf()),
                task_grant: None,
            };
            match AuthorizationEngine::resolve_task_grant(db, &check_ctx) {
                crate::governance::TaskGrantResolution::ValidationFailed { reason, .. } => {
                    return (
                        format!(
                            "Privileged operation denied: task grant validation failed: {}",
                            reason
                        ),
                        true,
                        None,
                    );
                }
                crate::governance::TaskGrantResolution::InvalidOrExpired { reason, .. } => {
                    return (
                        format!(
                            "Privileged operation denied: task grant expired or revoked immediately prior to execution: {}",
                            reason
                        ),
                        true,
                        None,
                    );
                }
                _ => {}
            }
        }
        match tc.name.as_str() {
            "execute_shell" => {
                let cmd = tc.arguments["command"].as_str().unwrap_or_default();
                let use_sandbox =
                    force_sandbox || tc.arguments["use_sandbox"].as_bool().unwrap_or(false);

                if use_sandbox {
                    let image = sandbox_image.unwrap_or_else(|| "alpine:3.20".to_string());
                    match DockerSandbox::run_in_sandbox_in_dir(&image, cmd, workspace_root) {
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

                    if let Some(root) = workspace_root {
                        cmd_builder.current_dir(root);
                    }

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
                // The same canonical roots that authorization validated are
                // what execution confines to (Bug 14): workspace root plus
                // the grant's authorized folders.
                match AuthorizationEngine::secure_read_contained_roots(&file_operation_roots, path)
                {
                    Ok(content) => (content, false, None),
                    Err(e) => (format!("Filesystem access denied: {}", e), true, None),
                }
            }
            "write_file" => {
                let path = tc.arguments["path"].as_str().unwrap_or_default();
                let content = tc.arguments["content"].as_str().unwrap_or_default();
                let safe_path = match AuthorizationEngine::secure_write_contained_roots(
                    &file_operation_roots,
                    path,
                    content.as_bytes(),
                ) {
                    Ok(p) => p,
                    Err(e) => return (format!("Filesystem access denied: {}", e), true, None),
                };

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
                match db.create_artifact(&rec) {
                    Ok(_) => (
                        format!(
                            "Successfully wrote {} bytes to '{}'",
                            content.len(),
                            safe_path.display()
                        ),
                        false,
                        None,
                    ),
                    Err(err) => (
                        format!(
                            "Partial success: Wrote {} bytes to '{}', but failed to persist artifact metadata in database: {}. File can be recovered via artifact reconciliation.",
                            content.len(),
                            safe_path.display(),
                            err
                        ),
                        false,
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
                    // Execution-boundary recheck (Bug 10): authorization and
                    // dispatch use the same trusted label. A mark whose label
                    // is missing, empty, or consequential at click time is
                    // refused here — coordinates are never clicked without a
                    // classifiable semantic target.
                    let label_ok = match SetOfMarksEngine::resolve_mark_semantics(mark_id) {
                        Some(sem) if !sem.label.is_empty() => {
                            !crate::governance::is_consequential_control_text(&sem.combined)
                        }
                        _ => false,
                    };
                    if !label_ok {
                        return (
                            format!(
                                "Mark #{} cannot be clicked: its trusted semantic label is missing or consequential in the current grounded session.",
                                mark_id
                            ),
                            true,
                            None,
                        );
                    }
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
            "run_safe_experiment" => {
                let name = tc.arguments["name"]
                    .as_str()
                    .unwrap_or("unnamed experiment");
                let runtime = tc.arguments["runtime"].as_str().unwrap_or("python");
                let script = tc.arguments["script"].as_str().map(|s| s.to_string());
                let ws_path = tc.arguments["workspace_path"]
                    .as_str()
                    .map(|s| s.to_string());
                let timeout_seconds = tc.arguments["timeout_seconds"].as_u64().unwrap_or(60);

                let spec = crate::sandbox::experiment::ExperimentSpec {
                    id: None,
                    name: name.to_string(),
                    runtime: runtime.to_string(),
                    custom_image: None,
                    script,
                    argv: None,
                    workspace_path: ws_path,
                    network_mode: Some("none".to_string()),
                    limits: Some(crate::sandbox::experiment::ResourceLimits {
                        timeout_seconds,
                        ..Default::default()
                    }),
                };

                let runner = crate::sandbox::experiment::ExperimentRunner::new(db.clone());
                match runner.execute(spec, Some(run_id), token.clone()).await {
                    Ok(rec) => {
                        let is_err = rec.status != "succeeded";
                        let summary = format!(
                            "Experiment '{}' status: {}.\nExit code: {:?}\nArtifacts: {}\nSTDOUT:\n{}\nSTDERR:\n{}",
                            rec.name,
                            rec.status,
                            rec.exit_code,
                            rec.artifact_ids_json.as_deref().unwrap_or("[]"),
                            rec.stdout_tail.as_deref().unwrap_or(""),
                            rec.stderr_tail.as_deref().unwrap_or("")
                        );
                        (summary, is_err, None)
                    }
                    Err(e) => (
                        format!("Failed to execute safe experiment: {}", e),
                        true,
                        None,
                    ),
                }
            }
            "delegate_task" => {
                let target = tc.arguments["target_bot_id"].as_str().unwrap_or_default();
                let objective = tc.arguments["objective"].as_str().unwrap_or_default();
                let context_summary = tc.arguments["context_summary"].as_str().unwrap_or_default();

                if objective.trim().is_empty() {
                    return (
                        "Delegation rejected: 'objective' must describe the sub-task for the teammate.".to_string(),
                        true,
                        None,
                    );
                }
                if target.trim().is_empty() {
                    return (
                        "Delegation rejected: 'target_bot_id' must identify a registered teammate agent.".to_string(),
                        true,
                        None,
                    );
                }

                // ---- Stage D: durable delegation governance ----------------
                // (1) The acting profile decides whether it may delegate, and
                //     its manifest caps both chain depth and fan-out.
                if !profile.delegation.enabled {
                    return (
                        format!(
                            "Delegation rejected: agent '@{}' has delegation disabled in its profile.",
                            profile.name
                        ),
                        true,
                        None,
                    );
                }

                // (2) Depth is walked from the durable agent_runs parent links,
                //     never parsed out of run id strings.
                let current_depth = db.delegation_depth(run_id).min(MAX_DELEGATION_DEPTH_WALK);
                let max_depth = profile.delegation.max_depth as usize;
                let new_depth = current_depth + 1;
                if new_depth > max_depth {
                    return (
                        format!(
                            "Delegation rejected: max delegation depth of {} exceeded (this task would run at depth {}); complete the work yourself instead.",
                            max_depth, new_depth
                        ),
                        true,
                        None,
                    );
                }

                // (3) The fan-out budget counts every child this run has ever
                //     created, so it survives restarts and is immune to races
                //     with the roster status map.
                let child_count = db.count_child_runs(run_id).unwrap_or(0);
                let max_children = profile.delegation.max_children as i64;
                if child_count >= max_children {
                    return (
                        format!(
                            "Delegation rejected: budget of {} delegated tasks per run exhausted ({} already created).",
                            max_children, child_count
                        ),
                        true,
                        None,
                    );
                }

                // (4) The target must be a registered, enabled teammate.
                let target_profile = {
                    let mut registry = self.registry.write().await;
                    registry.ensure_fresh();
                    match registry.resolve(target) {
                        Ok(s) => s.clone(),
                        Err(ResolveError::UnknownProfile(id)) => {
                            return (
                                format!(
                                    "No registered teammate agent matches '{}'. Available specialist agents are listed in the agent roster.",
                                    id
                                ),
                                true,
                                None,
                            )
                        }
                        Err(ResolveError::DisabledProfile(id)) => {
                            return (
                                format!(
                                    "Teammate agent '{}' is disabled and cannot accept delegated tasks.",
                                    id
                                ),
                                true,
                                None,
                            )
                        }
                        Err(ResolveError::InvalidProfile(id, reason)) => {
                            return (
                                format!(
                                    "Teammate agent '{}' cannot accept delegation: invalid profile ({})",
                                    id, reason
                                ),
                                true,
                                None,
                            )
                        }
                    }
                };

                // (5) Delegation cycle detection: prevent self-delegation and ancestor cycles
                if target_profile.id == profile.id {
                    return (
                        format!(
                            "Delegation rejected: self-delegation detected; agent '@{}' cannot delegate to itself.",
                            profile.name
                        ),
                        true,
                        None,
                    );
                }
                {
                    let mut ancestor_run_id = Some(run_id.to_string());
                    let mut ancestors = std::collections::HashSet::new();
                    ancestors.insert(profile.id.clone());
                    while let Some(pid) = ancestor_run_id {
                        if let Ok(Some(parent_run)) = db.get_run(&pid) {
                            ancestors.insert(parent_run.agent_id.clone());
                            ancestor_run_id = parent_run.parent_run_id;
                        } else {
                            break;
                        }
                    }
                    if ancestors.contains(&target_profile.id) {
                        return (
                            format!(
                                "Delegation rejected: cycle detected; agent '@{}' is already an active ancestor in the delegation chain.",
                                target_profile.name
                            ),
                            true,
                            None,
                        );
                    }
                }

                // Create child run record with unique ID
                let child_run_id = format!(
                    "{}_child_{}_{}",
                    run_id,
                    target_profile.id,
                    &uuid::Uuid::new_v4().to_string()[..8]
                );
                let now = chrono::Utc::now().timestamp_millis();
                let parent_model = db
                    .get_run(run_id)
                    .ok()
                    .flatten()
                    .map(|r| r.model)
                    .unwrap_or_else(|| "nvidia/nemotron-3-ultra-550b-a55b".to_string());

                // Respect profile model settings
                let child_model = match target_profile.model.mode.as_str() {
                    "fixed" => target_profile
                        .model
                        .model
                        .clone()
                        .unwrap_or_else(|| parent_model.clone()),
                    "inherit" => parent_model.clone(),
                    _ => parent_model.clone(),
                };

                let run_snapshot = RunConfigSnapshot {
                    profile: target_profile.clone(),
                    effective_model: child_model.clone(),
                    workspace_root: workspace_root.map(|p| p.to_string_lossy().to_string()),
                    isolated_worktree_path: None,
                    isolated_worktree: None,
                    base_commit: None,
                    workflow_id: None,
                    task_id: None,
                    attempt: None,
                    parent_run_id: Some(run_id.to_string()),
                    parent_agent: Some(profile.id.clone()),
                    delegation_depth: Some(new_depth),
                    context_summary: if context_summary.trim().is_empty() {
                        None
                    } else {
                        Some(context_summary.to_string())
                    },
                    task_grant: None,
                };

                let child_run = AgentRun {
                    id: child_run_id.clone(),
                    conversation_id: db
                        .get_run(run_id)
                        .ok()
                        .flatten()
                        .and_then(|r| r.conversation_id),
                    // Canonical profile id so the child run resolves exactly
                    // the target's profile even when the caller used an alias.
                    agent_id: target_profile.id.clone(),
                    model: child_model.clone(),
                    prompt: objective.to_string(),
                    status: "running".to_string(),
                    created_at: now,
                    started_at: Some(now),
                    completed_at: None,
                    error: None,
                    config_json: Some(serde_json::to_string(&run_snapshot).unwrap_or_default()),
                    parent_run_id: Some(run_id.to_string()),
                };

                if let Err(e) = db.create_run(&child_run) {
                    log::error!(
                        "[Delegation] Failed to persist child run record {}: {}",
                        child_run_id,
                        e
                    );
                    return (
                        format!(
                            "Delegation failed: unable to persist durable child run: {}",
                            e
                        ),
                        true,
                        None,
                    );
                }

                // Track the teammate's live task in the roster; the guard
                // clears exactly this (teammate, run) pair on every exit path.
                MultiBotOrchestrator::set_bot_task(
                    &target_profile.id,
                    &child_run_id,
                    Some(objective),
                );
                struct BotRosterGuard {
                    bot_id: String,
                    run_id: String,
                }
                impl Drop for BotRosterGuard {
                    fn drop(&mut self) {
                        MultiBotOrchestrator::set_bot_task(&self.bot_id, &self.run_id, None);
                    }
                }
                let _roster_guard = BotRosterGuard {
                    bot_id: target_profile.id.clone(),
                    run_id: child_run_id.clone(),
                };

                // The child composes its own system prompt from its resolved
                // profile (Stage C); the delegating agent supplies only the
                // objective and context.
                let child_task_message = if context_summary.trim().is_empty() {
                    objective.to_string()
                } else {
                    format!(
                        "{}\n\nContext from the delegating agent (@{}):\n{}",
                        objective, profile.name, context_summary
                    )
                };

                let child_history = vec![ChatMessage {
                    role: "user".to_string(),
                    content: MessageContent::from_text(child_task_message),
                    tool_calls: None,
                    tool_call_id: None,
                }];

                let child_sys_prompt =
                    "The operating prompt for this delegated task is resolved from the teammate agent's profile."
                        .to_string();

                // Execute child agent turn with linked child cancellation token
                let child_token = token.child_token();
                let keys = crate::storage::Vault::new(db.clone())
                    .get_all_keys()
                    .unwrap_or_default();

                // Resolve the child's own model route, credentials, and endpoint (Bug 15)
                let child_route =
                    match crate::agent::router::ModelRouter::resolve_model_route(&child_model) {
                        Ok(r) => r,
                        Err(e) => {
                            return (
                                format!(
                                    "Delegation failed: unsupported child model '{}': {}",
                                    child_model, e
                                ),
                                true,
                                None,
                            );
                        }
                    };

                let (caller_api_key, caller_model) = {
                    let map = self.active_runs.lock().await;
                    map.get(run_id)
                        .map(|ctx| (ctx.api_key.clone(), ctx.model.clone()))
                        .unwrap_or_default()
                };

                let parent_route =
                    crate::agent::router::ModelRouter::resolve_model_route(&caller_model).ok();
                let parent_is_test_mock = base_url
                    .map(|ep| ep.contains("127.0.0.1") || ep.contains("localhost"))
                    .unwrap_or(false);

                let child_api_key =
                    match crate::agent::router::ModelRouter::select_api_key_for_route(
                        &child_route,
                        &keys,
                    ) {
                        Ok(k) => k,
                        Err(e) => {
                            let same_provider = parent_route
                                .as_ref()
                                .map(|pr| pr.provider == child_route.provider)
                                .unwrap_or(false);
                            if (same_provider || parent_is_test_mock)
                                && !caller_api_key.trim().is_empty()
                            {
                                caller_api_key
                            } else {
                                return (
                                    format!(
                                        "Delegation failed: missing credential for child agent model '{}': {}",
                                        child_model, e
                                    ),
                                    true,
                                    None,
                                );
                            }
                        }
                    };

                // Explicit test mock endpoints are preserved across delegated children;
                // otherwise the child model resolves to its own provider route endpoint (Bug 15)
                let child_base_url = if let Some(parent_ep) = base_url {
                    if parent_is_test_mock || parent_ep == child_route.base_url {
                        Some(parent_ep.to_string())
                    } else {
                        Some(child_route.base_url.clone())
                    }
                } else {
                    Some(child_route.base_url.clone())
                };

                let child_reasoning_sink = reasoning_sink.clone();
                let res = Box::pin(self.run_step_extended(
                    &child_run_id,
                    &child_model,
                    &child_api_key,
                    child_base_url.as_deref(),
                    &child_sys_prompt,
                    child_history,
                    db.clone(),
                    mcp_manager.cloned(),
                    connector_manager.cloned(),
                    Some(child_token),
                    |_| {},
                    move |evt| {
                        if let Some(ref sink) = child_reasoning_sink {
                            if let Ok(mut guard) = sink.lock() {
                                guard(evt);
                            }
                        }
                    },
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
                                target_profile.name, out
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
                                target_profile.name, e
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
            "browser_inspect_element" => {
                let outcome = tools::browser_inspect_element(&tc.arguments, &self.browser).await;
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
            "organize_files_preview" => {
                let outcome = tools::organize_files_preview(&tc.arguments, db).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "organize_files_execute" => {
                let outcome = tools::organize_files_execute(&tc.arguments, db).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "organize_files_undo" => {
                let outcome = tools::organize_files_undo(&tc.arguments, db).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "analyze_document" => {
                let outcome = tools::analyze_document(&tc.arguments).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "compare_documents" => {
                let outcome = tools::compare_documents(&tc.arguments).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "create_code_checkpoint" => {
                let outcome = tools::create_code_checkpoint(&tc.arguments).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "rollback_code_checkpoint" => {
                let outcome = tools::rollback_code_checkpoint(&tc.arguments).await;
                (outcome.message, outcome.is_error, outcome.visual)
            }
            "diagnose_build_failure" => {
                let outcome = tools::diagnose_build_failure(&tc.arguments).await;
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
        let project_path_opt = args["project_path"]
            .as_str()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);
        let project_name_opt = args["project_name"]
            .as_str()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        if project_path_opt.is_none() && project_name_opt.is_none() {
            return ToolOutcome::fail(
                "audit_ui requires either 'project_path' (direct path) or 'project_name' to be specified.",
            );
        }

        let project_name = project_name_opt.unwrap_or_else(|| {
            project_path_opt
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default()
        });

        let search_root = if project_path_opt.is_some() && args["search_root"].as_str().is_none() {
            None
        } else {
            match tools::resolve_search_root(args) {
                Ok(r) => Some(r),
                Err(e) => return ToolOutcome::fail(e),
            }
        };

        let browser = args["browser"]
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("auto"));

        let default_headless = self.browser.default_headless().await;
        let headless = args["headless"].as_bool().unwrap_or(default_headless);

        let options = crate::audit::workflow::AuditOptions {
            project_name,
            project_path: project_path_opt,
            search_root,
            browser,
            headless,
            keep_server_running: args["keep_server_running"].as_bool().unwrap_or(false),
            ..Default::default()
        };

        let (run_model, run_api_key, run_base_url) = {
            let map = self.active_runs.lock().await;
            if let Some(ctx) = map.get(run_id) {
                (
                    Some(ctx.model.clone()),
                    Some(ctx.api_key.clone()),
                    ctx.base_url.clone(),
                )
            } else {
                (None, None, None)
            }
        };

        let mut workflow = crate::audit::workflow::UiAuditWorkflow::new(
            self.browser.clone(),
            self.dev_servers.clone(),
        );

        if let (Some(ref m), Some(ref k)) = (&run_model, &run_api_key) {
            let analyzer = Arc::new(crate::agent::vision::VisualAnalyzer::new(
                Arc::new(self.router.clone()),
                k.clone(),
                run_base_url,
            ));
            workflow = workflow.with_visual_analyzer(analyzer, Some(m.clone()));
        }

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
            conversation_id: conv_id.clone(),
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

        // Store all screenshots from outcome through Kyro's artifact registry
        let mut screenshot_artifact_ids: Vec<String> = Vec::new();
        for (shot_name, b64_png) in &outcome.screenshots {
            use base64::Engine;
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64_png) {
                let shot_art_id = format!("art_{}", uuid::Uuid::new_v4());
                let shot_filename = format!("{}_{}", &shot_art_id[4..12], shot_name);
                let shot_path = artifacts_dir.join(&shot_filename);
                if std::fs::write(&shot_path, &bytes).is_ok() {
                    let shot_rec = crate::storage::ArtifactRecord {
                        id: shot_art_id.clone(),
                        run_id: Some(run_id.to_string()),
                        conversation_id: conv_id.clone(),
                        name: shot_name.clone(),
                        file_path: shot_path.to_string_lossy().to_string(),
                        artifact_type: "image".to_string(),
                        mime_type: "image/png".to_string(),
                        size_bytes: bytes.len() as i64,
                        created_at: chrono::Utc::now().timestamp_millis(),
                        metadata_json: Some(
                            serde_json::json!({
                                "route": shot_name,
                                "source": "ui_audit",
                            })
                            .to_string(),
                        ),
                    };
                    let _ = db.create_artifact(&shot_rec);
                    screenshot_artifact_ids.push(shot_art_id);
                }
            }
        }

        let payload = serde_json::json!({
            "status": "success",
            "report_artifact_id": artifact_id,
            "report_path": report_path.to_string_lossy(),
            "screenshot_artifact_ids": screenshot_artifact_ids,
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
