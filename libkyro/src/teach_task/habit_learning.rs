//! Local habit learning and automatic routine suggestion subsystem.
//!
//! Analyzes eligible, completed, successful agent runs and pre-recorded skills to
//! detect repeated workflows, extracts safe parameterized signatures, computes
//! confidence scores, and produces human-reviewed routine suggestions.
//!
//! Privacy-first: Disabled by default. Sensitive content (credentials, passwords,
//! private tokens) is filtered out. Suggestions are never auto-scheduled or executed
//! without explicit human review and opt-in.

use crate::storage::{Database, HabitSuggestionRecord, RoutineRecord};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use uuid::Uuid;

/// Minimum confidence required for a suggestion to be surfaced.
pub const MIN_CONFIDENCE_THRESHOLD: f32 = 0.5;

/// Sensitive keywords and substrings that disqualify an action or run from habit learning.
const SENSITIVE_PATTERNS: &[&str] = &[
    "password",
    "secret",
    "token",
    "api_key",
    "apikey",
    "auth",
    "bearer",
    "credential",
    "private_key",
    "private key",
    "openssh",
    "ssh-rsa",
    "id_rsa",
    ".env",
    "sudo",
    "mfa",
    "captcha",
    "session_token",
];

/// Checks if a string contains any sensitive token or credential pattern.
pub fn contains_sensitive_info(text: &str) -> bool {
    let lower = text.to_lowercase();
    for pattern in SENSITIVE_PATTERNS {
        if lower.contains(pattern) {
            return true;
        }
    }
    false
}

/// Computes SHA-256 hex digest for a string.
fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let hash = hasher.finalize();
    hash.iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

/// A normalized workflow step extracted from a run event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedStep {
    pub tool_name: String,
    pub primary_arg: String,
}

/// Supporting evidence item explaining why a routine was suggested.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HabitEvidence {
    pub run_id: String,
    pub timestamp: i64,
    pub tools_used: Vec<String>,
    pub summary: String,
}

/// The habit learning engine.
pub struct HabitLearningEngine {
    db: Database,
}

impl HabitLearningEngine {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Checks if habit learning is currently enabled by the user.
    pub fn is_enabled(&self) -> bool {
        self.db
            .get_habit_settings()
            .map(|s| s.enabled)
            .unwrap_or(false)
    }

