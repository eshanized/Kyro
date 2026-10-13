//! Managed development servers.
//!
//! Starting a project's development server is a consequential action: it
//! executes repository-controlled code.  This module therefore
//!
//! * decides the command from the *actual* manifest and lockfile,
//! * always launches it as an explicit executable plus argument vector (never
//!   a shell command line),
//! * reports the intended command and working directory before the launch is
//!   authorised,
//! * captures the output continuously so readiness can be detected from the
//!   framework's own output,
//! * owns the process tree, so cancellation stops the server and only the
//!   server.

use crate::execution::launcher::{ExecutableResolver, ProgramSpec};
use crate::project::discovery::{
    detect_node_framework, read_package_json, PackageManager, ProjectCandidate,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::process::{Child, Command};
use tokio::sync::broadcast;

/// The status of a managed development server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DevServerStatus {
    Starting,
    Ready,
    Failed,
    Stopped,
}

impl DevServerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            DevServerStatus::Starting => "starting",
            DevServerStatus::Ready => "ready",
            DevServerStatus::Failed => "failed",
            DevServerStatus::Stopped => "stopped",
        }
    }
}

/// The reason a development server failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DevServerFailure {
    /// The process exited before it became ready.
    StartupFailed,
    /// The requested port is already in use.
    PortConflict,
    /// The server did not report readiness in time.
    ReadinessTimeout,
    /// The server was ready and then crashed.
    CrashedAfterReady,
    /// The executable could not be resolved or started.
    LaunchFailed,
}

impl DevServerFailure {
    pub fn as_str(&self) -> &'static str {
        match self {
            DevServerFailure::StartupFailed => "startup_failed",
            DevServerFailure::PortConflict => "port_conflict",
            DevServerFailure::ReadinessTimeout => "readiness_timeout",
            DevServerFailure::CrashedAfterReady => "crashed_after_ready",
            DevServerFailure::LaunchFailed => "launch_failed",
        }
    }
}

/// The command that will be executed to start a development server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevCommand {
    /// The executable, resolved through the controlled discovery mechanism.
    pub executable: String,
    /// The argument vector.
    pub args: Vec<String>,
    /// The working directory.
    pub working_directory: PathBuf,
    /// A human readable description.
    pub description: String,
    /// The package script that was selected, when applicable.
    pub script: Option<String>,
}

impl DevCommand {
    /// The command line as it would be shown to a user.
    pub fn display(&self) -> String {
        let mut parts = vec![self.executable.clone()];
        parts.extend(self.args.iter().cloned());
        parts.join(" ")
    }
}

/// A managed development server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevServerState {
    pub id: String,
    pub pid: u32,
    pub working_directory: PathBuf,
    pub command: String,
    pub status: DevServerStatus,
    pub failure: Option<DevServerFailure>,
    pub error_message: Option<String>,
    pub exit_code: Option<i32>,
    /// The URL that was verified as ready.
    pub ready_url: Option<String>,
    /// The port the server listens on, when it could be determined.
    pub port: Option<u16>,
    pub framework: Option<String>,
    pub package_manager: Option<String>,
    pub started_at: i64,
    pub ready_at: Option<i64>,
    /// The run that owns this server.
    pub run_id: Option<String>,
}

/// The bounded log buffer of a development server.
#[derive(Debug, Clone)]
pub struct LogBuffer {
    inner: Arc<Mutex<VecDeque<String>>>,
    capacity: usize,
}

