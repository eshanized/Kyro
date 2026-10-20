//! Developer checkpoints, change review, safe patch-based rollback,
//! and build/CI failure doctor diagnostics.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Pre-existing state of a repository before agent operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitWorkspaceState {
    pub is_git_repo: bool,
    pub current_branch: Option<String>,
    pub has_uncommitted_changes: bool,
    pub pre_existing_dirty_files: Vec<String>,
}

/// A reviewable diff for a single modified file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileDiff {
    pub file_path: String,
    pub original_content: Option<String>,
    pub new_content: String,
    pub additions: usize,
    pub deletions: usize,
    pub unified_diff: String,
    pub risk_assessment: String, // "low", "medium", "high"
}

/// A reversible checkpoint capturing all changes made by an agent run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeCheckpoint {
    pub checkpoint_id: String,
    pub workspace_path: PathBuf,
    pub description: String,
    pub created_at: i64,
    pub pre_existing_dirty_files: Vec<String>,
    pub modified_files: Vec<FileDiff>,
    pub status: String, // "active", "rolled_back"
}

/// The result of executing a checkpoint rollback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackResult {
    pub checkpoint_id: String,
    pub files_reverted: Vec<String>,
    pub conflicts_detected: Vec<String>,
    pub status: String, // "success", "partial", "failed"
}

/// Structured diagnostic from the Build Doctor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildDoctorDiagnostic {
    pub root_cause_error: String,
    pub file: Option<String>,
    pub line_number: Option<usize>,
    pub error_code: Option<String>,
    pub category: String, // "syntax", "type_error", "missing_import", "test_failure", "dependency", "runtime"
    pub suggested_repair: String,
    pub reproduced: bool,
}

pub struct CheckpointManager;

impl CheckpointManager {
    /// Inspects the Git workspace state, identifying pre-existing dirty files so they are preserved.
    pub fn inspect_git_state(workspace: &Path) -> GitWorkspaceState {
        let git_dir = workspace.join(".git");
        if !git_dir.exists() {
            return GitWorkspaceState {
                is_git_repo: false,
                current_branch: None,
                has_uncommitted_changes: false,
                pre_existing_dirty_files: Vec::new(),
            };
        }

        let branch_cmd = Command::new("git")
            .arg("-C")
            .arg(workspace)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output();

        let branch = branch_cmd
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string());

        let status_cmd = Command::new("git")
            .arg("-C")
            .arg(workspace)
            .args(["status", "--porcelain"])
            .output();

        let mut dirty = Vec::new();
        if let Ok(output) = status_cmd {
            if let Ok(s) = String::from_utf8(output.stdout) {
                for line in s.lines() {
                    let trimmed = line.trim();
                    if trimmed.len() >= 3 {
                        let path_part = &trimmed[3..].trim();
                        dirty.push(path_part.to_string());
                    }
                }
            }
        }