    /// Normalizes a tool invocation into a safe, parameterized signature.
    /// Returns None if the invocation contains sensitive arguments.
    pub fn normalize_tool_call(tool_name: &str, args_json: &str) -> Option<NormalizedStep> {
        if contains_sensitive_info(args_json) {
            return None;
        }

        let parsed: serde_json::Value =
            serde_json::from_str(args_json).unwrap_or(serde_json::Value::Null);

        let primary_arg = match tool_name {
            "execute_shell" | "bash" => {
                let cmd = parsed.get("command").and_then(|v| v.as_str()).unwrap_or("");
                if contains_sensitive_info(cmd) {
                    return None;
                }
                // Extract primary binary or command name (e.g. 'cargo test', 'npm run build')
                let first_two_words: Vec<&str> = cmd.split_whitespace().take(2).collect();
                first_two_words.join(" ")
            }
            "read_file" | "write_file" => {
                let path_str = parsed.get("path").and_then(|v| v.as_str()).unwrap_or("");
                if contains_sensitive_info(path_str) {
                    return None;
                }
                // Generalize path by keeping extension and filename only
                std::path::Path::new(path_str)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("file")
                    .to_string()
            }
            "discover_project" | "inspect_project" => {
                let name = parsed
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("project");
                name.to_string()
            }
            "run_safe_experiment" => {
                let runtime = parsed
                    .get("runtime")
                    .and_then(|v| v.as_str())
                    .unwrap_or("sandbox");
                format!("runtime:{}", runtime)
            }
            "create_artifact" => {
                let art_type = parsed
                    .get("artifact_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("artifact");
                format!("type:{}", art_type)
            }
            "invoke_skill" => {
                let skill_id = parsed
                    .get("skill_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                format!("skill:{}", skill_id)
            }
            other => other.to_string(),
        };

        Some(NormalizedStep {
            tool_name: tool_name.to_string(),
            primary_arg,
        })
    }

    /// Scans recent completed agent runs and extracts repetitive candidate workflows.
    pub fn analyze_recent_runs(&self) -> Result<Vec<HabitSuggestionRecord>, String> {
        let settings = self.db.get_habit_settings().map_err(|e| e.to_string())?;
        if !settings.enabled {
            return Ok(Vec::new());
        }

        let runs = self
            .db
            .list_runs(50)
            .map_err(|e| format!("Failed to list runs for habit analysis: {}", e))?;

        // Filter: Only successful, non-sensitive runs
        let eligible_runs: Vec<_> = runs
            .into_iter()
            .filter(|r| r.status == "succeeded" && !contains_sensitive_info(&r.prompt))
            .collect();

        if eligible_runs.len() < settings.min_occurrences {
            return Ok(Vec::new());
        }

        // Map signatures to occurrences
        // Signature string -> (NormalizedSteps, Vec<(run_id, timestamp, prompt)>)
        type SignatureEntry = (Vec<NormalizedStep>, Vec<(String, i64, String)>);
        let mut signature_groups: HashMap<String, SignatureEntry> = HashMap::new();

        for run in eligible_runs {
            let mut run_steps = Vec::new();
            if let Ok(events) = self.db.get_run_events(&run.id, None) {
                for event in events {
                    if event.event_type == "tool_call" {
                        if let Ok(val) =
                            serde_json::from_str::<serde_json::Value>(&event.payload_json)
                        {
                            let tool = val.get("name").and_then(|v| v.as_str()).unwrap_or("");
                            let args_str = val
                                .get("arguments")
                                .map(|v| v.to_string())
                                .unwrap_or_default();
                            if let Some(norm) = Self::normalize_tool_call(tool, &args_str) {
                                run_steps.push(norm);
                            }
                        }
                    }
                }
            }

            // Fallback to assistant messages with tool calls if no run events recorded
            if run_steps.is_empty() {
                if let Some(ref conv_id) = run.conversation_id {
                    if let Ok(messages) = self.db.get_messages(conv_id) {
                        for msg in messages {
                            if msg.role == "assistant" {
                                if let Some(ref tc_json) = msg.tool_calls_json {
                                    if let Ok(calls) =
                                        serde_json::from_str::<Vec<serde_json::Value>>(tc_json)
                                    {
                                        for call in calls {
                                            let tool = call
                                                .get("name")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("");
                                            let args_str = call
                                                .get("arguments")
                                                .map(|v| v.to_string())
                                                .unwrap_or_default();
                                            if let Some(norm) =
                                                Self::normalize_tool_call(tool, &args_str)
                                            {
                                                run_steps.push(norm);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if run_steps.len() >= 2 {
                let sig_repr = run_steps
                    .iter()
                    .map(|s| format!("{}({})", s.tool_name, s.primary_arg))
                    .collect::<Vec<_>>()
                    .join(" -> ");

                let sig_hash = sha256_hex(&sig_repr);
                signature_groups
                    .entry(sig_hash)
                    .or_insert_with(|| (run_steps, Vec::new()))
                    .1
                    .push((run.id.clone(), run.created_at, run.prompt.clone()));
            }
        }

        let now = chrono::Utc::now().timestamp_millis();
        let mut generated_suggestions = Vec::new();

        for (sig_hash, (steps, occurrences_data)) in signature_groups {
            let count = occurrences_data.len();
            if count >= settings.min_occurrences {
                // Confidence formula: base 0.5 + 0.1 per additional occurrence above threshold, capped at 0.95
                let confidence =
                    (0.5 + ((count - settings.min_occurrences) as f32 * 0.1)).min(0.95);

                let first_seen = occurrences_data
                    .iter()
                    .map(|(_, ts, _)| *ts)
                    .min()
                    .unwrap_or(now);
                let last_seen = occurrences_data
                    .iter()
                    .map(|(_, ts, _)| *ts)
                    .max()
                    .unwrap_or(now);

                // Generate human-friendly title and description
                let primary_tools: Vec<String> =
                    steps.iter().map(|s| s.tool_name.clone()).collect();
                let tool_summary = primary_tools.join(", ");

                let name = if tool_summary.contains("run_safe_experiment") {
                    "Automated Safe Experiment & Validation".to_string()
                } else if tool_summary.contains("read_file")
                    && tool_summary.contains("execute_shell")
                {
                    "Project Inspection and Test Suite Routine".to_string()
                } else {
                    format!(
                        "Repeated Workflow ({})",
                        steps
                            .first()
                            .map(|s| s.tool_name.as_str())
                            .unwrap_or("Task")
                    )
                };

                let description = format!(
                    "Observed {} times between {} and {}. Automated sequence: {}",
                    count,
                    chrono::DateTime::from_timestamp_millis(first_seen)
                        .map(|dt| dt.format("%Y-%m-%d").to_string())
                        .unwrap_or_default(),
                    chrono::DateTime::from_timestamp_millis(last_seen)
                        .map(|dt| dt.format("%Y-%m-%d").to_string())
                        .unwrap_or_default(),
                    steps
                        .iter()
                        .map(|s| format!("{}: {}", s.tool_name, s.primary_arg))
                        .collect::<Vec<_>>()
                        .join(" → ")
                );

                let evidence: Vec<HabitEvidence> = occurrences_data
                    .iter()
                    .take(5)
                    .map(|(rid, ts, p)| HabitEvidence {
                        run_id: rid.clone(),
                        timestamp: *ts,
                        tools_used: primary_tools.clone(),
                        summary: p.chars().take(80).collect(),
                    })
                    .collect();

                let evidence_json = serde_json::to_string(&evidence).unwrap_or_default();

                // Proposed routine prompt
                let proposed_prompt = occurrences_data
                    .first()
                    .map(|(_, _, p)| p.clone())
                    .unwrap_or_else(|| format!("Execute automated routine for {}", name));

                let suggestion_record = HabitSuggestionRecord {
                    id: format!("habit_{}", Uuid::new_v4().simple()),
                    name,
                    description,
                    signature_hash: sig_hash,
                    occurrences: count,
                    confidence,
                    proposed_action_type: "prompt".to_string(),
                    proposed_payload_json: serde_json::json!({
                        "prompt": proposed_prompt,
                        "suggested_cron": "0 9 * * 1-5" // Mon-Fri 9:00 AM placeholder
                    })
                    .to_string(),
                    status: "pending".to_string(),
                    evidence_json,
                    first_seen_at: first_seen,
                    last_seen_at: last_seen,
                    updated_at: now,
                };

                // Upsert to SQLite
                let _ = self.db.upsert_habit_suggestion(&suggestion_record);
                generated_suggestions.push(suggestion_record);
            }
        }

        Ok(generated_suggestions)
    }

    /// Accepts a suggestion and creates a routine.
    ///
    /// CRITICAL SECURITY REQUIREMENT: The resulting routine is ALWAYS created
    /// with `enabled = false`! It is NEVER scheduled or executed automatically.
    pub fn accept_suggestion(
        &self,
        suggestion_id: &str,
        custom_name: Option<String>,
        cron_expression: Option<String>,
    ) -> Result<RoutineRecord, String> {
        let suggestion = self
            .db
            .get_habit_suggestion(suggestion_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Habit suggestion '{}' not found", suggestion_id))?;

        let name = custom_name.unwrap_or(suggestion.name);
        let cron = cron_expression.unwrap_or_else(|| "0 9 * * 1-5".to_string());

        let routine_id = format!("routine_{}", Uuid::new_v4().simple());
        let routine = RoutineRecord {
            id: routine_id,
            name,
            cron_expression: cron,
            action_type: suggestion.proposed_action_type,
            target_id: suggestion.proposed_payload_json,
            enabled: false, // Explicitly disabled until user verifies and turns on
            last_run: None,
            next_run: None,
        };

        self.db
            .create_routine(&routine)
            .map_err(|e| format!("Failed to create routine from suggestion: {}", e))?;

        // Update suggestion status
        self.db
            .update_habit_suggestion_status(suggestion_id, "accepted")
            .map_err(|e| e.to_string())?;

        Ok(routine)
    }

    /// Dismisses a habit suggestion.
    pub fn dismiss_suggestion(&self, suggestion_id: &str) -> Result<bool, String> {
        self.db
            .update_habit_suggestion_status(suggestion_id, "dismissed")
            .map_err(|e| e.to_string())
    }

    /// Snoozes a habit suggestion.
    pub fn snooze_suggestion(&self, suggestion_id: &str) -> Result<bool, String> {
        self.db
            .update_habit_suggestion_status(suggestion_id, "snoozed")
            .map_err(|e| e.to_string())
    }

    /// Clears all habit learning suggestions and records.
    pub fn clear_data(&self) -> Result<(), String> {
        self.db.clear_all_habit_data().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensitive_information_filtering() {
        assert!(contains_sensitive_info("My password is 123"));
        assert!(contains_sensitive_info("Authorization: Bearer test_token"));
        assert!(contains_sensitive_info("OPENAI_API_KEY=sk-test"));
        assert!(contains_sensitive_info("read /home/user/.ssh/id_rsa"));
        assert!(contains_sensitive_info("sudo apt-get install"));

        assert!(!contains_sensitive_info("cargo test --workspace"));
        assert!(!contains_sensitive_info("npm run build"));
        assert!(!contains_sensitive_info("inspect project Kyro"));
    }

    #[test]
    fn test_normalize_tool_call() {
        let step = HabitLearningEngine::normalize_tool_call(
            "execute_shell",
            r#"{"command": "cargo test --all"}"#,
        )
        .unwrap();
        assert_eq!(step.tool_name, "execute_shell");
        assert_eq!(step.primary_arg, "cargo test");

        let sensitive_step = HabitLearningEngine::normalize_tool_call(
            "execute_shell",
            r#"{"command": "export API_KEY=secret123 && ./run"}"#,
        );
        assert!(sensitive_step.is_none());
    }
}