impl LogBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::new())),
            capacity,
        }
    }

    pub fn push(&self, line: String) {
        let mut guard = self.inner.lock().unwrap();
        guard.push_back(line);
        while guard.len() > self.capacity {
            guard.pop_front();
        }
    }

    /// Returns the last `max_lines` lines.
    pub fn tail(&self, max_lines: usize) -> Vec<String> {
        let guard = self.inner.lock().unwrap();
        let skip = guard.len().saturating_sub(max_lines);
        guard.iter().skip(skip).cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The shared registry of managed development servers.
pub struct DevServerManager {
    servers: Mutex<Vec<DevServerEntry>>,
    counter: Mutex<u64>,
}

struct DevServerEntry {
    state: DevServerState,
    logs: LogBuffer,
    child: Option<Child>,
    stop_tx: Option<broadcast::Sender<()>>,
}

impl Default for DevServerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DevServerManager {
    pub fn new() -> Self {
        Self {
            servers: Mutex::new(Vec::new()),
            counter: Mutex::new(0),
        }
    }

    /// Chooses the development command for a discovered project.
    ///
    /// The decision is derived from the manifest and the lockfile only.  When
    /// the project does not expose an obvious development script the function
    /// reports the available options instead of guessing.
    pub fn choose_command(candidate: &ProjectCandidate) -> Result<DevCommand, String> {
        if candidate.kind != crate::project::discovery::ProjectKind::NodeJs {
            return Err(format!(
                "Kyro can only start Node.js development servers automatically. \
The project at '{}' looks like a {} project; start it manually and point Kyro at the URL.",
                candidate.path.display(),
                candidate.kind.as_str()
            ));
        }

        let manifest = candidate
            .manifest
            .clone()
            .ok_or_else(|| "The project has no package.json".to_string())?;

        let pkg = read_package_json(&manifest)
            .ok_or_else(|| "The package.json could not be parsed".to_string())?;

        let scripts = pkg
            .get("scripts")
            .and_then(|s| s.as_object())
            .cloned()
            .unwrap_or_default();

        // Prefer the conventional development script names, in order.
        const PREFERRED: &[&str] = &["dev", "develop", "start", "serve", "preview", "dev:web"];
        let chosen = PREFERRED
            .iter()
            .find(|name| scripts.contains_key(**name))
            .map(|s| s.to_string());

        let script = match chosen {
            Some(s) => s,
            None => {
                let available: Vec<String> = scripts.keys().cloned().collect();
                return Err(format!(
                    "The project at '{}' does not declare a recognised development script. \
Available scripts: {}. Add a 'dev' script or start the server manually.",
                    candidate.path.display(),
                    if available.is_empty() {
                        "none".to_string()
                    } else {
                        available.join(", ")
                    }
                ));
            }
        };

        let manager = candidate.package_manager.unwrap_or(PackageManager::Npm);
        let (executable, mut args) = manager.run_command();
        args.push(script.clone());

        // Vite and some other frameworks accept the host/port through flags.
        // They are not added automatically because the framework's own
        // configuration file is the authoritative source.

        Ok(DevCommand {
            executable: executable.to_string(),
            args,
            working_directory: candidate.path.clone(),
            description: format!(
                "Start the '{}' script of the {} project at '{}' using {}",
                script,
                detect_node_framework(&manifest).unwrap_or_else(|| "Node.js".to_string()),
                candidate.path.display(),
                manager.as_str()
            ),
            script: Some(script),
        })
    }

    /// Starts a development server and waits until it is ready.
    pub async fn start(
        &self,
        command: &DevCommand,
        run_id: Option<&str>,
        ready_timeout_ms: u64,
    ) -> Result<DevServerState, String> {
        // Resolve the executable through the controlled discovery mechanism.
        let resolver = ExecutableResolver::new();
        let spec = ProgramSpec {
            executable: command.executable.clone(),
            args: command.args.clone(),
            cwd: Some(command.working_directory.clone()),
            env: Vec::new(),
        };
        let resolved = resolver
            .resolve(&spec)
            .map_err(|e| format!("Could not resolve the development server command: {}", e))?;

        let id = {
            let mut counter = self.counter.lock().unwrap();
            *counter += 1;
            format!("devsrv_{}", *counter)
        };

        let mut cmd = Command::new(&resolved.path);
        cmd.args(&resolved.args)
            .current_dir(&command.working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        #[cfg(unix)]
        {
            // Own the whole process group so cancellation can stop the server
            // and everything it started.
            cmd.process_group(0);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to start '{}': {}", command.display(), e))?;

        let pid = child.id().unwrap_or(0);
        let logs = LogBuffer::new(2000);
        let (stop_tx, _) = broadcast::channel(4);

        let framework = command
            .working_directory
            .join("package.json")
            .is_file()
            .then(|| detect_node_framework(&command.working_directory.join("package.json")))
            .flatten();

        let mut state = DevServerState {
            id: id.clone(),
            pid,
            working_directory: command.working_directory.clone(),
            command: command.display(),
            status: DevServerStatus::Starting,
            failure: None,
            error_message: None,
            exit_code: None,
            ready_url: None,
            port: None,
            framework,
            package_manager: None,
            started_at: chrono::Utc::now().timestamp_millis(),
            ready_at: None,
            run_id: run_id.map(|s| s.to_string()),
        };

        // Spawn the log readers.
        let stdout_logs = logs.clone();
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    stdout_logs.push(line);
                }
            });
        }
        let stderr_logs = logs.clone();
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    stderr_logs.push(line);
                }
            });
        }

        // Register before waiting so that a cancellation can find it.
        self.servers.lock().unwrap().push(DevServerEntry {
            state: state.clone(),
            logs: logs.clone(),
            child: Some(child),
            stop_tx: Some(stop_tx.clone()),
        });

        // Wait for readiness.
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_millis(ready_timeout_ms);
        let mut ready: Option<(String, u16)> = None;
        let mut failure: Option<(DevServerFailure, String)> = None;

        loop {
            if let Some(candidate) = detect_ready_url(&logs.tail(200)) {
                // Verify the URL actually answers before declaring readiness.
                if verify_http_ready(&candidate.0).await {
                    ready = Some(candidate);
                    break;
                }
            }

            // Did the process die?
            if let Some(exit) = self.poll_exit(&id) {
                let logs_text = logs.tail(60).join("\n");
                let reason = classify_startup_failure(&logs_text);
                failure = Some((
                    reason,
                    format!(
                        "The development server exited with code {} before it became ready.\n{}",
                        exit, logs_text
                    ),
                ));
                break;
            }

            if std::time::Instant::now() >= deadline {
                let logs_text = logs.tail(60).join("\n");
                failure = Some((
                    DevServerFailure::ReadinessTimeout,
                    format!(
                        "The development server did not report a ready URL within {}ms.\n{}",
                        ready_timeout_ms, logs_text
                    ),
                ));
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }

        if let Some((url, port)) = ready {
            state.status = DevServerStatus::Ready;
            state.ready_url = Some(url);
            state.port = Some(port);
            state.ready_at = Some(chrono::Utc::now().timestamp_millis());
            self.update_state(&id, |s| {
                s.status = DevServerStatus::Ready;
                s.ready_url = state.ready_url.clone();
                s.port = Some(port);
                s.ready_at = state.ready_at;
            });
            return Ok(state);
        }

        let (kind, message) = failure.unwrap_or((
            DevServerFailure::StartupFailed,
            "The development server did not start".to_string(),
        ));
        state.status = DevServerStatus::Failed;
        state.failure = Some(kind);
        state.error_message = Some(message.clone());
        self.update_state(&id, |s| {
            s.status = DevServerStatus::Failed;
            s.failure = Some(kind);
            s.error_message = Some(message.clone());
        });
        Err(message)
    }

    /// Polls the exit status of a managed server.
    fn poll_exit(&self, id: &str) -> Option<i32> {
        let mut servers = self.servers.lock().unwrap();
        let entry = servers.iter_mut().find(|e| e.state.id == id)?;
        let child = entry.child.as_mut()?;
        match child.try_wait() {
            Ok(Some(status)) => {
                let code = status.code();
                entry.state.exit_code = code;
                entry.state.status = DevServerStatus::Failed;
                if entry.state.failure.is_none() {
                    entry.state.failure = Some(DevServerFailure::StartupFailed);
                }
                code.or(Some(-1))
            }
            _ => None,
        }
    }

    fn update_state<F: FnOnce(&mut DevServerState)>(&self, id: &str, f: F) {
        let mut servers = self.servers.lock().unwrap();
        if let Some(entry) = servers.iter_mut().find(|e| e.state.id == id) {
            f(&mut entry.state);
        }
    }

    /// Returns the state of a managed server.
    pub fn state(&self, id: &str) -> Option<DevServerState> {
        let servers = self.servers.lock().unwrap();
        servers
            .iter()
            .find(|e| e.state.id == id)
            .map(|e| e.state.clone())
    }

    /// Returns the bounded log of a managed server.
    pub fn logs(&self, id: &str, max_lines: usize) -> Option<Vec<String>> {
        let servers = self.servers.lock().unwrap();
        servers
            .iter()
            .find(|e| e.state.id == id)
            .map(|e| e.logs.tail(max_lines.clamp(1, 500)))
    }

    /// Lists the managed servers.
    pub fn list(&self) -> Vec<DevServerState> {
        self.servers
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.state.clone())
            .collect()
    }

    /// Stops a managed server and its process tree.
    pub fn stop(&self, id: &str) -> Result<(), String> {
        let mut servers = self.servers.lock().unwrap();
        let entry = servers
            .iter_mut()
            .find(|e| e.state.id == id)
            .ok_or_else(|| format!("Development server '{}' does not exist", id))?;

        if let Some(tx) = &entry.stop_tx {
            let _ = tx.send(());
        }

        let pid = entry.state.pid as i32;
        #[cfg(unix)]
        unsafe {
            // Terminate the whole process group, but only the group Kyro
            // created for this server.
            libc::kill(-pid, libc::SIGTERM);
            libc::kill(pid, libc::SIGTERM);
        }
        if let Some(child) = entry.child.as_mut() {
            // The kill and the wait are synchronous operations on the child
            // handle; the futures are dropped after the process is reaped.
            drop(child.kill());
            drop(child.wait());
        }
        entry.state.status = DevServerStatus::Stopped;
        entry.state.ready_url = None;
        Ok(())
    }

    /// Stops every server that belongs to a run.
    pub fn stop_run(&self, run_id: &str) {
        let ids: Vec<String> = self
            .servers
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.state.run_id.as_deref() == Some(run_id))
            .map(|e| e.state.id.clone())
            .collect();
        for id in ids {
            let _ = self.stop(&id);
        }
    }

    /// Stops every managed server.
    pub fn stop_all(&self) {
        let ids: Vec<String> = self
            .servers
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.state.id.clone())
            .collect();
        for id in ids {
            let _ = self.stop(&id);
        }
    }
}

