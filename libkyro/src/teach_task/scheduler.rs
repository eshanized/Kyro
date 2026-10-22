use super::replay::SkillReplayer;
use super::skill::Skill;
use crate::agent::cancellation::CancellationToken;
use crate::agent::loop_engine::AgentLoopEngine;
use crate::agent::router::{ChatMessage, MessageContent, ModelRouter, DEFAULT_NEMOTRON_MODEL};
use crate::connector::ConnectorManager;
use crate::mcp::McpManager;
use crate::storage::{AgentRun, Database, RoutineRecord, RoutineRunRecord, Vault};
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

    /// Access active routines lock for synchronized lifecycle checks
    pub fn active_routines_lock() -> std::sync::MutexGuard<'static, HashSet<String>> {
        ACTIVE_ROUTINE_EXECUTIONS.lock().unwrap()
    }

    /// Synchronized deletion preventing race conditions with scheduler loop
    pub fn delete_routine_synchronized(db: &Database, id: &str) -> Result<bool, String> {
        let active = ACTIVE_ROUTINE_EXECUTIONS.lock().unwrap();
        if active.contains(id) {
            return Err(format!(
                "Cannot delete routine '{}': routine is currently executing. Please wait for execution to complete.",
                id
            ));
        }
        db.delete_routine(id).map_err(|e| e.to_string())
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
        connector_manager: Option<Arc<ConnectorManager>>,
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

                        // Stale snapshot guard: verify routine definition still exists before dispatching
                        match db.get_routine(&r.id) {
                            Ok(Some(active_def)) => {
                                if !active_def.enabled {
                                    continue;
                                }
                            }
                            Ok(None) => {
                                log::info!(
                                    "[Scheduler] Routine '{}' was deleted; skipping from stale list snapshot",
                                    r.id
                                );
                                continue;
                            }
                            Err(e) => {
                                log::error!("[Scheduler] Error verifying routine: {}", e);
                                continue;
                            }
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
                        let connector_clone = connector_manager.clone();
                        let routine_clone = r.clone();

                        tauri::async_runtime::spawn(async move {
                            let _ = Self::execute_routine_unified(
                                routine_clone,
                                db_clone,
                                vault_clone,
                                engine_clone,
                                mcp_clone,
                                connector_clone,
                                false,
                            )
                            .await;
                        });
                    }
                }
            }
        });
    }

    /// Shared execution service used by manual Run Now (Today & My Routines) and scheduled routine runs.
    pub async fn execute_routine_unified(
        routine: RoutineRecord,
        db: Database,
        vault: Vault,
        engine: Arc<AgentLoopEngine>,
        mcp: Option<McpManager>,
        connector_manager: Option<Arc<ConnectorManager>>,
        is_manual: bool,
    ) -> Result<RoutineRunRecord, String> {
        // Enforce concurrency limit: only one active execution per routine at a time
        {
            let mut active = ACTIVE_ROUTINE_EXECUTIONS.lock().unwrap();
            if active.contains(&routine.id) {
                return Err(format!(
                    "Routine '{}' is currently executing. Concurrent executions of the same routine are prevented.",
                    routine.name
                ));
            }
            active.insert(routine.id.clone());
        }

        struct Guard(String);
        impl Drop for Guard {
            fn drop(&mut self) {
                let mut active = ACTIVE_ROUTINE_EXECUTIONS.lock().unwrap();
                active.remove(&self.0);
            }
        }
        let _guard = Guard(routine.id.clone());

        // Ensure routine definition still exists and is eligible to run
        let latest_routine = match db.get_routine(&routine.id) {
            Ok(Some(def)) => def,
            Ok(None) => {
                return Err(format!(
                    "Routine '{}' ({}) was deleted or does not exist.",
                    routine.name, routine.id
                ));
            }
            Err(e) => {
                return Err(format!("Database error reading routine: {}", e));
            }
        };

        if !latest_routine.enabled && !is_manual {
            return Err(format!(
                "Routine '{}' is disabled; skipping scheduled execution.",
                latest_routine.name
            ));
        }

        let run_id = format!(
            "run_{}_{}",
            if is_manual { "manual" } else { "sched" },
            uuid::Uuid::new_v4()
        );
        let start_time = chrono::Utc::now().timestamp_millis();

        // 1. Persist initial running state in routine_runs
        let initial_record = RoutineRunRecord {
            id: run_id.clone(),
            routine_id: latest_routine.id.clone(),
            routine_name: latest_routine.name.clone(),
            status: "running".to_string(),
            started_at: start_time,
            completed_at: None,
            summary: Some(format!("Executing routine '{}'...", latest_routine.name)),
            error: None,
        };

        if let Err(e) = db.record_routine_run(&initial_record) {
            log::error!("[Scheduler] Failed to record initial routine run: {}", e);
            return Err(format!("Failed to record routine run: {}", e));
        }

        // 2. Persist in agent_runs for unified traceability
        let agent_run = AgentRun {
            id: run_id.clone(),
            conversation_id: None,
            agent_id: if is_manual {
                "manual-routine-worker".to_string()
            } else {
                "scheduled-routine-worker".to_string()
            },
            model: "configured".to_string(),
            prompt: format!(
                "Routine: {} ({})",
                latest_routine.name, latest_routine.action_type
            ),
            status: "running".to_string(),
            created_at: start_time,
            started_at: Some(start_time),
            completed_at: None,
            error: None,
            config_json: Some(
                serde_json::json!({
                    "routine_id": latest_routine.id,
                    "routine_name": latest_routine.name,
                    "cron_expression": latest_routine.cron_expression,
                    "action_type": latest_routine.action_type,
                    "is_manual": is_manual,
                })
                .to_string(),
            ),
            parent_run_id: None,
        };
        let _ = db.create_run(&agent_run);

        // 3. Execution setup
        let cancel_token = CancellationToken::new();
        engine
            .register_run_token(&run_id, cancel_token.clone())
            .await;

        let exec_result: Result<String, String> = if latest_routine.action_type == "skill" {
            match db.get_skill(&latest_routine.target_id) {
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
                            SkillReplayer::execute(
                                &skill,
                                Some(&db),
                                Some(&cancel_token),
                                |step, total, desc| {
                                    log::debug!("[Routine Skill] {}/{}: {}", step, total, desc);
                                },
                            )
                            .await
                            .map(|_| format!("Skill '{}' executed successfully.", skill.name))
                        }
                        Err(e) => Err(format!("Failed to parse skill steps: {}", e)),
                    }
                }
                Ok(None) => Err(format!(
                    "Target skill '{}' not found",
                    latest_routine.target_id
                )),
                Err(e) => Err(format!("Database error reading skill: {}", e)),
            }
        } else if latest_routine.action_type == "prompt" {
            // Resolve configured model from settings
            let configured_model = db
                .get_setting("selected_model")
                .ok()
                .flatten()
                .unwrap_or_else(|| DEFAULT_NEMOTRON_MODEL.to_string());

            let caps = ModelRouter::resolve_capabilities(&configured_model);
            if let Some(err) = caps.configuration_error {
                Err(format!(
                    "Model configuration error for '{}': {}",
                    configured_model, err
                ))
            } else {
                let keys = vault
                    .get_all_keys()
                    .map_err(|e| format!("Vault error: {}", e))?;
                let api_key = match caps.provider.as_str() {
                    "nvidia" => keys.nvidia.unwrap_or_default(),
                    _ => keys.nvidia.unwrap_or_default(),
                };

                if api_key.is_empty() {
                    Err(format!(
                        "{} API key not configured for routine. Please add your key in Kyro Settings under Model & Credentials.",
                        caps.provider.to_uppercase()
                    ))
                } else {
                    let history = vec![ChatMessage {
                        role: "user".to_string(),
                        content: MessageContent::from_text(&latest_routine.target_id),
                        tool_calls: None,
                        tool_call_id: None,
                    }];

                    let sys_prompt = "You are Kyro executing an autonomous routine. Complete the requested objective deterministically, safely, and truthfully.";

                    engine
                        .run_step_with_connectors(
                            &run_id,
                            &configured_model,
                            &api_key,
                            None,
                            sys_prompt,
                            history,
                            db.clone(),
                            mcp,
                            connector_manager,
                            Some(cancel_token.clone()),
                            |_| {},
                        )
                        .await
                }
            }
        } else {
            Err(format!(
                "Unsupported routine action type '{}'",
                latest_routine.action_type
            ))
        };

        let completed_at = chrono::Utc::now().timestamp_millis();
        let (status, summary, error) = if cancel_token.is_cancelled() {
            (
                "cancelled".to_string(),
                Some(format!(
                    "Routine '{}' execution was cancelled.",
                    latest_routine.name
                )),
                Some("Execution was cancelled by user or system.".to_string()),
            )
        } else {
            match exec_result {
                Ok(output) => {
                    let s = if output.trim().is_empty() {
                        format!("Routine '{}' completed successfully.", latest_routine.name)
                    } else {
                        output
                    };
                    ("succeeded".to_string(), Some(s), None)
                }
                Err(err_msg) => (
                    "failed".to_string(),
                    Some(format!("Routine failed: {}", err_msg)),
                    Some(err_msg),
                ),
            }
        };

        // 4. Update terminal status truthfully
        let _ = db.update_routine_run(
            &run_id,
            &status,
            completed_at,
            summary.as_deref(),
            error.as_deref(),
        );
        let _ = db.update_run_status(&run_id, &status, None, Some(completed_at), error.as_deref());
        let _ = db.update_routine_last_run(&latest_routine.id, start_time);

        let final_record = RoutineRunRecord {
            id: run_id,
            routine_id: latest_routine.id,
            routine_name: latest_routine.name,
            status,
            started_at: start_time,
            completed_at: Some(completed_at),
            summary,
            error,
        };

        Ok(final_record)
    }

    /// Backwards-compatibility wrapper
    pub async fn execute_routine(
        routine: RoutineRecord,
        db: Database,
        vault: Vault,
        engine: Arc<AgentLoopEngine>,
        mcp: Option<McpManager>,
    ) {
        let _ = Self::execute_routine_unified(routine, db, vault, engine, mcp, None, false).await;
    }
}
