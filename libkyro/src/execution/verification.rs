use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    Pending,
    Running,
    AwaitingApproval,
    Succeeded,
    Verified,
    Failed,
    Uncertain,
    Unverified,
    Cancelled,
    NotApplicable,
}

impl OutcomeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutcomeStatus::Pending => "pending",
            OutcomeStatus::Running => "running",
            OutcomeStatus::AwaitingApproval => "awaiting_approval",
            OutcomeStatus::Succeeded => "succeeded",
            OutcomeStatus::Verified => "verified",
            OutcomeStatus::Failed => "failed",
            OutcomeStatus::Uncertain => "uncertain",
            OutcomeStatus::Unverified => "unverified",
            OutcomeStatus::Cancelled => "cancelled",
            OutcomeStatus::NotApplicable => "not_applicable",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            OutcomeStatus::Succeeded
                | OutcomeStatus::Verified
                | OutcomeStatus::Failed
                | OutcomeStatus::Uncertain
                | OutcomeStatus::Unverified
                | OutcomeStatus::Cancelled
        )
    }
}

fn default_trusted_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerificationResult {
    pub status: OutcomeStatus,
    pub condition_checked: String,
    pub evidence: String,
    pub resource: Option<String>,
    pub human_intervention_required: bool,
    #[serde(default)]
    pub action_dispatched: bool,
    #[serde(default)]
    pub action_effect_observed: bool,
    #[serde(default)]
    pub task_postcondition_verified: bool,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub action_name: Option<String>,
    #[serde(default = "default_trusted_true")]
    pub trusted_verifier: bool,
    pub details: serde_json::Value,
}

impl VerificationResult {
    pub fn success(
        condition: impl Into<String>,
        evidence: impl Into<String>,
        resource: Option<String>,
    ) -> Self {
        Self {
            status: OutcomeStatus::Succeeded,
            condition_checked: condition.into(),
            evidence: evidence.into(),
            resource,
            human_intervention_required: false,
            action_dispatched: true,
            action_effect_observed: true,
            task_postcondition_verified: true,
            run_id: None,
            tool_call_id: None,
            action_name: None,
            trusted_verifier: true,
            details: serde_json::Value::Null,
        }
    }

    pub fn failure(
        condition: impl Into<String>,
        evidence: impl Into<String>,
        resource: Option<String>,
    ) -> Self {
        Self {
            status: OutcomeStatus::Failed,
            condition_checked: condition.into(),
            evidence: evidence.into(),
            resource,
            human_intervention_required: true,
            action_dispatched: true,
            action_effect_observed: false,
            task_postcondition_verified: false,
            run_id: None,
            tool_call_id: None,
            action_name: None,
            trusted_verifier: true,
            details: serde_json::Value::Null,
        }
    }

    pub fn uncertain(
        condition: impl Into<String>,
        evidence: impl Into<String>,
        resource: Option<String>,
        human_intervention_required: bool,
    ) -> Self {
        Self {
            status: OutcomeStatus::Uncertain,
            condition_checked: condition.into(),
            evidence: evidence.into(),
            resource,
            human_intervention_required,
            action_dispatched: true,
            action_effect_observed: false,
            task_postcondition_verified: false,
            run_id: None,
            tool_call_id: None,
            action_name: None,
            trusted_verifier: true,
            details: serde_json::Value::Null,
        }
    }

    pub fn not_applicable(
        condition: impl Into<String>,
        evidence: impl Into<String>,
        resource: Option<String>,
    ) -> Self {
        Self {
            status: OutcomeStatus::NotApplicable,
            condition_checked: condition.into(),
            evidence: evidence.into(),
            resource,
            human_intervention_required: false,
            action_dispatched: true,
            action_effect_observed: false,
            task_postcondition_verified: false,
            run_id: None,
            tool_call_id: None,
            action_name: None,
            trusted_verifier: true,
            details: serde_json::Value::Null,
        }
    }

    pub fn bind(mut self, run_id: &str, tool_call_id: &str, action_name: &str) -> Self {
        self.run_id = Some(run_id.to_string());
        self.tool_call_id = Some(tool_call_id.to_string());
        self.action_name = Some(action_name.to_string());
        self
    }
}

/// Verifies postcondition for file write operations:
/// Confirms that the target file exists, is readable, and its content/hash matches expectations.
pub fn verify_file_postcondition(
    path: &Path,
    expected_content: Option<&[u8]>,
    expected_hash: Option<&str>,
) -> VerificationResult {
    let resource = Some(path.to_string_lossy().to_string());

    if !path.exists() {
        return VerificationResult::failure(
            "File existence on disk",
            format!(
                "Target file '{}' does not exist after write operation",
                path.display()
            ),
            resource,
        );
    }

    let actual_bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            return VerificationResult::failure(
                "File readability on disk",
                format!(
                    "Failed to read back written file '{}': {}",
                    path.display(),
                    e
                ),
                resource,
            );
        }
    };

    if let Some(expected) = expected_content {
        if actual_bytes != expected {
            return VerificationResult::failure(
                "File content integrity",
                format!(
                    "Content mismatch in '{}': expected {} bytes, read back {} bytes with differing content",
                    path.display(),
                    expected.len(),
                    actual_bytes.len()
                ),
                resource,
            );
        }
    }

    if let Some(expected_h) = expected_hash {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&actual_bytes);
        let actual_hash = hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        if !actual_hash.eq_ignore_ascii_case(expected_h) {
            return VerificationResult::failure(
                "File cryptographic hash verification",
                format!(
                    "SHA-256 hash mismatch for '{}': expected {}, computed {}",
                    path.display(),
                    expected_h,
                    actual_hash
                ),
                resource,
            );
        }
    }

    VerificationResult::success(
        "File postcondition verification",
        format!(
            "File '{}' verified on disk (size: {} bytes)",
            path.display(),
            actual_bytes.len()
        ),
        resource,
    )
}

