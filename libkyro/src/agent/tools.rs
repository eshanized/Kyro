//! Agent tools for project discovery, development-server management and
//! browser automation.
//!
//! The tools are ordinary Kyro tools: they use the same schema, the same
//! Rust dispatch and the same central authorisation engine as the existing
//! shell and file tools.  They are deliberately *not* a parallel agent loop.

use crate::agent::router::ToolDefinition;
use crate::browser::{BrowserAutomationManager, ElementTarget};
use crate::governance::{AuthDecision, ExecutionContext};
use crate::project::devserver::DevServerManager;
use crate::project::discovery::{
    detect_node_framework, read_package_json, DiscoveryLimits, ProjectKind,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

/// The tool definitions for the new subsystems.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "discover_project".to_string(),
            description: "Find a project directory by name under a user authorised search root. \
Returns every plausible match with its absolute path, detected kind, package manager and scripts. \
Use this before running anything; never guess a project path.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "The project directory name to look for (case-insensitive), e.g. 'Runstead'" },
                    "search_root": { "type": "string", "description": "Absolute directory to search. Defaults to the user's Desktop." }
                },
                "required": ["name"]
            }),
        },
        ToolDefinition {
            name: "inspect_project".to_string(),
            description: "Inspect a discovered project: its manifest, scripts, lockfile, detected framework \
and the exact development command Kyro would run.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path of the project root" }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "start_dev_server".to_string(),
            description: "Start a project's development server as a managed, owned process. \
The command is derived from the project's manifest and lockfile; the working directory is the project root. \
Requires user approval because it executes repository-controlled code.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path of the project root" }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "dev_server_status".to_string(),
            description: "Read the status of a managed development server: readiness, verified URL, port, exit code.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "server_id": { "type": "string", "description": "The identifier returned by start_dev_server" }
                },
                "required": ["server_id"]
            }),
        },
        ToolDefinition {
            name: "dev_server_logs".to_string(),
            description: "Read a bounded tail of the managed development server's output. Read-only.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "server_id": { "type": "string", "description": "The identifier returned by start_dev_server" },
                    "max_lines": { "type": "integer", "description": "How many lines to return (default 100, max 500)" }
                },
                "required": ["server_id"]
            }),
        },
        ToolDefinition {
            name: "stop_dev_server".to_string(),
            description: "Stop a managed development server and its process tree. Only affects servers Kyro started.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "server_id": { "type": "string", "description": "The identifier returned by start_dev_server" }
                },
                "required": ["server_id"]
            }),
        },
        ToolDefinition {
            name: "browser_capabilities".to_string(),
            description: "List the browsers Kyro can drive on this machine, with their paths and versions, \
and the automation driver that will be used. Call this before opening a browser session.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "browser_open".to_string(),
            description: "Open a managed browser session with an isolated profile. \
Returns the session id that every other browser tool needs. \
Supports installed and automatable browsers such as Firefox, Chrome, Chromium, Edge, Brave. \
Kyro never silently substitutes another browser.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "browser": { "type": "string", "description": "Browser to drive (e.g. 'auto', 'firefox', 'chrome', 'chromium', 'brave', 'edge'). Defaults to Kyro setting or system default." },
                    "headless": { "type": "boolean", "description": "Run the browser without a visible window (default true)" }
                }
            }),
        },
        ToolDefinition {
            name: "browser_navigate".to_string(),
            description: "Navigate the browser session to a URL. The URL is validated against the session's \
navigation policy; blocked domains and destinations outside the application origin are refused.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "The browser session id" },
                    "url": { "type": "string", "description": "The absolute URL to open" }
                },
                "required": ["session_id", "url"]
            }),
        },
        ToolDefinition {
            name: "browser_snapshot".to_string(),
            description: "Read a semantic snapshot of the current page: the interactive elements with their role, \
accessible name and reference, the headings, the links and the visible page text. \
Use the element references and names to drive the other browser tools.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "The browser session id" }
                },
                "required": ["session_id"]
            }),
        },
        ToolDefinition {
            name: "browser_click".to_string(),
            description: "Click an element identified by its accessible name, role, text or a CSS selector. \
Never guess screen coordinates; use the semantic locator.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "The browser session id" },
                    "role": { "type": "string", "description": "The ARIA role, e.g. 'button' or 'link'" },
                    "name": { "type": "string", "description": "Part of the accessible name of the element" },
                    "text": { "type": "string", "description": "Part of the visible text of the element" },
                    "tag": { "type": "string", "description": "The HTML tag, e.g. 'button'" },
                    "test_id": { "type": "string", "description": "A data-testid value" },
                    "css": { "type": "string", "description": "A CSS selector (last resort)" },
                    "nth": { "type": "integer", "description": "Which match to use when several elements match" }
                },
                "required": ["session_id"]
            }),
        },
        ToolDefinition {
            name: "browser_fill".to_string(),
            description: "Type a value into a form field identified by its accessible name, role, label or selector.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "The browser session id" },
                    "value": { "type": "string", "description": "The text to enter" },
                    "role": { "type": "string", "description": "The ARIA role, e.g. 'textbox'" },
                    "name": { "type": "string", "description": "Part of the accessible name of the field" },
                    "text": { "type": "string", "description": "Part of the visible text of the field" },
                    "tag": { "type": "string", "description": "The HTML tag, e.g. 'input'" },
                    "test_id": { "type": "string", "description": "A data-testid value" },
                    "css": { "type": "string", "description": "A CSS selector (last resort)" }
                },
                "required": ["session_id", "value"]
            }),
        },
        ToolDefinition {
            name: "browser_press".to_string(),
            description: "Press a key in the browser session, for example 'Enter', 'Tab', 'Escape' or 'ArrowDown'.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "The browser session id" },
                    "key": { "type": "string", "description": "The key to press" }
                },
                "required": ["session_id", "key"]
            }),
        },
        ToolDefinition {
            name: "browser_screenshot".to_string(),
            description: "Capture a screenshot of the current page.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "The browser session id" },
                    "full_page": { "type": "boolean", "description": "Capture the whole document instead of the viewport" }
                },
                "required": ["session_id"]
            }),
        },
        ToolDefinition {
            name: "browser_console".to_string(),
            description: "Read the browser console output of the current page, including uncaught errors.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "The browser session id" }
                },
                "required": ["session_id"]
            }),
        },
        ToolDefinition {
            name: "browser_network".to_string(),
            description: "Read the network activity of the current page, including failed requests and error responses.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "The browser session id" }
                },
                "required": ["session_id"]
            }),
        },
        ToolDefinition {
            name: "browser_tabs".to_string(),
            description: "List the open tabs of the browser session.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "The browser session id" }
                },
                "required": ["session_id"]
            }),
        },
        ToolDefinition {
            name: "browser_close".to_string(),
            description: "Close the browser session and release the browser process.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "The browser session id" }
                },
                "required": ["session_id"]
            }),
        },
        ToolDefinition {
            name: "audit_ui".to_string(),
            description: "Run the complete UI audit workflow: discover the project, ask for approval to start its \
development server, verify the ready URL, open it in the configured or requested browser, discover every route, inspect each one, \
exercise safe controls and produce a Markdown bug report with evidence.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "project_name": { "type": "string", "description": "The project directory name to find (case-insensitive)" },
                    "search_root": { "type": "string", "description": "Absolute directory to search. Defaults to the user's Desktop." },
                    "browser": { "type": "string", "description": "Browser to use (e.g. 'auto', 'firefox', 'chrome', 'chromium', 'brave'). Defaults to 'auto'." },
                    "keep_server_running": { "type": "boolean", "description": "Leave the development server running after the audit" }
                },
                "required": ["project_name"]
            }),
        },
    ]
}

