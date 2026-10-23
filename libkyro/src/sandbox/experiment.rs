//! Sandboxed code-execution subsystem for safe experiments in Kyro.
//!
//! Provides isolated container execution with strict resource limits,
//! approved image allowlisting, network isolation by default, disposable
//! workspaces, artifact generation, and durable SQLite lifecycle persistence.

use super::docker::{is_approved_sandbox_image, normalize_sandbox_image, DockerSandbox};
use crate::agent::cancellation::CancellationToken;
use crate::storage::{ArtifactRecord, CodeExperimentRecord, Database};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use uuid::Uuid;

/// Hard security limits for container execution.
pub const MAX_CPU_LIMIT: f32 = 2.0;
pub const MAX_MEMORY_LIMIT_MB: u64 = 2048;
pub const MAX_TIMEOUT_SECONDS: u64 = 300;
pub const MAX_OUTPUT_BYTES: usize = 65536; // 64 KB stdout/stderr bounding

/// Supported experiment runtime environments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExperimentRuntime {
    Python,
    Node,
    Rust,
    Bash,
    Debian,
    Custom(String),
}

impl ExperimentRuntime {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Python => "python",
            Self::Node => "node",
            Self::Rust => "rust",
            Self::Bash => "bash",
            Self::Debian => "debian",
            Self::Custom(s) => s.as_str(),
        }
    }

    pub fn from_str_name(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "python" | "py" => Self::Python,
            "node" | "javascript" | "js" | "typescript" | "ts" => Self::Node,
            "rust" | "rs" => Self::Rust,
            "bash" | "sh" | "shell" => Self::Bash,
            "debian" => Self::Debian,
            other => Self::Custom(other.to_string()),
        }
    }

    pub fn default_image(&self) -> &'static str {
        match self {
            Self::Python => "python:3.12-alpine",
            Self::Node => "node:20-alpine",
            Self::Rust => "rust:1.80-alpine",
            Self::Bash => "alpine:3.20",
            Self::Debian => "debian:bookworm-slim",
            Self::Custom(_) => "alpine:3.20",
        }
    }

    pub fn script_filename(&self) -> &'static str {
        match self {
            Self::Python => "experiment_main.py",
            Self::Node => "experiment_main.js",
            Self::Rust => "experiment_main.rs",
            Self::Bash => "experiment_main.sh",
            Self::Debian => "experiment_main.sh",
            Self::Custom(_) => "experiment_main.sh",
        }
    }

    pub fn entrypoint_args(&self, script_path: &str) -> Vec<String> {
        match self {
            Self::Python => vec!["python".to_string(), script_path.to_string()],
            Self::Node => vec!["node".to_string(), script_path.to_string()],
            Self::Rust => vec![
                "sh".to_string(),
                "-c".to_string(),
                format!("rustc {} -o /tmp/runner && /tmp/runner", script_path),
            ],
            Self::Bash | Self::Debian | Self::Custom(_) => {
                vec!["sh".to_string(), script_path.to_string()]
            }
        }
    }
}

/// Network policy for the experiment container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExperimentNetworkMode {
    None,
    Bridge,
    Host,
}

impl ExperimentNetworkMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Bridge => "bridge",
            Self::Host => "host",
        }
    }

    pub fn from_str_name(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "bridge" => Self::Bridge,
            "host" => Self::Host,
            _ => Self::None,
        }
    }
}

/// Lifecycle status for an experiment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    Interrupted,
}

impl ExperimentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn from_str_name(s: &str) -> Self {
        match s {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "timed_out" => Self::TimedOut,
            _ => Self::Interrupted,
        }
    }
}

/// Resource constraints applied to the experiment container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub cpu: f32,
    pub memory_mb: u64,
    pub pids_limit: u32,
    pub timeout_seconds: u64,
    pub max_output_bytes: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu: 1.0,
            memory_mb: 512,
            pids_limit: 100,
            timeout_seconds: 60,
            max_output_bytes: MAX_OUTPUT_BYTES,
        }
    }
}

