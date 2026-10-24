use crate::execution::permissions::RiskLevel;
use crate::governance::policy::{GovernancePolicy, PolicyEngine};
use crate::governance::shell_lexer::lex;
use crate::governance::shell_policy::{is_protected_path, policy_for, PathArgPolicy};
use crate::governance::takeover::TakeoverController;
use crate::sandbox::DockerSandbox;
use crate::storage::{ApprovalRequest, Database};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileOp {
    Read,
    Write,
    Create,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskAutonomyMode {
    #[default]
    Autonomous,
    Supervised,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutineOpClass {
    FileRead,
    FileWrite,
    FileOrganize,
    SafeCommand,
    LocalVerification,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskAuthorizationGrant {
    pub run_id: String,
    pub mode: TaskAutonomyMode,
    pub max_risk: RiskLevel,
    pub allowed_operations: Vec<RoutineOpClass>,
    pub workspace_root: Option<PathBuf>,
    pub authorized_folders: Vec<PathBuf>,
    pub created_at: i64,
    pub expires_at: i64,
}

impl TaskAuthorizationGrant {
    pub fn new(
        run_id: String,
        workspace_root: Option<PathBuf>,
        authorized_folders: Vec<PathBuf>,
        mode: TaskAutonomyMode,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            run_id,
            mode,
            max_risk: RiskLevel::Moderate,
            allowed_operations: vec![
                RoutineOpClass::FileRead,
                RoutineOpClass::FileWrite,
                RoutineOpClass::FileOrganize,
                RoutineOpClass::SafeCommand,
                RoutineOpClass::LocalVerification,
            ],
            workspace_root,
            authorized_folders,
            created_at: now,
            expires_at: now + 3600 * 1000, // 1 hour lifetime
        }
    }

    pub fn is_active(&self, current_run_id: &str, now: i64) -> bool {
        self.run_id == current_run_id && now < self.expires_at
    }

    pub fn permits_op(&self, op: RoutineOpClass, risk: RiskLevel) -> bool {
        self.mode == TaskAutonomyMode::Autonomous
            && self.allowed_operations.contains(&op)
            && risk <= self.max_risk
    }

    pub fn path_in_scope(&self, path: &Path) -> bool {
        let resolved = if path.is_relative() {
            if let Some(ref ws) = self.workspace_root {
                ws.join(path)
            } else {
                path.to_path_buf()
            }
        } else {
            path.to_path_buf()
        };

        let candidate = if let Ok(canon) = resolved.canonicalize() {
            canon
        } else {
            resolved
        };

        // Check workspace root
        if let Some(ref ws) = self.workspace_root {
            let canon_ws = ws.canonicalize().unwrap_or_else(|_| ws.clone());
            if candidate.starts_with(&canon_ws) {
                return true;
            }
        }

        // Check authorized folders
        for folder in &self.authorized_folders {
            let canon_folder = folder.canonicalize().unwrap_or_else(|_| folder.clone());
            if candidate.starts_with(&canon_folder) {
                return true;
            }
        }

        false
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub run_id: String,
    pub agent_id: String,
    pub conversation_id: Option<String>,
    pub workspace_root: Option<PathBuf>,
    #[serde(default)]
    pub task_grant: Option<TaskAuthorizationGrant>,
}

impl ExecutionContext {
    pub fn new(
        run_id: String,
        agent_id: String,
        conversation_id: Option<String>,
        workspace_root: Option<PathBuf>,
    ) -> Self {
        Self {
            run_id,
            agent_id,
            conversation_id,
            workspace_root,
            task_grant: None,
        }
    }

    pub fn with_task_grant(mut self, grant: TaskAuthorizationGrant) -> Self {
        self.task_grant = Some(grant);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthDecision {
    Allow,
    AllowSandboxOnly {
        image: String,
    },
    RequireApproval {
        reason: String,
        risk: RiskLevel,
        summary: String,
    },
    RequireTakeover {
        reason: String,
    },
    Deny {
        reason: String,
    },
}

/// The result of analysing a shell command.
///
/// `argv` is the *exact* argument vector that will be executed when the
/// decision is `Allow`.  When `requires_shell` is `true` the command needs
/// real shell semantics and must be executed through `sh -c` behind an
/// approval; in that case `argv` is `None`.
#[derive(Debug, Clone)]
pub struct ShellAnalysis {
    pub decision: AuthDecision,
    pub argv: Option<Vec<String>>,
    pub requires_shell: bool,
    pub lexed_words: Vec<String>,
}

pub struct AuthorizationEngine;

impl AuthorizationEngine {
    /// Executables that are safe to auto-approve *as a name only*.
    ///
    /// Deprecated: this list is retained for backwards compatibility with the
    /// settings UI, but it is **no longer used for authorisation**.  Argument
    /// policies live in `governance::shell_policy`; a command is only
    /// auto-approved when its executable, its flags and its path arguments all
    /// satisfy that policy.
    pub const SAFE_COMMANDS: &'static [&'static str] = &[
        "ls",
        "dir",
        "pwd",
        "whoami",
        "uname",
        "date",
        "uptime",
        "cat",
        "head",
        "tail",
        "grep",
        "find",
        "wc",
        "stat",
        "file",
        "df",
        "free",
        "ps",
        "echo",
        "which",
        "git status",
        "git log",
        "git diff",
        "git branch",
        "cargo --version",
        "python --version",
        "node --version",
        "npm --version",
        "rustc --version",
    ];

    /// Sensitive paths where writes and modifications are strictly prohibited
    pub const PROHIBITED_PATHS: &'static [&'static str] = &[
        "/etc",
        "/boot",
        "/sys",
        "/proc",
        "/dev",
        "/usr/bin",
        "/usr/sbin",
        "/bin",
        "/sbin",
        "/var/run",
        ".ssh",
        ".gnupg",
        ".bashrc",
        ".zshrc",
        ".profile",
        "/etc/shadow",
        "/etc/passwd",
        "/etc/sudoers",
    ];

    /// Enforces rigorous filesystem boundary confinement for operations:
    /// 1. Requires explicit workspace root policy.
    /// 2. Canonicalizes paths and follows symlinks to ensure they reside strictly within workspace root.
    /// 3. Prevents parent directory traversal ('..') and unauthorized absolute paths.
    /// 4. Validates real destination for new files via deepest existing ancestor directory.
    /// 5. Strictly rejects access to prohibited system locations.
    pub fn validate_path_confinement(
        workspace_root: Option<&Path>,
        raw_path: &str,
        op: FileOp,
    ) -> Result<PathBuf, String> {
        let root = match workspace_root {
            Some(r) => r,
            None => {
                return Err("Filesystem access denied: No workspace root configured for this execution context".to_string());
            }
        };

        let trimmed = raw_path.trim();
        if trimmed.is_empty() {
            return Err("Filesystem path cannot be empty".to_string());
        }

        if trimmed.contains('\0') {
            return Err("Filesystem path contains prohibited null byte".to_string());
        }

        let canonical_root = root.canonicalize().map_err(|e| {
            format!(
                "Configured workspace root '{}' is invalid or inaccessible: {}",
                root.display(),
                e
            )
        })?;

        // 1. Prohibited sensitive paths check
        let lower = trimmed.to_lowercase();
        for &prohibited in Self::PROHIBITED_PATHS {
            if lower.contains(prohibited) {
                return Err(format!(
                    "Filesystem access to sensitive system location '{}' is strictly prohibited",
                    prohibited
                ));
            }
        }

        let input_path = Path::new(trimmed);

        // Disallow arbitrary absolute paths that don't reside in the canonical workspace
        let target = if input_path.is_absolute() {
            input_path.to_path_buf()
        } else {
            canonical_root.join(input_path)
        };

        // 2. Traversal verification: resolve relative parent segments
        if trimmed.contains("../") || trimmed.contains("..\\") || trimmed == ".." {
            let mut resolved = PathBuf::new();
            if target.is_absolute() {
                resolved.push(std::path::Component::RootDir);
            }
            for comp in target.components() {
                match comp {
                    std::path::Component::Prefix(p) => resolved.push(p.as_os_str()),
                    std::path::Component::RootDir => resolved.push(std::path::Component::RootDir),
                    std::path::Component::CurDir => {}
                    std::path::Component::ParentDir => {
                        if !resolved.pop() {
                            return Err(format!(
                                "Directory traversal ('..') escapes workspace root '{}'",
                                canonical_root.display()
                            ));
                        }
                    }
                    std::path::Component::Normal(c) => resolved.push(c),
                }
            }

            if !resolved.starts_with(&canonical_root) {
                return Err(format!(
                    "Directory traversal ('..') escapes workspace root '{}'",
                    canonical_root.display()
                ));
            }
        }

        // 3. Existing path vs non-existent path validation
        if target.exists() {
            let canonical_target = target.canonicalize().map_err(|e| {
                format!(
                    "Failed to canonicalize target path '{}': {}",
                    target.display(),
                    e
                )
            })?;

            if !canonical_target.starts_with(&canonical_root) {
                return Err(format!(
                    "Security denial: path '{}' resolves outside approved workspace root '{}'",
                    trimmed,
                    canonical_root.display()
                ));
            }

            let canon_lower = canonical_target.to_string_lossy().to_lowercase();
            for &prohibited in Self::PROHIBITED_PATHS {
                if canon_lower.contains(prohibited) {
                    return Err(format!(
                        "Target resolves to prohibited system location '{}'",
                        prohibited
                    ));
                }
            }

            Ok(canonical_target)
        } else {
            if op == FileOp::Read {
                return Err(format!("File does not exist: '{}'", trimmed));
            }

            // For new files (Write / Create): inspect the nearest existing ancestor
            let mut ancestor = target.as_path();
            while !ancestor.exists() {
                match ancestor.parent() {
                    Some(p) => ancestor = p,
                    None => {
                        return Err(
                            "Cannot establish filesystem boundary for new file destination".into(),
                        );
                    }
                }
            }

            let canonical_ancestor = ancestor.canonicalize().map_err(|e| {
                format!(
                    "Failed to canonicalize ancestor directory '{}': {}",
                    ancestor.display(),
                    e
                )
            })?;

            if !canonical_ancestor.starts_with(&canonical_root) {
                return Err(format!(
                    "Security denial: destination '{}' resides outside approved workspace root '{}'",
                    trimmed,
                    canonical_root.display()
                ));
            }

            // Verify remaining subpath does not contain traversal
            if let Ok(suffix) = target.strip_prefix(ancestor) {
                for comp in suffix.components() {
                    match comp {
                        std::path::Component::ParentDir => {
                            return Err("Parent traversal '..' detected in destination path".into());
                        }
                        std::path::Component::Normal(_) => {}
                        _ => return Err("Invalid path component in destination".into()),
                    }
                }
            }

            Ok(target)
        }
    }

    /// Securely writes a file within an approved workspace root, hardening against
    /// symlink replacement, directory substitution, and race conditions between validation and write.
    pub fn secure_write_contained(
        workspace_root: Option<&Path>,
        path_str: &str,
        content: &[u8],
    ) -> Result<PathBuf, String> {
        let safe_path = Self::validate_path_confinement(workspace_root, path_str, FileOp::Write)?;

        let root = workspace_root
            .ok_or_else(|| "Workspace root is required for contained file write".to_string())?;
        let canonical_root = root.canonicalize().map_err(|e| {
            format!(
                "Failed to canonicalize workspace root '{}': {}",
                root.display(),
                e
            )
        })?;

        // 1. Explicitly create parent directories and propagate any errors
        if let Some(parent) = safe_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.is_dir() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    format!(
                        "Failed to create parent directories for '{}': {}",
                        safe_path.display(),
                        e
                    )
                })?;
            }

            // 2. Revalidate parent directory canonicalization at the filesystem operation boundary
            let canonical_parent = parent.canonicalize().map_err(|e| {
                format!(
                    "Failed to revalidate parent directory '{}' after creation: {}",
                    parent.display(),
                    e
                )
            })?;

            if !canonical_parent.starts_with(&canonical_root) {
                return Err(format!(
                    "Security denial: parent directory '{}' escapes workspace root '{}' via symlink or substitution",
                    canonical_parent.display(),
                    canonical_root.display()
                ));
            }

            let parent_lower = canonical_parent.to_string_lossy().to_lowercase();
            for &prohibited in Self::PROHIBITED_PATHS {
                if parent_lower.contains(prohibited) {
                    return Err(format!(
                        "Security denial: parent directory resolves to prohibited system location '{}'",
                        prohibited
                    ));
                }
            }
        }

        // 3. Inspect existing destination to prevent symlink hijack attacks
        if let Ok(meta) = std::fs::symlink_metadata(&safe_path) {
            if meta.file_type().is_symlink() {
                // If the target is a symlink, verify its target canonicalizes strictly inside canonical_root
                let canonical_target = safe_path.canonicalize().map_err(|e| {
                    format!(
                        "Security denial: destination symlink '{}' cannot be resolved: {}",
                        safe_path.display(),
                        e
                    )
                })?;
                if !canonical_target.starts_with(&canonical_root) {
                    return Err(format!(
                        "Security denial: destination symlink '{}' points outside approved workspace root '{}'",
                        safe_path.display(),
                        canonical_root.display()
                    ));
                }
            }
        }

        // 4. Perform the write
        std::fs::write(&safe_path, content)
            .map_err(|e| format!("Failed to write file '{}': {}", safe_path.display(), e))?;

        // 5. Post-write verification: ensure the written file is genuinely within canonical root
        let written_canon = safe_path.canonicalize().map_err(|e| {
            format!(
                "Failed to verify written file '{}': {}",
                safe_path.display(),
                e
            )
        })?;
        if !written_canon.starts_with(&canonical_root) {
            // Rollback the escaped file immediately
            let _ = std::fs::remove_file(&written_canon);
            return Err(format!(
                "Security violation: written file escaped approved workspace root '{}'",
                canonical_root.display()
            ));
        }

        Ok(safe_path)
    }

    /// Securely reads a file within an approved workspace root, ensuring that
    /// symlink replacement cannot read files outside the approved root.
    pub fn secure_read_contained(
        workspace_root: Option<&Path>,
        path_str: &str,
    ) -> Result<String, String> {
        let safe_path = Self::validate_path_confinement(workspace_root, path_str, FileOp::Read)?;
        let root = workspace_root
            .ok_or_else(|| "Workspace root is required for contained file read".to_string())?;
        let canonical_root = root.canonicalize().map_err(|e| {
            format!(
                "Failed to canonicalize workspace root '{}': {}",
                root.display(),
                e
            )
        })?;

        // Revalidate canonicalization at the read boundary
        let canon = safe_path.canonicalize().map_err(|e| {
            format!(
                "Failed to canonicalize target path '{}': {}",
                safe_path.display(),
                e
            )
        })?;
        if !canon.starts_with(&canonical_root) {
            return Err(format!(
                "Security denial: path '{}' resolves outside approved workspace root '{}'",
                path_str,
                canonical_root.display()
            ));
        }

        let canon_lower = canon.to_string_lossy().to_lowercase();
        for &prohibited in Self::PROHIBITED_PATHS {
            if canon_lower.contains(prohibited) {
                return Err(format!(
                    "Security denial: target resolves to prohibited system location '{}'",
                    prohibited
                ));
            }
        }

        let mut content = std::fs::read_to_string(&canon)
            .map_err(|e| format!("Failed to read file '{}': {}", path_str, e))?;

        const MAX_READ_BYTES: usize = 512 * 1024;
        if content.len() > MAX_READ_BYTES {
            content.truncate(MAX_READ_BYTES);
            content.push_str("\n...[Content truncated at 512KB limit]");
        }

        Ok(content)
    }

    /// Resolves and validates the active task-scoped authorization grant.
    /// Checks:
    /// 1. Run ID match (strictly bounded to ctx.run_id, non-inheritable)
    /// 2. Lifetime expiry (now < expires_at)
    /// 3. Terminal state revocation (run status not terminal in DB)
    pub fn resolve_active_task_grant(
        db: &Database,
        ctx: &ExecutionContext,
    ) -> Option<TaskAuthorizationGrant> {
        let grant = ctx.task_grant.clone().or_else(|| {
            if let Ok(Some(r)) = db.get_run(&ctx.run_id) {
                if let Some(ref cfg_str) = r.config_json {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(cfg_str) {
                        return v
                            .get("task_grant")
                            .and_then(|g| serde_json::from_value(g.clone()).ok());
                    }
                }
            }
            None
        })?;

        // 1. Strict run_id binding: grant cannot be inherited across runs or by delegated sub-agents
        if grant.run_id != ctx.run_id {
            log::warn!(
                "[Security] Task grant run ID mismatch: approved for run '{}', attempted in run '{}'",
                grant.run_id,
                ctx.run_id
            );
            return None;
        }

        // 2. Lifetime expiry check
        let now = chrono::Utc::now().timestamp_millis();
        if now >= grant.expires_at {
            log::warn!(
                "[Security] Task grant for run '{}' expired at {}",
                grant.run_id,
                grant.expires_at
            );
            return None;
        }

        // 3. Durable lifecycle revocation: terminal status in database invalidates grant
        if let Ok(Some(r)) = db.get_run(&ctx.run_id) {
            if matches!(
                r.status.as_str(),
                "completed" | "succeeded" | "failed" | "cancelled"
            ) {
                log::warn!(
                    "[Security] Task grant for run '{}' is revoked because run is in terminal status '{}'",
                    grant.run_id,
                    r.status
                );
                return None;
            }
        }

        Some(grant)
    }

    fn evaluate_organize_execute(
        db: &Database,
        _ctx: &ExecutionContext,
        active_grant: Option<&TaskAuthorizationGrant>,
        args: &serde_json::Value,
    ) -> AuthDecision {
        let plan_val = &args["plan"];
        if plan_val.is_null() || !plan_val.is_object() {
            return AuthDecision::Deny {
                reason: "Missing or invalid 'plan' argument for organize_files_execute".to_string(),
            };
        }

        let plan: crate::storage::organizer::OrganizationPlan =
            match serde_json::from_value(plan_val.clone()) {
                Ok(p) => p,
                Err(e) => {
                    return AuthDecision::Deny {
                        reason: format!("Failed to parse organization plan: {}", e),
                    };
                }
            };

        // Prohibited system locations check
        let target_str = plan.target_directory.to_string_lossy().to_lowercase();
        for &prohibited in Self::PROHIBITED_PATHS {
            if target_str.contains(prohibited) {
                return AuthDecision::Deny {
                    reason: format!(
                        "Security rejection: Plan target directory references sensitive system location '{}'",
                        prohibited
                    ),
                };
            }
        }

        // Anti-tampering and freshness check against server-authoritative persisted plan
        if let Ok(Some((stored_plan, executed_at))) = db.get_organization_plan(&plan.plan_id) {
            // Replay protection: cannot execute an already executed plan
            if executed_at.is_some() {
                return AuthDecision::Deny {
                    reason: "Plan has already been executed. Repeated execution of the same plan is prohibited.".to_string(),
                };
            }

            // Verify submitted plan matches stored plan
            if stored_plan.target_directory != plan.target_directory
                || stored_plan.operations.len() != plan.operations.len()
            {
                return AuthDecision::Deny {
                    reason: "Security rejection: Plan tampering detected. The submitted plan does not match the reviewed plan.".to_string(),
                };
            }

            for (stored_op, sub_op) in stored_plan.operations.iter().zip(&plan.operations) {
                if stored_op.source_path != sub_op.source_path
                    || stored_op.destination_path != sub_op.destination_path
                    || stored_op.file_hash != sub_op.file_hash
                    || stored_op.category != sub_op.category
                    || stored_op.is_duplicate != sub_op.is_duplicate
                    || stored_op.conflict != sub_op.conflict
                {
                    return AuthDecision::Deny {
                        reason: "Security rejection: Plan tampering detected on operation details."
                            .to_string(),
                    };
                }
            }
        } else {
            return AuthDecision::Deny {
                reason: "Plan not found or expired. A valid server-persisted preview plan is required before execution.".to_string(),
            };
        }

        // Check active bounded authorization grant
        if let Some(grant) = active_grant {
            if grant.mode == TaskAutonomyMode::Autonomous
                && grant
                    .allowed_operations
                    .contains(&RoutineOpClass::FileOrganize)
                && RiskLevel::Moderate <= grant.max_risk
            {
                let target_in_scope = grant.path_in_scope(&plan.target_directory);
                let ops_in_scope = plan.operations.iter().all(|op| {
                    grant.path_in_scope(&op.source_path)
                        && grant.path_in_scope(&op.destination_path)
                });

                if target_in_scope && ops_in_scope {
                    return AuthDecision::Allow;
                }
            }
        }

        // Outside active grant scope, no grant, or Supervised mode -> request targeted approval
        AuthDecision::RequireApproval {
            reason: "Executing file reorganization moves files into new directory locations."
                .to_string(),
            risk: RiskLevel::Moderate,
            summary: "Move categorized files into organized folders".to_string(),
        }
    }

    fn evaluate_organize_undo(
        db: &Database,
        _ctx: &ExecutionContext,
        active_grant: Option<&TaskAuthorizationGrant>,
        args: &serde_json::Value,
    ) -> AuthDecision {
        let trx_id = if let Some(id) = args["transaction_id"].as_str() {
            id
        } else if let Some(id) = args["transaction"]["transaction_id"].as_str() {
            id
        } else {
            return AuthDecision::RequireApproval {
                reason: "Undoing file organization moves files back to their previous locations."
                    .to_string(),
                risk: RiskLevel::Moderate,
                summary: "Reverse organized file moves".to_string(),
            };
        };

        if let Ok(Some(trx)) = db.get_file_transaction(trx_id) {
            let target_path = Path::new(&trx.target_directory);
            if let Some(grant) = active_grant {
                if grant.mode == TaskAutonomyMode::Autonomous
                    && grant
                        .allowed_operations
                        .contains(&RoutineOpClass::FileOrganize)
                    && RiskLevel::Moderate <= grant.max_risk
                    && grant.path_in_scope(target_path)
                {
                    return AuthDecision::Allow;
                }
            }
        }

        AuthDecision::RequireApproval {
            reason: "Undoing file organization moves files back to their previous locations."
                .to_string(),
            risk: RiskLevel::Moderate,
            summary: "Reverse organized file moves".to_string(),
        }
    }

    /// Evaluates tool authorization without access to a database instance (used by standalone helpers)
    pub fn evaluate_without_db(
        policy: &GovernancePolicy,
        ctx: &ExecutionContext,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> AuthDecision {
        let args_str = args.to_string();
        if policy.require_takeover_for_credentials {
            if let Some(reason) = TakeoverController::detect_sensitive_checkpoint(&args_str) {
                return AuthDecision::RequireTakeover { reason };
            }
        }

        for domain in &policy.blocked_domains {
            if args_str.to_lowercase().contains(&domain.to_lowercase()) {
                return AuthDecision::Deny {
                    reason: format!(
                        "Operation targets blocked domain '{}' defined in governance policy",
                        domain
                    ),
                };
            }
        }

        match tool_name {
            "execute_shell" | "bash" | "terminal" => {
                Self::evaluate_shell_in_context(policy, ctx, args)
            }
            "read_file" => Self::evaluate_file_read(ctx, args),
            "write_file" => Self::evaluate_file_write(ctx, args),
            "create_artifact" => Self::evaluate_create_artifact(ctx, args),
            "computer_action" => Self::evaluate_computer_action(policy, args),
            "click_mark" => Self::evaluate_click_mark(policy, args),
            "capture_screen" | "capture_grounded_screen" => AuthDecision::Allow,
            "launch_ghost_app" => Self::evaluate_ghost_launch(policy, args),
            "invoke_skill" => Self::evaluate_skill_invoke(policy, args),
            "run_safe_experiment" => Self::evaluate_experiment(policy, args),
            "organize_files_preview" => {
                if policy.task_autonomy_mode == "Supervised" {
                    AuthDecision::RequireApproval {
                        reason: "Supervised mode requires approval for file organization preview"
                            .to_string(),
                        risk: RiskLevel::Safe,
                        summary: format!(
                            "Preview file organization for directory '{}'",
                            args["directory"].as_str().unwrap_or("")
                        ),
                    }
                } else {
                    AuthDecision::Allow
                }
            }
            "organize_files_execute" => {
                if let Some(ref grant) = ctx.task_grant {
                    if grant.mode == TaskAutonomyMode::Autonomous
                        && grant
                            .allowed_operations
                            .contains(&RoutineOpClass::FileOrganize)
                        && RiskLevel::Moderate <= grant.max_risk
                    {
                        if let Some(target_dir) = args["plan"]["target_directory"].as_str() {
                            if grant.path_in_scope(Path::new(target_dir)) {
                                return AuthDecision::Allow;
                            }
                        }
                    }
                }
                AuthDecision::RequireApproval {
                    reason:
                        "Executing file reorganization moves files into new directory locations."
                            .to_string(),
                    risk: RiskLevel::Moderate,
                    summary: "Move categorized files into organized folders".to_string(),
                }
            }
            "organize_files_undo" => {
                if let Some(ref grant) = ctx.task_grant {
                    if grant.mode == TaskAutonomyMode::Autonomous
                        && grant
                            .allowed_operations
                            .contains(&RoutineOpClass::FileOrganize)
                        && RiskLevel::Moderate <= grant.max_risk
                    {
                        return AuthDecision::Allow;
                    }
                }
                AuthDecision::RequireApproval {
                    reason:
                        "Undoing file organization moves files back to their previous locations."
                            .to_string(),
                    risk: RiskLevel::Moderate,
                    summary: "Reverse organized file moves".to_string(),
                }
            }
            "rollback_code_checkpoint" => AuthDecision::RequireApproval {
                reason: "Rolling back a code checkpoint restores previous file states.".to_string(),
                risk: RiskLevel::Moderate,
                summary: "Revert agent modified files to checkpoint state".to_string(),
            },
            "start_dev_server" | "audit_ui" => {
                let target = args["path"].as_str().unwrap_or("");
                AuthDecision::RequireApproval {
                    reason: "Starting a project's development server executes repository-controlled code".to_string(),
                    risk: RiskLevel::Moderate,
                    summary: format!(
                        "Start the development server of the project at '{}'",
                        if target.is_empty() {
                            "(discovered project)".to_string()
                        } else {
                            target.to_string()
                        }
                    ),
                }
            }
            "browser_navigate" => {
                let url = args["url"].as_str().unwrap_or("");
                if url.trim().is_empty() {
                    return AuthDecision::Deny {
                        reason: "browser_navigate requires a 'url' parameter".to_string(),
                    };
                }
                for domain in &policy.blocked_domains {
                    if url.to_lowercase().contains(&domain.to_lowercase()) {
                        return AuthDecision::Deny {
                            reason: format!(
                                "The requested URL targets the blocked domain '{}'",
                                domain
                            ),
                        };
                    }
                }
                AuthDecision::Allow
            }
            "browser_click" => {
                const CONSEQUENTIAL: &[&str] = &[
                    "delete",
                    "remove",
                    "purchase",
                    "buy",
                    "pay",
                    "checkout",
                    "send",
                    "publish",
                    "deploy",
                    "confirm",
                    "submit",
                    "transfer",
                    "upgrade",
                    "subscribe",
                    "invite",
                    "sign out",
                    "log out",
                    "logout",
                ];
                let label = format!(
                    "{} {}",
                    args["name"].as_str().unwrap_or(""),
                    args["text"].as_str().unwrap_or("")
                )
                .to_lowercase();
                if CONSEQUENTIAL.iter().any(|c| label.contains(c)) {
                    AuthDecision::RequireApproval {
                        reason: format!(
                            "Activating the control '{}' may perform a consequential action",
                            label.trim()
                        ),
                        risk: RiskLevel::Moderate,
                        summary: format!("Activate the control: {}", label.trim()),
                    }
                } else {
                    AuthDecision::Allow
                }
            }
            "browser_fill" => {
                let value = args["value"].as_str().unwrap_or("");
                if let Some(reason) = TakeoverController::detect_sensitive_checkpoint(value) {
                    AuthDecision::RequireTakeover { reason }
                } else {
                    AuthDecision::Allow
                }
            }
            "browser_open" | "stop_dev_server" | "browser_close" => AuthDecision::Allow,
            _ => AuthDecision::RequireApproval {
                reason: format!("Tool '{}' requires explicit authorization", tool_name),
                risk: RiskLevel::Moderate,
                summary: format!("Invoke tool '{}'", tool_name),
            },
        }
    }

    /// Evaluate an authorization request through the centralized policy matrix
    pub fn evaluate(
        db: &Database,
        ctx: &ExecutionContext,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> AuthDecision {
        let policy = PolicyEngine::load(db);

        // 1. Check for credential/takeover checkpoints
        let args_str = args.to_string();
        if policy.require_takeover_for_credentials {
            if let Some(reason) = TakeoverController::detect_sensitive_checkpoint(&args_str) {
                return AuthDecision::RequireTakeover { reason };
            }
        }

        // 2. Check blocked domains
        for domain in &policy.blocked_domains {
            if args_str.to_lowercase().contains(&domain.to_lowercase()) {
                return AuthDecision::Deny {
                    reason: format!(
                        "Operation targets blocked domain '{}' defined in governance policy",
                        domain
                    ),
                };
            }
        }

        // 3. Enforce explicit tool denial policies
        let perm_key = format!("mcp_perm:{}", tool_name);
        if let Ok(Some(policy_val)) = db.get_setting(&perm_key) {
            if policy_val.trim().to_lowercase() == "deny" {
                return AuthDecision::Deny {
                    reason: format!("Tool '{}' is explicitly blocked by policy", tool_name),
                };
            }
        }

        // 4. Resolve active bounded authorization grant
        let active_grant = Self::resolve_active_task_grant(db, ctx);

        match tool_name {
            "execute_shell" | "bash" | "terminal" => {
                Self::evaluate_shell_in_context(&policy, ctx, args)
            }
            "read_file" => Self::evaluate_file_read(ctx, args),
            "write_file" => Self::evaluate_file_write(ctx, args),
            "create_artifact" => Self::evaluate_create_artifact(ctx, args),
            "computer_action" => Self::evaluate_computer_action(&policy, args),
            "click_mark" => Self::evaluate_click_mark(&policy, args),
            "capture_screen" | "capture_grounded_screen" => AuthDecision::Allow,
            "launch_ghost_app" => Self::evaluate_ghost_launch(&policy, args),
            "invoke_skill" => Self::evaluate_skill_invoke(&policy, args),
            "run_safe_experiment" => Self::evaluate_experiment(&policy, args),
            "organize_files_preview" => {
                let perm_key = "mcp_perm:organize_files_preview";
                if let Ok(Some(policy_val)) = db.get_setting(perm_key) {
                    match policy_val.trim().to_lowercase().as_str() {
                        "deny" => {
                            return AuthDecision::Deny {
                                reason: "File organization preview is explicitly blocked by policy"
                                    .to_string(),
                            }
                        }
                        "ask" => {
                            return AuthDecision::RequireApproval {
                                reason: "File organization preview requires approval".to_string(),
                                risk: RiskLevel::Safe,
                                summary: format!(
                                    "Preview file organization for directory '{}'",
                                    args["directory"].as_str().unwrap_or("")
                                ),
                            }
                        }
                        _ => {}
                    }
                }
                if policy.task_autonomy_mode == "Supervised" {
                    AuthDecision::RequireApproval {
                        reason: "Supervised mode requires approval for file organization preview"
                            .to_string(),
                        risk: RiskLevel::Safe,
                        summary: format!(
                            "Preview file organization for directory '{}'",
                            args["directory"].as_str().unwrap_or("")
                        ),
                    }
                } else {
                    AuthDecision::Allow
                }
            }
            "organize_files_execute" => {
                let perm_key = "mcp_perm:organize_files_execute";
                if let Ok(Some(policy_val)) = db.get_setting(perm_key) {
                    if policy_val.trim().to_lowercase() == "deny" {
                        return AuthDecision::Deny {
                            reason: "File organization execution is explicitly blocked by policy"
                                .to_string(),
                        };
                    }
                }
                Self::evaluate_organize_execute(db, ctx, active_grant.as_ref(), args)
            }
            "organize_files_undo" => {
                let perm_key = "mcp_perm:organize_files_undo";
                if let Ok(Some(policy_val)) = db.get_setting(perm_key) {
                    if policy_val.trim().to_lowercase() == "deny" {
                        return AuthDecision::Deny {
                            reason: "File organization undo is explicitly blocked by policy"
                                .to_string(),
                        };
                    }
                }
                Self::evaluate_organize_undo(db, ctx, active_grant.as_ref(), args)
            }
            "rollback_code_checkpoint" => AuthDecision::RequireApproval {
                reason: "Rolling back a code checkpoint restores previous file states.".to_string(),
                risk: RiskLevel::Moderate,
                summary: "Revert agent modified files to checkpoint state".to_string(),
            },
            "start_dev_server" | "audit_ui" => {
                let target = args["path"].as_str().unwrap_or("");
                AuthDecision::RequireApproval {
                    reason: "Starting a project's development server executes repository-controlled code".to_string(),
                    risk: RiskLevel::Moderate,
                    summary: format!(
                        "Start the development server of the project at '{}'",
                        if target.is_empty() {
                            "(discovered project)".to_string()
                        } else {
                            target.to_string()
                        }
                    ),
                }
            }
            "browser_navigate" => {
                let url = args["url"].as_str().unwrap_or("");
                if url.trim().is_empty() {
                    return AuthDecision::Deny {
                        reason: "browser_navigate requires a 'url' parameter".to_string(),
                    };
                }
                for domain in &policy.blocked_domains {
                    if url.to_lowercase().contains(&domain.to_lowercase()) {
                        return AuthDecision::Deny {
                            reason: format!(
                                "The requested URL targets the blocked domain '{}'",
                                domain
                            ),
                        };
                    }
                }
                AuthDecision::Allow
            }
            "browser_click" => {
                const CONSEQUENTIAL: &[&str] = &[
                    "delete",
                    "remove",
                    "purchase",
                    "buy",
                    "pay",
                    "checkout",
                    "send",
                    "publish",
                    "deploy",
                    "confirm",
                    "submit",
                    "transfer",
                    "upgrade",
                    "subscribe",
                    "invite",
                    "sign out",
                    "log out",
                    "logout",
                ];
                let label = format!(
                    "{} {}",
                    args["name"].as_str().unwrap_or(""),
                    args["text"].as_str().unwrap_or("")
                )
                .to_lowercase();
                if CONSEQUENTIAL.iter().any(|c| label.contains(c)) {
                    AuthDecision::RequireApproval {
                        reason: format!(
                            "Activating the control '{}' may perform a consequential action",
                            label.trim()
                        ),
                        risk: RiskLevel::Moderate,
                        summary: format!("Activate the control: {}", label.trim()),
                    }
                } else {
                    AuthDecision::Allow
                }
            }
            "browser_fill" => {
                let value = args["value"].as_str().unwrap_or("");
                if let Some(reason) = TakeoverController::detect_sensitive_checkpoint(value) {
                    AuthDecision::RequireTakeover { reason }
                } else {
                    AuthDecision::Allow
                }
            }
            "browser_open" | "stop_dev_server" | "browser_close" => AuthDecision::Allow,
            _ => {
                // MCP or custom external tool
                Self::evaluate_mcp_or_custom(db, &policy, tool_name, args)
            }
        }
    }

    /// Evaluates shell command according to shell_mode policy
    pub fn evaluate_shell(policy: &GovernancePolicy, args: &serde_json::Value) -> AuthDecision {
        Self::analyze_shell_command(policy, None, args).decision
    }

    /// Analyses a shell command string against the configured shell policy.
    pub fn analyze_shell_command(
        policy: &GovernancePolicy,
        workspace_root: Option<&Path>,
        args: &serde_json::Value,
    ) -> ShellAnalysis {
        let dummy_ctx = workspace_root.map(|ws| ExecutionContext {
            run_id: String::new(),
            agent_id: String::new(),
            conversation_id: None,
            workspace_root: Some(ws.to_path_buf()),
            task_grant: None,
        });
        Self::analyze_shell_command_in_context(policy, dummy_ctx.as_ref(), args)
    }

    /// Evaluates a shell command with access to the workspace root so that
    /// path arguments can be constrained to the authorised working directory.
    pub fn evaluate_shell_in_context(
        policy: &GovernancePolicy,
        ctx: &ExecutionContext,
        args: &serde_json::Value,
    ) -> AuthDecision {
        Self::analyze_shell_command_in_context(policy, Some(ctx), args).decision
    }

    /// Analyses a shell command string against the configured shell policy and execution context.
    pub fn analyze_shell_command_in_context(
        policy: &GovernancePolicy,
        ctx: Option<&ExecutionContext>,
        args: &serde_json::Value,
    ) -> ShellAnalysis {
        let cmd = match args["command"].as_str() {
            Some(c) if !c.trim().is_empty() => c.trim(),
            _ => {
                return ShellAnalysis {
                    decision: AuthDecision::Deny {
                        reason: "Missing or empty shell command argument".into(),
                    },
                    argv: None,
                    requires_shell: false,
                    lexed_words: Vec::new(),
                }
            }
        };

        let requested_sandbox = args["use_sandbox"].as_bool().unwrap_or(false);

        // Fail-closed sandbox policy
        if policy.shell_mode == "SandboxOnly" || requested_sandbox {
            let sandbox_status = DockerSandbox::probe();
            if !sandbox_status.available {
                return ShellAnalysis {
                    decision: AuthDecision::Deny {
                        reason: format!(
                            "Sandbox execution required by policy ({}), but container engine is unavailable: {}",
                            policy.shell_mode, sandbox_status.message
                        ),
                    },
                    argv: None,
                    requires_shell: true,
                    lexed_words: Vec::new(),
                };
            }
            return ShellAnalysis {
                decision: AuthDecision::AllowSandboxOnly {
                    image: "alpine:3.20".to_string(),
                },
                argv: None,
                requires_shell: true,
                lexed_words: Vec::new(),
            };
        }

        // AskAlways mode
        if policy.shell_mode == "AskAlways" {
            return ShellAnalysis {
                decision: AuthDecision::RequireApproval {
                    reason: "Policy 'AskAlways' requires confirmation for all shell execution"
                        .to_string(),
                    risk: RiskLevel::Moderate,
                    summary: format!("Execute shell command: '{}'", cmd),
                },
                argv: None,
                requires_shell: true,
                lexed_words: Vec::new(),
            };
        }

        // SafeAutoApprove mode.
        let lexed = lex(cmd);

        if lexed.words.is_empty() {
            return ShellAnalysis {
                decision: AuthDecision::Deny {
                    reason: "Shell command did not contain an executable".into(),
                },
                argv: None,
                requires_shell: false,
                lexed_words: Vec::new(),
            };
        }

        // A command that relies on shell-only constructs must be executed by a
        // real shell, which means the authorisation can no longer guarantee
        // what runs.  Such commands always require explicit approval.
        if !lexed.is_simple {
            let kinds = lexed
                .constructs
                .iter()
                .map(|c| c.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return ShellAnalysis {
                decision: AuthDecision::RequireApproval {
                    reason: format!(
                        "Command uses shell-only constructs ({}); its expanded form cannot be statically verified",
                        kinds
                    ),
                    risk: RiskLevel::Moderate,
                    summary: format!("Run shell command: '{}'", cmd),
                },
                argv: None,
                requires_shell: true,
                lexed_words: lexed.words,
            };
        }

        let words = lexed.words.clone();
        let executable = words[0].clone();

        // Environment assignments preceding the command are never auto-approved.
        if executable.contains('=') {
            return ShellAnalysis {
                decision: AuthDecision::RequireApproval {
                    reason: "Command with environment variable assignments requires approval"
                        .to_string(),
                    risk: RiskLevel::Moderate,
                    summary: format!("Run command with env assignment: '{}'", cmd),
                },
                argv: None,
                requires_shell: true,
                lexed_words: words,
            };
        }

        let workspace_root = ctx.and_then(|c| c.workspace_root.as_deref());

        // Check if command is a routine local verification step permitted by active task grant
        if let Some(grant) = ctx.and_then(|c| c.task_grant.as_ref()) {
            if grant.mode == TaskAutonomyMode::Autonomous
                && grant
                    .allowed_operations
                    .contains(&RoutineOpClass::LocalVerification)
                && RiskLevel::Moderate <= grant.max_risk
                && workspace_root.is_some()
                && Self::is_local_verification_command(&executable, &words)
            {
                let mut paths_ok = true;
                for arg in &words[1..] {
                    if !arg.starts_with('-')
                        && Self::check_path_argument(arg, workspace_root).is_some()
                    {
                        paths_ok = false;
                        break;
                    }
                }
                if paths_ok {
                    return ShellAnalysis {
                        decision: AuthDecision::Allow,
                        argv: Some(words.clone()),
                        requires_shell: false,
                        lexed_words: words,
                    };
                }
            }
        }

        // Interpreters and shell wrappers can execute arbitrary code.
        const INTERPRETERS: &[&str] = &[
            "sh", "bash", "zsh", "ksh", "csh", "dash", "fish", "busybox", "env", "sudo", "su",
            "doas", "xargs", "eval", "exec", "source", "timeout", "nice", "nohup", "setsid",
            "stdbuf", "script", "command", "builtin", "trap", "wait", "python", "python3", "node",
            "ruby", "perl", "php", "lua", "awk", "gawk", "sed", "make", "cmake", "xargs",
        ];
        if INTERPRETERS.contains(&executable.as_str()) {
            return ShellAnalysis {
                decision: AuthDecision::RequireApproval {
                    reason: format!(
                        "Interpreter or wrapper command '{}' can execute arbitrary code and requires approval",
                        executable
                    ),
                    risk: RiskLevel::Moderate,
                    summary: format!("Run wrapper command: '{}'", cmd),
                },
                argv: None,
                requires_shell: true,
                lexed_words: words,
            };
        }

        // Look up the argument policy for this executable.
        let policy_entry = match policy_for(&executable) {
            Some(p) => p,
            None => {
                // An unknown executable is never auto-approved.  The risk is
                // raised to `Destructive` when the program is one that can
                // destroy data or take the machine down.
                const DESTRUCTIVE_EXECUTABLES: &[&str] = &[
                    "rm",
                    "rmdir",
                    "shred",
                    "dd",
                    "mkfs",
                    "fdisk",
                    "parted",
                    "wipefs",
                    "mkswap",
                    "chmod",
                    "chown",
                    "chgrp",
                    "kill",
                    "killall",
                    "pkill",
                    "reboot",
                    "shutdown",
                    "poweroff",
                    "halt",
                    "init",
                    "insmod",
                    "rmmod",
                    "modprobe",
                    "iptables",
                    "systemctl",
                    "service",
                    "mount",
                    "umount",
                    "swapoff",
                    "userdel",
                    "groupdel",
                    "passwd",
                    "visudo",
                    "truncate",
                    "shred",
                    "mktemp",
                    "install",
                ];
                // `mkfs.ext4`, `mkfs.xfs` and friends are variants of `mkfs`.
                let is_destructive = DESTRUCTIVE_EXECUTABLES.contains(&executable.as_str())
                    || DESTRUCTIVE_EXECUTABLES
                        .iter()
                        .any(|name| executable.starts_with(&format!("{}.", name)));
                let risk = if is_destructive {
                    RiskLevel::Destructive
                } else {
                    RiskLevel::Moderate
                };
                return ShellAnalysis {
                    decision: AuthDecision::RequireApproval {
                        reason: format!(
                            "Command '{}' is not in the read-only allowlist and requires approval",
                            executable
                        ),
                        risk,
                        summary: format!("Run host command: '{}'", cmd),
                    },
                    argv: None,
                    requires_shell: true,
                    lexed_words: words,
                };
            }
        };

        // Sub-command policy (git and friends).
        if !policy_entry.allowed_subcommands.is_empty() {
            let sub = words.get(1).map(|s| s.as_str()).unwrap_or("");
            let sub_ok = if sub.is_empty() {
                // `git` with no sub-command prints help; harmless.
                true
            } else {
                policy_entry.permits_subcommand(sub)
            };
            if !sub_ok {
                return ShellAnalysis {
                    decision: AuthDecision::RequireApproval {
                        reason: format!(
                            "Sub-command '{}' of '{}' is not read-only and requires approval",
                            sub, executable
                        ),
                        risk: RiskLevel::Moderate,
                        summary: format!("Execute {} command: '{}'", executable, cmd),
                    },
                    argv: None,
                    requires_shell: true,
                    lexed_words: words,
                };
            }
        }

        // Argument policy: flags, paths and the remaining arguments.
        let mut arg_start = 1usize;
        if !policy_entry.allowed_subcommands.is_empty() && words.len() > 1 {
            arg_start = 2;
        }

        for arg in words.iter().skip(arg_start) {
            if arg.starts_with('-') && arg.len() > 1 {
                if policy_entry.flag_is_denied(arg) {
                    return ShellAnalysis {
                        decision: AuthDecision::RequireApproval {
                            reason: format!(
                                "Flag '{}' is not permitted for '{}' because it can modify or execute state",
                                arg, executable
                            ),
                            risk: RiskLevel::Destructive,
                            summary: format!("Execute potentially destructive command: '{}'", cmd),
                        },
                        argv: None,
                        requires_shell: true,
                        lexed_words: words,
                    };
                }
                if !policy_entry.flag_is_allowed(arg) {
                    return ShellAnalysis {
                        decision: AuthDecision::RequireApproval {
                            reason: format!(
                                "Flag '{}' is not permitted for '{}' by the configured argument policy",
                                arg, executable
                            ),
                            risk: RiskLevel::Moderate,
                            summary: format!("Execute command: '{}'", cmd),
                        },
                        argv: None,
                        requires_shell: true,
                        lexed_words: words,
                    };
                }
                continue;
            }

            // Path-like argument handling.
            match policy_entry.path_args {
                PathArgPolicy::Text => {
                    // The argument is literal text, not a filesystem path.
                }
                PathArgPolicy::None => {
                    return ShellAnalysis {
                        decision: AuthDecision::RequireApproval {
                            reason: format!(
                                "'{}' does not accept path arguments; '{}' requires approval",
                                executable, arg
                            ),
                            risk: RiskLevel::Moderate,
                            summary: format!("Execute command: '{}'", cmd),
                        },
                        argv: None,
                        requires_shell: true,
                        lexed_words: words,
                    };
                }
                PathArgPolicy::NoProtectedPaths => {
                    if let Some(reason) = Self::check_path_argument(arg, workspace_root) {
                        return ShellAnalysis {
                            decision: AuthDecision::RequireApproval {
                                reason,
                                risk: RiskLevel::Destructive,
                                summary: format!(
                                    "Execute command reading sensitive path: '{}'",
                                    cmd
                                ),
                            },
                            argv: None,
                            requires_shell: true,
                            lexed_words: words,
                        };
                    }
                }
                PathArgPolicy::WorkspaceOnly => {
                    if let Some(reason) = Self::check_workspace_argument(arg, workspace_root) {
                        return ShellAnalysis {
                            decision: AuthDecision::RequireApproval {
                                reason,
                                risk: RiskLevel::Moderate,
                                summary: format!("Execute command: '{}'", cmd),
                            },
                            argv: None,
                            requires_shell: true,
                            lexed_words: words,
                        };
                    }
                }
            }
        }

        // The command is fully classified: execute the exact argument vector.
        ShellAnalysis {
            decision: AuthDecision::Allow,
            argv: Some(words.clone()),
            requires_shell: false,
            lexed_words: words,
        }
    }

    /// Checks a path-like argument against the protected-location list.
    ///
    /// Returns `None` when the argument is acceptable, otherwise a reason.
    fn check_path_argument(arg: &str, workspace_root: Option<&Path>) -> Option<String> {
        // A path argument that mentions a protected location is refused even if
        // it is only a prefix of a longer, legitimate path.
        let candidate = Path::new(arg);
        if candidate.is_absolute() {
            if is_protected_path(candidate) {
                return Some(format!(
                    "Access to protected system location '{}' is not permitted for read-only commands",
                    arg
                ));
            }
            return None;
        }

        // Relative paths are resolved against the workspace root when one is
        // configured; otherwise the relative form is checked for protected
        // components.
        if let Some(root) = workspace_root {
            let joined = root.join(candidate);
            if is_protected_path(&joined) {
                return Some(format!(
                    "Access to protected system location '{}' is not permitted for read-only commands",
                    arg
                ));
            }
        } else if is_protected_path(candidate) {
            return Some(format!(
                "Access to protected system location '{}' is not permitted for read-only commands",
                arg
            ));
        }

        // A relative path that climbs out of the workspace is not acceptable.
        if arg.split('/').any(|c| c == "..") {
            if let Some(root) = workspace_root {
                let joined = root.join(candidate);
                let normalised = joined.components().fold(PathBuf::new(), |mut acc, comp| {
                    match comp {
                        std::path::Component::ParentDir => {
                            acc.pop();
                        }
                        std::path::Component::CurDir => {}
                        other => acc.push(other),
                    }
                    acc
                });
                if !normalised.starts_with(root) {
                    return Some(format!(
                        "Path argument '{}' escapes the authorised workspace root",
                        arg
                    ));
                }
            }
        }

        None
    }

    /// Checks a path-like argument against the workspace boundary.
    fn check_workspace_argument(arg: &str, workspace_root: Option<&Path>) -> Option<String> {
        let root = match workspace_root {
            Some(r) => r,
            None => {
                return Some(
                    "No workspace root is configured for path arguments; approval required"
                        .to_string(),
                )
            }
        };
        let candidate = Path::new(arg);
        let joined = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            root.join(candidate)
        };
        let normalised = joined.components().fold(PathBuf::new(), |mut acc, comp| {
            match comp {
                std::path::Component::ParentDir => {
                    acc.pop();
                }
                std::path::Component::CurDir => {}
                other => acc.push(other),
            }
            acc
        });
        if !normalised.starts_with(root) {
            return Some(format!(
                "Path argument '{}' resolves outside the authorised workspace root",
                arg
            ));
        }
        None
    }

    fn is_local_verification_command(executable: &str, words: &[String]) -> bool {
        let base = executable.rsplit('/').next().unwrap_or(executable);
        match base {
            "cargo" => {
                if words.len() >= 2 {
                    matches!(words[1].as_str(), "test" | "check" | "build" | "clippy")
                } else {
                    false
                }
            }
            "npm" => {
                if words.len() >= 2 {
                    if words[1] == "test" {
                        true
                    } else if words[1] == "run" && words.len() >= 3 {
                        matches!(words[2].as_str(), "test" | "build" | "check" | "lint")
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            "pytest" => true,
            _ => false,
        }
    }

    /// Evaluates filesystem reads: canonical workspace / authorized folders confinement validation
    pub fn evaluate_file_read(ctx: &ExecutionContext, args: &serde_json::Value) -> AuthDecision {
        let path_str = match args["path"].as_str() {
            Some(p) if !p.trim().is_empty() => p.trim(),
            _ => {
                return AuthDecision::Deny {
                    reason: "Missing path parameter for read_file".into(),
                }
            }
        };

        // 1. Prohibited sensitive paths check
        let lower = path_str.to_lowercase();
        for &prohibited in Self::PROHIBITED_PATHS {
            if lower.contains(prohibited) {
                return AuthDecision::Deny {
                    reason: format!(
                        "Filesystem access to sensitive system location '{}' is strictly prohibited",
                        prohibited
                    ),
                };
            }
        }

        // 2. Traversal verification
        if path_str.contains("../") || path_str.contains("..\\") || path_str == ".." {
            return AuthDecision::Deny {
                reason: "Directory traversal ('..') escapes workspace root".into(),
            };
        }

        // 3. Task grant evaluation if present
        if let Some(ref grant) = ctx.task_grant {
            let now = chrono::Utc::now().timestamp_millis();
            if !grant.is_active(&ctx.run_id, now) || grant.mode == TaskAutonomyMode::Supervised {
                return AuthDecision::RequireApproval {
                    reason: if grant.mode == TaskAutonomyMode::Supervised {
                        "Supervised mode requires approval for file reads".to_string()
                    } else {
                        "Active task authorization grant has expired or is invalid for this run"
                            .to_string()
                    },
                    risk: RiskLevel::Safe,
                    summary: format!("Read file '{}'", path_str),
                };
            }
            if !grant.allowed_operations.contains(&RoutineOpClass::FileRead) {
                return AuthDecision::RequireApproval {
                    reason: "Task grant does not permit autonomous file reads".to_string(),
                    risk: RiskLevel::Safe,
                    summary: format!("Read file '{}'", path_str),
                };
            }
            let p = Path::new(path_str);
            if !grant.path_in_scope(p) {
                return AuthDecision::Deny {
                    reason: format!("Path '{}' is outside task authorized scope", path_str),
                };
            }
            return AuthDecision::Allow;
        }

        match Self::validate_path_confinement(ctx.workspace_root.as_deref(), path_str, FileOp::Read)
        {
            Ok(_) => AuthDecision::Allow,
            Err(reason) => AuthDecision::Deny { reason },
        }
    }

    /// Evaluates filesystem writes: prevents traversal, symlink escapes, and system modification
    pub fn evaluate_file_write(ctx: &ExecutionContext, args: &serde_json::Value) -> AuthDecision {
        let path_str = match args["path"].as_str() {
            Some(p) if !p.trim().is_empty() => p.trim(),
            _ => {
                return AuthDecision::Deny {
                    reason: "Missing path parameter for write_file".into(),
                }
            }
        };

        // 1. Prohibited sensitive paths check
        let lower = path_str.to_lowercase();
        for &prohibited in Self::PROHIBITED_PATHS {
            if lower.contains(prohibited) {
                return AuthDecision::Deny {
                    reason: format!(
                        "Filesystem access to sensitive system location '{}' is strictly prohibited",
                        prohibited
                    ),
                };
            }
        }

        // 2. Traversal verification
        if path_str.contains("../") || path_str.contains("..\\") || path_str == ".." {
            return AuthDecision::Deny {
                reason: "Directory traversal ('..') escapes workspace root".into(),
            };
        }

        // 3. Task grant evaluation if present
        if let Some(ref grant) = ctx.task_grant {
            let now = chrono::Utc::now().timestamp_millis();
            if !grant.is_active(&ctx.run_id, now) || grant.mode == TaskAutonomyMode::Supervised {
                return AuthDecision::RequireApproval {
                    reason: if grant.mode == TaskAutonomyMode::Supervised {
                        "Supervised mode requires approval for file writes".to_string()
                    } else {
                        "Active task authorization grant has expired or is invalid for this run"
                            .to_string()
                    },
                    risk: RiskLevel::Safe,
                    summary: format!("Write file '{}'", path_str),
                };
            }
            if !grant
                .allowed_operations
                .contains(&RoutineOpClass::FileWrite)
            {
                return AuthDecision::RequireApproval {
                    reason: "Task grant does not permit autonomous file writes".to_string(),
                    risk: RiskLevel::Safe,
                    summary: format!("Write file '{}'", path_str),
                };
            }
            let p = Path::new(path_str);
            if !grant.path_in_scope(p) {
                return AuthDecision::Deny {
                    reason: format!("Path '{}' is outside task authorized scope", path_str),
                };
            }
            return AuthDecision::Allow;
        }

        match Self::validate_path_confinement(
            ctx.workspace_root.as_deref(),
            path_str,
            FileOp::Write,
        ) {
            Ok(_) => AuthDecision::Allow,
            Err(reason) => AuthDecision::Deny { reason },
        }
    }

    /// Evaluates artifact creation: enforces safe filename and prevents traversal
    pub fn evaluate_create_artifact(
        _ctx: &ExecutionContext,
        args: &serde_json::Value,
    ) -> AuthDecision {
        let name = match args["name"].as_str() {
            Some(n) if !n.trim().is_empty() => n.trim(),
            _ => {
                return AuthDecision::Deny {
                    reason: "Missing name parameter for create_artifact".into(),
                }
            }
        };

        if name.contains("..") || name.contains('/') || name.contains('\\') || name.contains('\0') {
            return AuthDecision::Deny {
                reason:
                    "Artifact name cannot contain directory traversal, slash, or null characters"
                        .into(),
            };
        }

        AuthDecision::Allow
    }

    /// Evaluates computer use mouse/keyboard input
    pub fn evaluate_computer_action(
        policy: &GovernancePolicy,
        args: &serde_json::Value,
    ) -> AuthDecision {
        if policy.computer_use_mode == "AskAlways" {
            return AuthDecision::RequireApproval {
                reason:
                    "Computer-use policy 'AskAlways' requires confirmation for all UI automation"
                        .to_string(),
                risk: RiskLevel::Moderate,
                summary: format!("Perform desktop action: {:?}", args["action"]),
            };
        }

        let action_str = args["action"].as_str().unwrap_or_default();
        let text = args["text"].as_str().unwrap_or_default();

        if policy.computer_use_mode == "ConfirmDestructive" {
            let destructive_keys = ["alt+f4", "ctrl+w", "ctrl+q", "delete", "backspace"];
            if destructive_keys
                .iter()
                .any(|k| text.to_lowercase().contains(k))
            {
                return AuthDecision::RequireApproval {
                    reason: format!(
                        "Destructive keystroke '{}' requires user confirmation",
                        text
                    ),
                    risk: RiskLevel::Moderate,
                    summary: format!("Send keystroke: '{}'", text),
                };
            }
        }

        // Password detection in typed text
        if policy.require_takeover_for_credentials && action_str == "type" {
            if let Some(reason) = TakeoverController::detect_sensitive_checkpoint(text) {
                return AuthDecision::RequireTakeover { reason };
            }
        }

        AuthDecision::Allow
    }

    pub fn evaluate_click_mark(
        policy: &GovernancePolicy,
        args: &serde_json::Value,
    ) -> AuthDecision {
        if policy.computer_use_mode == "AskAlways" {
            let mark_id = args["mark_id"].as_u64().unwrap_or(0);
            return AuthDecision::RequireApproval {
                reason: "Policy 'AskAlways' requires confirmation before clicking UI elements"
                    .to_string(),
                risk: RiskLevel::Safe,
                summary: format!("Click Set-of-Marks tag #{}", mark_id),
            };
        }
        AuthDecision::Allow
    }

    /// Evaluates an application launch inside the Ghost display.
    ///
    /// The launch is expressed as an explicit executable plus argument vector,
    /// so the executable can be resolved and validated exactly like any other
    /// consequential action.  The previous implementation only checked whether
    /// a blocked domain appeared somewhere in a free-form command string,
    /// which is trivially bypassed and is not an authorisation check at all.
    pub fn evaluate_ghost_launch(
        policy: &GovernancePolicy,
        args: &serde_json::Value,
    ) -> AuthDecision {
        // Support the legacy free-form `command` argument only to produce a
        // clear denial: free-form command strings are no longer accepted.
        if let Some(cmd) = args["command"].as_str() {
            if !cmd.trim().is_empty() {
                return AuthDecision::Deny {
                    reason: "Free-form shell commands are not accepted for application launch. \
Use the 'executable' and 'args' fields so the launch can be validated."
                        .into(),
                };
            }
        }

        let executable = match args["executable"].as_str() {
            Some(e) if !e.trim().is_empty() => e.trim().to_string(),
            _ => {
                return AuthDecision::Deny {
                    reason: "Missing 'executable' parameter for application launch".into(),
                }
            }
        };

        if executable.contains('\0') {
            return AuthDecision::Deny {
                reason: "Executable name contains a null byte".into(),
            };
        }

        // The executable must not be a shell or interpreter, because those can
        // execute arbitrary code from their arguments.
        const INTERPRETERS: &[&str] = &[
            "sh",
            "bash",
            "zsh",
            "ksh",
            "csh",
            "dash",
            "fish",
            "env",
            "sudo",
            "su",
            "xargs",
            "eval",
            "exec",
            "python",
            "python3",
            "node",
            "ruby",
            "perl",
            "php",
            "awk",
            "sed",
            "cmd",
            "cmd.exe",
            "powershell",
            "pwsh",
            "source",
            "timeout",
            "nohup",
            "setsid",
        ];
        let base = executable
            .rsplit('/')
            .next()
            .unwrap_or(executable.as_str())
            .to_ascii_lowercase();
        if INTERPRETERS.contains(&base.as_str()) {
            return AuthDecision::Deny {
                reason: format!(
                    "Executable '{}' is a shell or interpreter and cannot be launched directly",
                    executable
                ),
            };
        }

        // Blocked domains are checked against the *resolved* argument values,
        // not against a substring of a command line.
        let arg_values: Vec<String> = args["args"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        for domain in &policy.blocked_domains {
            for value in std::iter::once(&executable).chain(arg_values.iter()) {
                if value.to_lowercase().contains(&domain.to_lowercase()) {
                    return AuthDecision::Deny {
                        reason: format!(
                            "Cannot launch application targeting blocked domain '{}'",
                            domain
                        ),
                    };
                }
            }
        }

        // The launch must be resolvable through the controlled discovery
        // mechanism before it is approved.
        let resolver = crate::execution::launcher::ExecutableResolver::new();
        let spec = crate::execution::launcher::ProgramSpec {
            executable: executable.clone(),
            args: arg_values.clone(),
            cwd: None,
            env: Vec::new(),
        };
        if let Err(e) = resolver.resolve(&spec) {
            return AuthDecision::Deny {
                reason: format!("Application executable rejected: {}", e),
            };
        }

        AuthDecision::RequireApproval {
            reason: format!(
                "Launching '{}' inside the isolated Ghost Display requires confirmation",
                executable
            ),
            risk: RiskLevel::Moderate,
            summary: format!(
                "Launch application: {} {}",
                executable,
                arg_values.join(" ")
            ),
        }
    }

    pub fn evaluate_skill_invoke(
        _policy: &GovernancePolicy,
        args: &serde_json::Value,
    ) -> AuthDecision {
        let skill_id = args["skill_id"].as_str().unwrap_or_default();
        if skill_id.is_empty() {
            return AuthDecision::Deny {
                reason: "Skill ID missing".into(),
            };
        }
        AuthDecision::Allow
    }

    pub fn evaluate_experiment(
        _policy: &GovernancePolicy,
        args: &serde_json::Value,
    ) -> AuthDecision {
        let name = args["name"].as_str().unwrap_or("unnamed experiment");
        let runtime = args["runtime"].as_str().unwrap_or("python");
        AuthDecision::RequireApproval {
            reason: format!(
                "Code experiment '{}' requests execution in isolated {} container sandbox",
                name, runtime
            ),
            risk: RiskLevel::Moderate,
            summary: format!("Run isolated {} experiment '{}'", runtime, name),
        }
    }

    pub fn evaluate_mcp_or_custom(
        db: &Database,
        _policy: &GovernancePolicy,
        tool_name: &str,
        args: &serde_json::Value,
    ) -> AuthDecision {
        // 1. Check if tool is a connector tool (either scoped e.g. "conn_...__tool" or raw tool name)
        let (account_id_opt, raw_name) = if let Some(idx) = tool_name.find("__") {
            let (prefix, rest) = tool_name.split_at(idx);
            let actual_tool = &rest[2..];
            (Some(prefix), actual_tool)
        } else {
            (None, tool_name)
        };

        // If scoped to a specific account, verify that account exists and is connected
        if let Some(account_id) = account_id_opt {
            match db.get_connected_account(account_id) {
                Ok(Some(account)) => {
                    if account.status != "connected" {
                        return AuthDecision::Deny {
                            reason: format!(
                                "Account '{}' is not connected (status: {})",
                                account.display_name, account.status
                            ),
                        };
                    }
                }
                _ => {
                    return AuthDecision::Deny {
                        reason: format!("Account '{}' not found or was disconnected", account_id),
                    };
                }
            }
        }

        // Check explicit connector tool policy in database
        if let Some(account_id) = account_id_opt {
            if let Ok(Some(policy_rec)) = db.get_connector_tool_policy(account_id, raw_name) {
                match policy_rec.policy.trim().to_lowercase().as_str() {
                    "allow" => {
                        // Destructive operations can NEVER be auto-approved
                        if policy_rec.risk_level.trim().to_lowercase() == "destructive"
                            || raw_name.contains("delete")
                            || raw_name.contains("destroy")
                        {
                            return AuthDecision::RequireApproval {
                                reason: format!(
                                    "Destructive connector action '{}' cannot be auto-approved and requires explicit user authorization",
                                    raw_name
                                ),
                                risk: RiskLevel::Destructive,
                                summary: format!(
                                    "Execute destructive connector tool '{}' with parameters: {}",
                                    raw_name, args
                                ),
                            };
                        }
                        return AuthDecision::Allow;
                    }
                    "deny" => {
                        return AuthDecision::Deny {
                            reason: format!(
                                "Connector action '{}' for account '{}' is blocked by security policy",
                                raw_name, account_id
                            ),
                        };
                    }
                    "ask" => {
                        let risk = match policy_rec.risk_level.as_str() {
                            "destructive" => RiskLevel::Destructive,
                            "safe" => RiskLevel::Safe,
                            _ => RiskLevel::Moderate,
                        };
                        return AuthDecision::RequireApproval {
                            reason: format!(
                                "Connector action '{}' requires explicit user authorization",
                                raw_name
                            ),
                            risk,
                            summary: format!(
                                "Run connector tool '{}' with parameters: {}",
                                raw_name, args
                            ),
                        };
                    }
                    _ => {}
                }
            }
        }

        // Known native connector safe read tools
        let safe_github_read_tools = [
            "github_get_user",
            "github_list_repositories",
            "github_get_repository",
            "github_list_issues",
            "github_get_issue",
            "github_list_pull_requests",
            "github_get_pull_request",
            "github_list_workflow_runs",
            "github_get_workflow_run_jobs",
        ];

        let write_github_tools = ["github_create_issue", "github_add_issue_comment"];

        if safe_github_read_tools.contains(&raw_name) {
            return AuthDecision::Allow;
        }

        if write_github_tools.contains(&raw_name) {
            return AuthDecision::RequireApproval {
                reason: format!(
                    "External modifying action '{}' requires user approval",
                    raw_name
                ),
                risk: RiskLevel::Moderate,
                summary: format!(
                    "Execute GitHub action '{}' with parameters: {}",
                    raw_name, args
                ),
            };
        }

        // Explicitly check persisted legacy MCP permissions
        let perm_key = format!("mcp_perm:{}", tool_name);
        if let Ok(Some(policy_val)) = db.get_setting(&perm_key) {
            match policy_val.trim().to_lowercase().as_str() {
                "allow" => return AuthDecision::Allow,
                "deny" => {
                    return AuthDecision::Deny {
                        reason: format!("MCP tool '{}' is explicitly blocked by policy", tool_name),
                    }
                }
                "ask" => {
                    return AuthDecision::RequireApproval {
                        reason: format!(
                            "MCP tool '{}' requires explicit user authorization",
                            tool_name
                        ),
                        risk: RiskLevel::Moderate,
                        summary: format!("Invoke MCP tool '{}' with args {}", tool_name, args),
                    }
                }
                _ => {}
            }
        }

        // Unknown / unconfigured external tools: conservative decision requires approval
        let risk = if raw_name.contains("delete") || raw_name.contains("destroy") {
            RiskLevel::Destructive
        } else {
            RiskLevel::Moderate
        };

        AuthDecision::RequireApproval {
            reason: format!(
                "External tool '{}' requires human authorization (unknown or modifying tool)",
                tool_name
            ),
            risk,
            summary: format!(
                "Invoke external tool '{}' with arguments: {}",
                tool_name, args
            ),
        }
    }

    /// Create and persist an approval request
    pub fn request_approval(
        db: &Database,
        run_id: &str,
        tool_name: &str,
        summary: &str,
        args: &serde_json::Value,
        risk: RiskLevel,
        reason: &str,
    ) -> Result<ApprovalRequest, String> {
        let id = format!("appr_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().timestamp_millis();
        let expires_at = now + 5 * 60 * 1000; // 5 minute expiration

        let risk_str = match risk {
            RiskLevel::Safe => "safe",
            RiskLevel::Moderate => "moderate",
            RiskLevel::Destructive => "destructive",
        };

        let req = ApprovalRequest {
            id,
            run_id: run_id.to_string(),
            tool_name: tool_name.to_string(),
            summary: summary.to_string(),
            args_json: args.to_string(),
            risk_level: risk_str.to_string(),
            reason: reason.to_string(),
            status: "pending".to_string(),
            created_at: now,
            expires_at,
            resolved_at: None,
            resolved_by: None,
        };

        db.create_approval_request(&req)
            .map_err(|e| format!("Failed to record approval request: {}", e))?;

        Ok(req)
    }
}
