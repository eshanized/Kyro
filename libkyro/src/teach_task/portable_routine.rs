//! Editable, portable, governed, and dependable routines.
//!
//! Provides step sequencing, parameterization, secret redaction,
//! compatibility validation during import/export, and execution history.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A parameter configured on a routine with safe defaults and secret tracking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutineParameter {
    pub name: String,
    pub label: String,
    pub default_value: String,
    pub is_secret: bool,
    pub description: String,
}

/// An individual editable step within a routine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutineStep {
    pub id: String,
    pub title: String,
    pub action_type: String, // "tool_call", "prompt", "skill"
    pub tool_name: Option<String>,
    pub parameters: serde_json::Value,
    pub expected_output: Option<String>,
    pub timeout_ms: u64,
    pub on_error: String, // "stop", "retry", "continue"
    pub requires_approval: bool,
}

/// A complete portable, versioned routine document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditableRoutine {
    pub version: String, // "kyro.routine.v1"
    pub id: String,
    pub name: String,
    pub description: String,
    pub cron_expression: Option<String>,
    pub enabled: bool,
    pub parameters: Vec<RoutineParameter>,
    pub steps: Vec<RoutineStep>,
    pub created_at: i64,
    pub updated_at: i64,
    pub trusted: bool, // Imported routines remain untrusted until reviewed
}

/// Record of a completed or failed execution of a routine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineRunRecord {
    pub run_id: String,
    pub routine_id: String,
    pub routine_name: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub status: String, // "succeeded", "failed", "cancelled", "running"
    pub steps_completed: usize,
    pub total_steps: usize,
    pub summary_output: Option<String>,
    pub error: Option<String>,
}

pub struct RoutineEngine;

impl RoutineEngine {
    /// Serializes and exports a routine, redacting any configured secret values.
    pub fn export_routine(routine: &EditableRoutine) -> Result<String, String> {
        let mut export_copy = routine.clone();

        // 1. Redact secrets in parameters
        for param in &mut export_copy.parameters {
            if param.is_secret {
                param.default_value = "[REDACTED_SECRET]".to_string();
            }
        }

        // 2. Scan step parameter payloads for potential hardcoded secret strings
        for step in &mut export_copy.steps {
            if let Some(obj) = step.parameters.as_object_mut() {
                for (key, val) in obj.iter_mut() {
                    let k_lower = key.to_lowercase();
                    if k_lower.contains("token")
                        || k_lower.contains("secret")
                        || k_lower.contains("password")
                        || k_lower.contains("api_key")
                    {
                        *val = serde_json::Value::String("[REDACTED_SECRET]".to_string());
                    }
                }
            }
        }

        serde_json::to_string_pretty(&export_copy)
            .map_err(|e| format!("Failed to export routine to JSON: {}", e))
    }

    /// Validates and imports a routine from a JSON string, ensuring tool compatibility.
    pub fn import_routine(
        json_str: &str,
        available_tools: &[String],
    ) -> Result<EditableRoutine, String> {
        let mut routine: EditableRoutine = serde_json::from_str(json_str).map_err(|e| {
            format!(
                "Invalid routine format. Expected valid kyro.routine.v1 JSON: {}",
                e
            )
        })?;

        if routine.version != "kyro.routine.v1" {
            return Err(format!(
                "Unsupported routine version '{}'. Only 'kyro.routine.v1' is supported.",
                routine.version
            ));
        }

        if routine.name.trim().is_empty() {
            return Err("Routine name cannot be empty.".to_string());
        }

        let tools_set: HashSet<&str> = available_tools.iter().map(|s| s.as_str()).collect();
        let mut missing_tools = Vec::new();

        // Compatibility validation: ensure all referenced tools exist
        for step in &routine.steps {
            if step.action_type == "tool_call" {
                if let Some(ref t_name) = step.tool_name {
                    if !tools_set.contains(t_name.as_str()) {
                        missing_tools.push(t_name.clone());
                    }
                } else {
                    return Err(format!(
                        "Step '{}' specifies action_type 'tool_call' but has no tool_name.",
                        step.title
                    ));
                }
            }
        }

        if !missing_tools.is_empty() {
            missing_tools.sort();
            missing_tools.dedup();
            return Err(format!(
                "Import compatibility error: Routine requires unavailable tools: {:?}",
                missing_tools
            ));
        }

        // Security guarantee: imported routines are NOT trusted by default,
        // are disabled initially, and receive a fresh unique ID.
        routine.id = format!("rtn_{}", uuid::Uuid::new_v4());
        routine.trusted = false;
        routine.enabled = false;
        routine.updated_at = chrono::Utc::now().timestamp_millis();

        Ok(routine)
    }

