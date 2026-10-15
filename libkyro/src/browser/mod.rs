//! Kyro's browser automation subsystem.
//!
//! The subsystem owns a *managed* browser session: a dedicated browser
//! process with an isolated profile, driven over WebDriver BiDi (Firefox family)
//! or Chrome DevTools Protocol (Chromium family) by a Node.js
//! worker that Kyro starts and owns. The manager never exposes arbitrary
//! JavaScript evaluation to the model; every operation is a fixed, typed
//! command with a structured result.

pub mod safety;
pub mod types;
pub mod worker;

pub use safety::{NavigationPolicy, UrlRejection, ValidatedUrl};
pub use types::*;
pub use worker::{WorkerError, WorkerHandle};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A managed browser session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSession {
    pub session_id: String,
    pub browser: String,
    pub browser_path: String,
    pub browser_version: Option<String>,
    pub headless: bool,
    pub profile_isolated: bool,
    pub driver: String,
    /// The origin the session is scoped to, when one was established.
    pub origin_scope: Option<String>,
    /// The run that owns this session.
    pub run_id: Option<String>,
    pub created_at: i64,
}

/// The outcome of a browser operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserOperationResult {
    /// The browser session the operation ran in.
    pub session_id: String,
    /// The run that owns the session.
    pub run_id: Option<String>,
    /// The operation that was performed.
    pub operation: String,
    /// `success` or `failed`.
    pub status: String,
    /// The current URL after the operation.
    pub url: Option<String>,
    /// The page title after the operation.
    pub title: Option<String>,
    /// Error category when the operation failed.
    pub error_category: Option<String>,
    /// Error message when the operation failed.
    pub error_message: Option<String>,
    /// How long the operation took.
    pub duration_ms: u64,
    /// Whether the page finished loading.
    pub navigation_status: Option<String>,
    /// Number of console errors observed so far.
    pub console_error_count: u32,
    /// Number of failed network requests observed so far.
    pub failed_request_count: u32,
    /// Operation specific payload.
    #[serde(default)]
    pub data: serde_json::Value,
}

impl BrowserOperationResult {
    fn base(session_id: &str, run_id: Option<&str>, operation: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            run_id: run_id.map(|s| s.to_string()),
            operation: operation.to_string(),
            status: "success".to_string(),
            url: None,
            title: None,
            error_category: None,
            error_message: None,
            duration_ms: 0,
            navigation_status: None,
            console_error_count: 0,
            failed_request_count: 0,
            data: serde_json::Value::Null,
        }
    }

    fn failed(
        session_id: &str,
        run_id: Option<&str>,
        operation: &str,
        category: &str,
        message: &str,
    ) -> Self {
        let mut r = Self::base(session_id, run_id, operation);
        r.status = "failed".to_string();
        r.error_category = Some(category.to_string());
        r.error_message = Some(message.to_string());
        r
    }
}

/// Capabilities reported by the browser layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserCapabilities {
    pub browsers: Vec<BrowserInfo>,
    #[serde(default)]
    pub default_browser_id: Option<String>,
    pub driver: DriverInfo,
    pub node_version: Option<String>,
    #[serde(default)]
    pub platform: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserInfo {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub family: String,
    pub path: String,
    pub version: Option<String>,
    pub available: bool,
    pub automatable: bool,
    pub adapter: String,
    #[serde(default)]
    pub unavailable_reason: Option<String>,
    #[serde(default)]
    pub is_default: bool,
}

pub type BrowserBinary = BrowserInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverInfo {
    pub name: String,
    pub version: String,
    pub external_driver_required: bool,
}

/// Browser automation preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSettings {
    pub selected_browser: String,
    pub headless_default: bool,
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            selected_browser: "auto".to_string(),
            headless_default: true,
        }
    }
}

impl BrowserSettings {
    pub fn load(db: &crate::storage::Database) -> Self {
        if let Ok(Some(json)) = db.get_setting("browser_settings") {
            if let Ok(settings) = serde_json::from_str::<BrowserSettings>(&json) {
                return settings;
            }
        }
        Self::default()
    }

    pub fn save(&self, db: &crate::storage::Database) -> Result<(), String> {
        let json = serde_json::to_string(self).map_err(|e| e.to_string())?;
        db.set_setting("browser_settings", &json)
            .map_err(|e| e.to_string())
    }
}

