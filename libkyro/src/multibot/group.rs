use super::bot::{BotProfile, BotStatus, BotTaskStatus};
use crate::agent::registry::ProfileSnapshot;
use crate::storage::Database;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

/// Live per-robot task map: bot/profile id → (owning run id → objective).
/// A teammate can serve several delegations at once; completion clears the
/// exact (bot, run) pair rather than the whole bot slot.
static ACTIVE_BOT_TASKS: Lazy<Mutex<HashMap<String, HashMap<String, String>>>> =
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
    /// Detects if any bot is mentioned in the prompt (e.g. "@Forge fix the docker container").
    ///
    /// Compat helper used by tests; production runs resolve identities through
    /// [`AgentRegistry::resolve`], which also understands aliases.
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

    /// Record or clear the task a teammate bot is executing for one run.
    /// `None` clears exactly the (bot, run) pair, leaving tasks owned by other
    /// runs untouched.
    pub fn set_bot_task(bot_id: &str, run_id: &str, task_desc: Option<&str>) {
        let mut map = ACTIVE_BOT_TASKS.lock().unwrap();
        let now_empty = {
            let entry = map.entry(bot_id.to_string()).or_default();
            match task_desc {
                Some(task) => {
                    entry.insert(run_id.to_string(), task.to_string());
                    false
                }
                None => {
                    entry.remove(run_id);
                    entry.is_empty()
                }
            }
        };
        if now_empty {
            map.remove(bot_id);
        }
    }

    /// Drop every task owned by a run (used on completion, failure or cancel).
    pub fn clear_run_tasks(run_id: &str) {
        let mut map = ACTIVE_BOT_TASKS.lock().unwrap();
        let mut emptied: Vec<String> = Vec::new();
        for (bot_id, tasks) in map.iter_mut() {
            tasks.remove(run_id);
            if tasks.is_empty() {
                emptied.push(bot_id.clone());
            }
        }
        for bot_id in emptied {
            map.remove(&bot_id);
        }
    }

    fn tasks_for(bot_id: &str) -> Vec<BotTaskStatus> {
        let map = ACTIVE_BOT_TASKS.lock().unwrap();
        match map.get(bot_id) {
            Some(tasks) => {
                let mut v: Vec<BotTaskStatus> = tasks
                    .iter()
                    .map(|(run_id, objective)| BotTaskStatus {
                        run_id: run_id.clone(),
                        objective: objective.clone(),
                    })
                    .collect();
                v.sort_by(|a, b| a.run_id.cmp(&b.run_id));
                v
            }
            None => Vec::new(),
        }
    }

    /// Retrieve live roster status for the seeded compat bots (bots table).
    /// Prefer [`MultiBotOrchestrator::get_roster_statuses_for`] + the agent
    /// registry, which is the canonical identity source.
    pub fn get_roster_statuses(db: &Database) -> Vec<BotStatus> {
        let bots = db.list_bots().unwrap_or_default();
        bots.into_iter()
            .map(|b| {
                let active = Self::tasks_for(&b.id);
                let state = if active.is_empty() {
                    "idle".to_string()
                } else {
                    "busy".to_string()
                };
                BotStatus {
                    id: b.id,
                    name: b.name,
                    avatar: b.avatar,
                    role: b.role,
                    state,
                    current_task: active.first().map(|t| t.objective.clone()),
                    active_tasks: active,
                }
            })
            .collect()
    }

    /// Live roster status for the validated profiles of the agent registry.
    /// Ids follow the manifest ids, so the roster always matches what the
    /// backend can actually run.
    pub fn get_roster_statuses_for(profiles: &[ProfileSnapshot]) -> Vec<BotStatus> {
        profiles
            .iter()
            .map(|p| {
                let active = Self::tasks_for(&p.id);
                let state = if active.is_empty() {
                    "idle".to_string()
                } else {
                    "busy".to_string()
                };
                BotStatus {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    avatar: p.avatar.clone(),
                    role: p.role.clone(),
                    state,
                    current_task: active.first().map(|t| t.objective.clone()),
                    active_tasks: active,
                }
            })
            .collect()
    }
}
