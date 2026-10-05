use crate::storage::Database;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernancePolicy {
    pub shell_mode: String,        // "SafeAutoApprove", "AskAlways", "SandboxOnly"
    pub computer_use_mode: String, // "FullAutonomous", "ConfirmDestructive", "AskAlways"
    pub require_takeover_for_credentials: bool,
    pub blocked_domains: Vec<String>,
}

impl Default for GovernancePolicy {
    fn default() -> Self {
        Self {
            shell_mode: "SafeAutoApprove".to_string(),
            computer_use_mode: "FullAutonomous".to_string(),
            require_takeover_for_credentials: true,
            blocked_domains: vec![
                "chase.com".into(),
                "bankofamerica.com".into(),
                "wellsfargo.com".into(),
                "paypal.com".into(),
            ],
        }
    }
}

pub struct PolicyEngine;

impl PolicyEngine {
    pub fn load(db: &Database) -> GovernancePolicy {
        if let Ok(Some(json)) = db.get_setting("governance_policy") {
            if let Ok(policy) = serde_json::from_str::<GovernancePolicy>(&json) {
                return policy;
            }
        }
        GovernancePolicy::default()
    }

    pub fn save(db: &Database, policy: &GovernancePolicy) -> Result<(), String> {
        let json = serde_json::to_string(policy).map_err(|e| e.to_string())?;
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