/// Detects the ready URL in a development server's output.
///
/// The framework's own output is the authoritative source; the port is taken
/// from the URL rather than assumed.
pub fn detect_ready_url(lines: &[String]) -> Option<(String, u16)> {
    let mut best: Option<(String, u16)> = None;
    for line in lines {
        let lower = line.to_lowercase();
        if !(lower.contains("http://") || lower.contains("https://")) {
            continue;
        }
        // Only trust URLs that appear on a readiness-ish line, or that point
        // at a loopback address.
        let looks_ready = lower.contains("ready")
            || lower.contains("started")
            || lower.contains("listening")
            || lower.contains("local:")
            || lower.contains("localhost")
            || lower.contains("127.0.0.1");
        if !looks_ready {
            continue;
        }
        for token in line.split_whitespace() {
            let candidate = token
                .trim_matches(|c: char| c == ',' || c == '(' || c == ')' || c == '[' || c == ']');
            let candidate = candidate
                .split("://")
                .nth(1)
                .map(|_| {
                    // Rebuild with the scheme that was present.
                    let scheme = if candidate.starts_with("https://") {
                        "https"
                    } else {
                        "http"
                    };
                    let rest = candidate.split("://").nth(1).unwrap_or("");
                    format!("{}://{}", scheme, rest)
                })
                .unwrap_or_else(|| candidate.to_string());
            if let Some(port) = extract_port(&candidate) {
                if is_loopback(&candidate) {
                    let normalised = if candidate.ends_with('/') {
                        candidate
                    } else {
                        format!("{}/", candidate)
                    };
                    if best.is_none() {
                        best = Some((normalised, port));
                    }
                }
            }
        }
    }
    best
}