impl ResourceLimits {
    pub fn sanitize(&self) -> Self {
        Self {
            cpu: self.cpu.clamp(0.1, MAX_CPU_LIMIT),
            memory_mb: self.memory_mb.clamp(64, MAX_MEMORY_LIMIT_MB),
            pids_limit: self.pids_limit.clamp(10, 200),
            timeout_seconds: self.timeout_seconds.clamp(1, MAX_TIMEOUT_SECONDS),
            max_output_bytes: self.max_output_bytes.clamp(1024, MAX_OUTPUT_BYTES),
        }
    }
}

/// Specification for requesting an experiment run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentSpec {
    pub id: Option<String>,
    pub name: String,
    pub runtime: String,
    pub custom_image: Option<String>,
    pub script: Option<String>,
    pub argv: Option<Vec<String>>,
    pub workspace_path: Option<String>,
    pub network_mode: Option<String>,
    pub limits: Option<ResourceLimits>,
}

/// Container mount configuration.
#[derive(Debug, Clone)]
pub struct ContainerMount {
    pub host_path: PathBuf,
    pub container_path: String,
    pub read_only: bool,
}

/// Raw execution result from a container runner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawExecutionResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub cancelled: bool,
}

/// Trait defining a container execution provider.
///
/// This provides a clean abstraction seam allowing unit and integration tests
/// to run deterministically without requiring a live Docker/Podman daemon.
#[async_trait]
pub trait ContainerRunner: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    async fn run(
        &self,
        image: &str,
        args: &[String],
        mounts: &[ContainerMount],
        limits: &ResourceLimits,
        network: &str,
        timeout: Duration,
        cancel_token: CancellationToken,
    ) -> Result<RawExecutionResult, String>;
}

/// Production container runner that invokes the host's Docker or Podman daemon.
pub struct DockerCliRunner;

impl Default for DockerCliRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl DockerCliRunner {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ContainerRunner for DockerCliRunner {
    #[allow(clippy::too_many_arguments)]
    async fn run(
        &self,
        image: &str,
        args: &[String],
        mounts: &[ContainerMount],
        limits: &ResourceLimits,
        network: &str,
        timeout: Duration,
        cancel_token: CancellationToken,
    ) -> Result<RawExecutionResult, String> {
        let probe = DockerSandbox::probe();
        if !probe.available {
            return Err(
                "Container sandbox engine is not available on this system. Execution failed closed."
                    .to_string(),
            );
        }

        let normalized_image = normalize_sandbox_image(image);
        if !is_approved_sandbox_image(normalized_image) {
            return Err(format!(
                "Image '{}' is not in the approved sandbox allowlist",
                image
            ));
        }

        let container_name = format!("kyro-exp-{}", Uuid::new_v4().simple());
        let binary = probe.engine.clone();

        let mut cmd_args: Vec<String> = vec![
            "run".to_string(),
            "--name".to_string(),
            container_name.clone(),
            "--rm".to_string(),
            format!("--network={}", network),
            format!("--memory={}m", limits.memory_mb),
            format!("--cpus={:.2}", limits.cpu),
            format!("--pids-limit={}", limits.pids_limit),
            "--security-opt=no-new-privileges".to_string(),
            "--cap-drop=ALL".to_string(),
            "--read-only".to_string(),
            "--tmpfs=/tmp:rw,noexec,nosuid,size=64m".to_string(),
            "-w=/workspace".to_string(),
        ];

        // Apply clean, non-inherited environment
        cmd_args.push("-e".to_string());
        cmd_args
            .push("PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string());
        cmd_args.push("-e".to_string());
        cmd_args.push("HOME=/tmp".to_string());

        // Apply mounts
        for mount in mounts {
            let mode = if mount.read_only { "ro" } else { "rw" };
            let host_str = mount.host_path.to_string_lossy().to_string();
            cmd_args.push("-v".to_string());
            cmd_args.push(format!("{}:{}:{}", host_str, mount.container_path, mode));
        }

        cmd_args.push(normalized_image.to_string());
        for arg in args {
            cmd_args.push(arg.clone());
        }

        let mut child = tokio::process::Command::new(&binary)
            .args(&cmd_args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Failed to spawn container runner: {}", e))?;

        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        let max_bytes = limits.max_output_bytes;

        let stdout_handle = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(mut stream) = stdout_pipe {
                let mut chunk = vec![0u8; 4096];
                while let Ok(n) = stream.read(&mut chunk).await {
                    if n == 0 {
                        break;
                    }
                    if buf.len() < max_bytes {
                        let to_take = n.min(max_bytes - buf.len());
                        buf.extend_from_slice(&chunk[..to_take]);
                    }
                }
            }
            String::from_utf8_lossy(&buf).to_string()
        });

        let stderr_handle = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(mut stream) = stderr_pipe {
                let mut chunk = vec![0u8; 4096];
                while let Ok(n) = stream.read(&mut chunk).await {
                    if n == 0 {
                        break;
                    }
                    if buf.len() < max_bytes {
                        let to_take = n.min(max_bytes - buf.len());
                        buf.extend_from_slice(&chunk[..to_take]);
                    }
                }
            }
            String::from_utf8_lossy(&buf).to_string()
        });

