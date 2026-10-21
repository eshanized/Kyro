//! Management of the browser automation worker process.
//!
//! The worker is a provider-neutral Node.js program that drives installed browsers
//! over WebDriver BiDi (Firefox) or Chrome DevTools Protocol (Chromium-family).
//! Kyro owns its lifecycle: the worker is started on demand, is torn down with the
//! process group that owns it, and never survives the run that created it.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex};

/// The worker source is embedded in the binary so that the subsystem can be
/// packaged and launched without depending on files next to the executable.
pub const WORKER_SOURCE: &str = include_str!("../../browser-worker/worker.mjs");

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerError {
    pub category: String,
    pub message: String,
}

impl std::fmt::Display for WorkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.category, self.message)
    }
}

impl std::error::Error for WorkerError {}

/// The channel that carries a worker's answer back to the caller.
type PendingRequests =
    Arc<Mutex<HashMap<u64, oneshot::Sender<Result<serde_json::Value, WorkerError>>>>>;

/// A handle to a running worker process.
pub struct WorkerHandle {
    child: Child,
    stdin: ChildStdin,
    pending: PendingRequests,
    node_path: PathBuf,
    script_path: PathBuf,
    temp_dir: Option<PathBuf>,
}

impl WorkerHandle {
    /// Starts the worker process.
    pub async fn start() -> Result<Self, WorkerError> {
        let node_path = find_node().ok_or_else(|| WorkerError {
            category: "node_not_found".into(),
            message: "Node.js was not found on PATH. The browser automation worker requires \
Node.js 20 or newer (Node's built-in WebSocket client is used). \
Install it with 'sudo apt install nodejs' or 'sudo dnf install nodejs'."
                .into(),
        })?;

        // The worker source is written to a private temporary file so that the
        // process can be launched from any working directory.
        let temp_dir =
            std::env::temp_dir().join(format!("kyro-browser-worker-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .map_err(|e| WorkerError {
                category: "worker_temp_dir_failed".into(),
                message: format!("Could not create the worker directory: {}", e),
            })?;
        let script_path = temp_dir.join("browser-worker.mjs");
        tokio::fs::write(&script_path, WORKER_SOURCE)
            .await
            .map_err(|e| WorkerError {
                category: "worker_write_failed".into(),
                message: format!("Could not write the browser worker script: {}", e),
            })?;

        let mut command = Command::new(&node_path);
        command
            .arg(&script_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        #[cfg(unix)]
        {
            // Own the whole process group so that a cancellation can stop the
            // browser and the worker together.
            command.process_group(0);
        }

        let mut child = command.spawn().map_err(|e| WorkerError {
            category: "worker_spawn_failed".into(),
            message: format!("Could not start the browser worker: {}", e),
        })?;

        let stdin = child.stdin.take().ok_or_else(|| WorkerError {
            category: "worker_spawn_failed".into(),
            message: "The browser worker did not expose a stdin pipe".into(),
        })?;

        let stdout = child.stdout.take().ok_or_else(|| WorkerError {
            category: "worker_spawn_failed".into(),
            message: "The browser worker did not expose a stdout pipe".into(),
        })?;

        let pending: PendingRequests = Arc::new(Mutex::new(HashMap::new()));

        // Drain stderr so the worker never blocks on a full pipe.
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    log::debug!("[browser-worker] {}", line);
                }
            });
        }

        // Dispatch responses to the waiting callers.
        {
            let pending = pending.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let line = line.trim().to_string();
                    if line.is_empty() {
                        continue;
                    }
                    let value: serde_json::Value = match serde_json::from_str(&line) {
                        Ok(v) => v,
                        Err(e) => {
                            log::warn!("[browser-worker] unparsable response: {}", e);
                            continue;
                        }
                    };
                    let id = match value.get("id").and_then(|i| i.as_u64()) {
                        Some(i) => i,
                        None => continue,
                    };
                    let sender = pending.lock().await.remove(&id);
                    if let Some(sender) = sender {
                        let ok = value.get("ok").and_then(|o| o.as_bool()).unwrap_or(false);
                        if ok {
                            let _ = sender.send(Ok(value
                                .get("result")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null)));
                        } else {
                            let err = value
                                .get("error")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null);
                            let category = err
                                .get("category")
                                .and_then(|c| c.as_str())
                                .unwrap_or("worker_error")
                                .to_string();
                            let message = err
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("The browser worker reported an unspecified error")
                                .to_string();
                            let _ = sender.send(Err(WorkerError { category, message }));
                        }
                    }
                }
                // The worker exited: fail every outstanding request.
                let mut guard = pending.lock().await;
                for (_, sender) in guard.drain() {
                    let _ = sender.send(Err(WorkerError {
                        category: "worker_exited".into(),
                        message: "The browser worker exited unexpectedly".into(),
                    }));
                }
            });
        }

        Ok(Self {
            child,
            stdin,
            pending,
            node_path,
            script_path,
            temp_dir: Some(temp_dir),
        })
    }

    /// Sends a request and waits for the matching response.
    pub async fn call(
        &mut self,
        op: &str,
        params: serde_json::Value,
        timeout_ms: u64,
    ) -> Result<serde_json::Value, WorkerError> {
        let id = REQUEST_ID.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        let request = serde_json::json!({ "id": id, "op": op, "params": params });
        let mut line = serde_json::to_string(&request).map_err(|e| WorkerError {
            category: "request_encoding_failed".into(),
            message: e.to_string(),
        })?;
        line.push('\n');

        if let Err(e) = self.stdin.write_all(line.as_bytes()).await {
            self.pending.lock().await.remove(&id);
            return Err(WorkerError {
                category: "worker_write_failed".into(),
                message: format!("Could not send a request to the browser worker: {}", e),
            });
        }
        if let Err(e) = self.stdin.flush().await {
            self.pending.lock().await.remove(&id);
            return Err(WorkerError {
                category: "worker_write_failed".into(),
                message: format!("Could not flush the browser worker pipe: {}", e),
            });
        }

        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(WorkerError {
                category: "worker_disconnected".into(),
                message: "The browser worker closed the request channel".into(),
            }),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(WorkerError {
                    category: "worker_timeout".into(),
                    message: format!(
                        "The browser operation '{}' did not complete within {}ms",
                        op, timeout_ms
                    ),
                })
            }
        }
    }

    /// Terminates the worker and every process it owns.
    pub async fn shutdown(&mut self) {
        // The worker terminates its browser sessions on SIGTERM.
        #[cfg(unix)]
        {
            let pid = self.child.id();
            if let Some(pid) = pid {
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGTERM);
                    libc::kill(pid as i32, libc::SIGTERM);
                }
            }
        }
        let _ =
            tokio::time::timeout(std::time::Duration::from_millis(1500), self.child.wait()).await;

        #[cfg(unix)]
        {
            if let Some(pid) = self.child.id() {
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
        }
        let _ = self.child.start_kill();

        if let Some(dir) = self.temp_dir.take() {
            let _ = tokio::fs::remove_dir_all(dir).await;
        }
    }

    pub fn node_path(&self) -> &std::path::Path {
        &self.node_path
    }

    pub fn script_path(&self) -> &std::path::Path {
        &self.script_path
    }
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            if let Some(pid) = self.child.id() {
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
        }
        let _ = self.child.start_kill();
        if let Some(dir) = self.temp_dir.take() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// Locates the Node.js runtime.
pub fn find_node() -> Option<PathBuf> {
    let resolver = crate::execution::launcher::ExecutableResolver::new();
    if let Ok(resolved) = resolver.resolve(&crate::execution::launcher::ProgramSpec::new("node")) {
        return Some(resolved.path);
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("node");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn worker_reports_missing_node_cleanly() {
        // On a machine without Node the worker must fail with an actionable
        // message instead of a panic.
        let result = WorkerHandle::start().await;
        match result {
            Ok(mut handle) => {
                let caps = handle
                    .call("capabilities", serde_json::json!({}), 20000)
                    .await;
                assert!(caps.is_ok(), "capabilities should succeed: {:?}", caps);
                handle.shutdown().await;
            }
            Err(e) => {
                assert_eq!(e.category, "node_not_found");
                assert!(e.message.contains("Node.js"));
            }
        }
    }
}
