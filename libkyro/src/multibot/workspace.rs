//! Developer Multi-Agent Workspace: a registry-driven team workflow scheduler.
//!
//! A workflow is a Markdown manifest (see `resources/workflows/`) describing a
//! DAG of delegated agent runs. Tasks reference agent profiles by id, resolved
//! and validated through the [`AgentRegistry`], never through hardcoded role
//! lists. Execution is a bounded-concurrency scheduler with durable
//! per-workspace persistence, per-task retry rules, Git-worktree isolation and
//! reviewed reconciliation — nothing is merged into the user's tree
//! automatically.

use crate::agent::cancellation::CancellationToken;
use crate::agent::registry::AgentRegistry;
use crate::storage::{AgentRun, Database};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_yaml_ng as yaml;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use crate::multibot::group::MultiBotOrchestrator;

/// Maximum number of concurrent team runs a workspace may schedule.
const MAX_WORKFLOW_CONCURRENCY: u32 = 8;

/// Template placeholder replaced with the workspace's primary objective.
pub const OBJECTIVE_PLACEHOLDER: &str = "{{objective}}";

/// What happens when a required task exhausts its retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FailurePolicy {
    /// Stop the workspace; pending tasks are cancelled.
    #[default]
    Abort,
    /// Keep running independent branches; dependents of the failed task are skipped.
    Continue,
}

/// One planned stage of a workflow: a delegated child run for one agent profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowTaskSpec {
    /// Unique id inside the workflow; also used for worktree/file naming.
    pub task_id: String,
    /// The agent profile that runs this stage (resolved via the registry).
    pub agent_id: String,
    /// Objective handed to the agent; `{{objective}}` is replaced by the
    /// workspace's primary objective.
    pub objective: String,
    /// Task ids that must complete (or be tolerably skipped) first.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// A permanently-failed optional task never aborts the workspace.
    #[serde(default)]
    pub optional: bool,
    /// Run inside an isolated Git worktree (or shadow copy) and leave the
    /// diff for reviewed reconciliation.
    #[serde(default)]
    pub isolated_worktree: bool,
    /// Retry count after the first attempt (0 = single attempt).
    #[serde(default)]
    pub retries: u32,
}

/// A team workflow: validated DAG metadata plus its tasks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowSpec {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
    #[serde(default)]
    pub on_failure: FailurePolicy,
    pub tasks: Vec<WorkflowTaskSpec>,
}

fn default_concurrency() -> u32 {
    2
}

/// One scheduled/running stage of a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpecialistTask {
    pub task_id: String,
    pub agent_id: String,
    /// The resolved objective (template already substituted).
    pub objective: String,
    pub depends_on: Vec<String>,
    /// Optional; a permanently failed optional task never aborts.
    #[serde(default)]
    pub optional: bool,
    /// Run inside an isolated Git worktree (or shadow copy) and leave the
    /// diff for reviewed reconciliation.
    #[serde(default)]
    pub isolated_worktree: bool,
    /// Retry budget: `retries + 1` attempts total.
    pub max_attempts: u32,
    /// Attempts consumed so far.
    #[serde(default)]
    pub attempts: u32,
    /// "pending", "running", "completed", "failed", "skipped", "cancelled"
    pub status: String,
    pub child_run_id: Option<String>,
    pub isolated_worktree_path: Option<PathBuf>,
    pub worktree_branch: Option<String>,
    #[serde(default)]
    pub base_commit: Option<String>,
    pub output: Option<String>,
    pub error: Option<String>,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

/// A multi-agent workspace coordinating delegated child runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiAgentWorkspace {
    pub workspace_id: String,
    pub parent_run_id: String,
    pub project_path: PathBuf,
    pub workflow_id: String,
    pub workflow_name: String,
    pub primary_objective: String,
    pub concurrency: u32,
    /// Legacy docs (pre-failure-policy) are treated as abort-on-failure.
    #[serde(default)]
    pub on_failure: FailurePolicy,
    pub tasks: Vec<SpecialistTask>,
    /// "active", "completed", "failed", "failed_partial", "cancelled", "reconciled"
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Outcome of reconciling isolated worktree changes back into the parent project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationOutcome {
    pub workspace_id: String,
    pub changes_detected: bool,
    pub modified_files: Vec<String>,
    pub unified_diff: String,
    pub conflicts_detected: Vec<String>,
    pub can_safely_merge: bool,
    pub reconciliation_summary: String,
}

/// Bundled workflow manifests, embedded at compile time.
pub const BUNDLED_WORKFLOWS: &[(&str, &str)] = &[(
    "software-change",
    include_str!("../../resources/workflows/software-change.md"),
)];

