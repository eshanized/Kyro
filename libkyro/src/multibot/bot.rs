use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotProfile {
    pub id: String,
    pub name: String,
    pub avatar: String,
    pub role: String,
    pub system_prompt: String,
    pub is_default: bool,
}

/// One live task a teammate is executing, keyed by the run that owns it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BotTaskStatus {
    /// The run whose delegation produced this task.
    pub run_id: String,
    /// The delegated objective.
    pub objective: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotStatus {
    pub id: String,
    pub name: String,
    pub avatar: String,
    pub role: String,
    pub state: String, // "idle", "busy", "delegating"
    /// First active objective, kept for consumers of the legacy payload.
    pub current_task: Option<String>,
    /// Every active objective, keyed by the run that owns it. A teammate may
    /// serve several delegations at once.
    #[serde(default)]
    pub active_tasks: Vec<BotTaskStatus>,
}
