use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    Safe,
    Moderate,
    Destructive,
}

impl RiskLevel {
    pub fn is_at_most(&self, ceiling: RiskLevel) -> bool {
        *self <= ceiling
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionDecision {
    pub allowed: bool,
    pub reason: String,
    pub requires_prompt: bool,
    pub risk: RiskLevel,
}

pub struct PermissionGate;

impl PermissionGate {
    pub fn assess_command(cmd: &str, auto_approve_safe: bool) -> PermissionDecision {
        let trimmed = cmd.trim();
        let lower = trimmed.to_lowercase();

        // Destructive command checks
        let destructive_patterns = [
            "rm -rf",
            "rm -r /",
            "mkfs",
            "dd if=",
            ":(){ :|:& };:",
            "> /dev/sd",
            "chmod -r 777 /",
            "shutdown",
            "reboot",
            "init 0",
        ];

        for pattern in &destructive_patterns {
            if lower.contains(pattern) {
                return PermissionDecision {
                    allowed: false,
                    reason: format!(
                        "Command contains prohibited/destructive pattern '{}'",
                        pattern
                    ),
                    requires_prompt: true,
                    risk: RiskLevel::Destructive,
                };
            }
        }

        // Moderate command checks (file modification, git pushes, package installs)
        let moderate_patterns = [
            "git push",
            "git reset --hard",
            "npm install",
            "pip install",
            "cargo install",
            "sudo",
            "systemctl",
            "kill",
            "pkill",
        ];

        for pattern in &moderate_patterns {
            if lower.contains(pattern) {
                return PermissionDecision {
                    allowed: false,
                    reason: format!("Command '{}' requires user confirmation", pattern),
                    requires_prompt: true,
                    risk: RiskLevel::Moderate,
                };
            }
        }

        // Safe command checks (reading, inspecting, directory traversal)
        if auto_approve_safe {
            PermissionDecision {
                allowed: true,
                reason: "Auto-approved read/inspection command".to_string(),
                requires_prompt: false,
                risk: RiskLevel::Safe,
            }
        } else {
            PermissionDecision {
                allowed: false,
                reason: "User confirmation requested by policy".to_string(),
                requires_prompt: true,
                risk: RiskLevel::Safe,
            }
        }
    }
}