/// The outcome of a tool call: a message for the model, whether it failed and
/// optional visual data.
pub struct ToolOutcome {
    pub message: String,
    pub is_error: bool,
    pub visual: Option<String>,
}

impl ToolOutcome {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            is_error: false,
            visual: None,
        }
    }

    pub fn fail(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            is_error: true,
            visual: None,
        }
    }

    pub fn with_visual(mut self, visual: Option<String>) -> Self {
        self.visual = visual;
        self
    }
}

/// Extracts the search root, defaulting to the user's Desktop.
pub fn resolve_search_root(args: &serde_json::Value) -> Result<PathBuf, String> {
    if let Some(root) = args["search_root"].as_str() {
        if !root.trim().is_empty() {
            let path = PathBuf::from(root.trim());
            if !path.is_absolute() {
                return Err(format!(
                    "The search root '{}' must be an absolute path.",
                    root
                ));
            }
            return Ok(path);
        }
    }
    crate::project::discovery::desktop_directory().ok_or_else(|| {
        "The user's Desktop directory could not be resolved on this system. \
Provide an explicit 'search_root' path."
            .to_string()
    })
}

/// Handles the `discover_project` tool.
pub async fn discover_project(args: &serde_json::Value) -> ToolOutcome {
    let name = args["name"].as_str().unwrap_or_default().trim().to_string();
    if name.is_empty() {
        return ToolOutcome::fail("discover_project requires a 'name' parameter.");
    }
    let root = match resolve_search_root(args) {
        Ok(r) => r,
        Err(e) => return ToolOutcome::fail(e),
    };

    match crate::project::discovery::discover_projects(&root, &name, &DiscoveryLimits::default()) {
        Ok(result) => {
            if result.candidates.is_empty() {
                return ToolOutcome::fail(format!(
                    "{}. Provide an explicit path or a different search_root.",
                    result.summary
                ));
            }
            let payload = serde_json::json!({
                "status": "success",
                "searched_root": result.searched_root,
                "summary": result.summary,
                "truncated": result.truncated,
                "visited_entries": result.visited_entries,
                "candidates": result.candidates,
            });
            ToolOutcome::ok(payload.to_string())
        }
        Err(e) => ToolOutcome::fail(format!("Project discovery failed: {}", e)),
    }
}

