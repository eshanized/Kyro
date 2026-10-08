use super::skill::{Skill, SkillStep};
use crate::governance::TakeoverController;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

static IS_RECORDING: AtomicBool = AtomicBool::new(false);
static RECORDING_STEPS: Lazy<Mutex<Vec<SkillStep>>> = Lazy::new(|| Mutex::new(Vec::new()));
static LAST_EVENT_TIME: Lazy<Mutex<Option<Instant>>> = Lazy::new(|| Mutex::new(None));
static CURRENT_SKILL_NAME: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));
static CURRENT_SKILL_DESC: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

pub struct TaskRecorder;

impl TaskRecorder {
    pub fn start(name: &str, description: &str) -> Result<(), String> {
        if IS_RECORDING.load(Ordering::SeqCst) {
            return Err("Recording session is already active".to_string());
        }

        let mut steps = RECORDING_STEPS.lock().unwrap();
        steps.clear();

        *CURRENT_SKILL_NAME.lock().unwrap() = name.to_string();
        *CURRENT_SKILL_DESC.lock().unwrap() = description.to_string();
        *LAST_EVENT_TIME.lock().unwrap() = Some(Instant::now());

        IS_RECORDING.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn record_action(
        action_type: &str,
        x: Option<i32>,
        y: Option<i32>,
        text: Option<String>,
        key: Option<String>,
    ) -> Result<(), String> {
        if !IS_RECORDING.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Sensitive information filtering: Do not record credentials, passwords, or tokens
        let sanitized_text = if let Some(ref t) = text {
            if TakeoverController::detect_sensitive_checkpoint(t).is_some() {
                Some("[REDACTED_SENSITIVE_INPUT]".to_string())
            } else {
                Some(t.clone())
            }
        } else {
            None
        };

        let mut last_time = LAST_EVENT_TIME.lock().unwrap();
        let delay_ms = if let Some(prev) = *last_time {
            let elapsed = prev.elapsed().as_millis() as u64;
            *last_time = Some(Instant::now());
            elapsed.clamp(50, 5000)
        } else {
            *last_time = Some(Instant::now());
            100
        };

        let step = SkillStep {
            action_type: action_type.to_string(),
            x,
            y,
            text: sanitized_text,
            key,
            delay_ms,
        };

        let mut steps = RECORDING_STEPS.lock().unwrap();
        steps.push(step);
        Ok(())
    }

    pub fn stop() -> Result<Skill, String> {
        if !IS_RECORDING.load(Ordering::SeqCst) {
            return Err("No recording session is currently active".to_string());
        }

        IS_RECORDING.store(false, Ordering::SeqCst);

        let steps = {
            let mut s = RECORDING_STEPS.lock().unwrap();
            let collected = s.clone();
            s.clear();
            collected
        };

        let name = CURRENT_SKILL_NAME.lock().unwrap().clone();
        let description = CURRENT_SKILL_DESC.lock().unwrap().clone();
        let id = format!(
            "skill-{}",
            uuid::Uuid::new_v4()
                .to_string()
                .chars()
                .take(8)
                .collect::<String>()
        );

        Ok(Skill {
            id,
            name: if name.is_empty() {
                "Recorded Workflow".into()
            } else {
                name
            },
            description,
            steps,
            created_at: chrono::Utc::now().timestamp_millis(),
        })
    }

    pub fn is_recording() -> bool {
        IS_RECORDING.load(Ordering::SeqCst)
    }

    pub fn get_current_count() -> usize {
        RECORDING_STEPS.lock().unwrap().len()
    }
}
