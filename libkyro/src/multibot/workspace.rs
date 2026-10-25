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
use crate::agent::ModelRouter;
use crate::storage::{AgentRun, Database};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_yaml_ng as yaml;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use crate::multibot::group::MultiBotOrchestrator;

/// Maximum number of concurrent team runs a workspace may schedule.
const MAX_WORKFLOW_CONCURRENCY: u32 = 8;

pub const SHADOW_MANIFEST_FILENAME: &str = ".kyro_shadow_manifest.json";

/// File manifest storing baseline relative file paths and their SHA256 hashes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowManifest {
    pub timestamp: i64,
    pub files: HashMap<String, String>,
}

/// Compute SHA-256 hash of a file's contents.
pub fn compute_file_sha256(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let out = hasher.finalize();
    Ok(out.iter().map(|b| format!("{:02x}", b)).collect::<String>())
}

/// Robust directory traversal collecting relative paths and SHA256 hashes.
/// Surfaces I/O errors and permission issues rather than silently ignoring entries.
pub fn collect_shadow_manifest(
    base: &Path,
    rel: &Path,
    manifest: &mut HashMap<String, String>,
) -> Result<(), String> {
    let dir = base.join(rel);
    if !dir.exists() {
        return Ok(());
    }
    let read_dir = fs::read_dir(&dir)
        .map_err(|e| format!("Failed to read directory {}: {}", dir.display(), e))?;

    for entry_res in read_dir {
        let entry = entry_res
            .map_err(|e| format!("Failed to read directory entry in {}: {}", dir.display(), e))?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str == ".git"
            || name_str == "node_modules"
            || name_str == "target"
            || name_str == "dist"
            || name_str == SHADOW_MANIFEST_FILENAME
        {
            continue;
        }

        let child_rel = if rel.as_os_str().is_empty() {
            PathBuf::from(&name)
        } else {
            rel.join(&name)
        };

        let file_type = entry
            .file_type()
            .map_err(|e| format!("Failed to inspect {}: {}", entry.path().display(), e))?;

        if file_type.is_dir() {
            collect_shadow_manifest(base, &child_rel, manifest)?;
        } else if file_type.is_file() {
            let hash = compute_file_sha256(&entry.path())?;
            manifest.insert(child_rel.to_string_lossy().to_string(), hash);
        }
    }
    Ok(())
}

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

