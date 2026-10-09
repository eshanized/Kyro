use super::replay::SkillReplayer;
use super::skill::Skill;
use crate::agent::cancellation::CancellationToken;
use crate::agent::loop_engine::AgentLoopEngine;
use crate::agent::router::ChatMessage;
use crate::mcp::McpManager;
use crate::storage::{AgentRun, Database, RoutineRecord, Vault};
use cron::Schedule;
use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;

static SCHEDULER_RUNNING: AtomicBool = AtomicBool::new(false);
static ACTIVE_ROUTINE_EXECUTIONS: Lazy<Mutex<HashSet<String>>> =
    Lazy::new(|| Mutex::new(HashSet::new()));

pub struct RoutineScheduler;

impl RoutineScheduler {
    /// Check whether a routine is currently executing
    pub fn is_routine_running(id: &str) -> bool {
        let active = ACTIVE_ROUTINE_EXECUTIONS.lock().unwrap();
        active.contains(id)
    }

    /// Mark a routine as actively executing (used for testing lifecycle rules)
    pub fn set_routine_running_for_test(id: &str, running: bool) {
        let mut active = ACTIVE_ROUTINE_EXECUTIONS.lock().unwrap();
        if running {
            active.insert(id.to_string());
        } else {
            active.remove(id);
        }
    }

    /// Validates and parses a cron expression, supporting standard 5-field and extended 6/7-field formats.
    /// Also strips out timezone prefixes like `CRON_TZ=...` or `TZ=...` if present.
    pub fn parse_schedule(expr: &str) -> Result<Schedule, String> {
        let (clean_expr, _) = Self::extract_timezone(expr);
        let trimmed = clean_expr.trim();
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();

        let normalized = match tokens.len() {
            5 => format!("0 {} *", trimmed), // 5-field (min hour dom month dow) -> prepend sec (0), append year (*)
            6 => format!("0 {}", trimmed),
            7 => trimmed.to_string(),
            _ => {
                return Err(format!(
                    "Invalid cron expression '{}': expected 5 to 7 fields",
                    expr
                ))
            }
        };

        Schedule::from_str(&normalized)
            .map_err(|e| format!("Cron parse error for '{}': {}", expr, e))
    }

    /// Extracts timezone specification from expression if formatted as `CRON_TZ=America/New_York ...` or `TZ=...`
    pub fn extract_timezone(expr: &str) -> (String, Option<String>) {
        let trimmed = expr.trim();
        if let Some(rest) = trimmed.strip_prefix("CRON_TZ=") {
            let parts: Vec<&str> = rest.splitn(2, ' ').collect();
            if parts.len() == 2 {
                return (parts[1].to_string(), Some(parts[0].to_string()));
            }
        } else if let Some(rest) = trimmed.strip_prefix("TZ=") {
            let parts: Vec<&str> = rest.splitn(2, ' ').collect();
            if parts.len() == 2 {
                return (parts[1].to_string(), Some(parts[0].to_string()));
            }
        }
        (trimmed.to_string(), None)
    }

    /// Computes the next execution timestamp (millis since epoch) for a cron expression
    /// with explicit timezone support and daylight saving time transition handling.
    pub fn compute_next_run_with_tz(expr: &str, tz_override: Option<&str>) -> Option<i64> {
        let (clean_expr, inline_tz) = Self::extract_timezone(expr);
        let tz_name = tz_override.or(inline_tz.as_deref()).unwrap_or("UTC");

        let tz: chrono_tz::Tz = tz_name.parse().unwrap_or(chrono_tz::UTC);
        let sched = Self::parse_schedule(&clean_expr).ok()?;
        let now_in_tz = chrono::Utc::now().with_timezone(&tz);

        sched
            .after(&now_in_tz)
            .next()
            .map(|dt| dt.timestamp_millis())
    }

    /// Default timezone calculation
    pub fn compute_next_run(expr: &str) -> Option<i64> {
        Self::compute_next_run_with_tz(expr, None)
    }