        tokio::select! {
            res = child.wait() => {
                let exit_status = res.map_err(|e| format!("Error waiting on container process: {}", e))?;
                let stdout = stdout_handle.await.unwrap_or_default();
                let stderr = stderr_handle.await.unwrap_or_default();
                Ok(RawExecutionResult {
                    exit_code: exit_status.code().unwrap_or(-1),
                    stdout,
                    stderr,
                    timed_out: false,
                    cancelled: false,
                })
            }
            _ = tokio::time::sleep(timeout) => {
                let _ = child.kill().await;
                // Force stop the container name
                let _ = std::process::Command::new(&binary).args(["rm", "-f", &container_name]).output();
                let stdout = stdout_handle.await.unwrap_or_default();
                let stderr = stderr_handle.await.unwrap_or_default();
                Ok(RawExecutionResult {
                    exit_code: -1,
                    stdout,
                    stderr: format!("{}\nExecution timed out after {} seconds.", stderr, timeout.as_secs()),
                    timed_out: true,
                    cancelled: false,
                })
            }
            _ = cancel_token.cancelled() => {
                let _ = child.kill().await;
                let _ = std::process::Command::new(&binary).args(["rm", "-f", &container_name]).output();
                let stdout = stdout_handle.await.unwrap_or_default();
                let stderr = stderr_handle.await.unwrap_or_default();
                Ok(RawExecutionResult {
                    exit_code: -1,
                    stdout,
                    stderr: format!("{}\nExecution cancelled by user.", stderr),
                    timed_out: false,
                    cancelled: true,
                })
            }
        }
    }
}

/// Mock container runner for deterministic unit and integration testing without a Docker daemon.
pub struct MockContainerRunner {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub delay: Duration,
    pub should_time_out: bool,
    pub should_fail_closed: bool,
}

impl Default for MockContainerRunner {
    fn default() -> Self {
        Self {
            exit_code: 0,
            stdout: "Mock execution succeeded".to_string(),
            stderr: String::new(),
            delay: Duration::from_millis(10),
            should_time_out: false,
            should_fail_closed: false,
        }
    }
}

impl MockContainerRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_exit(mut self, code: i32, stdout: &str, stderr: &str) -> Self {
        self.exit_code = code;
        self.stdout = stdout.to_string();
        self.stderr = stderr.to_string();
        self
    }

    pub fn with_timeout(mut self) -> Self {
        self.should_time_out = true;
        self
    }

    pub fn with_fail_closed(mut self) -> Self {
        self.should_fail_closed = true;
        self
    }
}