/// Handles the `inspect_project` tool.
pub async fn inspect_project(args: &serde_json::Value) -> ToolOutcome {
    let path = args["path"].as_str().unwrap_or_default().trim();
    if path.is_empty() {
        return ToolOutcome::fail("inspect_project requires a 'path' parameter.");
    }
    let root = PathBuf::from(path);
    if !root.is_absolute() {
        return ToolOutcome::fail("The project path must be absolute.");
    }
    if !root.is_dir() {
        return ToolOutcome::fail(format!(
            "The project path '{}' does not exist or is not a directory.",
            root.display()
        ));
    }

    let candidate = match crate::project::discovery::inspect_project_root(&root, true) {
        Some(c) => c,
        None => {
            return ToolOutcome::fail(format!(
                "'{}' does not look like a project root: no manifest, lockfile or Git metadata was found.",
                root.display()
            ))
        }
    };

    let framework = candidate
        .manifest
        .as_ref()
        .and_then(|m| detect_node_framework(m));
    let dev_command = crate::project::devserver::DevServerManager::choose_command(&candidate)
        .ok()
        .map(|c| {
            serde_json::json!({
                "command": c.display(),
                "executable": c.executable,
                "args": c.args,
                "working_directory": c.working_directory,
                "script": c.script,
                "description": c.description,
            })
        });

    let package_json = candidate
        .manifest
        .as_ref()
        .and_then(|m| read_package_json(m))
        .map(|v| {
            serde_json::json!({
                "name": v.get("name"),
                "version": v.get("version"),
                "scripts": v.get("scripts"),
                "dependencies": v.get("dependencies").map(|d| {
                    d.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>())
                }),
            })
        });

    let payload = serde_json::json!({
        "status": "success",
        "project": candidate,
        "framework": framework,
        "package_json": package_json,
        "dev_command": dev_command,
    });
    ToolOutcome::ok(payload.to_string())
}

/// Handles the `start_dev_server` tool.
pub async fn start_dev_server(
    args: &serde_json::Value,
    manager: &Arc<DevServerManager>,
    run_id: &str,
) -> ToolOutcome {
    let path = args["path"].as_str().unwrap_or_default().trim();
    if path.is_empty() {
        return ToolOutcome::fail("start_dev_server requires a 'path' parameter.");
    }
    let root = PathBuf::from(path);
    let candidate = match crate::project::discovery::inspect_project_root(&root, true) {
        Some(c) => c,
        None => return ToolOutcome::fail(format!("'{}' is not a project root.", root.display())),
    };
    let command = match crate::project::devserver::DevServerManager::choose_command(&candidate) {
        Ok(c) => c,
        Err(e) => return ToolOutcome::fail(e),
    };

    match manager.start(&command, Some(run_id), 120000).await {
        Ok(state) => ToolOutcome::ok(
            serde_json::json!({
                "status": "success",
                "server_id": state.id,
                "pid": state.pid,
                "command": state.command,
                "working_directory": state.working_directory,
                "ready_url": state.ready_url,
                "port": state.port,
                "framework": state.framework,
                "status_state": state.status.as_str(),
            })
            .to_string(),
        ),
        Err(e) => ToolOutcome::fail(format!(
            "The development server did not become ready: {}",
            e
        )),
    }
}

