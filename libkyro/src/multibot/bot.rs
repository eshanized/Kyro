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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotStatus {
    pub id: String,
    pub name: String,
    pub avatar: String,
    pub role: String,
    pub state: String, // "idle", "busy", "delegating"
    pub current_task: Option<String>,
}
