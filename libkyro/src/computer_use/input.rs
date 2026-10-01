use enigo::{Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static EMERGENCY_ABORT: AtomicBool = AtomicBool::new(false);

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
}

impl InputController {
    pub fn new() -> Result<Self, String> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Failed to initialize input controller: {:?}", e))?;
        Ok(Self { enigo })
    }

    pub fn trigger_abort() {
        EMERGENCY_ABORT.store(true, Ordering::SeqCst);
    }

    pub fn reset_abort() {
        EMERGENCY_ABORT.store(false, Ordering::SeqCst);
    }

    pub fn is_aborted() -> bool {
        EMERGENCY_ABORT.load(Ordering::SeqCst)
    }

    pub fn execute(&mut self, action: &ComputerAction) -> Result<(), String> {
        if Self::is_aborted() {
            return Err("Execution aborted by safety trigger".to_string());
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
                std::thread::sleep(Duration::from_millis(*ms));
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
