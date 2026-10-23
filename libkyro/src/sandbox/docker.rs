use serde::{Deserialize, Serialize};
use std::process::Command;

/// Approved container images for sandbox execution.
///
/// Images are strictly pinned to versioned tags to avoid mutable tag drift and unexpected image downloads.
pub const APPROVED_SANDBOX_IMAGES: &[&str] = &[
    "alpine:3.20",
    "alpine:3.19",
    "python:3.12-alpine",
    "node:20-alpine",
    "rust:1.80-alpine",
    "debian:bookworm-slim",
];

/// Validates whether a container image is permitted by the security allowlist.
pub fn is_approved_sandbox_image(image: &str) -> bool {
    APPROVED_SANDBOX_IMAGES.contains(&image)
}

/// Normalizes an image tag, mapping legacy unpinned tags like `alpine:latest` to the pinned default `alpine:3.20`.
pub fn normalize_sandbox_image(image: &str) -> &str {
    if image == "alpine:latest" || image == "alpine" {
        "alpine:3.20"
    } else {
        image
    }
}

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
            message: "Neither Docker nor Podman is currently running. Sandboxed execution is unavailable and will fail closed.".to_string(),
        }
    }

    pub fn run_in_sandbox(image: &str, command: &str) -> Result<ContainerExecResult, String> {
        let probe = Self::probe();
        if !probe.available {
            return Err("Container sandbox engine is not available on this system. Execution failed closed.".to_string());
        }

        let normalized_image = normalize_sandbox_image(image);
        if !is_approved_sandbox_image(normalized_image) {
            return Err(format!(
                "Image '{}' is not in the approved sandbox allowlist: {:?}",
                image, APPROVED_SANDBOX_IMAGES
            ));
        }

        let binary = &probe.engine;
        let output = Command::new(binary)
            .args([
                "run",
                "--rm",
                "--network=none", // Isolated network by default for security
                "--memory=512m",
                "--cpus=1.0",
                "--pids-limit=100",
                "--security-opt=no-new-privileges",
                "--cap-drop=ALL",
                "--read-only",
                "--tmpfs=/tmp:rw,noexec,nosuid,size=64m",
                normalized_image,
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