        let has_uncommitted = !dirty.is_empty();
        GitWorkspaceState {
            is_git_repo: true,
            current_branch: branch,
            has_uncommitted_changes: has_uncommitted,
            pre_existing_dirty_files: dirty,
        }
    }

    /// Creates a new checkpoint for an upcoming agent task.
    pub fn create_checkpoint(workspace: &Path, description: &str) -> CodeCheckpoint {
        let git_state = Self::inspect_git_state(workspace);
        CodeCheckpoint {
            checkpoint_id: format!("chk_{}", uuid::Uuid::new_v4()),
            workspace_path: workspace.to_path_buf(),
            description: description.to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
            pre_existing_dirty_files: git_state.pre_existing_dirty_files,
            modified_files: Vec::new(),
            status: "active".to_string(),
        }
    }

    /// Records a file change, computing unified diff and assessing risk.
    pub fn record_change(
        checkpoint: &mut CodeCheckpoint,
        file_path_rel: &str,
        original_content: Option<&str>,
        new_content: &str,
    ) {
        let (additions, deletions, diff_text) =
            Self::compute_unified_diff(file_path_rel, original_content, new_content);

        let risk = Self::assess_risk(file_path_rel, &diff_text);

        // Update if already exists in modified_files, or push new
        if let Some(existing) = checkpoint
            .modified_files
            .iter_mut()
            .find(|f| f.file_path == file_path_rel)
        {
            existing.new_content = new_content.to_string();
            existing.additions = additions;
            existing.deletions = deletions;
            existing.unified_diff = diff_text;
            existing.risk_assessment = risk;
        } else {
            checkpoint.modified_files.push(FileDiff {
                file_path: file_path_rel.to_string(),
                original_content: original_content.map(|s| s.to_string()),
                new_content: new_content.to_string(),
                additions,
                deletions,
                unified_diff: diff_text,
                risk_assessment: risk,
            });
        }
    }

    /// Evaluates modification risk based on file path and changes.
    pub fn assess_risk(file_path: &str, diff: &str) -> String {
        let lower = file_path.to_lowercase();
        if lower.contains(".env")
            || lower.contains("secret")
            || lower.contains("credentials")
            || lower.contains("package.json")
            || lower.contains("cargo.toml")
            || lower.contains("security")
            || lower.contains(".github/workflows")
        {
            "high".to_string()
        } else if lower.contains("test")
            || lower.contains("migration")
            || lower.contains("schema")
            || lower.contains("types")
            || diff.lines().count() > 100
        {
            "medium".to_string()
        } else {
            "low".to_string()
        }
    }

    /// Generates unified diff text line-by-line.
    pub fn compute_unified_diff(
        path: &str,
        orig: Option<&str>,
        new: &str,
    ) -> (usize, usize, String) {
        let orig_lines: Vec<&str> = orig.map(|s| s.lines().collect()).unwrap_or_default();
        let new_lines: Vec<&str> = new.lines().collect();

        let mut diff_str = String::new();
        diff_str.push_str(&format!("--- a/{}\n", path));
        diff_str.push_str(&format!("+++ b/{}\n", path));

        let mut additions = 0;
        let mut deletions = 0;

        // Simple Myers/LCS-approximated chunk diff
        let mut i = 0;
        let mut j = 0;

        while i < orig_lines.len() || j < new_lines.len() {
            if i < orig_lines.len() && j < new_lines.len() && orig_lines[i] == new_lines[j] {
                diff_str.push_str(&format!(" {}\n", orig_lines[i]));
                i += 1;
                j += 1;
            } else if i < orig_lines.len()
                && (j >= new_lines.len() || !new_lines[j..].contains(&orig_lines[i]))
            {
                diff_str.push_str(&format!("-{}\n", orig_lines[i]));
                deletions += 1;
                i += 1;
            } else if j < new_lines.len() {
                diff_str.push_str(&format!("+{}\n", new_lines[j]));
                additions += 1;
                j += 1;
            }
        }

        (additions, deletions, diff_str)
    }

    /// Reverts only the agent-modified files in this checkpoint, checking for external user modifications.
    pub fn rollback(checkpoint: &mut CodeCheckpoint) -> Result<RollbackResult, String> {
        if checkpoint.status == "rolled_back" {
            return Err("Checkpoint has already been rolled back.".to_string());
        }

        let mut reverted = Vec::new();
        let mut conflicts = Vec::new();

        for file_diff in &checkpoint.modified_files {
            let full_path = checkpoint.workspace_path.join(&file_diff.file_path);

            if !full_path.exists() {
                if file_diff.original_content.is_some() {
                    conflicts.push(format!(
                        "File '{}' was deleted after checkpoint creation.",
                        file_diff.file_path
                    ));
                    continue;
                }
            } else {
                // Conflict check: verify current content matches new_content before reverting
                let current_content = match fs::read_to_string(&full_path) {
                    Ok(c) => c,
                    Err(e) => {
                        conflicts.push(format!("Cannot read '{}': {}", file_diff.file_path, e));
                        continue;
                    }
                };

                if current_content != file_diff.new_content {
                    conflicts.push(format!(
                        "Conflict: '{}' has been modified by the user or another process since agent edits.",
                        file_diff.file_path
                    ));
                    continue;
                }
            }

            match &file_diff.original_content {
                Some(original) => {
                    if let Some(parent) = full_path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    if let Err(e) = fs::write(&full_path, original) {
                        conflicts.push(format!(
                            "Failed to restore '{}': {}",
                            file_diff.file_path, e
                        ));
                        continue;
                    }
                    reverted.push(file_diff.file_path.clone());
                }
                None => {
                    // File was newly created by agent; remove it
                    if full_path.exists() {
                        if let Err(e) = fs::remove_file(&full_path) {
                            conflicts.push(format!(
                                "Failed to delete newly created '{}': {}",
                                file_diff.file_path, e
                            ));
                            continue;
                        }
                    }
                    reverted.push(file_diff.file_path.clone());
                }
            }
        }

        let status = if conflicts.is_empty() {
            "success".to_string()
        } else if !reverted.is_empty() {
            "partial".to_string()
        } else {
            "failed".to_string()
        };

        checkpoint.status = "rolled_back".to_string();

        Ok(RollbackResult {
            checkpoint_id: checkpoint.checkpoint_id.clone(),
            files_reverted: reverted,
            conflicts_detected: conflicts,
            status,
        })
    }
}