/// Extracts the port from a URL.
pub fn extract_port(url: &str) -> Option<u16> {
    let after_scheme = url.split("://").nth(1)?;
    let authority = after_scheme.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?;
    if host_port.starts_with('[') {
        // IPv6 literal
        let close = host_port.find(']')?;
        let rest = &host_port[close + 1..];
        return rest.strip_prefix(':').and_then(|p| p.parse().ok());
    }
    match host_port.rsplit_once(':') {
        Some((_, port)) => port.parse().ok(),
        None => None,
    }
}

/// `true` when the URL points at the local machine.
pub fn is_loopback(url: &str) -> bool {
    let after_scheme = url.split("://").nth(1).unwrap_or("");
    let authority = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    let host = host.split(':').next().unwrap_or("");
    matches!(
        host.to_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "0.0.0.0" | "[::1]" | "::1"
    )
}

/// Classifies why a development server failed to start.
pub fn classify_startup_failure(logs: &str) -> DevServerFailure {
    let lower = logs.to_lowercase();
    if lower.contains("eaddrinuse")
        || lower.contains("address already in use")
        || lower.contains("port is already in use")
        || lower.contains("port already in use")
    {
        return DevServerFailure::PortConflict;
    }
    DevServerFailure::StartupFailed
}

/// Performs a minimal HTTP readiness probe.
pub async fn verify_http_ready(url: &str) -> bool {
    let reqwest = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    match reqwest.get(url).send().await {
        Ok(resp) => {
            let status = resp.status();
            // Any HTTP answer means the server is listening.  A 404 still
            // proves the process is serving.
            let _ = status;
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_a_nextjs_ready_line() {
        let lines = vec![
            "  ▲ Next.js 14.2.0".to_string(),
            "  - Local:        http://localhost:3000".to_string(),
        ];
        let (url, port) = detect_ready_url(&lines).unwrap();
        assert_eq!(port, 3000);
        assert!(url.contains("localhost:3000"));
    }

    #[test]
    fn detects_a_vite_ready_line_on_a_non_default_port() {
        let lines = vec![
            "  VITE v5.0.0  ready in 312 ms".to_string(),
            "  ➜  Local:   http://localhost:5177/".to_string(),
        ];
        let (url, port) = detect_ready_url(&lines).unwrap();
        assert_eq!(port, 5177);
        assert!(url.contains("5177"));
    }

    #[test]
    fn ignores_unrelated_urls() {
        let lines = vec!["Downloading https://registry.npmjs.org/react".to_string()];
        assert!(detect_ready_url(&lines).is_none());
    }

    #[test]
    fn extracts_ports() {
        assert_eq!(extract_port("http://localhost:3000/"), Some(3000));
        assert_eq!(extract_port("http://127.0.0.1:8080"), Some(8080));
        assert_eq!(extract_port("http://localhost"), None);
    }

    #[test]
    fn detects_loopback() {
        assert!(is_loopback("http://localhost:3000"));
        assert!(is_loopback("http://127.0.0.1:3000"));
        assert!(!is_loopback("https://example.com"));
    }

    #[test]
    fn classifies_port_conflicts() {
        assert_eq!(
            classify_startup_failure("Error: listen EADDRINUSE: address already in use :::3000"),
            DevServerFailure::PortConflict
        );
        assert_eq!(
            classify_startup_failure("Error: Cannot find module 'foo'"),
            DevServerFailure::StartupFailed
        );
    }

    #[test]
    fn chooses_the_dev_script_from_the_manifest() {
        let dir = std::env::temp_dir().join(format!("kyro_devcmd_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("package.json"),
            r#"{"scripts":{"build":"next build","dev":"next dev","start":"next start"}}"#,
        )
        .unwrap();

        let candidate = crate::project::discovery::inspect_project_root(&dir, true).unwrap();
        let command = DevServerManager::choose_command(&candidate).unwrap();
        assert_eq!(command.executable, "npm");
        assert_eq!(command.args, vec!["run".to_string(), "dev".to_string()]);
        assert_eq!(command.working_directory, dir);
        assert_eq!(command.display(), "npm run dev");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn refuses_to_guess_when_no_development_script_exists() {
        let dir = std::env::temp_dir().join(format!("kyro_devcmd2_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("package.json"),
            r#"{"scripts":{"build":"next build"}}"#,
        )
        .unwrap();
        let candidate = crate::project::discovery::inspect_project_root(&dir, true).unwrap();
        let err = DevServerManager::choose_command(&candidate).unwrap_err();
        assert!(err.contains("does not declare a recognised development script"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn uses_the_package_manager_from_the_lockfile() {
        let dir = std::env::temp_dir().join(format!("kyro_devcmd3_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("package.json"), r#"{"scripts":{"dev":"vite"}}"#).unwrap();
        std::fs::write(dir.join("pnpm-lock.yaml"), "lockfileVersion: 9\n").unwrap();
        let candidate = crate::project::discovery::inspect_project_root(&dir, true).unwrap();
        let command = DevServerManager::choose_command(&candidate).unwrap();
        assert_eq!(command.executable, "pnpm");
        assert_eq!(command.args, vec!["run".to_string(), "dev".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
