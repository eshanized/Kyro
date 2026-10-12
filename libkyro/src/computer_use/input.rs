use enigo::{Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// The scope used when a caller does not name one explicitly.
pub const DEFAULT_ABORT_SCOPE: &str = "global";

/// The scope used by agent-driven computer actions.
///
/// Every agent run gets its own scope so that one task can never reset or
/// bypass another task's active emergency stop.
pub fn agent_abort_scope(run_id: &str) -> String {
    format!("run:{}", run_id)
}

/// Registry of active emergency aborts, keyed by scope.
///
/// The previous implementation used a single global boolean.  That made it
/// impossible to (a) tell which task was stopped and (b) prevent an unrelated
/// task from clearing an active stop.
static ABORT_SCOPES: Lazy<Mutex<BTreeSet<String>>> = Lazy::new(|| Mutex::new(BTreeSet::new()));

/// A process-wide latch that is set when *any* scope is aborted.
///
/// It exists so that callers that do not carry a scope (for example the
/// desktop HUD) can still observe that automation is currently halted.
static ANY_ABORT: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", content = "params")]
pub enum ComputerAction {
    #[serde(rename = "mouse_move")]
    MouseMove { x: i32, y: i32 },

    #[serde(rename = "left_click")]
    LeftClick { x: Option<i32>, y: Option<i32> },

    #[serde(rename = "right_click")]
    RightClick { x: Option<i32>, y: Option<i32> },

    #[serde(rename = "double_click")]
    DoubleClick { x: Option<i32>, y: Option<i32> },

    #[serde(rename = "left_click_drag")]
    LeftClickDrag { x: i32, y: i32 },

    #[serde(rename = "type")]
    Type { text: String },

    #[serde(rename = "key")]
    Key { text: String },

    #[serde(rename = "hotkey")]
    Hotkey { keys: Vec<String> },

    #[serde(rename = "wait")]
    Wait { ms: u64 },
}

pub struct InputController {
    enigo: Enigo,
    /// The scope this controller is bound to.  Actions performed through this
    /// controller are refused while this scope is aborted.
    scope: String,
}

impl InputController {
    pub fn new() -> Result<Self, String> {
        Self::with_scope(DEFAULT_ABORT_SCOPE)
    }

    /// Creates a controller bound to a specific abort scope.
    pub fn with_scope(scope: &str) -> Result<Self, String> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Failed to initialize input controller: {:?}", e))?;
        Ok(Self {
            enigo,
            scope: scope.to_string(),
        })
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    /// Triggers an emergency abort for the given scope.
    ///
    /// The abort is scoped: other tasks keep running, but this scope can no
    /// longer perform input.
    pub fn trigger_abort_for(scope: &str) {
        let mut scopes = ABORT_SCOPES.lock().unwrap();
        scopes.insert(scope.to_string());
        ANY_ABORT.store(true, Ordering::SeqCst);
    }

    /// Clears the emergency abort for one scope only.
    ///
    /// Clearing a scope never clears a different scope's abort.
    pub fn reset_abort_for(scope: &str) {
        let mut scopes = ABORT_SCOPES.lock().unwrap();
        scopes.remove(scope);
        let still = !scopes.is_empty();
        ANY_ABORT.store(still, Ordering::SeqCst);
    }

    /// Legacy global trigger.  Kept for the desktop HUD, which is not bound to
    /// a particular agent run.
    pub fn trigger_abort() {
        Self::trigger_abort_for(DEFAULT_ABORT_SCOPE);
    }

    /// Legacy global reset.  Only clears the global scope.
    pub fn reset_abort() {
        Self::reset_abort_for(DEFAULT_ABORT_SCOPE);
    }

    /// `true` when the given scope is currently aborted.
    pub fn is_aborted_scope(scope: &str) -> bool {
        ABORT_SCOPES.lock().unwrap().contains(scope)
    }

    /// `true` when *any* scope is currently aborted.
    pub fn is_aborted() -> bool {
        ANY_ABORT.load(Ordering::SeqCst)
    }

    /// The scopes that are currently aborted.
    pub fn aborted_scopes() -> Vec<String> {
        ABORT_SCOPES.lock().unwrap().iter().cloned().collect()
    }

    /// Removes a scope entirely.  Used when a run finishes so that a later
    /// run is not blocked by a stale stop.
    pub fn release_scope(scope: &str) {
        Self::reset_abort_for(scope);
    }

    pub fn execute(&mut self, action: &ComputerAction) -> Result<(), String> {
        if Self::is_aborted_scope(&self.scope) {
            return Err(format!(
                "Execution aborted by safety trigger for scope '{}'",
                self.scope
            ));
        }

        match action {
            ComputerAction::MouseMove { x, y } => {
                self.enigo
                    .move_mouse(*x, *y, Coordinate::Abs)
                    .map_err(|e| format!("{:?}", e))?;
            }
            ComputerAction::LeftClick { x, y } => {
                if let (Some(x), Some(y)) = (x, y) {
                    self.enigo
                        .move_mouse(*x, *y, Coordinate::Abs)
                        .map_err(|e| format!("{:?}", e))?;
                    std::thread::sleep(Duration::from_millis(50));
                }
                self.enigo
                    .button(Button::Left, Direction::Click)
                    .map_err(|e| format!("{:?}", e))?;
            }
            ComputerAction::RightClick { x, y } => {
                if let (Some(x), Some(y)) = (x, y) {
                    self.enigo
                        .move_mouse(*x, *y, Coordinate::Abs)
                        .map_err(|e| format!("{:?}", e))?;
                    std::thread::sleep(Duration::from_millis(50));
                }
                self.enigo
                    .button(Button::Right, Direction::Click)
                    .map_err(|e| format!("{:?}", e))?;
            }
            ComputerAction::DoubleClick { x, y } => {
                if let (Some(x), Some(y)) = (x, y) {
                    self.enigo
                        .move_mouse(*x, *y, Coordinate::Abs)
                        .map_err(|e| format!("{:?}", e))?;
                    std::thread::sleep(Duration::from_millis(50));
                }
                self.enigo
                    .button(Button::Left, Direction::Click)
                    .map_err(|e| format!("{:?}", e))?;
                std::thread::sleep(Duration::from_millis(100));
                self.enigo
                    .button(Button::Left, Direction::Click)
                    .map_err(|e| format!("{:?}", e))?;
            }
            ComputerAction::LeftClickDrag { x, y } => {
                self.enigo
                    .button(Button::Left, Direction::Press)
                    .map_err(|e| format!("{:?}", e))?;
                std::thread::sleep(Duration::from_millis(50));
                self.enigo
                    .move_mouse(*x, *y, Coordinate::Abs)
                    .map_err(|e| format!("{:?}", e))?;
                std::thread::sleep(Duration::from_millis(50));
                self.enigo
                    .button(Button::Left, Direction::Release)
                    .map_err(|e| format!("{:?}", e))?;
            }
            ComputerAction::Type { text } => {
                self.enigo.text(text).map_err(|e| format!("{:?}", e))?;
            }
            ComputerAction::Key { text } => {
                let key = parse_key(text);
                self.enigo
                    .key(key, Direction::Click)
                    .map_err(|e| format!("{:?}", e))?;
            }
            ComputerAction::Hotkey { keys } => {
                let parsed: Vec<Key> = keys.iter().map(|k| parse_key(k)).collect();
                for k in &parsed {
                    self.enigo
                        .key(*k, Direction::Press)
                        .map_err(|e| format!("{:?}", e))?;
                }
                std::thread::sleep(Duration::from_millis(50));
                for k in parsed.iter().rev() {
                    self.enigo
                        .key(*k, Direction::Release)
                        .map_err(|e| format!("{:?}", e))?;
                }
            }
            ComputerAction::Wait { ms } => {
                // A long wait is broken into short slices so that an abort
                // that arrives during the wait is honoured promptly.
                let mut remaining = *ms;
                while remaining > 0 {
                    if Self::is_aborted_scope(&self.scope) {
                        return Err(format!(
                            "Execution aborted by safety trigger for scope '{}'",
                            self.scope
                        ));
                    }
                    let slice = remaining.min(50);
                    std::thread::sleep(Duration::from_millis(slice));
                    remaining -= slice;
                }
            }
        }

        Ok(())
    }
}

fn parse_key(k: &str) -> Key {
    match k.to_lowercase().as_str() {
        "return" | "enter" => Key::Return,
        "tab" => Key::Tab,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        "escape" | "esc" => Key::Escape,
        "super" | "meta" | "command" | "win" => Key::Meta,
        "control" | "ctrl" => Key::Control,
        "alt" => Key::Alt,
        "shift" => Key::Shift,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        other => {
            if let Some(ch) = other.chars().next() {
                Key::Unicode(ch)
            } else {
                Key::Return
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests share a process-wide registry, so they run serially and use
    /// unique scopes.
    fn unique_scope(tag: &str) -> String {
        format!("test:{}:{}", tag, uuid::Uuid::new_v4())
    }

    #[test]
    fn abort_is_scoped() {
        let a = unique_scope("a");
        let b = unique_scope("b");

        InputController::trigger_abort_for(&a);
        assert!(InputController::is_aborted_scope(&a));
        assert!(!InputController::is_aborted_scope(&b));

        // Resetting an unrelated scope must not clear the active stop.
        InputController::reset_abort_for(&b);
        assert!(InputController::is_aborted_scope(&a));

        InputController::reset_abort_for(&a);
        assert!(!InputController::is_aborted_scope(&a));
    }

    #[test]
    fn agent_scopes_are_distinct_per_run() {
        let a = agent_abort_scope("run_1");
        let b = agent_abort_scope("run_2");
        assert_ne!(a, b);
        assert!(a.starts_with("run:"));
    }

    #[test]
    fn global_scope_is_independent_of_run_scopes() {
        let run_scope = agent_abort_scope("run_x");
        InputController::trigger_abort_for(&run_scope);

        // The legacy global reset must not clear a run-scoped abort.
        InputController::reset_abort();
        assert!(InputController::is_aborted_scope(&run_scope));

        InputController::reset_abort_for(&run_scope);
    }

    #[test]
    fn aborted_scopes_are_listed() {
        let scope = unique_scope("list");
        InputController::trigger_abort_for(&scope);
        assert!(InputController::aborted_scopes().contains(&scope));
        InputController::reset_abort_for(&scope);
        assert!(!InputController::aborted_scopes().contains(&scope));
    }
}