pub struct BuildDoctor;

impl BuildDoctor {
    /// Parses build, test, and compiler error streams to find the first root cause and suggest a minimal fix.
    pub fn diagnose_failure(
        _working_dir: &Path,
        _command: &str,
        output: &str,
    ) -> BuildDoctorDiagnostic {
        // Rust rustc error: error[E0433]: cannot find type `X` in this scope --> src/foo.rs:12:5
        let re_rust_error =
            regex::Regex::new(r"error\[(E\d+)\]:\s*(.+)\n\s*-->\s*([^:]+):(\d+):(\d+)").unwrap();
        if let Some(cap) = re_rust_error.captures(output) {
            let code = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let desc = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let file = cap.get(3).map(|m| m.as_str().to_string());
            let line = cap.get(4).and_then(|m| m.as_str().parse::<usize>().ok());

            let (cat, repair) = if desc.contains("cannot find") || desc.contains("undeclared") {
                (
                    "missing_import".to_string(),
                    "Add missing `use` statement or verify dependency in Cargo.toml.".to_string(),
                )
            } else if desc.contains("mismatched types") || desc.contains("expected") {
                (
                    "type_error".to_string(),
                    "Adjust variable types or use conversion methods (.into(), .to_string(), as)."
                        .to_string(),
                )
            } else {
                (
                    "syntax".to_string(),
                    format!(
                        "Fix Rust compiler error {} in {}.",
                        code,
                        file.as_deref().unwrap_or("source")
                    ),
                )
            };

            return BuildDoctorDiagnostic {
                root_cause_error: format!("Rust Error [{}]: {}", code, desc),
                file,
                line_number: line,
                error_code: Some(code),
                category: cat,
                suggested_repair: repair,
                reproduced: true,
            };
        }

        // TypeScript error: src/App.tsx:42:15 - error TS2304: Cannot find name 'foo'.
        let re_ts_error =
            regex::Regex::new(r"([^\s:]+\.[tj]sx?):(\d+):(\d+)\s*-\s*error\s*(TS\d+):\s*(.+)")
                .unwrap();
        if let Some(cap) = re_ts_error.captures(output) {
            let file = cap.get(1).map(|m| m.as_str().to_string());
            let line = cap.get(2).and_then(|m| m.as_str().parse::<usize>().ok());
            let code = cap
                .get(4)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let desc = cap
                .get(5)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            let (cat, repair) = if desc.contains("Cannot find name")
                || desc.contains("Cannot find module")
            {
                (
                    "missing_import".to_string(),
                    "Import the missing module or symbol in the file header.".to_string(),
                )
            } else if desc.contains("Type '") && desc.contains("is not assignable to") {
                (
                    "type_error".to_string(),
                    "Align type annotation or cast expression with proper interface.".to_string(),
                )
            } else {
                (
                    "syntax".to_string(),
                    format!(
                        "Resolve TypeScript error {} in {}.",
                        code,
                        file.as_deref().unwrap_or("source")
                    ),
                )
            };

            return BuildDoctorDiagnostic {
                root_cause_error: format!("TypeScript Error [{}]: {}", code, desc),
                file,
                line_number: line,
                error_code: Some(code),
                category: cat,
                suggested_repair: repair,
                reproduced: true,
            };
        }

        // Vitest / Jest test failure: FAIL src/app.test.tsx > test description
        let re_test_fail = regex::Regex::new(r"(?i)(?:FAIL|AssertionError):\s*([^\n]+)").unwrap();
        if let Some(cap) = re_test_fail.captures(output) {
            let msg = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            return BuildDoctorDiagnostic {
                root_cause_error: format!("Test Assertion Failure: {}", msg),
                file: None,
                line_number: None,
                error_code: None,
                category: "test_failure".to_string(),
                suggested_repair: "Review test assertion expectations and update component state or implementation.".to_string(),
                reproduced: true,
            };
        }

        // Generic fallback error
        BuildDoctorDiagnostic {
            root_cause_error: "Non-zero exit code encountered during build or test execution.".to_string(),
            file: None,
            line_number: None,
            error_code: None,
            category: "runtime".to_string(),
            suggested_repair: "Inspect command output logs and verify dependencies, syntax, and environment variables.".to_string(),
            reproduced: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unified_diff_and_risk_assessment() {
        let orig = "fn main() {\n    println!(\"hello\");\n}\n";
        let new = "fn main() {\n    println!(\"hello world\");\n    println!(\"bonus\");\n}\n";

        let (additions, _deletions, diff) =
            CheckpointManager::compute_unified_diff("src/main.rs", Some(orig), new);

        assert!(additions >= 2);
        assert!(diff.contains("--- a/src/main.rs"));
        assert!(diff.contains("+++ b/src/main.rs"));

        let risk_src = CheckpointManager::assess_risk("src/main.rs", &diff);
        assert_eq!(risk_src, "low");

        let risk_env = CheckpointManager::assess_risk(".env.production", &diff);
        assert_eq!(risk_env, "high");

        let risk_test = CheckpointManager::assess_risk("tests/auth_test.rs", &diff);
        assert_eq!(risk_test, "medium");
    }

    #[test]
    fn test_checkpoint_creation_and_safe_rollback() {
        let temp = std::env::temp_dir().join(format!("kyro_chk_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp).unwrap();

        let file_path = temp.join("calculator.ts");
        let initial_content = "export function add(a: number, b: number) { return a + b; }\n";
        fs::write(&file_path, initial_content).unwrap();

        let mut chk = CheckpointManager::create_checkpoint(&temp, "Fix calculation bug");

        // Simulate agent editing calculator.ts
        let agent_content = "export function add(a: number, b: number) { return a + b + 1; }\n";
        fs::write(&file_path, agent_content).unwrap();
        CheckpointManager::record_change(
            &mut chk,
            "calculator.ts",
            Some(initial_content),
            agent_content,
        );

        assert_eq!(chk.modified_files.len(), 1);
        assert_eq!(chk.status, "active");

        // Rollback
        let res = CheckpointManager::rollback(&mut chk).expect("Rollback succeeds");
        assert_eq!(res.status, "success");
        assert_eq!(res.files_reverted, vec!["calculator.ts"]);
        assert!(res.conflicts_detected.is_empty());

        // File should be restored to initial_content
        let restored = fs::read_to_string(&file_path).unwrap();
        assert_eq!(restored, initial_content);

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_rollback_conflict_detection_when_user_modified_file() {
        let temp = std::env::temp_dir().join(format!("kyro_chk_conflict_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp).unwrap();

        let file_path = temp.join("config.json");
        let initial_content = "{\"port\": 3000}";
        fs::write(&file_path, initial_content).unwrap();

        let mut chk = CheckpointManager::create_checkpoint(&temp, "Update config");

        let agent_content = "{\"port\": 4000}";
        fs::write(&file_path, agent_content).unwrap();
        CheckpointManager::record_change(
            &mut chk,
            "config.json",
            Some(initial_content),
            agent_content,
        );

        // User edits file concurrently
        let user_diverged_content = "{\"port\": 8080, \"debug\": true}";
        fs::write(&file_path, user_diverged_content).unwrap();

        // Rollback must detect conflict and NOT blindly overwrite user's work!
        let res = CheckpointManager::rollback(&mut chk).expect("Rollback handles conflict");
        assert_eq!(res.status, "failed");
        assert_eq!(res.conflicts_detected.len(), 1);
        assert!(res.conflicts_detected[0].contains("Conflict"));
        // User content preserved
        assert_eq!(
            fs::read_to_string(&file_path).unwrap(),
            user_diverged_content
        );

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_build_doctor_rustc_and_typescript_parsing() {
        let rust_err = r"
   Compiling demo v0.1.0 (/path)
error[E0433]: cannot find type `MissingType` in this scope
   --> src/agent/mod.rs:42:15
    |
42  |     let x: MissingType = 10;
    |            ^^^^^^^^^^^ not found in this scope
";
        let diag = BuildDoctor::diagnose_failure(Path::new("."), "cargo build", rust_err);
        assert_eq!(diag.category, "missing_import");
        assert_eq!(diag.error_code, Some("E0433".to_string()));
        assert_eq!(diag.file, Some("src/agent/mod.rs".to_string()));
        assert_eq!(diag.line_number, Some(42));
        assert!(diag.suggested_repair.contains("missing `use` statement"));

        let ts_err = "src/index.ts:15:3 - error TS2304: Cannot find name 'render'.";
        let diag_ts = BuildDoctor::diagnose_failure(Path::new("."), "npx tsc", ts_err);
        assert_eq!(diag_ts.category, "missing_import");
        assert_eq!(diag_ts.error_code, Some("TS2304".to_string()));
        assert_eq!(diag_ts.file, Some("src/index.ts".to_string()));
        assert_eq!(diag_ts.line_number, Some(15));
    }
}
