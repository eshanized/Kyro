use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static GHOST_MODE_ACTIVE: AtomicBool = AtomicBool::new(false);
static GHOST_XVFB_CHILD: Lazy<Mutex<Option<Child>>> = Lazy::new(|| Mutex::new(None));
pub const GHOST_DISPLAY: &str = ":99";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhostStatus {
    pub is_enabled: bool,
    pub display: String,
    pub resolution: String,
}

pub struct GhostDisplayManager;

impl GhostDisplayManager {
    /// Starts the isolated virtual X11 framebuffer if not already running
    pub fn ensure_virtual_display() -> Result<(), String> {
        let mut guard = GHOST_XVFB_CHILD.lock().unwrap();
        if guard.is_some() {
            return Ok(());
        }

        // Check if Xvfb is already running on :99
        let check = Command::new("xset")
            .args(["-display", GHOST_DISPLAY, "q"])
            .output();

        if let Ok(out) = check {
            if out.status.success() {
                return Ok(()); // Already running
            }
        }

        // Spawn Xvfb on display :99 with 1920x1080 24-bit color
        let child = Command::new("Xvfb")
            .args([
                GHOST_DISPLAY,
                "-screen",
                "0",
                "1920x1080x24",
                "-ac",
                "+extension",
                "GLX",
                "+render",
                "-noreset",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn Xvfb on {}: {}", GHOST_DISPLAY, e))?;

        *guard = Some(child);
        std::thread::sleep(std::time::Duration::from_millis(300));
        Ok(())
    }

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        if enabled {
            Self::ensure_virtual_display()?;
        }
        GHOST_MODE_ACTIVE.store(enabled, Ordering::SeqCst);
        Ok(())
    }

    pub fn is_enabled() -> bool {
        GHOST_MODE_ACTIVE.load(Ordering::SeqCst)
    }

    pub fn get_status() -> GhostStatus {
        GhostStatus {
            is_enabled: Self::is_enabled(),
            display: GHOST_DISPLAY.to_string(),
            resolution: "1920x1080".to_string(),
        }
    }

    /// Launches a program directly inside the isolated virtual screen
    pub fn launch_in_ghost(cmd: &str) -> Result<(), String> {
        Self::ensure_virtual_display()?;
        Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .env("DISPLAY", GHOST_DISPLAY)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to launch '{}' in ghost display: {}", cmd, e))?;
        Ok(())
    }

    /// Captures the virtual display buffer without disturbing physical monitor
    pub fn capture_ghost_screen() -> Result<Vec<u8>, String> {
        Self::ensure_virtual_display()?;
        let temp_png =
            std::env::temp_dir().join(format!("kyro_ghost_{}.png", uuid::Uuid::new_v4()));

        let output = Command::new("import")
            .args(["-window", "root", "-display", GHOST_DISPLAY])
            .arg(&temp_png)
            .output()
            .map_err(|e| format!("Failed to capture ghost display: {}", e))?;

        if !output.status.success() {
            // Fallback: try xwd and convert if import isn't present
            return Err("Unable to grab virtual screen with ImageMagick import".into());
        }

        let bytes = std::fs::read(&temp_png).map_err(|e| e.to_string())?;
        std::fs::remove_file(&temp_png).ok();
        Ok(bytes)
    }

    /// Sends mouse clicks directly to the virtual display using xdotool
    pub fn click(x: i32, y: i32) -> Result<(), String> {
        Command::new("xdotool")
            .args([
                "mousemove",
                "--sync",
                &x.to_string(),
                &y.to_string(),
                "click",
                "1",
            ])
            .env("DISPLAY", GHOST_DISPLAY)
            .output()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Types text directly into the virtual display using xdotool
    pub fn type_text(text: &str) -> Result<(), String> {
        Command::new("xdotool")
            .args(["type", "--delay", "12", text])
            .env("DISPLAY", GHOST_DISPLAY)
            .output()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Sends a special key into the virtual display
    pub fn key_press(key: &str) -> Result<(), String> {
        Command::new("xdotool")
            .args(["key", key])
            .env("DISPLAY", GHOST_DISPLAY)
            .output()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
