use crate::storage::Database;
use serde::{Deserialize, Serialize};

pub const CURRENT_POLICY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernancePolicy {
    #[serde(default = "default_policy_version")]
    pub version: u32,
    pub shell_mode: String, // "SafeAutoApprove", "AskAlways", "SandboxOnly"
    pub computer_use_mode: String, // "ConfirmDestructive", "AskAlways", "FullAutonomous"
    pub require_takeover_for_credentials: bool,
    pub blocked_domains: Vec<String>,
    #[serde(default = "default_task_autonomy_mode")]
    pub task_autonomy_mode: String, // "Autonomous", "Supervised"
}

fn default_policy_version() -> u32 {
    CURRENT_POLICY_SCHEMA_VERSION
}

fn default_task_autonomy_mode() -> String {
    "Autonomous".to_string()
}

impl Default for GovernancePolicy {
    fn default() -> Self {
        Self {
            version: CURRENT_POLICY_SCHEMA_VERSION,
            shell_mode: "SafeAutoApprove".to_string(),
            computer_use_mode: "ConfirmDestructive".to_string(), // Fail-closed, restrictive safe default
            require_takeover_for_credentials: true,
            blocked_domains: vec![
                "chase.com".into(),
                "bankofamerica.com".into(),
                "wellsfargo.com".into(),
                "paypal.com".into(),
            ],
            task_autonomy_mode: default_task_autonomy_mode(),
        }
    }
}

impl GovernancePolicy {
    /// Validates policy configuration fields, failing closed to restrictive,
    /// safe permissions whenever an unknown, malformed, or corrupt value is found.
    pub fn validate_and_sanitize(mut self) -> Self {
        // Enforce valid schema version
        if self.version == 0 {
            self.version = CURRENT_POLICY_SCHEMA_VERSION;
        }

        // Fail-closed validation for shell_mode
        if !matches!(
            self.shell_mode.as_str(),
            "SafeAutoApprove" | "AskAlways" | "SandboxOnly"
        ) {
            log::warn!(
                "[Security] Unknown or malformed shell_mode '{}'; failing closed to 'AskAlways'",
                self.shell_mode
            );
            self.shell_mode = "AskAlways".to_string();
        }

        // Fail-closed validation for computer_use_mode
        if !matches!(
            self.computer_use_mode.as_str(),
            "ConfirmDestructive" | "AskAlways" | "FullAutonomous"
        ) {
            log::warn!(
                "[Security] Unknown or malformed computer_use_mode '{}'; failing closed to 'ConfirmDestructive'",
                self.computer_use_mode
            );
            self.computer_use_mode = "ConfirmDestructive".to_string();
        }

        // Fail-closed validation for task_autonomy_mode
        if !matches!(
            self.task_autonomy_mode.as_str(),
            "Autonomous" | "Supervised"
        ) {
            log::warn!(
                "[Security] Unknown or malformed task_autonomy_mode '{}'; failing closed to 'Supervised'",
                self.task_autonomy_mode
            );
            self.task_autonomy_mode = "Supervised".to_string();
        }

        self
    }
}

pub struct PolicyEngine;

impl PolicyEngine {
    pub fn load(db: &Database) -> GovernancePolicy {
        if let Ok(Some(json)) = db.get_setting("governance_policy") {
            // Validate JSON at the boundary: malformed JSON must fail closed
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json) {
                if let Ok(mut policy) = serde_json::from_value::<GovernancePolicy>(val.clone()) {
                    // Backward-compatible migration for existing settings:
                    // If stored settings had no version (v0), preserve deliberate choices
                    // while ensuring schema is upgraded to version 1.
                    let raw_version = val.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
                    if raw_version == 0 {
                        policy.version = CURRENT_POLICY_SCHEMA_VERSION;
                        // Persist the migrated configuration
                        let _ = Self::save(db, &policy);
                    }
                    return policy.validate_and_sanitize();
                } else {
                    log::warn!(
                        "[Security] Corrupted governance policy JSON in database; failing closed to safe default"
                    );
                }
            } else {
                log::warn!(
                    "[Security] Unparseable governance policy setting in database; failing closed to safe default"
                );
            }
        }
        GovernancePolicy::default()
    }

    pub fn save(db: &Database, policy: &GovernancePolicy) -> Result<(), String> {
        let sanitized = policy.clone().validate_and_sanitize();
        let json = serde_json::to_string(&sanitized).map_err(|e| e.to_string())?;
        db.set_setting("governance_policy", &json)
            .map_err(|e| e.to_string())
    }

    pub fn is_domain_blocked(policy: &GovernancePolicy, url_or_domain: &str) -> bool {
        let lower = url_or_domain.to_lowercase();
        for blocked in &policy.blocked_domains {
            if lower.contains(&blocked.to_lowercase()) {
                return true;
            }
        }
        false
    }
}
