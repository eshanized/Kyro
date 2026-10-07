use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxStatus {
    pub available: bool,
    pub engine: String, // "docker", "podman", "none"
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub struct DockerSandbox;

impl DockerSandbox {
    pub fn probe() -> SandboxStatus {
        // Check for docker
        if let Ok(output) = Command::new("docker").arg("info").output() {
            if output.status.success() {
                return SandboxStatus {
                    available: true,
                    engine: "docker".to_string(),
                    message: "Docker daemon is active and responsive".to_string(),
                };
            }
        }

        // Check for podman
        if let Ok(output) = Command::new("podman").arg("info").output() {
            if output.status.success() {
                return SandboxStatus {
                    available: true,
                    engine: "podman".to_string(),
                    message: "Podman engine is active and responsive".to_string(),
                };
            }
        }

        SandboxStatus {
            available: false,
            engine: "none".to_string(),
            message: "Neither Docker nor Podman is currently running. Execution will fall back to local host with permission gate.".to_string(),
        }
    }

    pub fn run_in_sandbox(image: &str, command: &str) -> Result<ContainerExecResult, String> {
        let probe = Self::probe();
        if !probe.available {
            return Err("Container sandbox engine is not available on this system".to_string());
        }

        let binary = &probe.engine;
        let output = Command::new(binary)
            .args([
                "run",
                "--rm",
                "--network=none", // Isolated network by default for security
                "--memory=512m",
                "--cpus=1.0",
                image,
                "sh",
                "-c",
                command,
            ])
            .output()
            .map_err(|e| format!("Failed to execute in {}: {}", binary, e))?;

        Ok(ContainerExecResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}
