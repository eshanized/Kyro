use super::bot::{BotProfile, BotStatus};
use crate::storage::Database;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

static ACTIVE_BOT_TASKS: Lazy<Mutex<HashMap<String, String>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegatedTaskRequest {
    pub target_bot_id: String,
    pub objective: String,
    pub context_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationResult {
    pub child_run_id: String,
    pub target_bot_id: String,
    pub outcome: String,
    pub is_success: bool,
}

pub struct MultiBotOrchestrator;

impl MultiBotOrchestrator {
    /// Detects if any bot is mentioned in the prompt (e.g. "@Forge fix the docker container")
    pub fn extract_mentioned_bot(prompt: &str, bots: &[BotProfile]) -> Option<BotProfile> {
        let re = Regex::new(r"@([a-zA-Z0-9_\-]+)").unwrap();
        for cap in re.captures_iter(prompt) {
            if let Some(matched) = cap.get(1) {
                let name_lower = matched.as_str().to_lowercase();
                for b in bots {
                    if b.name.to_lowercase() == name_lower || b.id.to_lowercase() == name_lower {
                        return Some(b.clone());
                    }
                }
            }
        }
        None
    }

    /// Formats the combined multi-bot group system prompt
    pub fn build_group_system_prompt(active_bot: &BotProfile, all_bots: &[BotProfile]) -> String {
        let mut teammates_list = String::new();
        for b in all_bots {
            if b.id != active_bot.id {
                teammates_list.push_str(&format!(
                    "- @{} ({}): {}\n",
                    b.name, b.role, b.system_prompt
                ));
            }
        }

        format!(
            "{}\n\n[Team Context]\nYou are currently responding as @{} ({}).\nYou are collaborating in a shared workspace with the following teammate bots:\n{}\nIf a user or task would be better handled by a teammate, or if you need their specialized expertise, you can invoke them by calling delegate_task or mentioning '@BotName'.",
            active_bot.system_prompt,
            active_bot.name,
            active_bot.role,
            teammates_list
        )
    }

    /// Update active execution status of a teammate bot
    pub fn set_bot_task(bot_id: &str, task_desc: Option<&str>) {
        let mut map = ACTIVE_BOT_TASKS.lock().unwrap();
        if let Some(task) = task_desc {
            map.insert(bot_id.to_string(), task.to_string());
        } else {
            map.remove(bot_id);
        }
    }

    /// Retrieve live roster status for all configured bots
    pub fn get_roster_statuses(db: &Database) -> Vec<BotStatus> {
        let map = ACTIVE_BOT_TASKS.lock().unwrap();
        let bots = db.list_bots().unwrap_or_default();

        bots.into_iter()
            .map(|b| {
                let current_task = map.get(&b.id).cloned();
                let state = if current_task.is_some() {
                    "busy".to_string()
                } else {
                    "idle".to_string()
                };
                BotStatus {
                    id: b.id,
                    name: b.name,
                    avatar: b.avatar,
                    role: b.role,
                    state,
                    current_task,
                }
            })
            .collect()
    }
}