/// Handles the `dev_server_status` tool.
pub async fn dev_server_status(
    args: &serde_json::Value,
    manager: &Arc<DevServerManager>,
) -> ToolOutcome {
    let id = args["server_id"].as_str().unwrap_or_default().trim();
    match manager.state(id) {
        Some(state) => ToolOutcome::ok(
            serde_json::json!({
                "status": "success",
                "server": state,
            })
            .to_string(),
        ),
        None => ToolOutcome::fail(format!("Development server '{}' does not exist.", id)),
    }
}

/// Handles the `dev_server_logs` tool.
pub async fn dev_server_logs(
    args: &serde_json::Value,
    manager: &Arc<DevServerManager>,
) -> ToolOutcome {
    let id = args["server_id"].as_str().unwrap_or_default().trim();
    let max_lines = args["max_lines"].as_u64().unwrap_or(100).clamp(1, 500) as usize;
    match manager.logs(id, max_lines) {
        Some(lines) => ToolOutcome::ok(
            serde_json::json!({
                "status": "success",
                "server_id": id,
                "lines": lines,
            })
            .to_string(),
        ),
        None => ToolOutcome::fail(format!("Development server '{}' does not exist.", id)),
    }
}

/// Handles the `stop_dev_server` tool.
pub async fn stop_dev_server(
    args: &serde_json::Value,
    manager: &Arc<DevServerManager>,
) -> ToolOutcome {
    let id = args["server_id"].as_str().unwrap_or_default().trim();
    match manager.stop(id) {
        Ok(()) => ToolOutcome::ok(
            serde_json::json!({ "status": "success", "server_id": id, "stopped": true })
                .to_string(),
        ),
        Err(e) => ToolOutcome::fail(e),
    }
}

/// Handles the `browser_capabilities` tool.
pub async fn browser_capabilities(manager: &Arc<BrowserAutomationManager>) -> ToolOutcome {
    match manager.capabilities().await {
        Ok(caps) => ToolOutcome::ok(
            serde_json::json!({ "status": "success", "capabilities": caps }).to_string(),
        ),
        Err(e) => ToolOutcome::fail(format!("Could not determine browser capabilities: {}", e)),
    }
}

/// Handles the `browser_open` tool.
pub async fn browser_open(
    args: &serde_json::Value,
    manager: &Arc<BrowserAutomationManager>,
    run_id: &str,
    blocked_domains: &[String],
) -> ToolOutcome {
    let browser = args["browser"]
        .as_str()
        .unwrap_or("auto")
        .trim()
        .to_string();
    let headless = args["headless"].as_bool().unwrap_or(true);

    match manager
        .create_session(&browser, headless, Some(run_id), blocked_domains)
        .await
    {
        Ok(session) => ToolOutcome::ok(
            serde_json::json!({ "status": "success", "session": session }).to_string(),
        ),
        Err(e) => ToolOutcome::fail(format!("Could not open a browser session: {}", e)),
    }
}

/// Builds an element target from the tool arguments.
pub fn target_from_args(args: &serde_json::Value) -> Result<ElementTarget, String> {
    let target = ElementTarget {
        role: args["role"].as_str().map(|s| s.to_string()),
        name: args["name"].as_str().map(|s| s.to_string()),
        text: args["text"].as_str().map(|s| s.to_string()),
        tag: args["tag"].as_str().map(|s| s.to_string()),
        test_id: args["test_id"].as_str().map(|s| s.to_string()),
        css: args["css"].as_str().map(|s| s.to_string()),
        nth: args["nth"].as_u64().map(|n| n as usize),
    };
    target.to_params()?;
    Ok(target)
}