#[async_trait]
impl ContainerRunner for MockContainerRunner {
    #[allow(clippy::too_many_arguments)]
    async fn run(
        &self,
        image: &str,
        _args: &[String],
        _mounts: &[ContainerMount],
        _limits: &ResourceLimits,
        _network: &str,
        _timeout: Duration,
        cancel_token: CancellationToken,
    ) -> Result<RawExecutionResult, String> {
        if self.should_fail_closed {
            return Err("Container sandbox engine is not available on this system. Execution failed closed.".to_string());
        }

        let normalized = normalize_sandbox_image(image);
        if !is_approved_sandbox_image(normalized) {
            return Err(format!(
                "Image '{}' is not in the approved sandbox allowlist",
                image
            ));
        }

        if cancel_token.is_cancelled() {
            return Ok(RawExecutionResult {
                exit_code: -1,
                stdout: String::new(),
                stderr: "Execution cancelled by user.".to_string(),
                timed_out: false,
                cancelled: true,
            });
        }

        if self.should_time_out {
            tokio::time::sleep(Duration::from_millis(50)).await;
            return Ok(RawExecutionResult {
                exit_code: -1,
                stdout: self.stdout.clone(),
                stderr: "Execution timed out.".to_string(),
                timed_out: true,
                cancelled: false,
            });
        }

        tokio::time::sleep(self.delay).await;

        if cancel_token.is_cancelled() {
            return Ok(RawExecutionResult {
                exit_code: -1,
                stdout: String::new(),
                stderr: "Execution cancelled by user.".to_string(),
                timed_out: false,
                cancelled: true,
            });
        }

        Ok(RawExecutionResult {
            exit_code: self.exit_code,
            stdout: self.stdout.clone(),
            stderr: self.stderr.clone(),
            timed_out: false,
            cancelled: false,
        })
    }
}

/// Comparison result between two experiments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentComparisonResult {
    pub exp_a_id: String,
    pub exp_a_name: String,
    pub exp_a_status: String,
    pub exp_a_exit_code: Option<i32>,
    pub exp_b_id: String,
    pub exp_b_name: String,
    pub exp_b_status: String,
    pub exp_b_exit_code: Option<i32>,
    pub summary_diff: String,
    pub stdout_match: bool,
    pub exit_codes_match: bool,
}

/// Experiment runner service coordinating persistence, governance, workspaces, and artifacts.
pub struct ExperimentRunner {
    db: Database,
    runner: Arc<dyn ContainerRunner>,
}