/// Legacy `SpecialistRole` values from the pre-DAG scheduler mapped onto the
/// agent profiles that replace them. Migration only; never used for new plans.
fn legacy_role_to_agent(role: &str) -> Option<&'static str> {
    match role {
        "repository_investigator" => Some("forge-devops"),
        "implementation_engineer" => Some("forge-devops"),
        "test_regression_specialist" => Some("forge-devops"),
        "code_reviewer" => Some("apex-orchestrator"),
        "documentation_reviewer" => Some("forge-devops"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Workflow manifest parsing
// ---------------------------------------------------------------------------

/// Parse a workflow manifest (YAML frontmatter describing the DAG + a
/// documentation body) into a validated [`WorkflowSpec`].
pub fn parse_workflow_manifest(source: &str) -> Result<WorkflowSpec, String> {
    let trimmed = source.trim();
    let rest = trimmed
        .strip_prefix("---\n")
        .or_else(|| trimmed.strip_prefix("---\r\n"))
        .ok_or_else(|| "Workflow manifest must start with a '---' frontmatter block".to_string())?;
    let (frontmatter, body) = rest
        .split_once("\n---")
        .ok_or_else(|| "Workflow manifest must close its frontmatter with '---'".to_string())?;
    let body = body.trim_start_matches(['-', '\r', '\n']).trim_start();

    let spec: WorkflowSpec =
        yaml::from_str(frontmatter).map_err(|e| format!("Invalid workflow frontmatter: {}", e))?;

    if body.trim().is_empty() {
        return Err(format!(
            "Workflow '{}' must document itself below the frontmatter",
            spec.id
        ));
    }

    validate_workflow(&spec)?;
    Ok(spec)
}

/// Structural validation of a workflow DAG. Detects duplicate/missing ids,
/// malformed names, non-empty objectives, duplicate or self dependencies and
/// cycles (Kahn's algorithm).
pub fn validate_workflow(spec: &WorkflowSpec) -> Result<(), String> {
    if spec.id.trim().is_empty() {
        return Err("Workflow id must not be empty".to_string());
    }
    if !id_component_ok(&spec.id) {
        return Err(format!(
            "Workflow id '{}' must be lowercase letters, digits, '-' or '_'",
            spec.id
        ));
    }
    if spec.tasks.is_empty() {
        return Err("Workflow must define at least one task".to_string());
    }
    if !(1..=MAX_WORKFLOW_CONCURRENCY).contains(&spec.concurrency) {
        return Err(format!(
            "Workflow concurrency must be between 1 and {}",
            MAX_WORKFLOW_CONCURRENCY
        ));
    }

    let mut ids: HashSet<&str> = HashSet::new();
    for task in &spec.tasks {
        if task.task_id.trim().is_empty() {
            return Err("Task id must not be empty".to_string());
        }
        if !id_component_ok(&task.task_id) {
            return Err(format!(
                "Task id '{}' must be lowercase letters, digits, '-' or '_' (it is used in worktree paths)",
                task.task_id
            ));
        }
        if task.agent_id.trim().is_empty() {
            return Err(format!("Task '{}' has no agent_id", task.task_id));
        }
        if !id_component_ok(&task.agent_id) {
            return Err(format!(
                "Task '{}' references malformed agent id '{}'",
                task.task_id, task.agent_id
            ));
        }
        if task.objective.trim().is_empty() {
            return Err(format!("Task '{}' has an empty objective", task.task_id));
        }
        if !ids.insert(&task.task_id) {
            return Err(format!("Duplicate task id '{}' in workflow", task.task_id));
        }
        let mut deps: HashSet<&str> = HashSet::new();
        for dep in &task.depends_on {
            if !deps.insert(dep.as_str()) {
                return Err(format!(
                    "Task '{}' declares dependency '{}' twice",
                    task.task_id, dep
                ));
            }
            if dep == &task.task_id {
                return Err(format!("Task '{}' depends on itself", task.task_id));
            }
        }
    }
    for task in &spec.tasks {
        for dep in &task.depends_on {
            if !ids.contains(dep.as_str()) {
                return Err(format!(
                    "Task '{}' depends on unknown task '{}'",
                    task.task_id, dep
                ));
            }
        }
    }

    // Cycle detection via topological order computation.
    let _ = topological_order(spec)
        .map_err(|e| format!("Workflow DAG is not a valid acyclic schedule: {}", e))?;
    Ok(())
}

/// Kahn topological order over the workflow tasks. Deterministic: ties keep
/// declaration order.
pub fn topological_order(spec: &WorkflowSpec) -> Result<Vec<String>, String> {
    let mut indegree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    for task in &spec.tasks {
        indegree.entry(task.task_id.as_str()).or_insert(0);
        for dep in &task.depends_on {
            *indegree.entry(task.task_id.as_str()).or_insert(0) += 1;
            dependents
                .entry(dep.as_str())
                .or_default()
                .push(&task.task_id);
        }
    }

    let mut queue: VecDeque<&str> = VecDeque::new();
    for task in &spec.tasks {
        if indegree.get(task.task_id.as_str()).copied().unwrap_or(0) == 0 {
            queue.push_back(&task.task_id);
        }
    }

    let mut order = Vec::with_capacity(spec.tasks.len());
    while let Some(id) = queue.pop_front() {
        order.push(id.to_string());
        if let Some(children) = dependents.get(id) {
            for child in children {
                let e = indegree.get_mut(*child).unwrap();
                *e -= 1;
                if *e == 0 {
                    queue.push_back(child);
                }
            }
        }
    }

    if order.len() != spec.tasks.len() {
        let mut cyclic: Vec<&str> = spec
            .tasks
            .iter()
            .map(|t| t.task_id.as_str())
            .collect::<Vec<_>>();
        let done: HashSet<&str> = order.iter().map(|s| s.as_str()).collect();
        cyclic.retain(|id| !done.contains(id));
        return Err(format!("cycle through tasks: {}", cyclic.join(", ")));
    }
    Ok(order)
}

/// Shape rule for workflow and agent ids: lowercase letters, digits, `-` or
/// `_`, 1..=64 chars. Exported so the agent-profile command layer reuses the
/// exact same identifier contract.
pub fn id_component_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Bundled factory workflow content for a workflow id.
pub fn builtin_workflow_content(id: &str) -> Option<&'static str> {
    BUNDLED_WORKFLOWS
        .iter()
        .find(|(bid, _)| *bid == id)
        .map(|(_, c)| *c)
}

/// Load a workflow by id: a user override file
/// (`<config>/kyro/agents/workflows/<id>.md`) wins, otherwise the bundled
/// factory manifest is used.
pub fn load_workflow(config_root: &Path, id: &str) -> Result<WorkflowSpec, String> {
    if !id_component_ok(id) {
        return Err(format!("Invalid workflow id '{}'", id));
    }
    let override_path = config_root
        .join("agents")
        .join("workflows")
        .join(format!("{}.md", id));
    let source = if override_path.is_file() {
        fs::read_to_string(&override_path).map_err(|e| {
            format!(
                "Failed to read workflow override {:?}: {}",
                override_path, e
            )
        })?
    } else {
        builtin_workflow_content(id)
            .ok_or_else(|| format!("Unknown workflow '{}'", id))?
            .to_string()
    };
    parse_workflow_manifest(&source)
}

// ---------------------------------------------------------------------------
// Workspace construction + registry validation
// ---------------------------------------------------------------------------

pub struct MultiAgentOrchestrator;

impl MultiAgentOrchestrator {
    /// Build a runnable workspace from a validated workflow spec: task
    /// objectives are templated with the primary objective, retry budgets are
    /// encoded, and every task starts pending.
    pub fn create_workspace(
        parent_run_id: &str,
        project_path: &Path,
        spec: &WorkflowSpec,
        primary_objective: &str,
    ) -> Result<MultiAgentWorkspace, String> {
        validate_workflow(spec)?;
        let objective = primary_objective.trim().to_string();
        if objective.is_empty() {
            return Err("Workspace primary objective must not be empty".to_string());
        }
        let now = chrono::Utc::now().timestamp_millis();
        let tasks = spec
            .tasks
            .iter()
            .map(|t| SpecialistTask {
                task_id: t.task_id.clone(),
                agent_id: t.agent_id.clone(),
                objective: if t.objective.contains(OBJECTIVE_PLACEHOLDER) {
                    t.objective.replace(OBJECTIVE_PLACEHOLDER, &objective)
                } else {
                    format!("{}. Primary objective: {}", t.objective, objective)
                },
                depends_on: t.depends_on.clone(),
                optional: t.optional,
                isolated_worktree: t.isolated_worktree,
                max_attempts: t.retries.saturating_add(1),
                attempts: 0,
                status: "pending".to_string(),
                child_run_id: None,
                isolated_worktree_path: None,
                worktree_branch: None,
                base_commit: None,
                output: None,
                error: None,
                started_at: None,
                completed_at: None,
            })
            .collect();

        Ok(MultiAgentWorkspace {
            workspace_id: format!("ws_{}", uuid::Uuid::new_v4()),
            parent_run_id: parent_run_id.to_string(),
            project_path: project_path.to_path_buf(),
            workflow_id: spec.id.clone(),
            workflow_name: spec.name.clone(),
            primary_objective: objective,
            concurrency: spec.concurrency,
            on_failure: spec.on_failure,
            tasks,
            status: "active".to_string(),
            created_at: now,
            updated_at: now,
        })
    }

    /// Every task's agent profile must resolve as an enabled, valid profile in
    /// the agent registry. Checked before scheduling so a stale workflow never
    /// silently substitutes a different agent.
    pub fn validate_against_registry(
        ws: &MultiAgentWorkspace,
        registry: &AgentRegistry,
    ) -> Result<(), String> {
        for task in &ws.tasks {
            match registry.resolve(&task.agent_id) {
                Ok(profile) => {
                    if !profile.accepts_delegation() {
                        return Err(format!(
                            "Agent '{}' is disabled and cannot run workspace task '{}'",
                            task.agent_id, task.task_id
                        ));
                    }
                }
                Err(e) => {
                    return Err(format!(
                        "Workspace task '{}' cannot run: {}",
                        task.task_id, e
                    ))
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------- persistence

    /// Directory where workspaces are durably stored.
    pub fn workspaces_dir(config_root: &Path) -> PathBuf {
        config_root.join("agent_workspaces")
    }

    /// Persist the full workspace state atomically (temp file + rename).
    pub fn save_workspace(config_root: &Path, ws: &MultiAgentWorkspace) -> Result<(), String> {
        let dir = Self::workspaces_dir(config_root);
        fs::create_dir_all(&dir).map_err(|e| format!("Failed to create workspace store: {}", e))?;
        let path = dir.join(format!("{}.json", ws.workspace_id));
        let json = serde_json::to_string_pretty(ws)
            .map_err(|e| format!("Failed to serialize workspace: {}", e))?;
        let tmp = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
        fs::write(&tmp, json).map_err(|e| format!("Failed to write workspace temp file: {}", e))?;
        if let Err(e) = fs::rename(&tmp, &path) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("Failed to persist workspace state: {}", e));
        }
        Ok(())
    }

    /// Load a persisted workspace. Documents written by the pre-DAG scheduler
    /// (with a `role` enum instead of `agent_id`) are migrated transparently
    /// by mapping each legacy role onto the agent profile replacing it.
    pub fn load_workspace(
        config_root: &Path,
        workspace_id: &str,
    ) -> Result<MultiAgentWorkspace, String> {
        let path = Self::workspaces_dir(config_root).join(format!("{}.json", workspace_id));
        let json = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read workspace {:?}: {}", path, e))?;
        let mut value: serde_json::Value = serde_json::from_str(&json)
            .map_err(|e| format!("Corrupted workspace document: {}", e))?;

        // Migration: legacy task docs carry `role` (snake_case SpecialistRole)
        // and miss the new per-task fields.
        if let Some(tasks) = value.get_mut("tasks").and_then(|t| t.as_array_mut()) {
            for task in tasks {
                if task.get("agent_id").is_none() {
                    if let Some(role) = task.get("role").and_then(|r| r.as_str()) {
                        let agent_id = legacy_role_to_agent(role).unwrap_or("apex-orchestrator");
                        task["agent_id"] = serde_json::Value::String(agent_id.to_string());
                    }
                }
                for (key, default) in [
                    ("max_attempts", serde_json::json!(1)),
                    ("attempts", serde_json::json!(0)),
                    ("worktree_branch", serde_json::Value::Null),
                    ("started_at", serde_json::Value::Null),
                    ("completed_at", serde_json::Value::Null),
                    ("error", serde_json::Value::Null),
                ] {
                    if task.get(key).is_none() {
                        task[key] = default;
                    }
                }
            }
        }
        // Legacy workspace docs miss the workflow provenance and run fields.
        if !value.is_object() {
            return Err("Corrupted workspace document: expected a JSON object".to_string());
        }
        let obj = value
            .as_object_mut()
            .ok_or_else(|| "Corrupted workspace document: expected a JSON object".to_string())?;
        for (key, default) in [
            ("workflow_id", serde_json::json!("legacy-static-pipeline")),
            ("workflow_name", serde_json::json!("Legacy Static Pipeline")),
            ("primary_objective", serde_json::json!("")),
            ("concurrency", serde_json::json!(1)),
            ("created_at", serde_json::json!(0)),
            ("updated_at", serde_json::json!(0)),
        ] {
            if obj.get(key).is_none() {
                obj.insert(key.to_string(), default);
            }
        }

        serde_json::from_value(value)
            .map_err(|e| format!("Workspace document rejected after migration: {}", e))
    }

    /// All persisted workspace ids, newest first.
    pub fn list_workspace_ids(config_root: &Path) -> Vec<String> {
        let dir = Self::workspaces_dir(config_root);
        let mut ids: Vec<String> = fs::read_dir(&dir)
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        name.strip_suffix(".json")
                            .filter(|_| !name.starts_with("."))
                            .map(|s| s.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default();
        ids.sort();
        ids.reverse();
        ids
    }

    /// Recover a workspace after a process restart: child runs recorded as
    /// "running" in the scheduler document are re-checked against the durable
    /// run table. Lost processes re-queue, finished runs adopt their durable
    /// outcome.
    pub fn recover_workspace(ws: &mut MultiAgentWorkspace, db: &Database) {
        for i in 0..ws.tasks.len() {
            if ws.tasks[i].status != "running" {
                continue;
            }
            let run_status = ws.tasks[i]
                .child_run_id
                .as_deref()
                .and_then(|rid| db.get_run(rid).ok().flatten())
                .map(|r| r.status)
                .unwrap_or_default();
            match run_status.as_str() {
                "succeeded" => {
                    ws.tasks[i].status = "completed".to_string();
                    ws.tasks[i].completed_at = Some(chrono::Utc::now().timestamp_millis());
                }
                "failed" => {
                    ws.tasks[i].status = "failed".to_string();
                    ws.tasks[i].completed_at = Some(chrono::Utc::now().timestamp_millis());
                }
                "cancelled" => {
                    ws.tasks[i].status = "cancelled".to_string();
                    ws.tasks[i].completed_at = Some(chrono::Utc::now().timestamp_millis());
                }
                // No durable record or still-running: the scheduling process
                // died mid-task, so the work is lost and must be re-queued.
                _ => {
                    ws.tasks[i].status = "pending".to_string();
                    ws.tasks[i].child_run_id = None;
                }
            }
        }
        Self::refresh_workspace_status(ws);
    }

    /// Drop stale scheduler-side state: short-lived per-process maps cannot
    /// survive restarts, so tasks that are "running" with a live child run
    /// keep their durable truth (handled by [`recover_workspace`]).
    pub fn refresh_workspace_status(ws: &mut MultiAgentWorkspace) {
        let has_open = ws
            .tasks
            .iter()
            .any(|t| t.status == "pending" || t.status == "running");
        if has_open {
            if ws.status != "cancelled" {
                ws.status = "active".to_string();
            }
        } else {
            let failed = ws.tasks.iter().filter(|t| t.status == "failed").count();
            let completed = ws.tasks.iter().filter(|t| t.status == "completed").count();
            let cancelled = ws.tasks.iter().filter(|t| t.status == "cancelled").count();
            if cancelled > 0 && completed == 0 {
                ws.status = "cancelled".to_string();
            } else if failed > 0 && completed > 0 {
                ws.status = "failed_partial".to_string();
            } else if failed > 0 {
                ws.status = "failed".to_string();
            } else if ws.status == "reconciled" {
                // keep reconciled label
            } else {
                ws.status = "completed".to_string();
            }
        }
        ws.updated_at = chrono::Utc::now().timestamp_millis();
    }

    // ------------------------------------------------------------- scheduling

    /// Run a workspace to a terminal state on the engine, honoring the DAG
    /// order, the configured concurrency, per-task retry budgets and the
    /// failure policy. Progress is persisted after every batch so a crash can
    /// be recovered via [`MultiAgentOrchestrator::recover_workspace`].
    pub async fn run_workspace(
        engine: &crate::agent::AgentLoopEngine,
        token: &CancellationToken,
        ws: &mut MultiAgentWorkspace,
        db: &Database,
        // Optional inference endpoint override; production runs use the
        // default provider configuration, tests point children at a local
        // mock so scheduling is deterministic and network-free.
        base_url: Option<&str>,
    ) -> Result<(), String> {
        // Every referenced profile must exist and accept work before anything
        // is scheduled.
        {
            let registry = engine.registry.read().await;
            Self::validate_against_registry(ws, &registry)?;
        }

        let config_root = engine.config_root().await.ok_or_else(|| {
            "Agent registry is not backed by a user directory; workspace state cannot persist"
                .to_string()
        })?;

        Self::save_workspace(&config_root, ws)?;

        ACTIVE_WORKSPACE_TOKENS
            .lock()
            .unwrap()
            .insert(ws.workspace_id.clone(), token.clone());

        let result = Self::run_workspace_inner(engine, token, ws, db, &config_root, base_url).await;
        let _ = Self::save_workspace(&config_root, ws);
        ACTIVE_WORKSPACE_TOKENS
            .lock()
            .unwrap()
            .remove(&ws.workspace_id);
        result
    }

    async fn run_workspace_inner(
        engine: &crate::agent::AgentLoopEngine,
        token: &CancellationToken,
        ws: &mut MultiAgentWorkspace,
        db: &Database,
        config_root: &Path,
        base_url: Option<&str>,
    ) -> Result<(), String> {
        loop {
            if token.is_cancelled() {
                for task in ws.tasks.iter_mut() {
                    if task.status == "pending" {
                        task.status = "cancelled".to_string();
                        task.error = Some("Workspace run was cancelled by the user".to_string());
                    }
                }
                Self::refresh_workspace_status(ws);
                Self::save_workspace(config_root, ws)?;
                return Ok(());
            }

            // Required task permanently failed under 'abort': stop everything.
            if let Some(failed) = Self::first_required_permanent_failure(ws) {
                if ws.on_failure == FailurePolicy::Abort {
                    for task in ws.tasks.iter_mut() {
                        if task.status == "pending" {
                            task.status = "cancelled".to_string();
                            task.error = Some(format!(
                                "Cancelled because required task '{}' failed permanently",
                                failed
                            ));
                        }
                    }
                    Self::refresh_workspace_status(ws);
                    Self::save_workspace(config_root, ws)?;
                    return Ok(());
                }
            }

            // Schedule one bounded batch of ready tasks.
            let ready: Vec<String> = Self::ready_tasks(ws)
                .into_iter()
                .take(ws.concurrency.max(1) as usize)
                .collect();

            if ready.is_empty() {
                // Nothing is runnable in this pass: any task still pending is
                // waiting on dependencies that can never complete (failed or
                // skipped beyond its retry budget). The whole frontier gets an
                // explicit skip with the reason, never a dangling pending or a
                // fake "completed" workspace.
                Self::skip_unsatisfiable(ws);
                Self::refresh_workspace_status(ws);
                Self::save_workspace(config_root, ws)?;
                return Ok(());
            }

            Self::execute_batch(engine, token, ws, db, &ready, config_root, base_url).await?;
            Self::refresh_workspace_status(ws);
            Self::save_workspace(config_root, ws)?;
        }
    }

    /// Task ids that can be scheduled now: pending, retry budget remaining,
    /// and every dependency terminal in a satisfying state.
    fn ready_tasks(ws: &MultiAgentWorkspace) -> Vec<String> {
        let mut order: Vec<&SpecialistTask> = ws.tasks.iter().collect();
        order.sort_by(|a, b| a.task_id.cmp(&b.task_id));
        let by_id: HashMap<&str, &SpecialistTask> =
            ws.tasks.iter().map(|t| (t.task_id.as_str(), t)).collect();
        let mut ready = Vec::new();
        for task in order {
            if task.status != "pending" || task.attempts >= task.max_attempts {
                continue;
            }
            let deps_ok = task.depends_on.iter().all(|dep| {
                by_id
                    .get(dep.as_str())
                    .map(|t| t.status == "completed" || t.status == "skipped")
                    .unwrap_or(false)
            });
            if deps_ok {
                ready.push(task.task_id.clone());
            }
        }
        ready
    }

    fn first_required_permanent_failure(ws: &MultiAgentWorkspace) -> Option<String> {
        ws.tasks
            .iter()
            .find(|t| t.status == "failed" && !t.optional && t.attempts >= t.max_attempts.max(1))
            .map(|t| t.task_id.clone())
    }

    /// Tasks still pending while nothing can run get an explicit skip reason.
    fn skip_unsatisfiable(ws: &mut MultiAgentWorkspace) {
        for task in ws.tasks.iter_mut() {
            if task.status == "pending" {
                task.status = "skipped".to_string();
                task.error = Some(format!(
                    "Skipped: dependencies did not complete ({})",
                    task.depends_on.join(", ")
                ));
                task.completed_at = Some(chrono::Utc::now().timestamp_millis());
            }
        }
    }

    /// Execute one concurrency-bounded batch of tasks concurrently.
    async fn execute_batch(
        engine: &crate::agent::AgentLoopEngine,
        token: &CancellationToken,
        ws: &mut MultiAgentWorkspace,
        db: &Database,
        ids: &[String],
        config_root: &Path,
        base_url: Option<&str>,
    ) -> Result<(), String> {
        // Parent context for lineage and model routing.
        let parent = db
            .get_run(&ws.parent_run_id)
            .ok()
            .flatten()
            .ok_or_else(|| {
                format!(
                    "Workspace parent run '{}' not found; the workspace cannot schedule child runs",
                    ws.parent_run_id
                )
            })?;
        let api_key = match crate::storage::Vault::new(db.clone()).get_all_keys() {
            Ok(k) => k.nvidia.unwrap_or_default(),
            Err(_) => String::new(),
        };

        // Kick off each batch member: consume an attempt, allocate a durable
        // child run id and (when required) an isolated worktree.
        struct JobSpec {
            task_index: usize,
            child_run_id: String,
            agent_id: String,
            objective: String,
            worktree: Option<PathBuf>,
            attempt: u32,
            model: String,
        }
        let mut jobs: Vec<JobSpec> = Vec::with_capacity(ids.len());
        for id in ids {
            let Some(index) = ws.tasks.iter().position(|t| t.task_id == *id) else {
                return Err(format!("Workspace task '{}' vanished mid-scheduling", id));
            };
            {
                let t = &mut ws.tasks[index];
                t.attempts += 1;
                t.started_at = Some(chrono::Utc::now().timestamp_millis());
                let base_child_run_id =
                    format!("{}_{}_a{}", ws.workspace_id, t.task_id, t.attempts);
                let child_run_id = ensure_unique_run_id(&base_child_run_id, db);
                t.child_run_id = Some(child_run_id.clone());
                t.status = "running".to_string();
            }
            if ws.tasks[index].isolated_worktree {
                match Self::setup_isolated_worktree(
                    &ws.project_path,
                    &ws.workspace_id,
                    id,
                    ws.tasks[index].attempts,
                ) {
                    Ok((dir, branch, base_commit)) => {
                        let t = &mut ws.tasks[index];
                        t.isolated_worktree_path = Some(dir);
                        t.worktree_branch = branch;
                        t.base_commit = base_commit;
                    }
                    Err(e) => {
                        // The attempt is consumed; the retry budget applies.
                        let t = &mut ws.tasks[index];
                        if t.attempts >= t.max_attempts {
                            t.status = "failed".to_string();
                            t.completed_at = Some(chrono::Utc::now().timestamp_millis());
                        } else {
                            t.status = "pending".to_string();
                        }
                        t.child_run_id = None;
                        t.error = Some(format!("Failed to prepare isolated worktree: {}", e));
                        continue;
                    }
                }
            }
            let t = &ws.tasks[index];
            jobs.push(JobSpec {
                task_index: index,
                child_run_id: t.child_run_id.clone().unwrap(),
                agent_id: t.agent_id.clone(),
                objective: t.objective.clone(),
                worktree: t.isolated_worktree_path.clone(),
                attempt: t.attempts,
                model: parent.model.clone(),
            });
        }

        Self::save_workspace(config_root, ws)?;

        // Create the child runs, then execute the batch concurrently — each
        // job owns its data, borrowing only the shared engine/token/db which
        // this parent scope awaits via join_all.
        let mut order: Vec<(String, usize)> = Vec::with_capacity(jobs.len());
        let mut futures = Vec::with_capacity(jobs.len());
        for job in jobs.drain(..) {
            order.push((job.child_run_id.clone(), job.task_index));
            let objective_message = match &job.worktree {
                Some(dir) => format!(
                    "{}\n\n(Workspace isolation: your working copies live in the isolated worktree at {:?}. Do not modify the original project tree.)",
                    job.objective, dir
                ),
                None => job.objective.clone(),
            };
            let history = vec![crate::agent::ChatMessage {
                role: "user".to_string(),
                content: crate::agent::MessageContent::from_text(objective_message),
                tool_calls: None,
                tool_call_id: None,
            }];
            let child_run = AgentRun {
                id: job.child_run_id.clone(),
                conversation_id: parent.conversation_id.clone(),
                agent_id: job.agent_id.clone(),
                model: job.model.clone(),
                prompt: job.objective.clone(),
                status: "running".to_string(),
                created_at: chrono::Utc::now().timestamp_millis(),
                started_at: Some(chrono::Utc::now().timestamp_millis()),
                completed_at: None,
                error: None,
                config_json: Some(
                    serde_json::json!({
                        "workspace_id": ws.workspace_id,
                        "task_id": ws.tasks[job.task_index].task_id,
                        "attempt": job.attempt,
                        "parent_run_id": ws.parent_run_id,
                        "workflow_id": ws.workflow_id,
                        "isolated_worktree": job.worktree.is_some(),
                        "isolated_worktree_path": job.worktree.as_ref().map(|p| p.to_string_lossy().to_string()),
                        "workspace_root": job.worktree.as_ref().map(|p| p.to_string_lossy().to_string())
                            .or_else(|| Some(ws.project_path.to_string_lossy().to_string())),
                        "base_commit": ws.tasks[job.task_index].base_commit,
                    })
                    .to_string(),
                ),
                parent_run_id: Some(ws.parent_run_id.clone()),
            };
            db.create_run(&child_run)
                .map_err(|e| format!("Failed to persist workflow child run: {}", e))?;
            MultiBotOrchestrator::set_bot_task(
                &job.agent_id,
                &job.child_run_id,
                Some(&job.objective),
            );
            let child_run_id = job.child_run_id.clone();
            let agent_id = job.agent_id.clone();
            let api_key = api_key.clone();
            let child_db = db.clone();
            let child_token = token.child_token();
            futures.push(async move {
                let outcome = engine
                    .run_step_with_connectors(
                        &child_run_id,
                        &job.model,
                        &api_key,
                        base_url,
                        "",
                        history,
                        child_db,
                        None,
                        None,
                        Some(child_token),
                        |_| {},
                    )
                    .await;
                (child_run_id, agent_id, outcome)
            });
        }

        // Concurrently await every batch member, then adopt their terminal
        // states against the corresponding tasks.
        let outcomes = futures::future::join_all(futures).await;
        for (child_run_id, agent_id, outcome) in outcomes {
            MultiBotOrchestrator::set_bot_task(&agent_id, &child_run_id, None);
            let Some((_, task_index)) = order.iter().find(|(rid, _)| rid == &child_run_id).cloned()
            else {
                continue;
            };
            match outcome {
                Ok(output) => {
                    let t = &mut ws.tasks[task_index];
                    t.status = "completed".to_string();
                    t.output = Some(output);
                    t.error = None;
                    t.completed_at = Some(chrono::Utc::now().timestamp_millis());
                }
                Err(e) => {
                    let t = &mut ws.tasks[task_index];
                    if token.is_cancelled() {
                        t.status = "cancelled".to_string();
                        t.completed_at = Some(chrono::Utc::now().timestamp_millis());
                    } else if t.attempts >= t.max_attempts {
                        t.status = "failed".to_string();
                        t.completed_at = Some(chrono::Utc::now().timestamp_millis());
                    } else {
                        // Retry: re-queue keeping the attempt count; the
                        // failure reason stays recorded for the next attempt.
                        t.status = "pending".to_string();
                    }
                    t.error = Some(e.clone());
                    t.child_run_id = None;
                }
            }
        }

        Ok(())
    }

    /// Set up an isolated working environment for a task using a Git worktree
    /// when the project is a Git repository, or a shadow scratch directory if
    /// not. Uses collision-resistant directory and branch names scoped to workspace, task, and attempt.
    pub fn setup_isolated_worktree(
        project_root: &Path,
        workspace_id: &str,
        task_id: &str,
        attempt: u32,
    ) -> Result<(PathBuf, Option<String>, Option<String>), String> {
        let scratch_base = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("kyro")
            .join("scratch")
            .join("worktrees");
        fs::create_dir_all(&scratch_base)
            .map_err(|e| format!("Failed to create worktree base directory: {}", e))?;

        let safe_ws: String = workspace_id
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let safe_task: String = task_id
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let dir_name = format!("{}_{}_a{}", safe_ws, safe_task, attempt);
        let worktree_dir = scratch_base.join(dir_name);
        let branch_name = format!("kyro-wt-{}-{}-a{}", safe_ws, safe_task, attempt);

        let is_git = project_root.join(".git").exists();
        if is_git {
            let base_commit = Command::new("git")
                .arg("-C")
                .arg(project_root)
                .args(["rev-parse", "HEAD"])
                .output()
                .ok()
                .and_then(|out| {
                    if out.status.success() {
                        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
                    } else {
                        None
                    }
                });

            if worktree_dir.exists() {
                let _ = fs::remove_dir_all(&worktree_dir);
            }

            let output = Command::new("git")
                .arg("-C")
                .arg(project_root)
                .args(["worktree", "add", "-b", &branch_name])
                .arg(&worktree_dir)
                .arg("HEAD")
                .output();

            match output {
                Ok(out) if out.status.success() => {
                    Ok((worktree_dir, Some(branch_name), base_commit))
                }
                Ok(out) => {
                    let err = String::from_utf8_lossy(&out.stderr);
                    log::warn!(
                        "[Workspace] git worktree add failed ({}): falling back to shadow copy",
                        err
                    );
                    Self::fallback_shadow_copy(project_root, &worktree_dir)?;
                    Ok((worktree_dir, None, base_commit))
                }
                Err(e) => {
                    log::warn!("[Workspace] Failed to run git worktree add: {}: falling back to shadow copy", e);
                    Self::fallback_shadow_copy(project_root, &worktree_dir)?;
                    Ok((worktree_dir, None, base_commit))
                }
            }
        } else {
            Self::fallback_shadow_copy(project_root, &worktree_dir)?;
            Ok((worktree_dir, None, None))
        }
    }

    /// Cleans up an isolated worktree after completion or cancellation.
    pub fn cleanup_isolated_worktree(
        project_root: &Path,
        worktree_path: &Path,
        branch_name: Option<&str>,
    ) {
        if project_root.join(".git").exists() {
            let _ = Command::new("git")
                .arg("-C")
                .arg(project_root)
                .args(["worktree", "remove", "--force"])
                .arg(worktree_path)
                .output();

            if let Some(branch) = branch_name {
                let _ = Command::new("git")
                    .arg("-C")
                    .arg(project_root)
                    .args(["branch", "-D", branch])
                    .output();
            }
        }

        if worktree_path.exists() {
            let _ = fs::remove_dir_all(worktree_path);
        }
    }

    /// Fallback shadow copy of source tree into isolated scratch folder.
    fn fallback_shadow_copy(src: &Path, dst: &Path) -> Result<(), String> {
        let _ = fs::create_dir_all(dst);
        let read_dir = fs::read_dir(src).map_err(|e| format!("Failed to read source: {}", e))?;

        for entry in read_dir.flatten() {
            let path = entry.path();
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();

            // Skip heavy build or vcs folders
            if name_str == "node_modules"
                || name_str == "target"
                || name_str == ".git"
                || name_str == "dist"
            {
                continue;
            }

            let target = dst.join(&file_name);
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() {
                    let _ = fs::copy(&path, &target);
                } else if file_type.is_dir() {
                    let _ = Self::fallback_shadow_copy(&path, &target);
                }
            }
        }
        Ok(())
    }

    /// Reconciles changes from an isolated implementation worktree against the
    /// parent repository with base-commit tracking and conflict detection.
    pub fn reconcile_changes(
        project_root: &Path,
        worktree_path: &Path,
        workspace_id: &str,
    ) -> Result<ReconciliationOutcome, String> {
        if !worktree_path.exists() {
            return Ok(ReconciliationOutcome {
                workspace_id: workspace_id.to_string(),
                changes_detected: false,
                modified_files: vec![],
                unified_diff: String::new(),
                conflicts_detected: vec!["Worktree path no longer exists.".to_string()],
                can_safely_merge: false,
                reconciliation_summary: "No isolated worktree found to reconcile.".to_string(),
            });
        }

        let is_git = project_root.join(".git").exists();
        if is_git {
            let parent_head = Command::new("git")
                .arg("-C")
                .arg(project_root)
                .args(["rev-parse", "HEAD"])
                .output()
                .ok()
                .and_then(|out| {
                    if out.status.success() {
                        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
                    } else {
                        None
                    }
                });

            // Find common ancestor base_commit between worktree and parent_head
            let base_commit = if let Some(ref phead) = parent_head {
                Command::new("git")
                    .arg("-C")
                    .arg(worktree_path)
                    .args(["merge-base", "HEAD", phead])
                    .output()
                    .ok()
                    .and_then(|out| {
                        if out.status.success() {
                            Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
                        } else {
                            None
                        }
                    })
            } else {
                None
            };

            // Unified diff of worktree changes against base_commit or HEAD
            let diff_target = base_commit.as_deref().unwrap_or("HEAD");
            let diff_cmd = Command::new("git")
                .arg("-C")
                .arg(worktree_path)
                .args(["diff", diff_target])
                .output();

            let mut diff_text = String::new();
            if let Ok(out) = diff_cmd {
                diff_text = String::from_utf8_lossy(&out.stdout).to_string();
            }

            // Modified / added / untracked files in worktree
            let status_cmd = Command::new("git")
                .arg("-C")
                .arg(worktree_path)
                .args(["status", "--porcelain"])
                .output();

            let mut modified = Vec::new();
            if let Ok(s_out) = status_cmd {
                let s_text = String::from_utf8_lossy(&s_out.stdout);
                for line in s_text.lines() {
                    let trimmed = line.trim();
                    if trimmed.len() >= 3 {
                        let path_part = trimmed[3..].trim();
                        let file_name = if let Some((_, dest)) = path_part.split_once(" -> ") {
                            dest.trim().to_string()
                        } else {
                            path_part.to_string()
                        };
                        if !modified.contains(&file_name) {
                            modified.push(file_name);
                        }
                    }
                }
            }

            // Also include files from git diff --name-only <base_commit>
            if let Ok(name_out) = Command::new("git")
                .arg("-C")
                .arg(worktree_path)
                .args(["diff", "--name-only", diff_target])
                .output()
            {
                let text = String::from_utf8_lossy(&name_out.stdout);
                for line in text.lines() {
                    let f = line.trim().to_string();
                    if !f.is_empty() && !modified.contains(&f) {
                        modified.push(f);
                    }
                }
            }

            // Conflict detection: did project_root modify any of the same files since base_commit?
            let mut conflicts = Vec::new();
            if let (Some(ref base), Some(ref phead)) = (&base_commit, &parent_head) {
                if base != phead {
                    if let Ok(p_diff) = Command::new("git")
                        .arg("-C")
                        .arg(project_root)
                        .args(["diff", "--name-only", base, phead])
                        .output()
                    {
                        let p_text = String::from_utf8_lossy(&p_diff.stdout);
                        for line in p_text.lines() {
                            let changed_in_parent = line.trim();
                            if !changed_in_parent.is_empty()
                                && modified.iter().any(|m| m == changed_in_parent)
                            {
                                conflicts.push(format!(
                                    "Concurrent modification in project root: {}",
                                    changed_in_parent
                                ));
                            }
                        }
                    }
                }
            }

            let changes_detected = !modified.is_empty() || !diff_text.is_empty();
            let can_safely_merge = changes_detected && conflicts.is_empty();
            let summary = if !conflicts.is_empty() {
                format!(
                    "Conflicts detected in {} file(s) between worktree and project root. Manual resolution required.",
                    conflicts.len()
                )
            } else if changes_detected {
                "Changes successfully isolated and ready for reviewed integration.".to_string()
            } else {
                "No code modifications made in isolated worktree.".to_string()
            };

            return Ok(ReconciliationOutcome {
                workspace_id: workspace_id.to_string(),
                changes_detected,
                modified_files: modified,
                unified_diff: diff_text,
                conflicts_detected: conflicts,
                can_safely_merge,
                reconciliation_summary: summary,
            });
        }

        // Generic directory diff for non-git projects
        Ok(ReconciliationOutcome {
            workspace_id: workspace_id.to_string(),
            changes_detected: false,
            modified_files: vec![],
            unified_diff: String::new(),
            conflicts_detected: vec![],
            can_safely_merge: true,
            reconciliation_summary: "Generic directory comparison complete.".to_string(),
        })
    }

    /// Safely apply reconciled changes into the project root if no conflicts were detected.
    pub fn apply_reconciliation(
        project_root: &Path,
        worktree_path: &Path,
        branch_name: Option<&str>,
    ) -> Result<String, String> {
        if !project_root.exists() || !worktree_path.exists() {
            return Err("Project root or worktree path does not exist".to_string());
        }

        let is_git = project_root.join(".git").exists();
        if is_git {
            if let Some(branch) = branch_name {
                let merge_out = Command::new("git")
                    .arg("-C")
                    .arg(project_root)
                    .args([
                        "merge",
                        "--no-ff",
                        "-m",
                        &format!("Reconcile worktree branch '{}'", branch),
                        branch,
                    ])
                    .output()
                    .map_err(|e| format!("Failed to execute git merge: {}", e))?;

                if merge_out.status.success() {
                    Ok(format!(
                        "Successfully merged branch '{}' into project root.",
                        branch
                    ))
                } else {
                    let err = String::from_utf8_lossy(&merge_out.stderr);
                    Err(format!("Git merge failed: {}", err))
                }
            } else {
                let diff_out = Command::new("git")
                    .arg("-C")
                    .arg(worktree_path)
                    .args(["diff", "HEAD"])
                    .output()
                    .map_err(|e| format!("Failed to generate diff: {}", e))?;

                if diff_out.stdout.is_empty() {
                    return Ok("No changes to apply.".to_string());
                }

                let mut apply_child = Command::new("git")
                    .arg("-C")
                    .arg(project_root)
                    .args(["apply"])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("Failed to spawn git apply: {}", e))?;

                if let Some(mut stdin) = apply_child.stdin.take() {
                    use std::io::Write;
                    let _ = stdin.write_all(&diff_out.stdout);
                }

                let apply_out = apply_child
                    .wait_with_output()
                    .map_err(|e| format!("Failed to wait for git apply: {}", e))?;

                if apply_out.status.success() {
                    Ok("Successfully applied worktree changes to project root.".to_string())
                } else {
                    let err = String::from_utf8_lossy(&apply_out.stderr);
                    Err(format!("Failed to apply diff: {}", err))
                }
            }
        } else {
            Self::fallback_shadow_copy(worktree_path, project_root)?;
            Ok("Successfully synced files from worktree back to project root.".to_string())
        }
    }

    /// Mark workspace "reconciled" and persist the label.
    pub fn mark_reconciled(config_root: &Path, ws: &mut MultiAgentWorkspace) -> Result<(), String> {
        ws.status = "reconciled".to_string();
        ws.updated_at = chrono::Utc::now().timestamp_millis();
        Self::save_workspace(config_root, ws)
    }

    /// Request cancellation of a running workspace (no-op when orphaned).
    pub fn cancel_workspace(workspace_id: &str) -> bool {
        if let Some(token) = ACTIVE_WORKSPACE_TOKENS.lock().unwrap().get(workspace_id) {
            token.cancel();
            true
        } else {
            false
        }
    }
}

/// Child run ids must be unique in the durable table; on a retry-after-restart
/// collision (impossible with attempt suffixes, but never fabricate state) a
/// discriminant suffix is added so create_run cannot fail on a primary key.
fn ensure_unique_run_id(base: &str, db: &Database) -> String {
    let mut id = base.to_string();
    let mut n = 0u32;
    while db.get_run(&id).ok().flatten().is_some() {
        n += 1;
        id = format!("{}~{}", base, uuid::Uuid::new_v4().simple());
        if n > 8 {
            break;
        }
    }
    id
}

/// Live per-workspace cancellation tokens, keyed by workspace id.
static ACTIVE_WORKSPACE_TOKENS: Lazy<Mutex<HashMap<String, CancellationToken>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_WORKFLOW: &str = "---\nschema_version: 1\nid: sample\nname: Sample\navatar: zap\nconcurrency: 2\non_failure: continue\ntasks:\n  - task_id: one\n    agent_id: forge-devops\n    objective: \"Do one {{objective}}\"\n    depends_on: []\n    retries: 1\n  - task_id: two\n    agent_id: scout-web\n    objective: \"Do two for {{objective}}\"\n    depends_on: [one]\n    optional: true\n---\n\nDoc body.\n";

    #[test]
    fn parses_and_validates_bundled_workflow() {
        let content = builtin_workflow_content("software-change").unwrap();
        let spec = parse_workflow_manifest(content).expect("bundled workflow parses");
        assert_eq!(spec.id, "software-change");
        assert_eq!(spec.concurrency, 2);
        assert_eq!(spec.tasks.len(), 4);
        assert_eq!(spec.tasks[1].depends_on, vec!["investigate"]);
        assert!(spec.tasks[1].isolated_worktree);
        let order = topological_order(&spec).unwrap();
        let pos = |id: &str| order.iter().position(|t| t == id).unwrap();
        assert!(pos("investigate") < pos("implement"));
        assert!(pos("implement") < pos("verify"));
        assert!(pos("verify") < pos("review"));
    }

    #[test]
    fn rejects_malformed_workflow_docs() {
        // No frontmatter
        assert!(parse_workflow_manifest("id: nope").is_err());
        // Empty body
        let no_body = "---\nid: sample\nname: s\ntasks:\n  - task_id: a\n    agent_id: forge-devops\n    objective: do\n---\n";
        assert!(parse_workflow_manifest(no_body).is_err());
        // Missing dependency
        let missing = parse_workflow_manifest(
            &SAMPLE_WORKFLOW.replace("depends_on: [one]", "depends_on: [gone]"),
        )
        .expect_err("missing dep must fail");
        assert!(missing.contains("unknown task"), "{}", missing);
        // Cycle
        let cyclic = "---\nid: cyc\nname: cyc\ntasks:\n  - task_id: a\n    agent_id: forge-devops\n    objective: a\n    depends_on: [b]\n  - task_id: b\n    agent_id: forge-devops\n    objective: b\n    depends_on: [a]\n---\n\nCycle doc.\n";
        let err = parse_workflow_manifest(cyclic).expect_err("cycle must fail");
        assert!(err.contains("cycle"), "{}", err);
        // Duplicate task ids
        let dup = SAMPLE_WORKFLOW.replace("task_id: two", "task_id: one");
        let err = parse_workflow_manifest(&dup).expect_err("duplicate ids must fail");
        assert!(err.contains("Duplicate"), "{}", err);
        // Self dependency
        let selv = SAMPLE_WORKFLOW.replace("depends_on: [one]", "depends_on: [two]");
        let err = parse_workflow_manifest(&selv).expect_err("self dep must fail");
        assert!(err.contains("itself"), "{}", err);
        // Unknown failure policy
        let bad_policy = SAMPLE_WORKFLOW.replace("on_failure: continue", "on_failure: explode");
        assert!(parse_workflow_manifest(&bad_policy).is_err());
        // Concurrency bounds
        let bad_conc = SAMPLE_WORKFLOW.replace("concurrency: 2", "concurrency: 99");
        assert!(parse_workflow_manifest(&bad_conc).is_err());
    }

    #[test]
    fn create_workspace_templates_objectives_and_budgets() {
        let spec = parse_workflow_manifest(SAMPLE_WORKFLOW).unwrap();
        let ws = MultiAgentOrchestrator::create_workspace(
            "run_p",
            Path::new("/project"),
            &spec,
            "Fix the flaky test",
        )
        .unwrap();
        assert_eq!(ws.tasks.len(), 2);
        assert!(ws.tasks[0].objective.contains("Do one Fix the flaky test"));
        assert!(ws.tasks[1]
            .objective
            .contains("Do two for Fix the flaky test"));
        assert_eq!(ws.tasks[0].max_attempts, 2); // retries: 1
        assert_eq!(ws.tasks[0].status, "pending");
        assert!(ws.tasks.iter().all(|t| t.optional
            == ws
                .tasks
                .iter()
                .find(|x| x.task_id == t.task_id)
                .unwrap()
                .optional));
        assert_eq!(ws.on_failure, FailurePolicy::Continue);
        assert_eq!(ws.status, "active");
    }

    #[test]
    fn temp_registry_and_workspace_roundtrip() {
        let dir = std::env::temp_dir().join(format!("kyro_ws_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let registry = AgentRegistry::embedded_only();
        let spec = parse_workflow_manifest(SAMPLE_WORKFLOW).unwrap();
        let ws = MultiAgentOrchestrator::create_workspace("run_p", Path::new("/p"), &spec, "obj")
            .unwrap();
        MultiAgentOrchestrator::validate_against_registry(&ws, &registry)
            .expect("bundled profiles resolve");

        MultiAgentOrchestrator::save_workspace(&dir, &ws).unwrap();
        let loaded = MultiAgentOrchestrator::load_workspace(&dir, &ws.workspace_id).unwrap();
        assert_eq!(loaded.workspace_id, ws.workspace_id);
        assert_eq!(loaded.tasks, ws.tasks);

        // Legacy migration: a pre-DAG document with `role` gains `agent_id` +
        // the new runtime fields without losing data.
        let legacy_json = serde_json::json!({
            "workspace_id": "ws_legacy",
            "parent_run_id": "run_p",
            "project_path": "/p",
            "tasks": [{
                "task_id": "t1",
                "role": "implementation_engineer",
                "objective": "Do thing",
                "depends_on": [],
                "status": "completed",
                "child_run_id": null,
                "isolated_worktree_path": null,
                "output": "fine",
                "started_at": 5,
                "completed_at": 6
            }],
            "status": "reconciled"
        })
        .to_string();
        fs::write(
            dir.join("agent_workspaces").join("ws_legacy.json"),
            legacy_json,
        )
        .unwrap();
        let migrated = MultiAgentOrchestrator::load_workspace(&dir, "ws_legacy").unwrap();
        assert_eq!(migrated.tasks[0].agent_id, "forge-devops");
        assert_eq!(migrated.tasks[0].max_attempts, 1);
        assert_eq!(migrated.status, "reconciled");
        assert_eq!(migrated.tasks[0].output.as_deref(), Some("fine"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn scheduler_honors_retry_budget_and_skip_semantics_without_llm() {
        let dir = std::env::temp_dir().join(format!("kyro_ws_run_{}", uuid::Uuid::new_v4()));
        let db = crate::storage::Database::init(dir.clone()).unwrap();

        // Parent run for lineage.
        let parent = AgentRun {
            id: "run_ws_parent".to_string(),
            conversation_id: None,
            agent_id: "apex-orchestrator".to_string(),
            model: "test-model".to_string(),
            prompt: "Plan".to_string(),
            status: "running".to_string(),
            created_at: 1,
            started_at: Some(1),
            completed_at: None,
            error: None,
            config_json: None,
            parent_run_id: None,
        };
        db.create_run(&parent).unwrap();

        // Workspace persistence requires a user-backed registry.
        let cfg_dir = dir.join("config");
        let registry = AgentRegistry::load(&cfg_dir).unwrap();
        let engine = crate::agent::AgentLoopEngine::with_registry(
            std::sync::Arc::new(crate::browser::BrowserAutomationManager::new()),
            std::sync::Arc::new(crate::project::DevServerManager::new()),
            std::sync::Arc::new(tokio::sync::RwLock::new(registry)),
        );

        let spec = parse_workflow_manifest(SAMPLE_WORKFLOW).unwrap();
        let mut ws = MultiAgentOrchestrator::create_workspace(
            "run_ws_parent",
            Path::new(&dir.to_string_lossy().to_string()),
            &spec,
            "the objective",
        )
        .unwrap();

        // Children point at a closed local port so scheduling is deterministic
        // and network-free; they fail fast, exercising the retry budget.
        let token = CancellationToken::default();
        let result = MultiAgentOrchestrator::run_workspace(
            &engine,
            &token,
            &mut ws,
            &db,
            Some("http://127.0.0.1:1/v1"),
        )
        .await;

        assert!(result.is_ok(), "scheduler must complete, saw {:?}", result);
        let one = ws.tasks.iter().find(|t| t.task_id == "one").unwrap();
        let two = ws.tasks.iter().find(|t| t.task_id == "two").unwrap();
        // Task 'one' consumed its full budget (retries: 1 → 2 attempts).
        assert_eq!(one.attempts, one.max_attempts);
        assert_eq!(one.status, "failed");
        assert!(one.error.is_some(), "failure reason must be recorded");
        // Dependent task skipped with the dependency reasons recorded.
        assert_eq!(two.status, "skipped");
        assert!(two.error.as_deref().unwrap_or("").contains("one"));
        // Workspace status honestly reflects the failure (no fake success).
        assert_eq!(ws.status, "failed");
        // Child runs were durably recorded with workspace provenance.
        let runs = db.list_runs(50).unwrap();
        let config_root = engine.config_root().await.unwrap();
        assert!(MultiAgentOrchestrator::load_workspace(&config_root, &ws.workspace_id).is_ok());
        assert!(runs
            .iter()
            .all(|r| r.parent_run_id.as_deref() != Some("run_ws_parent")
                || json_config_contains_workspace_id(r, &ws.workspace_id)));
        let _ = fs::remove_dir_all(&dir);
    }

    fn json_config_contains_workspace_id(run: &AgentRun, workspace_id: &str) -> bool {
        run.config_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|v| {
                v.get("workspace_id")
                    .and_then(|w| w.as_str())
                    .map(|w| w == workspace_id)
            })
            .unwrap_or(false)
    }
}
