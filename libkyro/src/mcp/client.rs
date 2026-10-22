use crate::agent::ToolDefinition;
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

pub struct McpClient {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
    request_id: u64,
    pub command: String,
    pub server_name: String,
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Child has kill_on_drop set, but explicitly ensure termination
        let _ = self.child.start_kill();
    }
}

impl McpClient {
    pub async fn spawn(
        server_name: &str,
        command: &str,
        args: &[String],
        env_vars: &HashMap<String, String>,
    ) -> Result<Self, String> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env_vars)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn MCP server '{}': {}", command, e))?;

        let stdin = child.stdin.take().ok_or("Failed to open child stdin")?;
        let stdout = child.stdout.take().ok_or("Failed to open child stdout")?;
        let reader = BufReader::new(stdout);

        let mut client = Self {
            child,
            stdin,
            reader,
            request_id: 1,
            command: command.to_string(),
            server_name: server_name.to_string(),
        };

        // Initialize handshake with timeout
        timeout(Duration::from_secs(15), client.initialize())
            .await
            .map_err(|_| {
                format!(
                    "MCP server '{}' timed out during initialization handshake",
                    server_name
                )
            })??;

        Ok(client)
    }

    /// Check if child process is still actively running
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    async fn send_request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let req_id = self.request_id;
        self.request_id += 1;

        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": method,
            "params": params
        });

        let mut line = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
        line.push('\n');

        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| format!("Failed to write to MCP stdin: {}", e))?;
        self.stdin.flush().await.map_err(|e| e.to_string())?;

        let mut response_line = String::new();
        self.reader
            .read_line(&mut response_line)
            .await
            .map_err(|e| format!("Failed to read from MCP stdout: {}", e))?;

        let resp: Value = serde_json::from_str(&response_line)
            .map_err(|e| format!("Failed to parse MCP JSON-RPC: {}", e))?;

        if let Some(err) = resp.get("error") {
            return Err(format!("MCP error from {}: {:?}", self.server_name, err));
        }

        Ok(resp.get("result").cloned().unwrap_or(Value::Null))
    }

    async fn initialize(&mut self) -> Result<(), String> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "roots": { "listChanged": true }
            },
            "clientInfo": {
                "name": "kyro-desktop",
                "version": "0.1.1"
            }
        });

        let _ = self.send_request("initialize", params).await?;

        // Send initialized notification
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        let mut line = serde_json::to_string(&notification).map_err(|e| e.to_string())?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).await.ok();
        self.stdin.flush().await.ok();

        Ok(())
    }

    pub async fn list_tools(&mut self) -> Result<Vec<ToolDefinition>, String> {
        let result = timeout(
            Duration::from_secs(10),
            self.send_request("tools/list", serde_json::json!({})),
        )
        .await
        .map_err(|_| "Timed out waiting for tools/list response from MCP server".to_string())??;

        let mut list = Vec::new();
        if let Some(tools) = result["tools"].as_array() {
            for t in tools {
                let name = t["name"].as_str().unwrap_or_default().to_string();
                let description = t["description"].as_str().unwrap_or_default().to_string();
                let schema = t["inputSchema"].clone();

                list.push(ToolDefinition {
                    name,
                    description,
                    parameters: schema,
                });
            }
        }

        Ok(list)
    }

    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<String, String> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments
        });

        let result = timeout(
            Duration::from_secs(30),
            self.send_request("tools/call", params),
        )
        .await
        .map_err(|_| format!("MCP tool '{}' timed out after 30 seconds", name))??;

        let mut text_out = if let Some(content) = result["content"].as_array() {
            let mut buf = String::new();
            for c in content {
                if let Some(t) = c["text"].as_str() {
                    buf.push_str(t);
                }
            }
            buf
        } else {
            result.to_string()
        };

        // Output size bounding (max 256KB)
        const MAX_OUTPUT_BYTES: usize = 256 * 1024;
        if text_out.len() > MAX_OUTPUT_BYTES {
            text_out.truncate(MAX_OUTPUT_BYTES);
            text_out.push_str("\n...[MCP Output truncated at 256KB limit]");
        }

        Ok(text_out)
    }
}