    pub fn start_worker(
        db: Database,
        vault: Vault,
        engine: Arc<AgentLoopEngine>,
        mcp: Option<McpManager>,
    ) {
        if SCHEDULER_RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }

        tauri::async_runtime::spawn(async move {
            log::info!("[Scheduler] Routine worker started with timezone engine");

            loop {
                sleep(Duration::from_secs(5)).await;

                let routines = match db.list_routines() {
                    Ok(r) => r,
                    Err(e) => {
                        log::error!("[Scheduler] Error fetching routines: {}", e);
                        continue;
                    }
                };

                let now = chrono::Utc::now().timestamp_millis();

                for r in routines {
                    if !r.enabled {
                        continue;
                    }

                    // Check if already executing (prevent concurrency overlap)
                    {
                        let active = ACTIVE_ROUTINE_EXECUTIONS.lock().unwrap();
                        if active.contains(&r.id) {
                            continue;
                        }
                    }

                    // Determine if routine is due
                    let is_due = match r.next_run {
                        Some(next) => now >= next,
                        None => match r.last_run {
                            None => true,
                            Some(last) => {
                                if let Ok(sched) = Self::parse_schedule(&r.cron_expression) {
                                    let dt = chrono::DateTime::from_timestamp_millis(last)
                                        .unwrap_or_default();
                                    if let Some(next) = sched.after(&dt).next() {
                                        now >= next.timestamp_millis()
                                    } else {
                                        false
                                    }
                                } else {
                                    (now - last) >= 10 * 60 * 1000
                                }
                            }
                        },
                    };

                    if is_due {
                        // Missed run policy: if missed by > 24 hours (e.g. system was suspended/off),
                        // advance next_run directly without executing stale duplicate backlogs.
                        let is_very_stale = match r.next_run {
                            Some(next) => (now - next) > 24 * 60 * 60 * 1000,
                            None => false,
                        };

                        let next_run = Self::compute_next_run_with_tz(&r.cron_expression, None);
                        let _ = db.update_routine_execution(&r.id, now, next_run);

                        if is_very_stale {
                            log::info!(
                                "[Scheduler] Routine '{}' missed run by >24h; skipped backlog and scheduled next occurrence",
                                r.name
                            );
                            continue;
                        }

                        // Mark active to enforce overlap prevention
                        {
                            let mut active = ACTIVE_ROUTINE_EXECUTIONS.lock().unwrap();
                            active.insert(r.id.clone());
                        }

                        log::info!(
                            "[Scheduler] Dispatching due routine: {} ({})",
                            r.name,
                            r.action_type
                        );

                        let db_clone = db.clone();
                        let vault_clone = vault.clone();
                        let engine_clone = engine.clone();
                        let mcp_clone = mcp.clone();
                        let routine_clone = r.clone();

                        tauri::async_runtime::spawn(async move {
                            Self::execute_routine(
                                routine_clone,
                                db_clone,
                                vault_clone,
                                engine_clone,
                                mcp_clone,
                            )
                            .await;
                        });
                    }
                }
            }
        });
    }

    async fn execute_routine(
        routine: RoutineRecord,
        db: Database,
        vault: Vault,
        engine: Arc<AgentLoopEngine>,
        mcp: Option<McpManager>,
    ) {
        let routine_id = routine.id.clone();
        struct Guard(String);
        impl Drop for Guard {
            fn drop(&mut self) {
                let mut active = ACTIVE_ROUTINE_EXECUTIONS.lock().unwrap();
                active.remove(&self.0);
            }
        }
        let _guard = Guard(routine_id);

        let run_id = format!("run_sched_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().timestamp_millis();

        let run = AgentRun {
            id: run_id.clone(),
            conversation_id: None,
            agent_id: "scheduled-worker".to_string(),
            model: "nvidia/nemotron-3-ultra-550b-a55b".to_string(),
            prompt: format!(
                "Scheduled routine: {} ({})",
                routine.name, routine.action_type
            ),
            status: "running".to_string(),
            created_at: now,
            started_at: Some(now),
            completed_at: None,
            error: None,
            config_json: Some(
                serde_json::json!({
                    "routine_id": routine.id,
                    "cron_expression": routine.cron_expression,
                    "action_type": routine.action_type,
                })
                .to_string(),
            ),
            parent_run_id: None,
        };

        if let Err(e) = db.create_run(&run) {
            log::error!("[Scheduler] Failed to persist run for routine: {}", e);
            return;
        }

        let cancel_token = CancellationToken::new();

        if routine.action_type == "skill" {
            match db.get_skill(&routine.target_id) {
                Ok(Some(skill_rec)) => {
                    match serde_json::from_str::<Vec<super::skill::SkillStep>>(
                        &skill_rec.steps_json,
                    ) {
                        Ok(steps) => {
                            let skill = Skill {
                                id: skill_rec.id,
                                name: skill_rec.name,
                                description: skill_rec.description,
                                steps,
                                created_at: skill_rec.created_at,
                            };
                            match SkillReplayer::execute(
                                &skill,
                                Some(&db),
                                Some(&cancel_token),
                                |step, total, desc| {
                                    log::debug!("[Scheduler Skill] {}/{}: {}", step, total, desc);
                                },
                            )
                            .await
                            {
                                Ok(()) => {
                                    let comp = chrono::Utc::now().timestamp_millis();
                                    let _ = db.update_run_status(
                                        &run_id,
                                        "succeeded",
                                        None,
                                        Some(comp),
                                        None,
                                    );
                                }
                                Err(e) => {
                                    let comp = chrono::Utc::now().timestamp_millis();
                                    let _ = db.update_run_status(
                                        &run_id,
                                        "failed",
                                        None,
                                        Some(comp),
                                        Some(&e),
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            let comp = chrono::Utc::now().timestamp_millis();
                            let err_str = format!("Failed to parse skill steps: {}", e);
                            let _ = db.update_run_status(
                                &run_id,
                                "failed",
                                None,
                                Some(comp),
                                Some(&err_str),
                            );
                        }
                    }
                }
                Ok(None) => {
                    let comp = chrono::Utc::now().timestamp_millis();
                    let err_str = format!("Target skill '{}' not found", routine.target_id);
                    let _ =
                        db.update_run_status(&run_id, "failed", None, Some(comp), Some(&err_str));
                }
                Err(e) => {
                    let comp = chrono::Utc::now().timestamp_millis();
                    let _ = db.update_run_status(
                        &run_id,
                        "failed",
                        None,
                        Some(comp),
                        Some(&e.to_string()),
                    );
                }
            }
        } else if routine.action_type == "prompt" {
            let api_key = match vault.get_all_keys() {
                Ok(keys) => keys.nvidia.unwrap_or_default(),
                Err(_) => String::new(),
            };

            if api_key.is_empty() {
                let comp = chrono::Utc::now().timestamp_millis();
                let _ = db.update_run_status(
                    &run_id,
                    "failed",
                    None,
                    Some(comp),
                    Some("NVIDIA API key not configured for scheduled prompt routine"),
                );
                return;
            }

            let history = vec![ChatMessage {
                role: "user".to_string(),
                content: crate::agent::router::MessageContent::from_text(&routine.target_id),
                tool_calls: None,
                tool_call_id: None,
            }];

            let sys_prompt = "You are Kyro executing a scheduled autonomous routine. Complete the objective deterministically.";

            match engine
                .run_step(
                    &run_id,
                    "nvidia/nemotron-3-ultra-550b-a55b",
                    &api_key,
                    None,
                    sys_prompt,
                    history,
                    db.clone(),
                    mcp,
                    Some(cancel_token),
                    |_: crate::agent::AgentStepEvent| {},
                )
                .await
            {
                Ok(_) => {
                    let comp = chrono::Utc::now().timestamp_millis();
                    let _ = db.update_run_status(&run_id, "succeeded", None, Some(comp), None);
                }
                Err(e) => {
                    let comp = chrono::Utc::now().timestamp_millis();
                    let _ = db.update_run_status(&run_id, "failed", None, Some(comp), Some(&e));
                }
            }
        }
    }
}