impl ExperimentRunner {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            runner: Arc::new(DockerCliRunner::new()),
        }
    }

    pub fn with_runner(db: Database, runner: Arc<dyn ContainerRunner>) -> Self {
        Self { db, runner }
    }

    /// Recursively copy files from a source directory to a target directory,
    /// skipping hidden VCS and build cache artifacts (`.git`, `node_modules`, `target`).
    fn copy_workspace_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
        if !dst.exists() {
            std::fs::create_dir_all(dst)?;
        }

        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();

            if name_str == ".git"
                || name_str == "node_modules"
                || name_str == "target"
                || name_str == ".cargo"
            {
                continue;
            }

            let target_path = dst.join(file_name);
            if path.is_dir() {
                Self::copy_workspace_recursive(&path, &target_path)?;
            } else if path.is_file() {
                std::fs::copy(&path, &target_path)?;
            }
        }
        Ok(())
    }

    /// Executes an experiment according to its specification.
    pub async fn execute(
        &self,
        spec: ExperimentSpec,
        run_id: Option<&str>,
        cancel_token: CancellationToken,
    ) -> Result<CodeExperimentRecord, String> {
        let id = spec.id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let runtime = ExperimentRuntime::from_str_name(&spec.runtime);
        let image = spec
            .custom_image
            .as_deref()
            .unwrap_or_else(|| runtime.default_image());

        let normalized_image = normalize_sandbox_image(image);
        if !is_approved_sandbox_image(normalized_image) {
            return Err(format!(
                "Image '{}' is rejected: only allowlisted runtime images are permitted",
                image
            ));
        }

        let limits = spec.limits.unwrap_or_default().sanitize();
        let net_mode =
            ExperimentNetworkMode::from_str_name(spec.network_mode.as_deref().unwrap_or("none"));

        // Workspace isolation
        let mut mounts = Vec::new();

        let temp_base = std::env::temp_dir().join(format!("kyro_exp_{}", id));
        let isolated_ws = temp_base.join("workspace");
        let isolated_out = temp_base.join("output");

        std::fs::create_dir_all(&isolated_ws)
            .map_err(|e| format!("Failed to create isolated workspace: {}", e))?;
        std::fs::create_dir_all(&isolated_out)
            .map_err(|e| format!("Failed to create isolated output dir: {}", e))?;
        let isolated_workspace_parent = Some(temp_base.clone());

        if let Some(ref ws_path_str) = spec.workspace_path {
            let p = Path::new(ws_path_str);
            let canonical = self
                .db
                .validate_authorized_path(p)
                .map_err(|e| format!("Workspace path authorization failed: {}", e))?;

            Self::copy_workspace_recursive(&canonical, &isolated_ws)
                .map_err(|e| format!("Failed to populate isolated workspace: {}", e))?;
        }

        // Script delivery
        let final_args = if let Some(ref script_text) = spec.script {
            let script_filename = runtime.script_filename();
            let host_script_path = isolated_ws.join(script_filename);
            std::fs::write(&host_script_path, script_text)
                .map_err(|e| format!("Failed to write experiment script: {}", e))?;

            runtime.entrypoint_args(script_filename)
        } else if let Some(ref argv) = spec.argv {
            argv.clone()
        } else {
            return Err(
                "Experiment specification must include either 'script' or 'argv'".to_string(),
            );
        };

        mounts.push(ContainerMount {
            host_path: isolated_ws.clone(),
            container_path: "/workspace".to_string(),
            read_only: false,
        });

        mounts.push(ContainerMount {
            host_path: isolated_out.clone(),
            container_path: "/output".to_string(),
            read_only: false,
        });

        let now = chrono::Utc::now().timestamp_millis();
        let mut record = CodeExperimentRecord {
            id: id.clone(),
            name: spec.name.clone(),
            run_id: run_id.map(|s| s.to_string()),
            runtime: runtime.as_str().to_string(),
            image: normalized_image.to_string(),
            script: spec.script.clone(),
            argv_json: serde_json::to_string(&final_args).unwrap_or_else(|_| "[]".to_string()),
            workspace_path: spec.workspace_path.clone(),
            output_dir: Some(isolated_out.to_string_lossy().to_string()),
            network_mode: net_mode.as_str().to_string(),
            cpu_limit: limits.cpu,
            memory_limit_mb: limits.memory_mb,
            timeout_seconds: limits.timeout_seconds,
            status: ExperimentStatus::Running.as_str().to_string(),
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
            result_summary: None,
            artifact_ids_json: None,
            created_at: now,
            started_at: Some(now),
            completed_at: None,
            error: None,
        };

        // Persist initial record
        self.db
            .create_code_experiment(&record)
            .map_err(|e| format!("Database error creating experiment record: {}", e))?;

        // Execute via runner
        let timeout_dur = Duration::from_secs(limits.timeout_seconds);
        let exec_res = self
            .runner
            .run(
                normalized_image,
                &final_args,
                &mounts,
                &limits,
                net_mode.as_str(),
                timeout_dur,
                cancel_token,
            )
            .await;

        let completed_now = chrono::Utc::now().timestamp_millis();
        record.completed_at = Some(completed_now);

        let mut artifact_ids = Vec::new();

        match exec_res {
            Ok(raw) => {
                record.exit_code = Some(raw.exit_code);
                record.stdout_tail = Some(raw.stdout.clone());
                record.stderr_tail = Some(raw.stderr.clone());

                if raw.cancelled {
                    record.status = ExperimentStatus::Cancelled.as_str().to_string();
                    record.error = Some("Experiment cancelled by user".to_string());
                } else if raw.timed_out {
                    record.status = ExperimentStatus::TimedOut.as_str().to_string();
                    record.error = Some(format!(
                        "Experiment timed out after {} seconds",
                        limits.timeout_seconds
                    ));
                } else if raw.exit_code == 0 {
                    record.status = ExperimentStatus::Succeeded.as_str().to_string();
                    record.result_summary = Some(format!(
                        "Experiment '{}' finished successfully (exit 0)",
                        record.name
                    ));
                } else {
                    record.status = ExperimentStatus::Failed.as_str().to_string();
                    record.result_summary = Some(format!(
                        "Experiment '{}' failed with exit code {}",
                        record.name, raw.exit_code
                    ));
                }

                // Register generated output files as artifacts
                if isolated_out.exists() {
                    if let Ok(entries) = std::fs::read_dir(&isolated_out) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.is_file() {
                                let art_id = format!("art_{}", Uuid::new_v4().simple());
                                let file_name = entry.file_name().to_string_lossy().to_string();
                                let size_bytes =
                                    entry.metadata().map(|m| m.len()).unwrap_or(0) as i64;

                                let art = ArtifactRecord {
                                    id: art_id.clone(),
                                    run_id: run_id.map(|s| s.to_string()),
                                    conversation_id: None,
                                    name: file_name,
                                    file_path: path.to_string_lossy().to_string(),
                                    artifact_type: "log".to_string(),
                                    mime_type: "text/plain".to_string(),
                                    size_bytes,
                                    created_at: completed_now,
                                    metadata_json: Some(
                                        serde_json::json!({
                                            "experiment_id": id,
                                            "is_output": true
                                        })
                                        .to_string(),
                                    ),
                                };
                                if self.db.create_artifact(&art).is_ok() {
                                    artifact_ids.push(art_id);
                                }
                            }
                        }
                    }
                }

                // Create summary report artifact
                let report_content = format!(
                    "# Experiment Report: {}\n\n\
                     - **ID**: `{}`\n\
                     - **Runtime**: `{}` ({})\n\
                     - **Status**: **{}**\n\
                     - **Exit Code**: {}\n\
                     - **Duration**: {} ms\n\
                     - **Network Policy**: `{}`\n\n\
                     ## Standard Output\n```\n{}\n```\n\n\
                     ## Standard Error\n```\n{}\n```\n",
                    record.name,
                    record.id,
                    record.runtime,
                    record.image,
                    record.status,
                    raw.exit_code,
                    completed_now - now,
                    record.network_mode,
                    raw.stdout,
                    raw.stderr
                );

                let report_id = format!("art_report_{}", Uuid::new_v4().simple());
                let report_filename = format!("experiment_{}_report.md", id);
                let report_path = temp_base.join(&report_filename);
                let _ = std::fs::write(&report_path, &report_content);

                let report_art = ArtifactRecord {
                    id: report_id.clone(),
                    run_id: run_id.map(|s| s.to_string()),
                    conversation_id: None,
                    name: report_filename,
                    file_path: report_path.to_string_lossy().to_string(),
                    artifact_type: "markdown".to_string(),
                    mime_type: "text/markdown".to_string(),
                    size_bytes: report_content.len() as i64,
                    created_at: completed_now,
                    metadata_json: Some(
                        serde_json::json!({
                            "experiment_id": id,
                            "is_report": true
                        })
                        .to_string(),
                    ),
                };
                if self.db.create_artifact(&report_art).is_ok() {
                    artifact_ids.push(report_id);
                }
            }
            Err(e) => {
                record.status = ExperimentStatus::Failed.as_str().to_string();
                record.error = Some(e.clone());
                record.result_summary = Some(format!("Experiment runner error: {}", e));
            }
        }

        record.artifact_ids_json = Some(serde_json::to_string(&artifact_ids).unwrap_or_default());

        // Update record in SQLite
        self.db
            .update_code_experiment(&record)
            .map_err(|e| format!("Database error updating experiment: {}", e))?;

        // Cleanup temporary workspace if needed (preserving report artifact if referenced)
        if let Some(parent) = isolated_workspace_parent {
            let _ = std::fs::remove_dir_all(parent.join("workspace"));
        }

        Ok(record)
    }

    /// Compares two experiments and highlights differences in status, exit code, and output.
    pub fn compare(
        a: &CodeExperimentRecord,
        b: &CodeExperimentRecord,
    ) -> ExperimentComparisonResult {
        let stdout_match = a.stdout_tail == b.stdout_tail;
        let exit_codes_match = a.exit_code == b.exit_code;

        let mut diff_desc = format!(
            "Comparison between '{}' and '{}':\n- Status: {} vs {}\n- Exit Code: {:?} vs {:?}\n- Runtime: {} ({}) vs {} ({})",
            a.name, b.name, a.status, b.status, a.exit_code, b.exit_code, a.runtime, a.image, b.runtime, b.image
        );

        if let (Some(sa), Some(sb)) = (&a.result_summary, &b.result_summary) {
            diff_desc.push_str(&format!("\n- Summary A: {}\n- Summary B: {}", sa, sb));
        }

        ExperimentComparisonResult {
            exp_a_id: a.id.clone(),
            exp_a_name: a.name.clone(),
            exp_a_status: a.status.clone(),
            exp_a_exit_code: a.exit_code,
            exp_b_id: b.id.clone(),
            exp_b_name: b.name.clone(),
            exp_b_status: b.status.clone(),
            exp_b_exit_code: b.exit_code,
            summary_diff: diff_desc,
            stdout_match,
            exit_codes_match,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_limits_sanitization() {
        let limits = ResourceLimits {
            cpu: 10.0,
            memory_mb: 8192,
            pids_limit: 1000,
            timeout_seconds: 1000,
            max_output_bytes: 1_000_000,
        };
        let sanitized = limits.sanitize();
        assert_eq!(sanitized.cpu, 2.0);
        assert_eq!(sanitized.memory_mb, 2048);
        assert_eq!(sanitized.pids_limit, 200);
        assert_eq!(sanitized.timeout_seconds, 300);
        assert_eq!(sanitized.max_output_bytes, 65536);
    }

    #[test]
    fn test_runtime_defaults_and_allowlist() {
        let py = ExperimentRuntime::Python;
        assert_eq!(py.default_image(), "python:3.12-alpine");
        assert!(is_approved_sandbox_image(py.default_image()));

        let node = ExperimentRuntime::Node;
        assert_eq!(node.default_image(), "node:20-alpine");
        assert!(is_approved_sandbox_image(node.default_image()));

        let rust = ExperimentRuntime::Rust;
        assert_eq!(rust.default_image(), "rust:1.80-alpine");
        assert!(is_approved_sandbox_image(rust.default_image()));

        let bash = ExperimentRuntime::Bash;
        assert_eq!(bash.default_image(), "alpine:3.20");
        assert!(is_approved_sandbox_image(bash.default_image()));

        assert!(!is_approved_sandbox_image("unapproved/malicious:latest"));
    }

    #[test]
    fn test_network_mode_default_none() {
        let mode = ExperimentNetworkMode::from_str_name("arbitrary");
        assert_eq!(mode, ExperimentNetworkMode::None);
        assert_eq!(mode.as_str(), "none");
    }

    #[tokio::test]
    async fn test_mock_runner_execution_and_cancellation() {
        let mock = MockContainerRunner::new().with_exit(0, "test stdout", "");
        let cancel_token = CancellationToken::new();
        let limits = ResourceLimits::default();

        let res = mock
            .run(
                "alpine:3.20",
                &["echo".to_string(), "hi".to_string()],
                &[],
                &limits,
                "none",
                Duration::from_secs(5),
                cancel_token.clone(),
            )
            .await
            .unwrap();

        assert_eq!(res.exit_code, 0);
        assert_eq!(res.stdout, "test stdout");
        assert!(!res.cancelled);
        assert!(!res.timed_out);

        // Test cancellation
        let cancel_token2 = CancellationToken::new();
        cancel_token2.cancel();
        let res2 = mock
            .run(
                "alpine:3.20",
                &["echo".to_string()],
                &[],
                &limits,
                "none",
                Duration::from_secs(5),
                cancel_token2,
            )
            .await
            .unwrap();

        assert!(res2.cancelled);
    }

    #[tokio::test]
    async fn test_mock_runner_fail_closed_and_unapproved_image() {
        let mock_fail = MockContainerRunner::new().with_fail_closed();
        let cancel_token = CancellationToken::new();
        let limits = ResourceLimits::default();

        let res = mock_fail
            .run(
                "alpine:3.20",
                &[],
                &[],
                &limits,
                "none",
                Duration::from_secs(5),
                cancel_token.clone(),
            )
            .await;
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Execution failed closed"));

        let mock = MockContainerRunner::new();
        let res_bad_img = mock
            .run(
                "evil-image:latest",
                &[],
                &[],
                &limits,
                "none",
                Duration::from_secs(5),
                cancel_token,
            )
            .await;
        assert!(res_bad_img.is_err());
    }
}