/// Verifies postcondition for command execution.
pub fn verify_command_postcondition(
    exit_code: i32,
    stdout: &str,
    stderr: &str,
    expected_pattern: Option<&str>,
) -> VerificationResult {
    let combined = format!("{}\n{}", stdout, stderr);
    let resource = Some(format!("exit_code: {}", exit_code));

    if exit_code != 0 {
        return VerificationResult::failure(
            "Process exit status check",
            format!("Process terminated with non-zero exit code: {}", exit_code),
            resource,
        );
    }

    if let Some(pattern) = expected_pattern {
        if !combined.contains(pattern) {
            return VerificationResult::uncertain(
                "Command output pattern check",
                format!("Expected pattern '{}' not found in command output", pattern),
                resource,
                false,
            );
        }
    }

    VerificationResult::success(
        "Process exit status check",
        "Process completed with exit code 0".to_string(),
        resource,
    )
}

/// Verifies postcondition for computer-use UI automation.
/// Distinguishes input dispatch (`action_dispatched`), visual observation (`action_effect_observed`),
/// and task postcondition (`task_postcondition_verified`).
/// Visual changes alone are observations, not affirmative proof of task completion.
pub fn verify_computer_action_postcondition(
    action: &str,
    x: Option<i32>,
    y: Option<i32>,
    diff_percent: f64,
) -> VerificationResult {
    let resource = match (x, y) {
        (Some(px), Some(py)) => Some(format!("action '{}' at ({}, {})", action, px, py)),
        _ => Some(format!("action '{}'", action)),
    };

    if x.is_some() && y.is_some() {
        if diff_percent > 0.05 {
            VerificationResult {
                status: OutcomeStatus::Uncertain,
                condition_checked: "UI visual state transition observation".to_string(),
                evidence: format!(
                    "Visual difference of {:.2}% detected (observation of UI transition); visual changes alone cannot establish task postcondition without application-state verification",
                    diff_percent * 100.0
                ),
                resource,
                human_intervention_required: false,
                action_dispatched: true,
                action_effect_observed: true,
                task_postcondition_verified: false,
                run_id: None,
                tool_call_id: None,
                action_name: Some(action.to_string()),
                trusted_verifier: true,
                details: serde_json::json!({
                    "visual_change_percent": diff_percent,
                    "observation_only": true,
                }),
            }
        } else {
            VerificationResult {
                status: OutcomeStatus::Uncertain,
                condition_checked: "UI visual state transition observation".to_string(),
                evidence: format!(
                    "The input was delivered but visual difference was only {:.2}% (below 5% threshold); the target UI element may not have changed state",
                    diff_percent * 100.0
                ),
                resource,
                human_intervention_required: true,
                action_dispatched: true,
                action_effect_observed: false,
                task_postcondition_verified: false,
                run_id: None,
                tool_call_id: None,
                action_name: Some(action.to_string()),
                trusted_verifier: true,
                details: serde_json::json!({
                    "visual_change_percent": diff_percent,
                    "negligible_change": true,
                }),
            }
        }
    } else {
        VerificationResult {
            status: OutcomeStatus::NotApplicable,
            condition_checked: "Non-coordinate action dispatch".to_string(),
            evidence: format!(
                "Action '{}' dispatched without coordinate targeting; application outcome is unverified without an authoritative postcondition",
                action
            ),
            resource,
            human_intervention_required: false,
            action_dispatched: true,
            action_effect_observed: false,
            task_postcondition_verified: false,
            run_id: None,
            tool_call_id: None,
            action_name: Some(action.to_string()),
            trusted_verifier: true,
            details: serde_json::json!({
                "dispatch_only": true,
            }),
        }
    }
}

/// Conclusively verifies a computer action when an authoritative postcondition verifier is provided.
pub fn verify_computer_action_with_target_postcondition<F>(
    action: &str,
    x: Option<i32>,
    y: Option<i32>,
    diff_percent: f64,
    postcondition_check: F,
) -> VerificationResult
where
    F: FnOnce() -> (bool, String, Option<String>),
{
    let resource = match (x, y) {
        (Some(px), Some(py)) => Some(format!("action '{}' at ({}, {})", action, px, py)),
        _ => Some(format!("action '{}'", action)),
    };
    let (passed, check_evidence, check_resource) = postcondition_check();
    let final_resource = check_resource.or(resource);
    if passed {
        VerificationResult {
            status: OutcomeStatus::Succeeded,
            condition_checked: "Authoritative application-state postcondition".to_string(),
            evidence: check_evidence,
            resource: final_resource,
            human_intervention_required: false,
            action_dispatched: true,
            action_effect_observed: diff_percent > 0.05,
            task_postcondition_verified: true,
            run_id: None,
            tool_call_id: None,
            action_name: Some(action.to_string()),
            trusted_verifier: true,
            details: serde_json::json!({
                "visual_change_percent": diff_percent,
                "authoritative_verified": true,
            }),
        }
    } else {
        VerificationResult {
            status: OutcomeStatus::Failed,
            condition_checked: "Authoritative application-state postcondition".to_string(),
            evidence: check_evidence,
            resource: final_resource,
            human_intervention_required: true,
            action_dispatched: true,
            action_effect_observed: diff_percent > 0.05,
            task_postcondition_verified: false,
            run_id: None,
            tool_call_id: None,
            action_name: Some(action.to_string()),
            trusted_verifier: true,
            details: serde_json::json!({
                "visual_change_percent": diff_percent,
                "authoritative_verified": false,
            }),
        }
    }
}