/// Handles the `browser_navigate` tool.
pub async fn browser_navigate(
    args: &serde_json::Value,
    manager: &Arc<BrowserAutomationManager>,
) -> ToolOutcome {
    let session_id = args["session_id"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    let url = args["url"].as_str().unwrap_or_default().trim().to_string();
    if session_id.is_empty() {
        return ToolOutcome::fail("browser_navigate requires a 'session_id' parameter.");
    }
    if url.is_empty() {
        return ToolOutcome::fail("browser_navigate requires a 'url' parameter.");
    }

    let result = match manager.navigate(&session_id, &url, 60000, false).await {
        Ok(r) => r,
        Err(e) => return ToolOutcome::fail(format!("Navigation failed: {}", e)),
    };
    ToolOutcome::ok(
        serde_json::json!({
            "status": result.status,
            "session_id": result.session_id,
            "url": result.url,
            "title": result.title,
            "navigation_status": result.navigation_status,
            "duration_ms": result.duration_ms,
            "error_category": result.error_category,
            "error_message": result.error_message,
        })
        .to_string(),
    )
}

/// Handles the `browser_snapshot` tool.
pub async fn browser_snapshot(
    args: &serde_json::Value,
    manager: &Arc<BrowserAutomationManager>,
) -> ToolOutcome {
    let session_id = args["session_id"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    let result = match manager.snapshot(&session_id).await {
        Ok(r) => r,
        Err(e) => return ToolOutcome::fail(format!("Could not read the page: {}", e)),
    };
    ToolOutcome::ok(
        serde_json::json!({
            "status": result.status,
            "session_id": result.session_id,
            "url": result.url,
            "title": result.title,
            "error_category": result.error_category,
            "error_message": result.error_message,
            "page": result.data,
        })
        .to_string(),
    )
}

/// Handles the `browser_click` tool.
pub async fn browser_click(
    args: &serde_json::Value,
    manager: &Arc<BrowserAutomationManager>,
) -> ToolOutcome {
    let session_id = args["session_id"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    let target = match target_from_args(args) {
        Ok(t) => t,
        Err(e) => return ToolOutcome::fail(e),
    };
    let params = match target.to_params() {
        Ok(p) => p,
        Err(e) => return ToolOutcome::fail(e),
    };
    let result = match manager.click(&session_id, params, 10000).await {
        Ok(r) => r,
        Err(e) => return ToolOutcome::fail(format!("Click failed: {}", e)),
    };
    ToolOutcome::ok(
        serde_json::json!({
            "status": result.status,
            "session_id": result.session_id,
            "url": result.url,
            "title": result.title,
            "duration_ms": result.duration_ms,
            "error_category": result.error_category,
            "error_message": result.error_message,
            "clicked": result.data,
        })
        .to_string(),
    )
}

/// Handles the `browser_fill` tool.
pub async fn browser_fill(
    args: &serde_json::Value,
    manager: &Arc<BrowserAutomationManager>,
) -> ToolOutcome {
    let session_id = args["session_id"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    let value = args["value"].as_str().unwrap_or_default().to_string();
    let target = match target_from_args(args) {
        Ok(t) => t,
        Err(e) => return ToolOutcome::fail(e),
    };
    let params = match target.to_params() {
        Ok(p) => p,
        Err(e) => return ToolOutcome::fail(e),
    };
    let result = match manager.fill(&session_id, params, value, 10000).await {
        Ok(r) => r,
        Err(e) => return ToolOutcome::fail(format!("Fill failed: {}", e)),
    };
    ToolOutcome::ok(
        serde_json::json!({
            "status": result.status,
            "session_id": result.session_id,
            "duration_ms": result.duration_ms,
            "error_category": result.error_category,
            "error_message": result.error_message,
            "filled": result.data,
        })
        .to_string(),
    )
}

/// Handles the `browser_press` tool.
pub async fn browser_press(
    args: &serde_json::Value,
    manager: &Arc<BrowserAutomationManager>,
) -> ToolOutcome {
    let session_id = args["session_id"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    let key = args["key"].as_str().unwrap_or_default().trim().to_string();
    if key.is_empty() {
        return ToolOutcome::fail("browser_press requires a 'key' parameter.");
    }
    let result = match manager.press(&session_id, key, 10000).await {
        Ok(r) => r,
        Err(e) => return ToolOutcome::fail(format!("Key press failed: {}", e)),
    };
    ToolOutcome::ok(
        serde_json::json!({
            "status": result.status,
            "session_id": result.session_id,
            "url": result.url,
            "title": result.title,
            "error_category": result.error_category,
            "error_message": result.error_message,
        })
        .to_string(),
    )
}

/// Handles the `browser_screenshot` tool.
pub async fn browser_screenshot(
    args: &serde_json::Value,
    manager: &Arc<BrowserAutomationManager>,
) -> ToolOutcome {
    let session_id = args["session_id"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    let full_page = args["full_page"].as_bool().unwrap_or(false);
    let result = match manager.screenshot(&session_id, full_page).await {
        Ok(r) => r,
        Err(e) => return ToolOutcome::fail(format!("Screenshot failed: {}", e)),
    };
    if result.status != "success" {
        return ToolOutcome::fail(format!(
            "Screenshot failed: {}",
            result.error_message.unwrap_or_default()
        ));
    }
    let png = result
        .data
        .get("base64_png")
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_string();
    let summary = serde_json::json!({
        "status": "success",
        "session_id": result.session_id,
        "bytes": png.len(),
    });
    ToolOutcome::ok(summary.to_string()).with_visual(Some(png))
}

/// Handles the `browser_console` tool.
pub async fn browser_console(
    args: &serde_json::Value,
    manager: &Arc<BrowserAutomationManager>,
) -> ToolOutcome {
    let session_id = args["session_id"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    let result = match manager.console(&session_id).await {
        Ok(r) => r,
        Err(e) => return ToolOutcome::fail(format!("Could not read the console: {}", e)),
    };
    ToolOutcome::ok(
        serde_json::json!({
            "status": result.status,
            "session_id": result.session_id,
            "console_error_count": result.console_error_count,
            "console": result.data,
        })
        .to_string(),
    )
}

/// Handles the `browser_network` tool.
pub async fn browser_network(
    args: &serde_json::Value,
    manager: &Arc<BrowserAutomationManager>,
) -> ToolOutcome {
    let session_id = args["session_id"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    let result = match manager.network(&session_id).await {
        Ok(r) => r,
        Err(e) => return ToolOutcome::fail(format!("Could not read the network log: {}", e)),
    };
    ToolOutcome::ok(
        serde_json::json!({
            "status": result.status,
            "session_id": result.session_id,
            "failed_request_count": result.failed_request_count,
            "network": result.data,
        })
        .to_string(),
    )
}

/// Handles the `browser_tabs` tool.
pub async fn browser_tabs(
    args: &serde_json::Value,
    manager: &Arc<BrowserAutomationManager>,
) -> ToolOutcome {
    let session_id = args["session_id"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    let result = match manager.list_tabs(&session_id).await {
        Ok(r) => r,
        Err(e) => return ToolOutcome::fail(format!("Could not list the tabs: {}", e)),
    };
    ToolOutcome::ok(
        serde_json::json!({
            "status": result.status,
            "session_id": result.session_id,
            "tabs": result.data,
        })
        .to_string(),
    )
}

/// Handles the `browser_close` tool.
pub async fn browser_close(
    args: &serde_json::Value,
    manager: &Arc<BrowserAutomationManager>,
) -> ToolOutcome {
    let session_id = args["session_id"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_string();
    match manager.close_session(&session_id).await {
        Ok(()) => ToolOutcome::ok(
            serde_json::json!({ "status": "success", "session_id": session_id, "closed": true })
                .to_string(),
        ),
        Err(e) => ToolOutcome::fail(e),
    }
}

/// The authorisation decisions for the new tools.
pub fn evaluate_tool(
    policy: &crate::governance::GovernancePolicy,
    ctx: &ExecutionContext,
    tool_name: &str,
    args: &serde_json::Value,
) -> Option<AuthDecision> {
    match tool_name {
        "start_dev_server" | "audit_ui" => {
            let target = args["path"].as_str().unwrap_or("");
            Some(AuthDecision::RequireApproval {
                reason:
                    "Starting a project's development server executes repository-controlled code"
                        .to_string(),
                risk: crate::execution::permissions::RiskLevel::Moderate,
                summary: format!(
                    "Start the development server of the project at '{}'",
                    if target.is_empty() {
                        "(discovered project)".to_string()
                    } else {
                        target.to_string()
                    }
                ),
            })
        }
        "browser_navigate" => {
            let url = args["url"].as_str().unwrap_or("");
            if url.trim().is_empty() {
                return Some(AuthDecision::Deny {
                    reason: "browser_navigate requires a 'url' parameter".to_string(),
                });
            }
            for domain in &policy.blocked_domains {
                if url.to_lowercase().contains(&domain.to_lowercase()) {
                    return Some(AuthDecision::Deny {
                        reason: format!(
                            "The requested URL targets the blocked domain '{}'",
                            domain
                        ),
                    });
                }
            }
            None
        }
        "browser_click" => {
            // Activating a control that could commit a destructive or
            // transactional action requires explicit confirmation.
            const CONSEQUENTIAL: &[&str] = &[
                "delete",
                "remove",
                "purchase",
                "buy",
                "pay",
                "checkout",
                "send",
                "publish",
                "deploy",
                "confirm",
                "submit",
                "transfer",
                "upgrade",
                "subscribe",
                "invite",
                "sign out",
                "log out",
                "logout",
            ];
            let label = format!(
                "{} {}",
                args["name"].as_str().unwrap_or(""),
                args["text"].as_str().unwrap_or("")
            )
            .to_lowercase();
            if CONSEQUENTIAL.iter().any(|c| label.contains(c)) {
                return Some(AuthDecision::RequireApproval {
                    reason: format!(
                        "Activating the control '{}' may perform a consequential action",
                        label.trim()
                    ),
                    risk: crate::execution::permissions::RiskLevel::Moderate,
                    summary: format!("Activate the control: {}", label.trim()),
                });
            }
            None
        }
        "browser_fill" => {
            let value = args["value"].as_str().unwrap_or("");
            if let Some(reason) =
                crate::governance::TakeoverController::detect_sensitive_checkpoint(value)
            {
                return Some(AuthDecision::RequireTakeover { reason });
            }
            None
        }
        "browser_open" => {
            None
        }
        "stop_dev_server" | "browser_close" => {
            let _ = ctx;
            None
        }
        _ => None,
    }
}

/// A summary of the project that the audit tool is about to work with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditPlanSummary {
    pub project_path: String,
    pub command: String,
    pub working_directory: String,
    pub framework: Option<String>,
    pub kind: ProjectKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unsupported_browser_is_refused_without_silent_substitution() {
        let args = serde_json::json!({ "browser": "nonexistent_custom_browser" });
        let manager = Arc::new(BrowserAutomationManager::new());
        let outcome = browser_open(&args, &manager, "run_1", &[]).await;
        assert!(outcome.is_error);
        assert!(outcome.message.contains("will not silently substitute"));
    }

    #[test]
    fn a_click_on_a_consequential_control_requires_approval() {
        let policy = crate::governance::GovernancePolicy::default();
        let ctx = ExecutionContext {
            run_id: "r".into(),
            agent_id: "a".into(),
            conversation_id: None,
            workspace_root: None,
        };
        let args = serde_json::json!({ "name": "Delete account" });
        let decision = evaluate_tool(&policy, &ctx, "browser_click", &args).unwrap();
        assert!(matches!(decision, AuthDecision::RequireApproval { .. }));
    }

    #[test]
    fn an_ordinary_click_is_not_gated() {
        let policy = crate::governance::GovernancePolicy::default();
        let ctx = ExecutionContext {
            run_id: "r".into(),
            agent_id: "a".into(),
            conversation_id: None,
            workspace_root: None,
        };
        let args = serde_json::json!({ "name": "Open menu" });
        assert!(evaluate_tool(&policy, &ctx, "browser_click", &args).is_none());
    }

    #[test]
    fn a_blocked_domain_cannot_be_navigated_to() {
        let policy = crate::governance::GovernancePolicy::default();
        let ctx = ExecutionContext {
            run_id: "r".into(),
            agent_id: "a".into(),
            conversation_id: None,
            workspace_root: None,
        };
        let args = serde_json::json!({ "url": "https://paypal.com/checkout" });
        let decision = evaluate_tool(&policy, &ctx, "browser_navigate", &args).unwrap();
        assert!(matches!(decision, AuthDecision::Deny { .. }));
    }

    #[test]
    fn filling_in_a_password_requires_takeover() {
        let policy = crate::governance::GovernancePolicy::default();
        let ctx = ExecutionContext {
            run_id: "r".into(),
            agent_id: "a".into(),
            conversation_id: None,
            workspace_root: None,
        };
        let args = serde_json::json!({ "value": "my credit card 4111 1111 1111 1111" });
        let decision = evaluate_tool(&policy, &ctx, "browser_fill", &args).unwrap();
        assert!(matches!(decision, AuthDecision::RequireTakeover { .. }));
    }

    /// A tiny blocking executor so the tests do not need a full runtime.
    #[allow(dead_code)]
    fn futures_block_on<F: std::future::Future>(fut: F) -> F::Output {
        use std::task::{Context, Poll};
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(waker);
        let mut fut = Box::pin(fut);
        loop {
            match fut.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }
}
