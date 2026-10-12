use crate::execution::launcher::{ExecutableResolver, ProgramSpec, ResolvedProgram};
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

/// The outcome of launching a program inside the Ghost display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GhostLaunchResult {
    /// Absolute path of the executable that was started.
    pub executable: String,
    /// The exact argument vector that was started.
    pub args: Vec<String>,
    /// PID of the launched process.
    pub pid: u32,
    /// The display the process was started on.
    pub display: String,
}

/// A program that was launched inside the Ghost display.
///
/// The child handle is retained so the process can be stopped later; Kyro
/// never leaks the `sh -c` shell that previously wrapped every launch.
pub struct GhostLaunchedApp {
    pub child: Child,
    pub result: GhostLaunchResult,
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

    /// Launches a program inside the isolated virtual screen.
    ///
    /// The launch is expressed as an explicit executable plus argument vector.
    /// The command is **never** passed to a shell, so shell metacharacters in
    /// the argument list remain literal and cannot start a second process.
    /// The executable is resolved through the controlled discovery mechanism
    /// before it is started.
    pub fn launch_in_ghost(spec: &ProgramSpec) -> Result<GhostLaunchResult, String> {
        Self::ensure_virtual_display()?;

        let resolver = ExecutableResolver::new();
        let resolved: ResolvedProgram = resolver.resolve(spec).map_err(|e| e.to_string())?;

        let mut command = Command::new(&resolved.path);
        command.args(&resolved.args);
        command.env("DISPLAY", GHOST_DISPLAY);
        command.stdin(Stdio::null());
        command.stdout(Stdio::null());
        command.stderr(Stdio::null());

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }

        let child = command.spawn().map_err(|e| {
            format!(
                "Failed to launch '{}' in ghost display: {}",
                resolved.path.display(),
                e
            )
        })?;

        let pid = child.id();

        Ok(GhostLaunchResult {
            executable: resolved.path.to_string_lossy().to_string(),
            args: resolved.args,
            pid,
            display: GHOST_DISPLAY.to_string(),
        })
    }

    /// Launches a program and retains the child handle so the caller can stop it.
    pub fn launch_managed(spec: &ProgramSpec) -> Result<GhostLaunchedApp, String> {
        Self::ensure_virtual_display()?;

        let resolver = ExecutableResolver::new();
        let resolved: ResolvedProgram = resolver.resolve(spec).map_err(|e| e.to_string())?;

        let mut command = Command::new(&resolved.path);
        command.args(&resolved.args);
        command.env("DISPLAY", GHOST_DISPLAY);
        command.stdin(Stdio::null());
        command.stdout(Stdio::null());
        command.stderr(Stdio::null());

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }

        let child = command.spawn().map_err(|e| {
            format!(
                "Failed to launch '{}' in ghost display: {}",
                resolved.path.display(),
                e
            )
        })?;

        let pid = child.id();

        Ok(GhostLaunchedApp {
            result: GhostLaunchResult {
                executable: resolved.path.to_string_lossy().to_string(),
                args: resolved.args,
                pid,
                display: GHOST_DISPLAY.to_string(),
            },
            child,
        })
    }

    /// Terminates a process that Kyro started, together with its process
    /// group, so that helper processes do not survive.
    pub fn stop_managed(app: &mut GhostLaunchedApp) {
        let pid = app.result.pid as i32;
        #[cfg(unix)]
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
            libc::kill(pid, libc::SIGTERM);
        }
        let _ = app.child.kill();
        let _ = app.child.wait();
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
            let _ = std::fs::remove_file(&temp_png);
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(format!(
                "Unable to grab virtual screen with ImageMagick import: {}",
                if stderr.is_empty() {
                    "command failed"
                } else {
                    &stderr
                }
            ));
        }

        let bytes = std::fs::read(&temp_png).map_err(|e| e.to_string());
        let _ = std::fs::remove_file(&temp_png);
        bytes
    }

    /// Sends mouse clicks directly to the virtual display using xdotool.
    ///
    /// The exit status of `xdotool` is checked: a failed click is reported as
    /// an error instead of being silently swallowed.
    pub fn click(x: i32, y: i32) -> Result<(), String> {
        Self::run_xdotool(&[
            "mousemove",
            "--sync",
            &x.to_string(),
            &y.to_string(),
            "click",
            "1",
        ])
    }

    /// Moves the pointer on the virtual display.
    pub fn move_mouse(x: i32, y: i32) -> Result<(), String> {
        Self::run_xdotool(&["mousemove", "--sync", &x.to_string(), &y.to_string()])
    }

    /// Performs a right click on the virtual display.
    pub fn right_click(x: i32, y: i32) -> Result<(), String> {
        Self::run_xdotool(&[
            "mousemove",
            "--sync",
            &x.to_string(),
            &y.to_string(),
            "click",
            "3",
        ])
    }

    /// Performs a double click on the virtual display.
    pub fn double_click(x: i32, y: i32) -> Result<(), String> {
        Self::run_xdotool(&[
            "mousemove",
            "--sync",
            &x.to_string(),
            &y.to_string(),
            "click",
            "--repeat",
            "2",
            "--delay",
            "120",
            "1",
        ])
    }

    /// Types text directly into the virtual display using xdotool.
    pub fn type_text(text: &str) -> Result<(), String> {
        Self::run_xdotool(&["type", "--delay", "12", text])
    }

    /// Sends a special key into the virtual display.
    pub fn key_press(key: &str) -> Result<(), String> {
        Self::run_xdotool(&["key", key])
    }

    fn run_xdotool(args: &[&str]) -> Result<(), String> {
        let output = Command::new("xdotool")
            .args(args)
            .env("DISPLAY", GHOST_DISPLAY)
            .output()
            .map_err(|e| format!("Failed to run xdotool: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(format!(
                "xdotool {} failed: {}",
                args.first().copied().unwrap_or(""),
                if stderr.is_empty() {
                    format!("exit status {}", output.status)
                } else {
                    stderr
                }
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_rejects_shell_metacharacters_as_executable() {
        let spec = ProgramSpec::new("firefox; rm -rf /");
        let err = GhostDisplayManager::launch_in_ghost(&spec).unwrap_err();
        // The command is never handed to a shell, so it simply cannot be found
        // as an executable.
        assert!(
            err.contains("not found")
                || err.contains("controlled search path")
                || err.contains("untrusted location"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn launch_rejects_relative_executable_paths() {
        let spec = ProgramSpec::new("../../bin/sh");
        let err = GhostDisplayManager::launch_in_ghost(&spec).unwrap_err();
        assert!(
            err.contains("untrusted") || err.contains("not found"),
            "{}",
            err
        );
    }

    #[test]
    fn launch_error_is_reported_for_missing_binary() {
        let spec = ProgramSpec::new("kyro-definitely-missing-binary");
        let err = GhostDisplayManager::launch_in_ghost(&spec).unwrap_err();
        assert!(err.contains("not found"), "{}", err);
    }
}