    /// Resolves step parameters by substituting user-provided argument values.
    pub fn resolve_step_parameters(
        step: &RoutineStep,
        provided_args: &std::collections::HashMap<String, String>,
    ) -> serde_json::Value {
        let mut val = step.parameters.clone();
        if let Some(obj) = val.as_object_mut() {
            for (_, v) in obj.iter_mut() {
                if let Some(s) = v.as_str() {
                    let mut rendered = s.to_string();
                    for (arg_key, arg_val) in provided_args {
                        let placeholder = format!("{{{{{}}}}}", arg_key);
                        rendered = rendered.replace(&placeholder, arg_val);
                    }
                    *v = serde_json::Value::String(rendered);
                }
            }
        }
        val
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routine_export_secret_redaction() {
        let routine = EditableRoutine {
            version: "kyro.routine.v1".to_string(),
            id: "rtn_123".to_string(),
            name: "Deploy Service".to_string(),
            description: "Automated deployment".to_string(),
            cron_expression: Some("0 0 * * *".to_string()),
            enabled: true,
            parameters: vec![
                RoutineParameter {
                    name: "api_key".to_string(),
                    label: "API Key".to_string(),
                    default_value: "sk-super-secret-123".to_string(),
                    is_secret: true,
                    description: "Secret token".to_string(),
                },
                RoutineParameter {
                    name: "environment".to_string(),
                    label: "Environment".to_string(),
                    default_value: "production".to_string(),
                    is_secret: false,
                    description: "Target environment".to_string(),
                },
            ],
            steps: vec![RoutineStep {
                id: "step_1".to_string(),
                title: "Run curl".to_string(),
                action_type: "tool_call".to_string(),
                tool_name: Some("execute_shell".to_string()),
                parameters: serde_json::json!({
                    "command": "curl -H 'Authorization: Bearer secret'",
                    "api_key": "raw_sensitive_key"
                }),
                expected_output: None,
                timeout_ms: 5000,
                on_error: "stop".to_string(),
                requires_approval: true,
            }],
            created_at: 1000,
            updated_at: 1000,
            trusted: true,
        };

        let exported = RoutineEngine::export_routine(&routine).expect("Export succeeds");
        assert!(!exported.contains("sk-super-secret-123"));
        assert!(!exported.contains("raw_sensitive_key"));
        assert!(exported.contains("[REDACTED_SECRET]"));
        assert!(exported.contains("production"));
    }

    #[test]
    fn test_routine_import_validation_and_untrusted_flag() {
        let valid_json = r#"{
            "version": "kyro.routine.v1",
            "id": "old_id",
            "name": "Audit and Report",
            "description": "Daily audit routine",
            "cron_expression": "0 9 * * 1-5",
            "enabled": true,
            "parameters": [],
            "steps": [
                {
                    "id": "s1",
                    "title": "Run UI audit",
                    "action_type": "tool_call",
                    "tool_name": "audit_ui",
                    "parameters": {"project_name": "Kyro"},
                    "expected_output": null,
                    "timeout_ms": 10000,
                    "on_error": "stop",
                    "requires_approval": true
                }
            ],
            "created_at": 1000,
            "updated_at": 1000,
            "trusted": true
        }"#;

        let available = vec!["audit_ui".to_string(), "read_file".to_string()];
        let imported =
            RoutineEngine::import_routine(valid_json, &available).expect("Import succeeds");

        // Verification of security guarantees:
        assert_ne!(imported.id, "old_id"); // Fresh ID
        assert!(!imported.trusted); // Must NOT be trusted automatically
        assert!(!imported.enabled); // Must be disabled until reviewed
        assert_eq!(imported.name, "Audit and Report");
        assert_eq!(imported.steps.len(), 1);

        // Missing tool must be rejected
        let unavailable = vec!["read_file".to_string()]; // lacks audit_ui
        let err_import = RoutineEngine::import_routine(valid_json, &unavailable);
        assert!(err_import.is_err());
        assert!(err_import
            .unwrap_err()
            .contains("requires unavailable tools"));
    }
}