/// Context and candidate changes inherited from an upstream dependency.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DependencyEvidence {
    pub task_id: String,
    pub agent_id: String,
    pub status: String,
    pub output: Option<String>,
    pub modified_files: Vec<String>,
    pub diff_stat: Option<String>,
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
        "test_regression_specialist" => Some("test-regression-specialist"),
        "code_reviewer" => Some("code-reviewer"),
        "documentation_reviewer" => Some("documentation-reviewer"),
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
        let vault_keys_snapshot = crate::storage::Vault::new(db.clone())
            .get_all_keys()
            .unwrap_or_default();
        let api_key = vault_keys_snapshot.nvidia.clone().unwrap_or_default();

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
            dependency_context: Vec<DependencyEvidence>,
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

            // Collect upstream dependencies
            let upstream_tasks: Vec<SpecialistTask> = ws
                .tasks
                .iter()
                .filter(|t| ws.tasks[index].depends_on.contains(&t.task_id))
                .cloned()
                .collect();
            let upstream_refs: Vec<&SpecialistTask> = upstream_tasks.iter().collect();

            // Snapshot any uncommitted changes in upstream worktrees and gather evidence
            let mut dep_evidence: Vec<DependencyEvidence> = Vec::new();
            for dep in &upstream_tasks {
                let (modified_files, diff_stat) = if let Some(ref dir) = dep.isolated_worktree_path
                {
                    if dir.exists() {
                        let _ = snapshot_worktree_changes(dir, &dep.task_id);
                        let files = get_worktree_changed_files(
                            &ws.project_path,
                            dir,
                            dep.base_commit.as_deref(),
                        );
                        let stat = get_worktree_diff_stat(dir, dep.base_commit.as_deref());
                        (files, stat)
                    } else {
                        (Vec::new(), None)
                    }
                } else {
                    (Vec::new(), None)
                };
                dep_evidence.push(DependencyEvidence {
                    task_id: dep.task_id.clone(),
                    agent_id: dep.agent_id.clone(),
                    status: dep.status.clone(),
                    output: dep.output.clone(),
                    modified_files,
                    diff_stat,
                });
            }

            if ws.tasks[index].isolated_worktree {
                match Self::setup_isolated_worktree_with_dependencies(
                    &ws.project_path,
                    &ws.workspace_id,
                    id,
                    ws.tasks[index].attempts,
                    &upstream_refs,
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
                dependency_context: dep_evidence,
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
            let objective_message = format_task_prompt(
                &job.objective,
                job.worktree.as_deref(),
                &ws.project_path,
                &job.dependency_context,
            );
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
            let vault_keys = vault_keys_snapshot.clone();
            let child_db = db.clone();
            let child_token = token.child_token();
            futures.push(async move {
                // Resolve each child's provider route and credential from the
                // one authoritative resolver instead of silently sending
                // every child model to the parent's endpoint with the
                // parent's credentials (Bug 15). An explicit caller-supplied
                // base URL (tests, self-hosted proxies) keeps its authority
                // for both endpoint and credentials.
                let (child_base_url, child_api_key) = match base_url {
                    Some(explicit) => (Some(explicit.to_string()), api_key),
                    None => match ModelRouter::resolve_model_route(&job.model) {
                        Ok(route) => {
                            let key = ModelRouter::select_api_key_for_route(&route, &vault_keys)
                                .unwrap_or_default();
                            (Some(route.base_url), key)
                        }
                        Err(reason) => (None, format!("__routing_error__{}", reason)),
                    },
                };
                if child_api_key.is_empty() || child_api_key.starts_with("__routing_error__") {
                    let reason = child_api_key
                        .strip_prefix("__routing_error__")
                        .map(|r| r.to_string())
                        .unwrap_or_else(|| {
                            format!(
                                "No API key configured for provider of model '{}'.",
                                job.model
                            )
                        });
                    return (child_run_id, agent_id, Err(reason));
                }
                let outcome = engine
                    .run_step_with_connectors(
                        &child_run_id,
                        &job.model,
                        &child_api_key,
                        child_base_url.as_deref(),
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
                    if let Some(ref dir) = t.isolated_worktree_path {
                        let _ = snapshot_worktree_changes(dir, &t.task_id);
                    }
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
    /// not. Inherits candidate changes from upstream isolated dependencies.
    pub fn setup_isolated_worktree(
        project_root: &Path,
        workspace_id: &str,
        task_id: &str,
        attempt: u32,
    ) -> Result<(PathBuf, Option<String>, Option<String>), String> {
        Self::setup_isolated_worktree_with_dependencies(
            project_root,
            workspace_id,
            task_id,
            attempt,
            &[],
        )
    }

    /// Set up an isolated working environment for a task, cleanly transferring
    /// candidate changes from completed upstream tasks while detecting conflicts.
    pub fn setup_isolated_worktree_with_dependencies(
        project_root: &Path,
        workspace_id: &str,
        task_id: &str,
        attempt: u32,
        upstream_tasks: &[&SpecialistTask],
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

        let upstream_isolated: Vec<&&SpecialistTask> = upstream_tasks
            .iter()
            .filter(|t| t.isolated_worktree && t.isolated_worktree_path.is_some())
            .collect();

        // 1. Snapshot any uncommitted working tree changes in upstream worktrees
        for up in &upstream_isolated {
            if let Some(ref up_dir) = up.isolated_worktree_path {
                if up_dir.exists() {
                    let _ = snapshot_worktree_changes(up_dir, &up.task_id);
                }
            }
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

            // 2. Conflict detection: if multiple upstreams modified the same file, reject early
            if upstream_isolated.len() > 1 {
                for i in 0..upstream_isolated.len() {
                    for j in (i + 1)..upstream_isolated.len() {
                        let path_i = upstream_isolated[i]
                            .isolated_worktree_path
                            .as_ref()
                            .unwrap();
                        let path_j = upstream_isolated[j]
                            .isolated_worktree_path
                            .as_ref()
                            .unwrap();
                        let files_i = get_worktree_changed_files(
                            project_root,
                            path_i,
                            upstream_isolated[i].base_commit.as_deref(),
                        );
                        let files_j = get_worktree_changed_files(
                            project_root,
                            path_j,
                            upstream_isolated[j].base_commit.as_deref(),
                        );
                        let overlap: Vec<String> = files_i
                            .into_iter()
                            .filter(|f| files_j.contains(f))
                            .collect();
                        if !overlap.is_empty() {
                            return Err(format!(
                                "Dependency conflict: upstream tasks '{}' and '{}' both modified: {}",
                                upstream_isolated[i].task_id,
                                upstream_isolated[j].task_id,
                                overlap.join(", ")
                            ));
                        }
                    }
                }
            }

            if worktree_dir.exists() {
                let _ = fs::remove_dir_all(&worktree_dir);
            }

            // 3. Determine start point and base commit
            let (start_point, base_commit) = if let Some(first_up) = upstream_isolated.first() {
                let start = first_up
                    .worktree_branch
                    .clone()
                    .unwrap_or_else(|| "HEAD".to_string());
                let base = first_up.base_commit.clone().or_else(|| parent_head.clone());
                (start, base)
            } else {
                ("HEAD".to_string(), parent_head.clone())
            };

            let output = Command::new("git")
                .arg("-C")
                .arg(project_root)
                .args(["worktree", "add", "-b", &branch_name])
                .arg(&worktree_dir)
                .arg(&start_point)
                .output();

            match output {
                Ok(out) if out.status.success() => {
                    // Merge any additional upstream branches into this new worktree branch
                    if upstream_isolated.len() > 1 {
                        for other_up in &upstream_isolated[1..] {
                            if let Some(ref other_branch) = other_up.worktree_branch {
                                let merge_out = Command::new("git")
                                    .arg("-C")
                                    .arg(&worktree_dir)
                                    .args([
                                        "merge",
                                        "--no-ff",
                                        "-m",
                                        &format!(
                                            "Merge non-conflicting changes from task '{}'",
                                            other_up.task_id
                                        ),
                                        other_branch,
                                    ])
                                    .output();
                                match merge_out {
                                    Ok(m) if m.status.success() => {}
                                    Ok(m) => {
                                        let err = String::from_utf8_lossy(&m.stderr);
                                        return Err(format!(
                                            "Failed to merge upstream branch '{}': {}",
                                            other_branch, err
                                        ));
                                    }
                                    Err(e) => {
                                        return Err(format!(
                                            "Failed to run git merge for '{}': {}",
                                            other_branch, e
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    Ok((worktree_dir, Some(branch_name), base_commit))
                }
                Ok(out) => {
                    let err = String::from_utf8_lossy(&out.stderr);
                    log::warn!(
                        "[Workspace] git worktree add failed ({}): falling back to shadow copy",
                        err
                    );
                    Self::setup_shadow_from_upstreams(
                        project_root,
                        &worktree_dir,
                        &upstream_isolated,
                    )?;
                    Ok((worktree_dir, None, base_commit))
                }
                Err(e) => {
                    log::warn!(
                        "[Workspace] Failed to run git worktree add: {}: falling back to shadow copy",
                        e
                    );
                    Self::setup_shadow_from_upstreams(
                        project_root,
                        &worktree_dir,
                        &upstream_isolated,
                    )?;
                    Ok((worktree_dir, None, base_commit))
                }
            }
        } else {
            Self::setup_shadow_from_upstreams(project_root, &worktree_dir, &upstream_isolated)?;
            Ok((worktree_dir, None, None))
        }
    }

    /// Selects the terminal candidate tasks in the dependency graph.
    /// If candidate tasks form a linear pipeline, selects the final completed stage.
    /// If multiple independent terminal tasks exist, returns all of them.
    pub fn find_terminal_candidates(ws: &MultiAgentWorkspace) -> Vec<&SpecialistTask> {
        let worktree_tasks: Vec<&SpecialistTask> = ws
            .tasks
            .iter()
            .filter(|t| t.isolated_worktree_path.is_some())
            .collect();
        if worktree_tasks.is_empty() {
            return Vec::new();
        }

        let completed: Vec<&SpecialistTask> = worktree_tasks
            .iter()
            .filter(|t| t.status == "completed")
            .copied()
            .collect();
        let pool = if !completed.is_empty() {
            completed
        } else {
            worktree_tasks
        };

        let mut depended_on = HashSet::new();
        for t in &pool {
            for dep in &t.depends_on {
                depended_on.insert(dep.as_str());
            }
        }

        let terminal: Vec<&SpecialistTask> = pool
            .into_iter()
            .filter(|t| !depended_on.contains(t.task_id.as_str()))
            .collect();

        terminal
    }

    fn setup_shadow_from_upstreams(
        project_root: &Path,
        worktree_dir: &Path,
        upstream_isolated: &[&&SpecialistTask],
    ) -> Result<(), String> {
        if upstream_isolated.is_empty() {
            Self::fallback_shadow_copy(project_root, worktree_dir)?;
        } else {
            let first_dir = upstream_isolated[0]
                .isolated_worktree_path
                .as_ref()
                .unwrap();
            Self::fallback_shadow_copy(first_dir, worktree_dir)?;
            if upstream_isolated.len() > 1 {
                for other_up in &upstream_isolated[1..] {
                    let other_dir = other_up.isolated_worktree_path.as_ref().unwrap();
                    let changed_files = get_worktree_changed_files(
                        project_root,
                        other_dir,
                        other_up.base_commit.as_deref(),
                    );
                    for file_rel in changed_files {
                        let src_file = other_dir.join(&file_rel);
                        let dst_file = worktree_dir.join(&file_rel);
                        if src_file.is_file() {
                            if let Some(parent) = dst_file.parent() {
                                fs::create_dir_all(parent)
                                    .map_err(|e| format!("Failed to create parent dir: {}", e))?;
                            }
                            fs::copy(&src_file, &dst_file)
                                .map_err(|e| format!("Failed to copy upstream file: {}", e))?;
                        }
                    }
                }
            }
        }

        // Record baseline manifest and file hashes so non-git reconciliation can detect
        // additions, modifications, deletions, and concurrent external changes safely.
        let mut baseline_files = HashMap::new();
        collect_shadow_manifest(project_root, Path::new(""), &mut baseline_files)?;
        let manifest = ShadowManifest {
            timestamp: chrono::Utc::now().timestamp_millis(),
            files: baseline_files,
        };
        let json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| format!("Failed to serialize shadow manifest: {}", e))?;
        fs::write(worktree_dir.join(SHADOW_MANIFEST_FILENAME), json)
            .map_err(|e| format!("Failed to write shadow baseline manifest: {}", e))?;

        Ok(())
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
    /// Traverses robustly and surfaces I/O and permission errors.
    pub fn fallback_shadow_copy(src: &Path, dst: &Path) -> Result<(), String> {
        fs::create_dir_all(dst).map_err(|e| {
            format!(
                "Failed to create destination directory {}: {}",
                dst.display(),
                e
            )
        })?;
        let read_dir = fs::read_dir(src)
            .map_err(|e| format!("Failed to read source directory {}: {}", src.display(), e))?;

        for entry_res in read_dir {
            let entry = entry_res.map_err(|e| {
                format!("Failed to read directory entry in {}: {}", src.display(), e)
            })?;
            let path = entry.path();
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();

            // Skip heavy build or vcs folders and shadow manifest metadata
            if name_str == "node_modules"
                || name_str == "target"
                || name_str == ".git"
                || name_str == "dist"
                || name_str == SHADOW_MANIFEST_FILENAME
            {
                continue;
            }

            let target = dst.join(&file_name);
            let file_type = entry
                .file_type()
                .map_err(|e| format!("Failed to inspect {}: {}", path.display(), e))?;
            if file_type.is_file() {
                fs::copy(&path, &target).map_err(|e| {
                    format!(
                        "Failed to copy {} to {}: {}",
                        path.display(),
                        target.display(),
                        e
                    )
                })?;
            } else if file_type.is_dir() {
                Self::fallback_shadow_copy(&path, &target)?;
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

        let _ = snapshot_worktree_changes(worktree_path, "reconcile");

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

        // Generic directory diff for non-git projects with baseline manifest and conflict detection
        let manifest_path = worktree_path.join(SHADOW_MANIFEST_FILENAME);
        let baseline_files = if manifest_path.exists() {
            let content = fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
            let manifest: ShadowManifest =
                serde_json::from_str(&content).map_err(|e| e.to_string())?;
            manifest.files
        } else {
            let mut base = HashMap::new();
            collect_shadow_manifest(project_root, Path::new(""), &mut base)?;
            base
        };

        let mut candidate_files = HashMap::new();
        collect_shadow_manifest(worktree_path, Path::new(""), &mut candidate_files)?;

        let mut current_project_files = HashMap::new();
        collect_shadow_manifest(project_root, Path::new(""), &mut current_project_files)?;

        let mut added = Vec::new();
        let mut modified = Vec::new();
        let mut deleted = Vec::new();
        let mut conflicts = Vec::new();
        let mut diff_builder = String::new();

        // 1. Candidate modifications & additions relative to baseline
        for (file_rel, cand_hash) in &candidate_files {
            match baseline_files.get(file_rel) {
                Some(base_hash) => {
                    if cand_hash != base_hash {
                        modified.push(file_rel.clone());
                        if let Some(curr_hash) = current_project_files.get(file_rel) {
                            if curr_hash != base_hash {
                                conflicts.push(format!(
                                    "Concurrent modification in project root: {}",
                                    file_rel
                                ));
                            }
                        } else {
                            conflicts.push(format!(
                                "Concurrent external deletion in project root: {}",
                                file_rel
                            ));
                        }

                        let base_content =
                            fs::read_to_string(project_root.join(file_rel)).unwrap_or_default();
                        let cand_content =
                            fs::read_to_string(worktree_path.join(file_rel)).unwrap_or_default();
                        diff_builder.push_str(&format!("--- a/{}\n+++ b/{}\n", file_rel, file_rel));
                        for line in cand_content.lines() {
                            if !base_content.contains(line) {
                                diff_builder.push_str(&format!("+{}\n", line));
                            }
                        }
                    }
                }
                None => {
                    added.push(file_rel.clone());
                    if current_project_files.contains_key(file_rel) {
                        conflicts
                            .push(format!("Concurrent creation in project root: {}", file_rel));
                    }
                    let cand_content =
                        fs::read_to_string(worktree_path.join(file_rel)).unwrap_or_default();
                    diff_builder.push_str(&format!("--- /dev/null\n+++ b/{}\n", file_rel));
                    for line in cand_content.lines() {
                        diff_builder.push_str(&format!("+{}\n", line));
                    }
                }
            }
        }

        // 2. Candidate deletions
        for (file_rel, base_hash) in &baseline_files {
            if !candidate_files.contains_key(file_rel) {
                deleted.push(file_rel.clone());
                if let Some(curr_hash) = current_project_files.get(file_rel) {
                    if curr_hash != base_hash {
                        conflicts.push(format!(
                            "Concurrent modification of file deleted by candidate: {}",
                            file_rel
                        ));
                    }
                }
                diff_builder.push_str(&format!(
                    "--- a/{}\n+++ /dev/null\n@@ -1 +0,0 @@\n-(file deleted)\n",
                    file_rel
                ));
            }
        }

        let mut all_modified = Vec::new();
        all_modified.extend(modified.clone());
        all_modified.extend(added.clone());
        all_modified.extend(deleted.clone());
        all_modified.sort();

        let changes_detected = !all_modified.is_empty();
        let can_safely_merge = changes_detected && conflicts.is_empty();
        let summary = if !conflicts.is_empty() {
            format!(
                "Non-git conflicts detected in {} file(s) between shadow workspace and project root. Manual resolution required.",
                conflicts.len()
            )
        } else if changes_detected {
            format!(
                "Non-git changes successfully detected: {} modified, {} added, {} deleted.",
                modified.len(),
                added.len(),
                deleted.len()
            )
        } else {
            "No code modifications made in isolated worktree.".to_string()
        };

        Ok(ReconciliationOutcome {
            workspace_id: workspace_id.to_string(),
            changes_detected,
            modified_files: all_modified,
            unified_diff: diff_builder,
            conflicts_detected: conflicts,
            can_safely_merge,
            reconciliation_summary: summary,
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

        let _ = snapshot_worktree_changes(worktree_path, "reconcile");

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
                // If branch_name is None, diff against base_commit or HEAD~1 to avoid empty diff against HEAD
                let diff_target = "HEAD~1";
                let diff_out = Command::new("git")
                    .arg("-C")
                    .arg(worktree_path)
                    .args(["diff", diff_target])
                    .output()
                    .map_err(|e| format!("Failed to generate diff: {}", e))?;

                let patch_bytes = if !diff_out.stdout.is_empty() {
                    diff_out.stdout
                } else {
                    // Fall back to git diff HEAD
                    Command::new("git")
                        .arg("-C")
                        .arg(worktree_path)
                        .args(["diff", "HEAD"])
                        .output()
                        .map_err(|e| format!("Failed to generate diff: {}", e))?
                        .stdout
                };

                if patch_bytes.is_empty() {
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
                    let _ = stdin.write_all(&patch_bytes);
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
            // Non-git safe apply:
            // 1. Re-verify for concurrent modifications
            let manifest_path = worktree_path.join(SHADOW_MANIFEST_FILENAME);
            let baseline_files = if manifest_path.exists() {
                let content = fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
                let manifest: ShadowManifest =
                    serde_json::from_str(&content).map_err(|e| e.to_string())?;
                manifest.files
            } else {
                HashMap::new()
            };

            let mut candidate_files = HashMap::new();
            collect_shadow_manifest(worktree_path, Path::new(""), &mut candidate_files)?;

            let mut current_project_files = HashMap::new();
            collect_shadow_manifest(project_root, Path::new(""), &mut current_project_files)?;

            // Fail closed if destination files were concurrently modified
            if !baseline_files.is_empty() {
                for (file_rel, cand_hash) in &candidate_files {
                    if let Some(base_hash) = baseline_files.get(file_rel) {
                        if cand_hash != base_hash {
                            if let Some(curr_hash) = current_project_files.get(file_rel) {
                                if curr_hash != base_hash {
                                    return Err(format!("Cannot apply reconciliation: file '{}' was concurrently modified in project root", file_rel));
                                }
                            }
                        }
                    } else if current_project_files.contains_key(file_rel) {
                        return Err(format!("Cannot apply reconciliation: file '{}' was concurrently created in project root", file_rel));
                    }
                }
            }

            // 2. Apply ONLY candidate-modified and added files
            let mut applied_count = 0;
            for (file_rel, cand_hash) in &candidate_files {
                let is_changed = baseline_files
                    .get(file_rel)
                    .map(|h| h != cand_hash)
                    .unwrap_or(true);
                if is_changed {
                    let src = worktree_path.join(file_rel);
                    let dst = project_root.join(file_rel);
                    if let Some(parent) = dst.parent() {
                        fs::create_dir_all(parent)
                            .map_err(|e| format!("Failed to create parent dir: {}", e))?;
                    }
                    fs::copy(&src, &dst).map_err(|e| {
                        format!(
                            "Failed to copy {} to {}: {}",
                            src.display(),
                            dst.display(),
                            e
                        )
                    })?;
                    applied_count += 1;
                }
            }

            // 3. Remove deleted files
            let mut deleted_count = 0;
            for file_rel in baseline_files.keys() {
                if !candidate_files.contains_key(file_rel) {
                    let dst = project_root.join(file_rel);
                    if dst.exists() {
                        let _ = fs::remove_file(&dst);
                        deleted_count += 1;
                    }
                }
            }

            // Clean up shadow manifest from destination if it exists
            let _ = fs::remove_file(project_root.join(SHADOW_MANIFEST_FILENAME));

            Ok(format!(
                "Successfully applied non-git changes: {} file(s) updated/added, {} file(s) removed.",
                applied_count, deleted_count
            ))
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

    /// Stage and commit any dirty working tree or untracked changes in an isolated worktree
    /// so they are durable on its branch and available for inheritance or diffing.
    pub fn snapshot_worktree_changes(worktree_path: &Path, task_id: &str) -> bool {
        snapshot_worktree_changes(worktree_path, task_id)
    }

    /// Retrieve the list of files modified, added, or deleted in an isolated worktree.
    pub fn get_worktree_changed_files(
        project_root: &Path,
        worktree_path: &Path,
        base_commit: Option<&str>,
    ) -> Vec<String> {
        get_worktree_changed_files(project_root, worktree_path, base_commit)
    }

    /// Retrieve the diff stat summary of an isolated worktree.
    pub fn get_worktree_diff_stat(
        worktree_path: &Path,
        base_commit: Option<&str>,
    ) -> Option<String> {
        get_worktree_diff_stat(worktree_path, base_commit)
    }

    /// Compose the bounded user message for a workflow task, injecting upstream context,
    /// candidate change evidence, and worktree isolation instructions.
    pub fn format_task_prompt(
        objective: &str,
        worktree: Option<&Path>,
        project_path: &Path,
        dependencies: &[DependencyEvidence],
    ) -> String {
        format_task_prompt(objective, worktree, project_path, dependencies)
    }
}

/// Stage and commit any dirty working tree or untracked changes in an isolated worktree
/// so they are durable on its branch and available for inheritance or diffing.
pub fn snapshot_worktree_changes(worktree_path: &Path, task_id: &str) -> bool {
    if !worktree_path.exists() {
        return false;
    }
    let has_git = worktree_path.join(".git").exists();
    if !has_git {
        return false;
    }
    let status_out = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(["status", "--porcelain"])
        .output();
    let Ok(status) = status_out else {
        return false;
    };
    if status.status.success() && !status.stdout.is_empty() {
        let _ = Command::new("git")
            .arg("-C")
            .arg(worktree_path)
            .args(["add", "-A"])
            .output();
        let commit_res = Command::new("git")
            .arg("-C")
            .arg(worktree_path)
            .args([
                "-c",
                "user.name=Kyro Agent",
                "-c",
                "user.email=agent@kyro.local",
                "commit",
                "-m",
                &format!("Snapshot candidate changes from task '{}'", task_id),
            ])
            .output();
        return commit_res.map(|c| c.status.success()).unwrap_or(false);
    }
    false
}

/// Retrieve the list of files modified, added, or deleted in an isolated worktree
/// relative to the given base commit or project root.
pub fn get_worktree_changed_files(
    project_root: &Path,
    worktree_path: &Path,
    base_commit: Option<&str>,
) -> Vec<String> {
    if !worktree_path.exists() {
        return Vec::new();
    }
    if worktree_path.join(".git").exists() {
        let mut files = HashSet::new();
        let target = base_commit.unwrap_or("HEAD");
        if let Ok(out) = Command::new("git")
            .arg("-C")
            .arg(worktree_path)
            .args(["diff", "--name-only", target])
            .output()
        {
            if out.status.success() {
                for line in String::from_utf8_lossy(&out.stdout).lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        files.insert(trimmed.to_string());
                    }
                }
            }
        }
        if let Ok(out) = Command::new("git")
            .arg("-C")
            .arg(worktree_path)
            .args(["status", "--porcelain"])
            .output()
        {
            if out.status.success() {
                for line in String::from_utf8_lossy(&out.stdout).lines() {
                    let trimmed = line.trim();
                    if trimmed.len() >= 3 {
                        let p = trimmed[3..].trim();
                        let f = if let Some((_, dest)) = p.split_once(" -> ") {
                            dest.trim()
                        } else {
                            p
                        };
                        if !f.is_empty() {
                            files.insert(f.to_string());
                        }
                    }
                }
            }
        }
        let mut v: Vec<String> = files.into_iter().collect();
        v.sort();
        v
    } else {
        let mut files = Vec::new();
        collect_dir_diff(project_root, worktree_path, Path::new(""), &mut files);
        files.sort();
        files
    }
}

fn collect_dir_diff(base: &Path, modified: &Path, rel: &Path, out: &mut Vec<String>) {
    let mod_dir = modified.join(rel);
    let Ok(entries) = fs::read_dir(&mod_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();
        if name_str == ".git" || name_str == "node_modules" || name_str == "target" {
            continue;
        }
        let child_rel = if rel.as_os_str().is_empty() {
            PathBuf::from(&file_name)
        } else {
            rel.join(&file_name)
        };
        let base_file = base.join(&child_rel);
        let mod_file = mod_dir.join(&file_name);
        if mod_file.is_dir() {
            collect_dir_diff(base, modified, &child_rel, out);
        } else if mod_file.is_file() {
            if !base_file.exists() {
                out.push(child_rel.to_string_lossy().to_string());
            } else if let (Ok(m1), Ok(m2)) = (fs::read(&base_file), fs::read(&mod_file)) {
                if m1 != m2 {
                    out.push(child_rel.to_string_lossy().to_string());
                }
            }
        }
    }
}

/// Retrieve the diff stat summary of an isolated worktree.
pub fn get_worktree_diff_stat(worktree_path: &Path, base_commit: Option<&str>) -> Option<String> {
    if !worktree_path.join(".git").exists() {
        return None;
    }
    let target = base_commit.unwrap_or("HEAD");
    if let Ok(out) = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(["diff", "--stat", target])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

/// Compose the bounded user message for a workflow task, injecting upstream context,
/// candidate change evidence, and worktree isolation instructions.
pub fn format_task_prompt(
    objective: &str,
    worktree: Option<&Path>,
    project_path: &Path,
    dependencies: &[DependencyEvidence],
) -> String {
    let mut message = format!("# Task Objective\n{}\n", objective);

    if let Some(dir) = worktree {
        message.push_str(&format!(
            "\n# Workspace Isolation\nYour working directory is the isolated worktree at `{}`.\nAll file edits and test commands must be executed within this directory.\nDo not modify the original project tree at `{}`.\n",
            dir.display(),
            project_path.display()
        ));
    }

    if !dependencies.is_empty() {
        message.push_str("\n# Upstream Dependency Context & Candidate Changes\n");
        message.push_str("The following upstream stages have completed and produced candidate outputs and changes:\n\n");
        for dep in dependencies {
            message.push_str(&format!(
                "## Stage: `{}` (Agent: @{})\n",
                dep.task_id, dep.agent_id
            ));
            message.push_str(&format!("- Status: {}\n", dep.status));
            if !dep.modified_files.is_empty() {
                message.push_str("- Modified / Added Files in Candidate Change:\n");
                for f in &dep.modified_files {
                    message.push_str(&format!("  - `{}`\n", f));
                }
            }
            if let Some(ref stat) = dep.diff_stat {
                if !stat.trim().is_empty() {
                    message.push_str(&format!("- Change Summary:\n```\n{}\n```\n", stat.trim()));
                }
            }
            if let Some(ref out) = dep.output {
                let trimmed = out.trim();
                if !trimmed.is_empty() {
                    let bounded = if trimmed.chars().count() > 8000 {
                        let head: String = trimmed.chars().take(8000).collect();
                        format!("{}...\n[Output truncated at 8000 characters]", head)
                    } else {
                        trimmed.to_string()
                    };
                    message.push_str(&format!("- Output Deliverable:\n{}\n\n", bounded));
                }
            }
        }
    }

    message
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
        assert_eq!(spec.tasks.len(), 5);
        assert_eq!(spec.tasks[1].depends_on, vec!["investigate"]);
        assert!(spec.tasks[1].isolated_worktree);
        assert_eq!(spec.tasks[2].agent_id, "test-regression-specialist");
        assert_eq!(spec.tasks[3].agent_id, "code-reviewer");
        assert_eq!(spec.tasks[4].agent_id, "documentation-reviewer");
        let order = topological_order(&spec).unwrap();
        let pos = |id: &str| order.iter().position(|t| t == id).unwrap();
        assert!(pos("investigate") < pos("implement"));
        assert!(pos("implement") < pos("verify"));
        assert!(pos("verify") < pos("review"));
        assert!(pos("review") < pos("documentation"));
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

    #[test]
    fn workflow_handoff_carries_candidate_changes_across_isolated_stages_in_git() {
        let test_dir =
            std::env::temp_dir().join(format!("kyro_handoff_git_{}", uuid::Uuid::new_v4()));
        let project_root = test_dir.join("project");
        fs::create_dir_all(&project_root).unwrap();

        // 1. Initialize git repo with baseline commit
        let _ = Command::new("git")
            .arg("-C")
            .arg(&project_root)
            .args(["init"])
            .output()
            .unwrap();
        fs::write(project_root.join("README.md"), "Initial project\n").unwrap();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&project_root)
            .args(["add", "README.md"])
            .output()
            .unwrap();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&project_root)
            .args([
                "-c",
                "user.name=Tester",
                "-c",
                "user.email=test@test.local",
                "commit",
                "-m",
                "Initial commit",
            ])
            .output()
            .unwrap();

        let ws_id = format!("ws_{}", uuid::Uuid::new_v4());

        // Stage 1: Implementer runs in isolated worktree
        let (impl_dir, impl_branch, impl_base) =
            MultiAgentOrchestrator::setup_isolated_worktree_with_dependencies(
                &project_root,
                &ws_id,
                "implement",
                1,
                &[],
            )
            .expect("implement worktree must succeed");

        // Implementer modifies README.md and creates calculator.rs
        fs::write(
            impl_dir.join("README.md"),
            "Initial project\nModified by Implementer\n",
        )
        .unwrap();
        fs::create_dir_all(impl_dir.join("src")).unwrap();
        fs::write(
            impl_dir.join("src/calculator.rs"),
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        )
        .unwrap();

        // Verification requirement 6: original project root remains UNCHANGED!
        assert_eq!(
            fs::read_to_string(project_root.join("README.md")).unwrap(),
            "Initial project\n"
        );
        assert!(!project_root.join("src/calculator.rs").exists());

        // Snapshot changes in implement worktree
        assert!(MultiAgentOrchestrator::snapshot_worktree_changes(
            &impl_dir,
            "implement"
        ));

        let impl_task = SpecialistTask {
            task_id: "implement".to_string(),
            agent_id: "forge-devops".to_string(),
            objective: "Implement calculator".to_string(),
            depends_on: vec![],
            optional: false,
            isolated_worktree: true,
            max_attempts: 1,
            attempts: 1,
            status: "completed".to_string(),
            child_run_id: Some("c1".to_string()),
            isolated_worktree_path: Some(impl_dir.clone()),
            worktree_branch: impl_branch.clone(),
            base_commit: impl_base.clone(),
            output: Some("Added calculator.rs and updated README".to_string()),
            error: None,
            started_at: Some(10),
            completed_at: Some(20),
        };

        // Stage 2: Verifier runs in its own isolated worktree depending on implement
        let (verify_dir, verify_branch, verify_base) =
            MultiAgentOrchestrator::setup_isolated_worktree_with_dependencies(
                &project_root,
                &ws_id,
                "verify",
                1,
                &[&impl_task],
            )
            .expect("verify worktree must succeed and inherit candidate changes");

        // Verification requirement 3: verifier can inspect implementation's actual changes
        assert!(verify_dir.join("src/calculator.rs").exists());
        assert_eq!(
            fs::read_to_string(verify_dir.join("src/calculator.rs")).unwrap(),
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n"
        );
        assert!(fs::read_to_string(verify_dir.join("README.md"))
            .unwrap()
            .contains("Modified by Implementer"));

        // Verifier adds test suite to candidate changes
        fs::create_dir_all(verify_dir.join("tests")).unwrap();
        fs::write(
            verify_dir.join("tests/test_calculator.rs"),
            "#[test] fn test_add() { assert_eq!(add(2, 2), 4); }\n",
        )
        .unwrap();

        assert!(MultiAgentOrchestrator::snapshot_worktree_changes(
            &verify_dir,
            "verify"
        ));

        let verify_task = SpecialistTask {
            task_id: "verify".to_string(),
            agent_id: "test-regression-specialist".to_string(),
            objective: "Verify calculator".to_string(),
            depends_on: vec!["implement".to_string()],
            optional: false,
            isolated_worktree: true,
            max_attempts: 1,
            attempts: 1,
            status: "completed".to_string(),
            child_run_id: Some("c2".to_string()),
            isolated_worktree_path: Some(verify_dir.clone()),
            worktree_branch: verify_branch.clone(),
            base_commit: verify_base.clone(),
            output: Some("All 1 unit tests passed with 100% assertion coverage".to_string()),
            error: None,
            started_at: Some(25),
            completed_at: Some(35),
        };

        // Stage 3: Reviewer runs in its own isolated worktree depending on verify
        let (review_dir, review_branch, _review_base) =
            MultiAgentOrchestrator::setup_isolated_worktree_with_dependencies(
                &project_root,
                &ws_id,
                "review",
                1,
                &[&verify_task],
            )
            .expect("review worktree must succeed and inherit verified candidate changes");

        // Verification requirement 4: reviewer can inspect candidate changes AND tests
        assert!(review_dir.join("src/calculator.rs").exists());
        assert!(review_dir.join("tests/test_calculator.rs").exists());
        assert!(fs::read_to_string(review_dir.join("README.md"))
            .unwrap()
            .contains("Modified by Implementer"));

        // Check reconciliation analysis on reviewer's candidate worktree
        let outcome = MultiAgentOrchestrator::reconcile_changes(&project_root, &review_dir, &ws_id)
            .expect("reconcile_changes must succeed");

        assert!(outcome.changes_detected);
        assert!(outcome.can_safely_merge);
        assert!(outcome.conflicts_detected.is_empty());
        assert!(outcome
            .modified_files
            .iter()
            .any(|f| f.ends_with("README.md")));
        assert!(outcome
            .modified_files
            .iter()
            .any(|f| f.ends_with("calculator.rs")));
        assert!(outcome
            .modified_files
            .iter()
            .any(|f| f.ends_with("test_calculator.rs")));
        assert!(outcome.unified_diff.contains("pub fn add"));
        assert!(outcome.unified_diff.contains("test_add"));

        // Verification requirement 6: original project root STILL untouched before reconciliation!
        assert_eq!(
            fs::read_to_string(project_root.join("README.md")).unwrap(),
            "Initial project\n"
        );
        assert!(!project_root.join("src/calculator.rs").exists());

        // Verification requirement 7: final reconciliation integrates the work cleanly
        let apply_res = MultiAgentOrchestrator::apply_reconciliation(
            &project_root,
            &review_dir,
            review_branch.as_deref(),
        )
        .expect("apply_reconciliation must succeed");

        assert!(apply_res.contains("Successfully merged branch"));
        assert_eq!(
            fs::read_to_string(project_root.join("src/calculator.rs")).unwrap(),
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n"
        );
        assert_eq!(
            fs::read_to_string(project_root.join("tests/test_calculator.rs")).unwrap(),
            "#[test] fn test_add() { assert_eq!(add(2, 2), 4); }\n"
        );

        // Cleanup isolated worktrees
        MultiAgentOrchestrator::cleanup_isolated_worktree(
            &project_root,
            &impl_dir,
            impl_branch.as_deref(),
        );
        MultiAgentOrchestrator::cleanup_isolated_worktree(
            &project_root,
            &verify_dir,
            verify_branch.as_deref(),
        );
        MultiAgentOrchestrator::cleanup_isolated_worktree(
            &project_root,
            &review_dir,
            review_branch.as_deref(),
        );
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn workflow_handoff_detects_conflicts_between_concurrent_dependencies() {
        let test_dir =
            std::env::temp_dir().join(format!("kyro_conflict_test_{}", uuid::Uuid::new_v4()));
        let project_root = test_dir.join("project");
        fs::create_dir_all(&project_root).unwrap();

        let _ = Command::new("git")
            .arg("-C")
            .arg(&project_root)
            .args(["init"])
            .output()
            .unwrap();
        fs::write(project_root.join("shared.txt"), "Original content\n").unwrap();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&project_root)
            .args(["add", "shared.txt"])
            .output()
            .unwrap();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&project_root)
            .args([
                "-c",
                "user.name=Tester",
                "-c",
                "user.email=test@test.local",
                "commit",
                "-m",
                "Initial",
            ])
            .output()
            .unwrap();

        let ws_id = format!("ws_{}", uuid::Uuid::new_v4());

        // Upstream Task 1
        let (dir_1, branch_1, base_1) =
            MultiAgentOrchestrator::setup_isolated_worktree_with_dependencies(
                &project_root,
                &ws_id,
                "task_1",
                1,
                &[],
            )
            .unwrap();
        fs::write(dir_1.join("shared.txt"), "Branch 1 edit\n").unwrap();
        MultiAgentOrchestrator::snapshot_worktree_changes(&dir_1, "task_1");

        let task_1 = SpecialistTask {
            task_id: "task_1".to_string(),
            agent_id: "forge-devops".to_string(),
            objective: "Edit shared".to_string(),
            depends_on: vec![],
            optional: false,
            isolated_worktree: true,
            max_attempts: 1,
            attempts: 1,
            status: "completed".to_string(),
            child_run_id: Some("c1".to_string()),
            isolated_worktree_path: Some(dir_1.clone()),
            worktree_branch: branch_1.clone(),
            base_commit: base_1.clone(),
            output: Some("Edited shared".to_string()),
            error: None,
            started_at: Some(1),
            completed_at: Some(2),
        };

        // Upstream Task 2 modifies the SAME file
        let (dir_2, branch_2, base_2) =
            MultiAgentOrchestrator::setup_isolated_worktree_with_dependencies(
                &project_root,
                &ws_id,
                "task_2",
                1,
                &[],
            )
            .unwrap();
        fs::write(dir_2.join("shared.txt"), "Branch 2 conflicting edit\n").unwrap();
        MultiAgentOrchestrator::snapshot_worktree_changes(&dir_2, "task_2");

        let task_2 = SpecialistTask {
            task_id: "task_2".to_string(),
            agent_id: "forge-devops".to_string(),
            objective: "Conflicting edit".to_string(),
            depends_on: vec![],
            optional: false,
            isolated_worktree: true,
            max_attempts: 1,
            attempts: 1,
            status: "completed".to_string(),
            child_run_id: Some("c2".to_string()),
            isolated_worktree_path: Some(dir_2.clone()),
            worktree_branch: branch_2.clone(),
            base_commit: base_2.clone(),
            output: Some("Also edited shared".to_string()),
            error: None,
            started_at: Some(3),
            completed_at: Some(4),
        };

        // Downstream Task 3 depends on BOTH task_1 and task_2
        let err = MultiAgentOrchestrator::setup_isolated_worktree_with_dependencies(
            &project_root,
            &ws_id,
            "task_3",
            1,
            &[&task_1, &task_2],
        )
        .expect_err("conflicting dependencies must be detected and rejected");

        assert!(err.contains("Dependency conflict"), "Error: {}", err);
        assert!(err.contains("shared.txt"), "Error: {}", err);

        // Cleanup
        MultiAgentOrchestrator::cleanup_isolated_worktree(
            &project_root,
            &dir_1,
            branch_1.as_deref(),
        );
        MultiAgentOrchestrator::cleanup_isolated_worktree(
            &project_root,
            &dir_2,
            branch_2.as_deref(),
        );
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn workflow_handoff_merges_disjoint_concurrent_dependencies() {
        let test_dir =
            std::env::temp_dir().join(format!("kyro_disjoint_test_{}", uuid::Uuid::new_v4()));
        let project_root = test_dir.join("project");
        fs::create_dir_all(&project_root).unwrap();

        let _ = Command::new("git")
            .arg("-C")
            .arg(&project_root)
            .args(["init"])
            .output()
            .unwrap();
        fs::write(project_root.join("init.txt"), "Base\n").unwrap();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&project_root)
            .args(["add", "init.txt"])
            .output()
            .unwrap();
        let _ = Command::new("git")
            .arg("-C")
            .arg(&project_root)
            .args([
                "-c",
                "user.name=Tester",
                "-c",
                "user.email=test@test.local",
                "commit",
                "-m",
                "Initial",
            ])
            .output()
            .unwrap();

        let ws_id = format!("ws_{}", uuid::Uuid::new_v4());

        // Task A creates module_a.rs
        let (dir_a, branch_a, base_a) =
            MultiAgentOrchestrator::setup_isolated_worktree_with_dependencies(
                &project_root,
                &ws_id,
                "task_a",
                1,
                &[],
            )
            .unwrap();
        fs::write(dir_a.join("module_a.rs"), "pub fn a() -> bool { true }\n").unwrap();
        MultiAgentOrchestrator::snapshot_worktree_changes(&dir_a, "task_a");

        let task_a = SpecialistTask {
            task_id: "task_a".to_string(),
            agent_id: "forge-devops".to_string(),
            objective: "Add module A".to_string(),
            depends_on: vec![],
            optional: false,
            isolated_worktree: true,
            max_attempts: 1,
            attempts: 1,
            status: "completed".to_string(),
            child_run_id: Some("ca".to_string()),
            isolated_worktree_path: Some(dir_a.clone()),
            worktree_branch: branch_a.clone(),
            base_commit: base_a.clone(),
            output: Some("Created module A".to_string()),
            error: None,
            started_at: Some(1),
            completed_at: Some(2),
        };

        // Task B creates module_b.rs (disjoint file!)
        let (dir_b, branch_b, base_b) =
            MultiAgentOrchestrator::setup_isolated_worktree_with_dependencies(
                &project_root,
                &ws_id,
                "task_b",
                1,
                &[],
            )
            .unwrap();
        fs::write(dir_b.join("module_b.rs"), "pub fn b() -> bool { false }\n").unwrap();
        MultiAgentOrchestrator::snapshot_worktree_changes(&dir_b, "task_b");

        let task_b = SpecialistTask {
            task_id: "task_b".to_string(),
            agent_id: "forge-devops".to_string(),
            objective: "Add module B".to_string(),
            depends_on: vec![],
            optional: false,
            isolated_worktree: true,
            max_attempts: 1,
            attempts: 1,
            status: "completed".to_string(),
            child_run_id: Some("cb".to_string()),
            isolated_worktree_path: Some(dir_b.clone()),
            worktree_branch: branch_b.clone(),
            base_commit: base_b.clone(),
            output: Some("Created module B".to_string()),
            error: None,
            started_at: Some(3),
            completed_at: Some(4),
        };

        // Downstream Task C depends on both task_a and task_b
        let (dir_c, branch_c, _) =
            MultiAgentOrchestrator::setup_isolated_worktree_with_dependencies(
                &project_root,
                &ws_id,
                "task_c",
                1,
                &[&task_a, &task_b],
            )
            .expect("disjoint dependencies must merge cleanly into downstream worktree");

        // Task C receives both module_a.rs and module_b.rs!
        assert!(dir_c.join("module_a.rs").exists());
        assert!(dir_c.join("module_b.rs").exists());

        MultiAgentOrchestrator::cleanup_isolated_worktree(
            &project_root,
            &dir_a,
            branch_a.as_deref(),
        );
        MultiAgentOrchestrator::cleanup_isolated_worktree(
            &project_root,
            &dir_b,
            branch_b.as_deref(),
        );
        MultiAgentOrchestrator::cleanup_isolated_worktree(
            &project_root,
            &dir_c,
            branch_c.as_deref(),
        );
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn workflow_handoff_shadow_copy_mode_without_git() {
        let test_dir =
            std::env::temp_dir().join(format!("kyro_shadow_test_{}", uuid::Uuid::new_v4()));
        let project_root = test_dir.join("project");
        fs::create_dir_all(&project_root).unwrap();

        fs::write(project_root.join("config.json"), "{\"env\": \"prod\"}\n").unwrap();
        let ws_id = format!("ws_{}", uuid::Uuid::new_v4());

        // Stage 1 in shadow copy mode
        let (dir_1, branch_1, _) =
            MultiAgentOrchestrator::setup_isolated_worktree_with_dependencies(
                &project_root,
                &ws_id,
                "shadow_impl",
                1,
                &[],
            )
            .unwrap();
        assert!(branch_1.is_none()); // Non-git has no branch
        fs::write(dir_1.join("config.json"), "{\"env\": \"staging\"}\n").unwrap();
        fs::write(dir_1.join("new_shadow.txt"), "Shadow file\n").unwrap();

        let task_1 = SpecialistTask {
            task_id: "shadow_impl".to_string(),
            agent_id: "forge-devops".to_string(),
            objective: "Update config".to_string(),
            depends_on: vec![],
            optional: false,
            isolated_worktree: true,
            max_attempts: 1,
            attempts: 1,
            status: "completed".to_string(),
            child_run_id: Some("cs1".to_string()),
            isolated_worktree_path: Some(dir_1.clone()),
            worktree_branch: None,
            base_commit: None,
            output: Some("Config updated in shadow".to_string()),
            error: None,
            started_at: Some(1),
            completed_at: Some(2),
        };

        // Stage 2 receives shadow copy changes from Stage 1
        let (dir_2, _, _) = MultiAgentOrchestrator::setup_isolated_worktree_with_dependencies(
            &project_root,
            &ws_id,
            "shadow_verify",
            1,
            &[&task_1],
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(dir_2.join("config.json")).unwrap(),
            "{\"env\": \"staging\"}\n"
        );
        assert_eq!(
            fs::read_to_string(dir_2.join("new_shadow.txt")).unwrap(),
            "Shadow file\n"
        );

        // Reconcile shadow changes
        let outcome =
            MultiAgentOrchestrator::reconcile_changes(&project_root, &dir_2, &ws_id).unwrap();
        assert!(outcome.changes_detected);
        assert!(outcome.modified_files.contains(&"config.json".to_string()));
        assert!(outcome
            .modified_files
            .contains(&"new_shadow.txt".to_string()));

        // Apply reconciliation
        MultiAgentOrchestrator::apply_reconciliation(&project_root, &dir_2, None).unwrap();
        assert_eq!(
            fs::read_to_string(project_root.join("config.json")).unwrap(),
            "{\"env\": \"staging\"}\n"
        );
        assert_eq!(
            fs::read_to_string(project_root.join("new_shadow.txt")).unwrap(),
            "Shadow file\n"
        );

        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn format_task_prompt_constructs_bounded_evidence_message() {
        let worktree = PathBuf::from("/tmp/worktree_test");
        let project = PathBuf::from("/home/user/project");

        let long_output = "a".repeat(10_000);
        let deps = vec![
            DependencyEvidence {
                task_id: "investigate".to_string(),
                agent_id: "forge-devops".to_string(),
                status: "completed".to_string(),
                output: Some("Mapped 3 files and 2 modules".to_string()),
                modified_files: vec![],
                diff_stat: None,
            },
            DependencyEvidence {
                task_id: "implement".to_string(),
                agent_id: "forge-devops".to_string(),
                status: "completed".to_string(),
                output: Some(long_output),
                modified_files: vec!["src/lib.rs".to_string(), "Cargo.toml".to_string()],
                diff_stat: Some("2 files changed, 10 insertions(+)".to_string()),
            },
        ];

        let prompt = MultiAgentOrchestrator::format_task_prompt(
            "Verify all changes",
            Some(&worktree),
            &project,
            &deps,
        );

        assert!(prompt.contains("# Task Objective\nVerify all changes"));
        assert!(prompt.contains("# Workspace Isolation"));
        assert!(prompt.contains("/tmp/worktree_test"));
        assert!(prompt.contains("## Stage: `investigate` (Agent: @forge-devops)"));
        assert!(prompt.contains("Mapped 3 files and 2 modules"));
        assert!(prompt.contains("## Stage: `implement` (Agent: @forge-devops)"));
        assert!(prompt.contains("`src/lib.rs`"));
        assert!(prompt.contains("`Cargo.toml`"));
        assert!(prompt.contains("2 files changed, 10 insertions(+)"));
        // Output bounded to 8000 chars
        assert!(prompt.contains("[Output truncated at 8000 characters]"));
        assert!(!prompt.contains(&"a".repeat(9000)));
    }
}