/// The manager owns the worker process and the set of live sessions.
pub struct BrowserAutomationManager {
    worker: Arc<Mutex<Option<WorkerHandle>>>,
    sessions: Arc<Mutex<HashMap<String, BrowserSession>>>,
    policies: Arc<Mutex<HashMap<String, NavigationPolicy>>>,
    db: Arc<Mutex<Option<crate::storage::Database>>>,
}

impl Default for BrowserAutomationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserAutomationManager {
    pub fn new() -> Self {
        Self {
            worker: Arc::new(Mutex::new(None)),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            policies: Arc::new(Mutex::new(HashMap::new())),
            db: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_database(self, db: crate::storage::Database) -> Self {
        self.set_database(db);
        self
    }

    pub fn set_database(&self, db: crate::storage::Database) {
        if let Ok(mut guard) = self.db.try_lock() {
            *guard = Some(db);
        }
    }

    /// Ensures that a worker process is running.
    async fn ensure_worker(&self) -> Result<(), WorkerError> {
        let mut guard = self.worker.lock().await;
        if guard.is_some() {
            return Ok(());
        }
        let handle = WorkerHandle::start().await?;
        *guard = Some(handle);
        Ok(())
    }

    /// Executes a worker operation.
    async fn raw_call(
        &self,
        op: &str,
        params: serde_json::Value,
        timeout_ms: u64,
    ) -> Result<serde_json::Value, WorkerError> {
        self.ensure_worker().await?;
        let mut guard = self.worker.lock().await;
        match guard.as_mut() {
            Some(handle) => handle.call(op, params, timeout_ms).await,
            None => Err(WorkerError {
                category: "worker_unavailable".into(),
                message: "The browser worker is not running".into(),
            }),
        }
    }

    /// Lists the browsers that Kyro can drive.
    pub async fn capabilities(&self) -> Result<BrowserCapabilities, String> {
        let raw = self
            .raw_call("capabilities", serde_json::json!({}), 30000)
            .await
            .map_err(|e| e.to_string())?;

        let browsers = raw
            .get("browsers")
            .and_then(|b| b.as_array())
            .map(|items| {
                items
                    .iter()
                    .map(|b| {
                        let id = b.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let name = b.get("name").and_then(|v| v.as_str()).unwrap_or(&id).to_string();
                        let display_name = b.get("display_name").and_then(|v| v.as_str()).unwrap_or(&name).to_string();
                        let family = b.get("family").and_then(|v| v.as_str()).unwrap_or("other").to_string();
                        let path = b.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let version = b.get("version").and_then(|v| v.as_str()).map(|s| s.to_string());
                        let available = b.get("available").and_then(|v| v.as_bool()).unwrap_or(false);
                        let automatable = b.get("automatable").and_then(|v| v.as_bool()).unwrap_or(available);
                        let adapter = b.get("adapter").and_then(|v| v.as_str()).unwrap_or("none").to_string();
                        let unavailable_reason = b.get("unavailable_reason").and_then(|v| v.as_str()).map(|s| s.to_string());
                        let is_default = b.get("is_default").and_then(|v| v.as_bool()).unwrap_or(false);
                        BrowserInfo {
                            id,
                            name,
                            display_name,
                            family,
                            path,
                            version,
                            available,
                            automatable,
                            adapter,
                            unavailable_reason,
                            is_default,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let default_browser_id = raw
            .get("default_browser_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let driver = raw
            .get("driver")
            .map(|d| DriverInfo {
                name: d
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                version: d
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                external_driver_required: d
                    .get("external_driver_required")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            })
            .unwrap_or_else(|| DriverInfo {
                name: "provider-neutral".into(),
                version: "2.0".into(),
                external_driver_required: false,
            });

        Ok(BrowserCapabilities {
            browsers,
            default_browser_id,
            driver,
            node_version: raw
                .get("node_version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            platform: raw
                .get("platform")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
    }

    /// Returns the default headless setting saved in the database, or true if not configured.
    pub async fn default_headless(&self) -> bool {
        let guard = self.db.lock().await;
        guard
            .as_ref()
            .map(|db| BrowserSettings::load(db).headless_default)
            .unwrap_or(true)
    }

    /// Returns the saved browser settings.
    pub async fn saved_settings(&self) -> BrowserSettings {
        let guard = self.db.lock().await;
        guard
            .as_ref()
            .map(BrowserSettings::load)
            .unwrap_or_default()
    }

    /// Starts a managed browser session.
    pub async fn create_session(
        &self,
        requested_browser: &str,
        headless: bool,
        run_id: Option<&str>,
        blocked_domains: &[String],
    ) -> Result<BrowserSession, String> {
        let caps = self.capabilities().await?;

        // 1. Resolve requested browser according to priority order
        let mut target = requested_browser.trim();

        // If requested is empty or "auto", check user's saved setting
        let user_setting = {
            let guard = self.db.lock().await;
            guard.as_ref().map(BrowserSettings::load).unwrap_or_default()
        };

        if (target.is_empty() || target.eq_ignore_ascii_case("auto"))
            && !user_setting.selected_browser.eq_ignore_ascii_case("auto")
            && !user_setting.selected_browser.trim().is_empty()
        {
            target = user_setting.selected_browser.as_str();
        }

        let chosen_id = if target.is_empty() || target.eq_ignore_ascii_case("auto") {
            // Automatic: use operating system default browser if automatable
            if let Some(ref def_id) = caps.default_browser_id {
                if let Some(def_b) = caps.browsers.iter().find(|b| b.id == *def_id) {
                    if def_b.available && def_b.automatable {
                        def_b.id.clone()
                    } else {
                        let alternatives: Vec<String> = caps
                            .browsers
                            .iter()
                            .filter(|b| b.available && b.automatable)
                            .map(|b| format!("{} ({})", b.display_name, b.id))
                            .collect();
                        return Err(format!(
                            "The system default browser '{}' is {} and cannot be automated. Available supported alternatives: {}. Kyro will not silently substitute another browser.",
                            def_b.display_name,
                            if !def_b.available { "not installed" } else { "not automatable" },
                            if alternatives.is_empty() { "none".to_string() } else { alternatives.join(", ") }
                        ));
                    }
                } else {
                    let alternatives: Vec<String> = caps
                        .browsers
                        .iter()
                        .filter(|b| b.available && b.automatable)
                        .map(|b| format!("{} ({})", b.display_name, b.id))
                        .collect();
                    return Err(format!(
                        "The system default browser could not be automated. Available supported alternatives: {}. Kyro will not silently substitute another browser.",
                        if alternatives.is_empty() { "none".to_string() } else { alternatives.join(", ") }
                    ));
                }
            } else {
                let alternatives: Vec<String> = caps
                    .browsers
                    .iter()
                    .filter(|b| b.available && b.automatable)
                    .map(|b| format!("{} ({})", b.display_name, b.id))
                    .collect();
                if alternatives.is_empty() {
                    return Err(
                        "No automatable browser is currently installed. Install Mozilla Firefox ('sudo apt install firefox-esr') or Chromium ('sudo apt install chromium-browser') and retry.".to_string(),
                    );
                } else {
                    return Err(format!(
                        "The system default browser could not be determined. Available supported alternatives: {}. Please specify a browser explicitly or configure a default in Kyro settings. Kyro will not silently substitute another browser.",
                        alternatives.join(", ")
                    ));
                }
            }
        } else {
            // Explicit browser requested
            let matched = caps.browsers.iter().find(|b| {
                b.id.eq_ignore_ascii_case(target)
                    || b.name.eq_ignore_ascii_case(target)
                    || b.display_name.eq_ignore_ascii_case(target)
            });
            match matched {
                Some(b) if b.available && b.automatable => b.id.clone(),
                Some(b) if !b.available => {
                    let alternatives: Vec<String> = caps
                        .browsers
                        .iter()
                        .filter(|a| a.available && a.automatable)
                        .map(|a| format!("{} ({})", a.display_name, a.id))
                        .collect();
                    return Err(format!(
                        "Requested browser '{}' is not installed ({}). Available supported alternatives: {}. Kyro will not silently substitute another browser.",
                        b.display_name,
                        b.unavailable_reason.as_deref().unwrap_or("executable not found"),
                        if alternatives.is_empty() { "none".to_string() } else { alternatives.join(", ") }
                    ));
                }
                Some(b) => {
                    let alternatives: Vec<String> = caps
                        .browsers
                        .iter()
                        .filter(|a| a.available && a.automatable)
                        .map(|a| format!("{} ({})", a.display_name, a.id))
                        .collect();
                    return Err(format!(
                        "Requested browser '{}' is installed but cannot be automated by Kyro ({}). Available supported alternatives: {}. Kyro will not silently substitute another browser.",
                        b.display_name,
                        b.unavailable_reason.as_deref().unwrap_or("unsupported automation adapter"),
                        if alternatives.is_empty() { "none".to_string() } else { alternatives.join(", ") }
                    ));
                }
                None => {
                    let alternatives: Vec<String> = caps
                        .browsers
                        .iter()
                        .filter(|a| a.available && a.automatable)
                        .map(|a| format!("{} ({})", a.display_name, a.id))
                        .collect();
                    return Err(format!(
                        "Kyro does not support the browser '{}'. Available supported alternatives: {}. Kyro will not silently substitute another browser.",
                        target,
                        if alternatives.is_empty() { "none".to_string() } else { alternatives.join(", ") }
                    ));
                }
            }
        };

        let raw = self
            .raw_call(
                "session.create",
                serde_json::json!({ "browser": chosen_id, "headless": headless }),
                60000,
            )
            .await
            .map_err(|e| e.to_string())?;

        let session_id = raw
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "The browser worker did not return a session id".to_string())?
            .to_string();

        let session = BrowserSession {
            session_id: session_id.clone(),
            browser: raw
                .get("browser")
                .and_then(|v| v.as_str())
                .unwrap_or(&chosen_id)
                .to_string(),
            browser_path: raw
                .get("browser_path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            browser_version: raw
                .get("browser_version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            headless: raw
                .get("headless")
                .and_then(|v| v.as_bool())
                .unwrap_or(headless),
            profile_isolated: raw
                .get("profile_isolated")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            driver: raw
                .get("driver")
                .and_then(|v| v.as_str())
                .unwrap_or("webdriver-bidi")
                .to_string(),
            origin_scope: None,
            run_id: run_id.map(|s| s.to_string()),
            created_at: chrono::Utc::now().timestamp_millis(),
        };

        self.sessions
            .lock()
            .await
            .insert(session_id.clone(), session.clone());
        self.policies.lock().await.insert(
            session_id.clone(),
            NavigationPolicy::default().with_blocked_domains(blocked_domains),
        );

        Ok(session)
    }

    /// Restricts a session to a single origin.
    pub async fn set_origin_scope(&self, session_id: &str, origin: Option<String>) {
        let mut policies = self.policies.lock().await;
        if let Some(policy) = policies.get_mut(session_id) {
            policy.origin_scope = origin.clone();
        }
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.origin_scope = origin;
        }
    }

    /// Returns the known sessions.
    pub async fn list_sessions(&self) -> Vec<BrowserSession> {
        self.sessions.lock().await.values().cloned().collect()
    }

    pub async fn get_session(&self, session_id: &str) -> Option<BrowserSession> {
        self.sessions.lock().await.get(session_id).cloned()
    }

    /// Closes a session and releases its resources.
    pub async fn close_session(&self, session_id: &str) -> Result<(), String> {
        let result = self
            .raw_call(
                "session.close",
                serde_json::json!({ "session_id": session_id }),
                20000,
            )
            .await;
        self.sessions.lock().await.remove(session_id);
        self.policies.lock().await.remove(session_id);
        match result {
            Ok(_) => Ok(()),
            // The session may already be gone; that is not an error for the
            // caller who asked for it to be closed.
            Err(e) if e.category == "unknown_session" => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Closes every session and stops the worker.
    pub async fn shutdown(&self) {
        let sessions: Vec<String> = self.sessions.lock().await.keys().cloned().collect();
        for id in sessions {
            let _ = self.close_session(&id).await;
        }
        let mut guard = self.worker.lock().await;
        if let Some(handle) = guard.as_mut() {
            handle.shutdown().await;
        }
        *guard = None;
    }

    /// Navigates to a URL after validating it against the session policy.
    pub async fn navigate(
        &self,
        session_id: &str,
        url: &str,
        timeout_ms: u64,
        allow_external: bool,
    ) -> Result<BrowserOperationResult, String> {
        let session = self
            .get_session(session_id)
            .await
            .ok_or_else(|| format!("Browser session '{}' does not exist", session_id))?;

        let mut policy = self
            .policies
            .lock()
            .await
            .get(session_id)
            .cloned()
            .unwrap_or_default();

        if allow_external {
            policy.origin_scope = None;
        }

        let validated = match policy.check(url) {
            Ok(v) => v,
            Err(rejection) => {
                let mut result = BrowserOperationResult::failed(
                    session_id,
                    session.run_id.as_deref(),
                    "navigate",
                    "navigation_blocked",
                    &rejection.to_string(),
                );
                result.url = Some(url.to_string());
                return Ok(result);
            }
        };

        let started = std::time::Instant::now();
        let raw = self
            .raw_call(
                "page.navigate",
                serde_json::json!({
                    "session_id": session_id,
                    "url": validated.url,
                    "timeout_ms": timeout_ms,
                }),
                timeout_ms + 10000,
            )
            .await;

        let mut result =
            BrowserOperationResult::base(session_id, session.run_id.as_deref(), "navigate");
        result.duration_ms = started.elapsed().as_millis() as u64;

        match raw {
            Ok(raw) => {
                result.url = raw
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                result.title = raw
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                result.navigation_status = raw
                    .get("navigation_status")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                result.data = raw;
            }
            Err(e) => {
                result.status = "failed".to_string();
                result.error_category = Some(e.category.clone());
                result.error_message = Some(e.message.clone());
                result.url = Some(validated.url.clone());
            }
        }
        Ok(result)
    }

    /// Retrieves the current URL and title.
    pub async fn page_info(&self, session_id: &str) -> Result<BrowserOperationResult, String> {
        let session = self
            .get_session(session_id)
            .await
            .ok_or_else(|| format!("Browser session '{}' does not exist", session_id))?;
        let raw = self
            .raw_call(
                "page.info",
                serde_json::json!({ "session_id": session_id }),
                20000,
            )
            .await;
        let mut result =
            BrowserOperationResult::base(session_id, session.run_id.as_deref(), "page_info");
        match raw {
            Ok(raw) => {
                result.url = raw
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                result.title = raw
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                result.data = raw;
            }
            Err(e) => {
                result.status = "failed".into();
                result.error_category = Some(e.category);
                result.error_message = Some(e.message);
            }
        }
        Ok(result)
    }

    /// Retrieves a semantic snapshot of the page.
    pub async fn snapshot(&self, session_id: &str) -> Result<BrowserOperationResult, String> {
        let session = self
            .get_session(session_id)
            .await
            .ok_or_else(|| format!("Browser session '{}' does not exist", session_id))?;
        let raw = self
            .raw_call(
                "page.snapshot",
                serde_json::json!({ "session_id": session_id }),
                30000,
            )
            .await;
        let mut result =
            BrowserOperationResult::base(session_id, session.run_id.as_deref(), "snapshot");
        match raw {
            Ok(raw) => {
                result.url = raw
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                result.title = raw
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                result.data = raw;
            }
            Err(e) => {
                result.status = "failed".into();
                result.error_category = Some(e.category);
                result.error_message = Some(e.message);
            }
        }
        Ok(result)
    }

    /// Performs targeted inspection of a selected element.
    pub async fn inspect_element(
        &self,
        session_id: &str,
        target: ElementTarget,
        timeout_ms: u64,
    ) -> Result<ElementInspectionResult, String> {
        let params = target.to_params()?;
        let raw = self
            .raw_call(
                "page.inspect_element",
                serde_json::json!({
                    "session_id": session_id,
                    "target": params,
                    "timeout_ms": timeout_ms,
                }),
                timeout_ms + 10000,
            )
            .await
            .map_err(|e| e.to_string())?;

        serde_json::from_value::<ElementInspectionResult>(raw)
            .map_err(|e| format!("Failed to parse element inspection result: {}", e))
    }

    /// Clicks an element using a semantic locator.
    pub async fn click(
        &self,
        session_id: &str,
        target: serde_json::Value,
        timeout_ms: u64,
    ) -> Result<BrowserOperationResult, String> {
        let session = self
            .get_session(session_id)
            .await
            .ok_or_else(|| format!("Browser session '{}' does not exist", session_id))?;
        let started = std::time::Instant::now();
        let raw = self
            .raw_call(
                "page.click",
                serde_json::json!({
                    "session_id": session_id,
                    "target": target,
                    "timeout_ms": timeout_ms,
                }),
                timeout_ms + 10000,
            )
            .await;
        let mut result =
            BrowserOperationResult::base(session_id, session.run_id.as_deref(), "click");
        result.duration_ms = started.elapsed().as_millis() as u64;
        match raw {
            Ok(raw) => {
                result.url = raw
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                result.title = raw
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                result.data = raw;
            }
            Err(e) => {
                result.status = "failed".into();
                result.error_category = Some(e.category);
                result.error_message = Some(e.message);
            }
        }
        Ok(result)
    }

    /// Fills a form field using a semantic locator.
    pub async fn fill(
        &self,
        session_id: &str,
        target: serde_json::Value,
        value: String,
        timeout_ms: u64,
    ) -> Result<BrowserOperationResult, String> {
        let session = self
            .get_session(session_id)
            .await
            .ok_or_else(|| format!("Browser session '{}' does not exist", session_id))?;
        let started = std::time::Instant::now();
        let raw = self
            .raw_call(
                "page.fill",
                serde_json::json!({
                    "session_id": session_id,
                    "target": target,
                    "value": value,
                    "timeout_ms": timeout_ms,
                }),
                timeout_ms + 15000,
            )
            .await;
        let mut result =
            BrowserOperationResult::base(session_id, session.run_id.as_deref(), "fill");
        result.duration_ms = started.elapsed().as_millis() as u64;
        match raw {
            Ok(raw) => {
                result.url = raw
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                result.data = raw;
            }
            Err(e) => {
                result.status = "failed".into();
                result.error_category = Some(e.category);
                result.error_message = Some(e.message);
            }
        }
        Ok(result)
    }

    /// Checks whether a form field validates an obviously invalid value.
    ///
    /// The check never submits the form: the value is entered, the browser's
    /// own constraint validation is consulted and the original value is
    /// restored.
    pub async fn check_validation(
        &self,
        session_id: &str,
        target: serde_json::Value,
        timeout_ms: u64,
    ) -> Result<BrowserOperationResult, String> {
        let session = self
            .get_session(session_id)
            .await
            .ok_or_else(|| format!("Browser session '{}' does not exist", session_id))?;
        let started = std::time::Instant::now();
        let raw = self
            .raw_call(
                "page.check_validation",
                serde_json::json!({
                    "session_id": session_id,
                    "target": target,
                    "timeout_ms": timeout_ms,
                }),
                timeout_ms + 10000,
            )
            .await;
        let mut result =
            BrowserOperationResult::base(session_id, session.run_id.as_deref(), "check_validation");
        result.duration_ms = started.elapsed().as_millis() as u64;
        match raw {
            Ok(raw) => {
                result.data = raw;
            }
            Err(e) => {
                result.status = "failed".into();
                result.error_category = Some(e.category);
                result.error_message = Some(e.message);
            }
        }
        Ok(result)
    }

    /// Presses a key.
    pub async fn press(
        &self,
        session_id: &str,
        key: String,
        timeout_ms: u64,
    ) -> Result<BrowserOperationResult, String> {
        let session = self
            .get_session(session_id)
            .await
            .ok_or_else(|| format!("Browser session '{}' does not exist", session_id))?;
        let started = std::time::Instant::now();
        let raw = self
            .raw_call(
                "page.press",
                serde_json::json!({
                    "session_id": session_id,
                    "key": key,
                    "timeout_ms": timeout_ms,
                }),
                timeout_ms + 10000,
            )
            .await;
        let mut result =
            BrowserOperationResult::base(session_id, session.run_id.as_deref(), "press");
        result.duration_ms = started.elapsed().as_millis() as u64;
        match raw {
            Ok(raw) => {
                result.url = raw
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                result.title = raw
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                result.data = raw;
            }
            Err(e) => {
                result.status = "failed".into();
                result.error_category = Some(e.category);
                result.error_message = Some(e.message);
            }
        }
        Ok(result)
    }

    /// Captures a screenshot of the page.
    pub async fn screenshot(
        &self,
        session_id: &str,
        full_page: bool,
    ) -> Result<BrowserOperationResult, String> {
        let session = self
            .get_session(session_id)
            .await
            .ok_or_else(|| format!("Browser session '{}' does not exist", session_id))?;
        let raw = self
            .raw_call(
                "page.screenshot",
                serde_json::json!({ "session_id": session_id, "full_page": full_page }),
                30000,
            )
            .await;
        let mut result =
            BrowserOperationResult::base(session_id, session.run_id.as_deref(), "screenshot");
        match raw {
            Ok(raw) => {
                result.data = raw;
            }
            Err(e) => {
                result.status = "failed".into();
                result.error_category = Some(e.category);
                result.error_message = Some(e.message);
            }
        }
        Ok(result)
    }

    /// Lists the open tabs.
    pub async fn list_tabs(&self, session_id: &str) -> Result<BrowserOperationResult, String> {
        let session = self
            .get_session(session_id)
            .await
            .ok_or_else(|| format!("Browser session '{}' does not exist", session_id))?;
        let raw = self
            .raw_call(
                "page.tabs",
                serde_json::json!({ "session_id": session_id }),
                20000,
            )
            .await;
        let mut result =
            BrowserOperationResult::base(session_id, session.run_id.as_deref(), "tabs");
        match raw {
            Ok(raw) => {
                result.data = raw;
            }
            Err(e) => {
                result.status = "failed".into();
                result.error_category = Some(e.category);
                result.error_message = Some(e.message);
            }
        }
        Ok(result)
    }

    /// Sets the viewport size.
    pub async fn set_viewport(
        &self,
        session_id: &str,
        width: u32,
        height: u32,
    ) -> Result<BrowserOperationResult, String> {
        let session = self
            .get_session(session_id)
            .await
            .ok_or_else(|| format!("Browser session '{}' does not exist", session_id))?;
        let raw = self
            .raw_call(
                "page.set_viewport",
                serde_json::json!({ "session_id": session_id, "width": width, "height": height }),
                20000,
            )
            .await;
        let mut result =
            BrowserOperationResult::base(session_id, session.run_id.as_deref(), "set_viewport");
        match raw {
            Ok(raw) => {
                result.data = raw;
            }
            Err(e) => {
                result.status = "failed".into();
                result.error_category = Some(e.category);
                result.error_message = Some(e.message);
            }
        }
        Ok(result)
    }

    /// Reads the console log.
    pub async fn console(&self, session_id: &str) -> Result<BrowserOperationResult, String> {
        let session = self
            .get_session(session_id)
            .await
            .ok_or_else(|| format!("Browser session '{}' does not exist", session_id))?;
        let raw = self
            .raw_call(
                "diagnostics.console",
                serde_json::json!({ "session_id": session_id }),
                20000,
            )
            .await;
        let mut result =
            BrowserOperationResult::base(session_id, session.run_id.as_deref(), "console");
        match raw {
            Ok(raw) => {
                result.console_error_count =
                    raw.get("error_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                result.data = raw;
            }
            Err(e) => {
                result.status = "failed".into();
                result.error_category = Some(e.category);
                result.error_message = Some(e.message);
            }
        }
        Ok(result)
    }

    /// Reads the network diagnostics.
    pub async fn network(&self, session_id: &str) -> Result<BrowserOperationResult, String> {
        let session = self
            .get_session(session_id)
            .await
            .ok_or_else(|| format!("Browser session '{}' does not exist", session_id))?;
        let raw = self
            .raw_call(
                "diagnostics.network",
                serde_json::json!({ "session_id": session_id }),
                20000,
            )
            .await;
        let mut result =
            BrowserOperationResult::base(session_id, session.run_id.as_deref(), "network");
        match raw {
            Ok(raw) => {
                result.failed_request_count = raw
                    .get("failed_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                result.data = raw;
            }
            Err(e) => {
                result.status = "failed".into();
                result.error_category = Some(e.category);
                result.error_message = Some(e.message);
            }
        }
        Ok(result)
    }

    /// Clears the accumulated diagnostics.
    pub async fn reset_diagnostics(&self, session_id: &str) -> Result<(), String> {
        self.raw_call(
            "diagnostics.reset",
            serde_json::json!({ "session_id": session_id }),
            10000,
        )
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unknown_session_is_reported_not_panicked() {
        let manager = BrowserAutomationManager::new();
        let err = manager.page_info("does-not-exist").await.unwrap_err();
        assert!(err.contains("does not exist"), "{}", err);
    }

    #[tokio::test]
    async fn navigation_to_a_blocked_domain_is_refused_without_a_browser() {
        let manager = BrowserAutomationManager::new();
        let session = BrowserSession {
            session_id: "s1".into(),
            browser: "firefox".into(),
            browser_path: "/usr/bin/firefox".into(),
            browser_version: None,
            headless: true,
            profile_isolated: true,
            driver: "webdriver-bidi".into(),
            origin_scope: None,
            run_id: Some("run_1".into()),
            created_at: 0,
        };
        manager.sessions.lock().await.insert("s1".into(), session);
        manager.policies.lock().await.insert(
            "s1".into(),
            NavigationPolicy::default().with_blocked_domains(&["paypal.com".to_string()]),
        );

        let result = manager
            .navigate("s1", "https://paypal.com/checkout", 5000, false)
            .await
            .unwrap();
        assert_eq!(result.status, "failed");
        assert_eq!(result.error_category.as_deref(), Some("navigation_blocked"));
        assert!(result.error_message.unwrap().contains("paypal.com"));
    }

    #[tokio::test]
    async fn navigation_outside_the_origin_scope_is_refused() {
        let manager = BrowserAutomationManager::new();
        manager.sessions.lock().await.insert(
            "s2".into(),
            BrowserSession {
                session_id: "s2".into(),
                browser: "firefox".into(),
                browser_path: "/usr/bin/firefox".into(),
                browser_version: None,
                headless: true,
                profile_isolated: true,
                driver: "webdriver-bidi".into(),
                origin_scope: Some("http://localhost:3000".into()),
                run_id: None,
                created_at: 0,
            },
        );
        manager.policies.lock().await.insert(
            "s2".into(),
            NavigationPolicy::default().with_origin_scope(Some("http://localhost:3000".into())),
        );

        let result = manager
            .navigate("s2", "https://example.com/", 5000, false)
            .await
            .unwrap();
        assert_eq!(result.status, "failed");
        assert_eq!(result.error_category.as_deref(), Some("navigation_blocked"));
    }

    #[tokio::test]
    async fn browser_capabilities_reports_supported_and_unsupported_browsers() {
        let manager = BrowserAutomationManager::new();
        let caps = manager.capabilities().await.expect("capabilities");
        assert!(!caps.browsers.is_empty(), "capabilities should list known browsers");
        for b in &caps.browsers {
            assert!(!b.id.is_empty());
            assert!(!b.name.is_empty());
            assert!(!b.family.is_empty());
            assert!(!b.adapter.is_empty());
            if !b.available || !b.automatable {
                assert!(b.unavailable_reason.is_some(), "browser {} must give a reason when unavailable/unsupported", b.id);
            }
        }
    }

    #[tokio::test]
    async fn browser_settings_persistence() {
        let temp_dir = std::env::temp_dir().join(format!("kyro_browser_settings_{}", uuid::Uuid::new_v4()));
        let db = crate::storage::db::Database::init(temp_dir.clone()).expect("db init");

        let settings = BrowserSettings {
            selected_browser: "chromium".to_string(),
            headless_default: false,
        };
        settings.save(&db).expect("save settings");

        let loaded = BrowserSettings::load(&db);
        assert_eq!(loaded.selected_browser, "chromium");
        assert!(!loaded.headless_default);

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[tokio::test]
    async fn unsupported_browser_rejection_names_alternatives() {
        let manager = BrowserAutomationManager::new();
        let err = manager
            .create_session("safari", true, None, &[])
            .await
            .expect_err("safari should be rejected");
        assert!(err.contains("safari"));
        assert!(err.contains("Available supported alternatives"));
    }
}
