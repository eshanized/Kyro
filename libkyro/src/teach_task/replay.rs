use super::skill::Skill;
use crate::agent::cancellation::CancellationToken;
use crate::computer_use::{ComputerAction, InputController};
use crate::governance::{AuthDecision, AuthorizationEngine, PolicyEngine, TakeoverController};
use crate::storage::Database;
use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::sleep;

static ACTIVE_SKILL_EXECUTIONS: Lazy<Mutex<HashSet<String>>> =
    Lazy::new(|| Mutex::new(HashSet::new()));

pub struct SkillReplayer;

impl SkillReplayer {
    /// Check whether a skill is currently executing
    pub fn is_skill_running(id: &str) -> bool {
        let active = ACTIVE_SKILL_EXECUTIONS.lock().unwrap();
        active.contains(id)
    }

    /// Mark a skill as actively executing (used for testing lifecycle rules)
    pub fn set_skill_running_for_test(id: &str, running: bool) {
        let mut active = ACTIVE_SKILL_EXECUTIONS.lock().unwrap();
        if running {
            active.insert(id.to_string());
        } else {
            active.remove(id);
        }
    }

    /// Synchronized deletion preventing race conditions with concurrent execution dispatches
    pub fn delete_skill_synchronized(
        db: &Database,
        id: &str,
        associated_routines: &[crate::storage::RoutineRecord],
    ) -> Result<bool, String> {
        let active_skills = ACTIVE_SKILL_EXECUTIONS.lock().unwrap();
        if active_skills.contains(id) {
            return Err(format!(
                "Cannot delete task '{}': task is currently executing. Please wait for execution to complete.",
                id
            ));
        }

        let active_routines =
            crate::teach_task::scheduler::RoutineScheduler::active_routines_lock();
        for r in associated_routines {
            if active_routines.contains(&r.id) {
                return Err(format!(
                    "Cannot delete task '{}': associated scheduled routine '{}' is currently executing. Please wait for execution to complete.",
                    id, r.name
                ));
            }
        }

        db.delete_skill(id).map_err(|e| e.to_string())
    }

    pub async fn execute<F>(
        skill: &Skill,
        db: Option<&Database>,
        cancel_token: Option<&CancellationToken>,
        mut on_progress: F,
    ) -> Result<(), String>
    where
        F: FnMut(usize, usize, String),
    {
        if let Some(d) = db {
            match d.get_skill(&skill.id) {
                Ok(Some(_)) => {}
                Ok(None) => {
                    return Err(format!(
                        "Cannot replay task '{}': task definition was deleted",
                        skill.id
                    ))
                }
                Err(e) => return Err(format!("Failed to verify task definition: {}", e)),
            }
        }

        {
            let mut active = ACTIVE_SKILL_EXECUTIONS.lock().unwrap();
            active.insert(skill.id.clone());
        }
        struct SkillGuard(String);
        impl Drop for SkillGuard {
            fn drop(&mut self) {
                let mut active = ACTIVE_SKILL_EXECUTIONS.lock().unwrap();
                active.remove(&self.0);
            }
        }
        let _guard = SkillGuard(skill.id.clone());

        let total_steps = skill.steps.len();
        let mut controller = InputController::new()?;

        for (idx, step) in skill.steps.iter().enumerate() {
            if let Some(token) = cancel_token {
                if token.is_cancelled() {
                    return Err("Skill replay cancelled by token".to_string());
                }
            }

            if InputController::is_aborted() {
                return Err("Skill replay aborted by safety trigger".to_string());
            }

            if step.delay_ms > 0 {
                sleep(Duration::from_millis(step.delay_ms)).await;
            }

            on_progress(
                idx + 1,
                total_steps,
                format!("{}: {:?}", step.action_type, step.text),
            );

            let action = match step.action_type.as_str() {
                "click" => Some(ComputerAction::LeftClick {
                    x: step.x,
                    y: step.y,
                }),
                "double_click" => Some(ComputerAction::DoubleClick {
                    x: step.x,
                    y: step.y,
                }),
                "right_click" => Some(ComputerAction::RightClick {
                    x: step.x,
                    y: step.y,
                }),
                "move" => Some(ComputerAction::MouseMove {
                    x: step.x.unwrap_or(0),
                    y: step.y.unwrap_or(0),
                }),
                "type" => step
                    .text
                    .as_ref()
                    .map(|t| ComputerAction::Type { text: t.clone() }),
                "key" => step
                    .key
                    .as_ref()
                    .map(|k| ComputerAction::Key { text: k.clone() }),
                "delay" => {
                    sleep(Duration::from_millis(step.delay_ms)).await;
                    None
                }
                _ => None,
            };

            if let Some(act) = action {
                // Policy and Governance Authorization Check
                if let Some(database) = db {
                    let policy = PolicyEngine::load(database);
                    let act_val = serde_json::to_value(&act).unwrap_or_default();
                    let decision = AuthorizationEngine::evaluate_computer_action(&policy, &act_val);
                    match decision {
                        AuthDecision::Allow => {}
                        AuthDecision::RequireApproval { reason, .. } => {
                            return Err(format!(
                                "Skill replay step '{}' requires approval: {}",
                                step.action_type, reason
                            ));
                        }
                        AuthDecision::RequireTakeover { reason } => {
                            TakeoverController::request(&reason);
                            while TakeoverController::is_active() {
                                if let Some(token) = cancel_token {
                                    if token.is_cancelled() {
                                        return Err(
                                            "Skill replay cancelled during human takeover".into()
                                        );
                                    }
                                }
                                sleep(Duration::from_millis(300)).await;
                            }
                        }
                        AuthDecision::Deny { reason } => {
                            return Err(format!(
                                "Skill replay step '{}' denied by policy: {}",
                                step.action_type, reason
                            ));
                        }
                        AuthDecision::AllowSandboxOnly { .. } => {
                            return Err("Computer actions cannot run in sandbox".into());
                        }
                    }
                }

                controller.execute(&act)?;
            }
        }

        Ok(())
    }
}
