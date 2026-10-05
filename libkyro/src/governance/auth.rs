use crate::execution::permissions::RiskLevel;
use crate::governance::policy::{GovernancePolicy, PolicyEngine};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    pub run_id: String,
    pub agent_id: String,
    pub conversation_id: Option<String>,
    pub workspace_root: Option<PathBuf>,
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

pub struct AuthorizationEngine;

impl AuthorizationEngine {
    /// Safe read-only executables permitted for SafeAutoApprove shell mode
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

        match tool_name {
            "execute_shell" | "bash" | "terminal" => Self::evaluate_shell(&policy, args),
            "read_file" => Self::evaluate_file_read(ctx, args),
            "write_file" => Self::evaluate_file_write(ctx, args),
            "create_artifact" => Self::evaluate_create_artifact(ctx, args),
            "computer_action" => Self::evaluate_computer_action(&policy, args),
            "click_mark" => Self::evaluate_click_mark(&policy, args),
            "capture_screen" | "capture_grounded_screen" => AuthDecision::Allow,
            "launch_ghost_app" => Self::evaluate_ghost_launch(&policy, args),
            "invoke_skill" => Self::evaluate_skill_invoke(&policy, args),
            _ => {
                // MCP or custom external tool
                Self::evaluate_mcp_or_custom(db, &policy, tool_name, args)
            }
        }
    }

    /// Evaluates shell command according to shell_mode policy
    pub fn evaluate_shell(policy: &GovernancePolicy, args: &serde_json::Value) -> AuthDecision {
        let cmd = match args["command"].as_str() {
            Some(c) if !c.trim().is_empty() => c.trim(),
            _ => {
                return AuthDecision::Deny {
                    reason: "Missing or empty shell command argument".into(),
                }
            }
        };

        let requested_sandbox = args["use_sandbox"].as_bool().unwrap_or(false);

        // Fail-closed sandbox policy
        if policy.shell_mode == "SandboxOnly" || requested_sandbox {
            let sandbox_status = DockerSandbox::probe();
            if !sandbox_status.available {
                return AuthDecision::Deny {
                    reason: format!(
                        "Sandbox execution required by policy ({}), but container engine is unavailable: {}",
                        policy.shell_mode, sandbox_status.message
                    ),
                };
            }
            return AuthDecision::AllowSandboxOnly {
                image: "alpine:latest".to_string(),
            };
        }

        // AskAlways mode
        if policy.shell_mode == "AskAlways" {
            return AuthDecision::RequireApproval {
                reason: "Policy 'AskAlways' requires confirmation for all shell execution"
                    .to_string(),
                risk: RiskLevel::Moderate,
                summary: format!("Execute shell command: '{}'", cmd),
            };
        }

        // SafeAutoApprove mode: Conservative parser
        // 1. Multiline input protection: Newlines are strictly prohibited from auto-approval
        if cmd.contains('\n') || cmd.contains('\r') {
            return AuthDecision::RequireApproval {
                reason: "Multiline commands require explicit approval".to_string(),
                risk: RiskLevel::Moderate,
                summary: format!("Execute multiline shell command:\n{}", cmd),
            };
        }

        // 2. Reject shell operators, redirs, pipes, substitutions, chained commands
        let has_complex_operators = cmd.contains('|')
            || cmd.contains('&')
            || cmd.contains(';')
            || cmd.contains('>')
            || cmd.contains('<')
            || cmd.contains('`')
            || cmd.contains("$(")
            || cmd.contains("${")
            || cmd.contains("||")
            || cmd.contains("&&")
            || cmd.contains('\\');

        if has_complex_operators {
            return AuthDecision::RequireApproval {
                reason: "Command contains shell operators, pipelines, substitutions, or chained execution".to_string(),
                risk: RiskLevel::Moderate,
                summary: format!("Run complex command: '{}'", cmd),
            };
        }

        // Tokenize using shell-words
        let words = match shell_words::split(cmd) {
            Ok(w) if !w.is_empty() => w,
            _ => {
                return AuthDecision::RequireApproval {
                    reason: "Failed to parse command arguments safely".to_string(),
                    risk: RiskLevel::Moderate,
                    summary: format!("Run unparsed shell command: '{}'", cmd),
                };
            }
        };

        // 3. Reject environment assignments preceding command
        if words[0].contains('=') {
            return AuthDecision::RequireApproval {
                reason: "Command with environment variable assignments requires approval"
                    .to_string(),
                risk: RiskLevel::Moderate,
                summary: format!("Run command with env assignment: '{}'", cmd),
            };
        }

        let primary_binary = words[0].as_str();

        // 4. Reject wrapper commands and shell interpreters
        let wrappers = [
            "sh", "bash", "zsh", "ksh", "csh", "dash", "busybox", "env", "sudo", "su", "xargs",
            "eval", "exec", "python", "python3", "node", "ruby", "perl", "awk", "sed",
        ];
        if wrappers.contains(&primary_binary) {
            return AuthDecision::RequireApproval {
                reason: format!(
                    "Wrapper command or interpreter '{}' requires approval",
                    primary_binary
                ),
                risk: RiskLevel::Moderate,
                summary: format!("Run wrapper command: '{}'", cmd),
            };
        }

        // 5. Check destructive commands
        let lower = cmd.to_lowercase();
        let destructive_patterns = [
            "rm ",
            "rm -",
            "mkfs",
            "dd if=",
            ":(){ :|:& };:",
            "> /dev/",
            "chmod ",
            "chown ",
            "shutdown",
            "reboot",
            "init 0",
            "pkill",
            "killall",
            "kill ",
            "wipefs",
            "fdisk",
        ];
        for pattern in &destructive_patterns {
            if lower.contains(pattern) || primary_binary == "rm" {
                return AuthDecision::RequireApproval {
                    reason: format!(
                        "Command contains prohibited destructive pattern '{}'",
                        pattern
                    ),
                    risk: RiskLevel::Destructive,
                    summary: format!("Execute potentially destructive command: '{}'", cmd),
                };
            }
        }

        // 6. Check if command is in safe read-only list
        let is_safe = Self::SAFE_COMMANDS.iter().any(|&safe| {
            if safe.contains(' ') {
                cmd == safe || cmd.starts_with(&format!("{} ", safe))
            } else {
                primary_binary == safe
            }
        });

        // For git, ensure subcommands are strictly read-only
        if primary_binary == "git" {
            let subcmd = words.get(1).map(|s| s.as_str()).unwrap_or("");
            let safe_git_subcmds = [
                "status",
                "log",
                "diff",
                "branch",
                "--version",
                "tag",
                "show",
            ];
            if !safe_git_subcmds.contains(&subcmd) {
                return AuthDecision::RequireApproval {
                    reason: format!("Git command '{}' modifies repository state", cmd),
                    risk: RiskLevel::Moderate,
                    summary: format!("Execute git command: '{}'", cmd),
                };
            }
        }

        // For compiler/runtime tools: only allow version query
        let version_only_tools = ["cargo", "python", "node", "npm", "rustc"];
        if version_only_tools.contains(&primary_binary) {
            let is_version_check = words.len() == 2
                && (words[1] == "--version" || words[1] == "-V" || words[1] == "-v");
            if !is_version_check {
                return AuthDecision::RequireApproval {
                    reason: format!(
                        "Command '{}' executes code or mutates system state",
                        primary_binary
                    ),
                    risk: RiskLevel::Moderate,
                    summary: format!("Run tool command: '{}'", cmd),
                };
            }
        }

        if is_safe {
            AuthDecision::Allow
        } else {
            AuthDecision::RequireApproval {
                reason: format!(
                    "Command '{}' modifies system state or is not in safe read-only allowlist",
                    primary_binary
                ),
                risk: RiskLevel::Moderate,
                summary: format!("Run host command: '{}'", cmd),
            }
        }
    }

    /// Evaluates filesystem reads: canonical workspace confinement validation
    pub fn evaluate_file_read(ctx: &ExecutionContext, args: &serde_json::Value) -> AuthDecision {
        let path_str = match args["path"].as_str() {
            Some(p) if !p.trim().is_empty() => p.trim(),
            _ => {
                return AuthDecision::Deny {
                    reason: "Missing path parameter for read_file".into(),
                }
            }
        };

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

    pub fn evaluate_ghost_launch(
        policy: &GovernancePolicy,
        args: &serde_json::Value,
    ) -> AuthDecision {
        let cmd = args["command"].as_str().unwrap_or_default();
        for domain in &policy.blocked_domains {
            if cmd.to_lowercase().contains(&domain.to_lowercase()) {
                return AuthDecision::Deny {
                    reason: format!(
                        "Cannot launch application targeting blocked domain '{}'",
                        domain
                    ),
                };
            }
        }
        AuthDecision::Allow
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
