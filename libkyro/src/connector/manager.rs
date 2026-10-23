use super::auth::{OAuthFlowInit, OAuthManager, OAuthSessionStatus, PendingOAuthSession};
use super::catalog::{get_default_catalog_items, ConnectorCatalogItem};
use super::native::github::GitHubAdapter;
use super::native::google::GoogleAdapter;
use super::native::NativeConnectorAdapter;
use super::remote_mcp::RemoteMcpClient;
use crate::agent::ToolDefinition;
use crate::execution::permissions::RiskLevel;
use crate::governance::AuthDecision;
use crate::mcp::McpManager;
use crate::storage::{ConnectedAccountRecord, Database, Vault};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Digest;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedAccountSummary {
    pub id: String,
    pub connector_id: String,
    pub account_identifier: String,
    pub display_name: String,
    pub status: String, // "connected", "needs_auth", "needs_reauth", "disconnected", "error", "disabled"
    pub status_message: Option<String>,
    pub granted_scopes: Vec<String>,
    pub config: Value,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_connected_at: Option<i64>,
    pub last_error: Option<String>,
    pub tool_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountToolView {
    pub name: String,
    pub original_name: String,
    pub description: String,
    pub is_read_only: bool,
    pub risk_level: String,
    pub default_policy: String,
    pub can_auto_approve: bool,
    pub schema_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationAuthProof {
    pub approval_id: String,
    pub run_id: String,
    pub tool_name: String,
    pub args_hash: String,
    pub timestamp: i64,
}

impl InvocationAuthProof {
    pub fn compute_args_hash(args: &Value) -> String {
        let mut hasher = sha2::Sha256::new();
        let canonical = serde_json::to_string(&canonicalize_json_value(args)).unwrap_or_default();
        hasher.update(canonical.as_bytes());
        let result = hasher.finalize();
        result.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

pub fn canonicalize_json_value(val: &Value) -> Value {
    match val {
        Value::Object(map) => {
            let mut btree = std::collections::BTreeMap::new();
            for (k, v) in map {
                btree.insert(k.clone(), canonicalize_json_value(v));
            }
            serde_json::to_value(btree).unwrap_or(Value::Null)
        }
        Value::Array(arr) => {
            let canonical_arr: Vec<_> = arr.iter().map(canonicalize_json_value).collect();
            Value::Array(canonical_arr)
        }
        other => other.clone(),
    }
}

pub fn compute_schema_fingerprint(schema: &Value) -> String {
    let mut hasher = sha2::Sha256::new();
    let canonical = serde_json::to_string(&canonicalize_json_value(schema)).unwrap_or_default();
    hasher.update(canonical.as_bytes());
    let result = hasher.finalize();
    result.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Classifies risk for a dynamic external tool based on name, description, and schema properties.
/// Returns (RiskLevel, is_read_only, can_auto_approve).
pub fn classify_external_tool(
    name: &str,
    description: &str,
    schema: &Value,
) -> (RiskLevel, bool, bool) {
    let lower_name = name.to_lowercase();
    let lower_desc = description.to_lowercase();

    // 1. Destructive keywords
    let destructive_patterns = [
        "delete", "destroy", "drop", "truncate", "wipe", "format", "purge", "unlink", "remove",
        "kill",
    ];
    let is_destructive_name = destructive_patterns.iter().any(|&p| lower_name.contains(p));
    let is_destructive_desc = destructive_patterns.iter().any(|&p| lower_desc.contains(p));

    if is_destructive_name || is_destructive_desc {
        return (RiskLevel::Destructive, false, false);
    }

    // 2. Modifying / Write / Execution keywords in name or schema
    let modifying_patterns = [
        "create", "write", "update", "modify", "patch", "post", "put", "send", "upload", "set",
        "insert", "execute", "exec", "shell", "run", "spawn", "cmd", "command", "eval", "script",
        "sql", "install", "publish",
    ];
    let is_modifying_name = modifying_patterns.iter().any(|&p| lower_name.contains(p));

    // Inspect schema properties
    let mut has_execution_props = false;
    let mut has_write_props = false;
    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
        for (prop_name, _) in props {
            let p_lower = prop_name.to_lowercase();
            if p_lower.contains("command")
                || p_lower.contains("cmd")
                || p_lower.contains("exec")
                || p_lower.contains("shell")
                || p_lower.contains("script")
                || p_lower.contains("eval")
                || p_lower.contains("code")
                || p_lower.contains("sql")
                || p_lower.contains("query")
            {
                has_execution_props = true;
            }
            if p_lower.contains("path")
                || p_lower.contains("file")
                || p_lower.contains("content")
                || p_lower.contains("payload")
                || p_lower.contains("data")
                || p_lower.contains("body")
                || p_lower.contains("write")
            {
                has_write_props = true;
            }
        }
    }

    if has_execution_props {
        // Any tool accepting shell/code execution properties cannot be auto-approved
        return (RiskLevel::Destructive, false, false);
    }

    if is_modifying_name || has_write_props {
        return (RiskLevel::Moderate, false, false);
    }

    // 3. Explicit read-only verification
    let read_only_prefixes = [
        "get_",
        "list_",
        "read_",
        "describe_",
        "fetch_",
        "search_",
        "check_",
        "inspect_",
        "status",
        "view_",
        "info",
    ];
    let is_read_only_name = read_only_prefixes
        .iter()
        .any(|&p| lower_name.starts_with(p) || lower_name == p.trim_end_matches('_'));

    // Check for trusted, explicit read-only declaration in schema or annotations
    let has_explicit_read_only = schema
        .get("readOnly")
        .and_then(|v| v.as_bool())
        .or_else(|| schema.get("x-read-only").and_then(|v| v.as_bool()))
        .or_else(|| schema.get("x-mcp-read-only").and_then(|v| v.as_bool()))
        .or_else(|| {
            schema.get("annotations").and_then(|a| {
                a.get("readOnlyHint")
                    .or_else(|| a.get("read_only"))
                    .and_then(|v| v.as_bool())
            })
        })
        .unwrap_or(false);

    if is_read_only_name {
        // External tools cannot auto-approve based on naming heuristics alone.
        // Auto-approval strictly requires an explicit, trusted read-only declaration.
        (RiskLevel::Safe, true, has_explicit_read_only)
    } else if has_explicit_read_only {
        (RiskLevel::Safe, true, true)
    } else {
        // Unknown/unclassified external tool: fail closed!
        (RiskLevel::Moderate, false, false)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectorKind {
    Native,
    RemoteMcp,
    LocalMcp,
}

#[derive(Clone, Debug)]
pub struct RegisteredConnectorTool {
    pub scoped_name: String,
    pub alias_name: Option<String>,
    pub account_id: String,
    pub connector_id: String,
    pub original_tool_name: String,
    pub definition: ToolDefinition,
    pub risk_level: RiskLevel,
    pub is_read_only: bool,
    pub kind: ConnectorKind,
    pub can_auto_approve: bool,
    pub schema_fingerprint: String,
}

#[derive(Clone)]
pub struct ConnectorManager {
    db: Database,
    vault: Vault,
    local_mcp: McpManager,
    native_adapters: Arc<HashMap<String, Arc<dyn NativeConnectorAdapter>>>,
    remote_clients: Arc<Mutex<HashMap<String, RemoteMcpClient>>>,
    oauth_manager: OAuthManager,
    tool_registry: Arc<Mutex<HashMap<String, RegisteredConnectorTool>>>,
    account_refresh_locks: Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
}

impl ConnectorManager {
    pub fn new(db: Database, vault: Vault, local_mcp: McpManager) -> Self {
        let mut adapters: HashMap<String, Arc<dyn NativeConnectorAdapter>> = HashMap::new();
        adapters.insert("github".to_string(), Arc::new(GitHubAdapter::new()));
        adapters.insert("google".to_string(), Arc::new(GoogleAdapter::new()));
        adapters.insert(
            "google_calendar".to_string(),
            Arc::new(GoogleAdapter::new()),
        );

        Self {
            db,
            vault,
            local_mcp,
            native_adapters: Arc::new(adapters),
            remote_clients: Arc::new(Mutex::new(HashMap::new())),
            oauth_manager: OAuthManager::new(),
            tool_registry: Arc::new(Mutex::new(HashMap::new())),
            account_refresh_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get_native_adapter(
        &self,
        connector_id: &str,
    ) -> Option<Arc<dyn NativeConnectorAdapter>> {
        self.native_adapters.get(connector_id).cloned()
    }

    /// List all catalog integrations with real-time status and connected account counts
    pub fn list_catalog(&self) -> Result<Vec<ConnectorCatalogItem>, String> {
        let mut catalog = get_default_catalog_items();
        let all_accounts = self.db.list_connected_accounts().unwrap_or_default();

        for item in &mut catalog {
            let accounts_for_item: Vec<&ConnectedAccountRecord> = all_accounts
                .iter()
                .filter(|a| a.connector_id == item.id)
                .collect();

            item.connected_accounts_count = accounts_for_item.len();

            if item.id == "local_mcp" {
                let local_servers = self.db.list_mcp_servers().unwrap_or_default();
                item.connected_accounts_count = local_servers.len();
                if local_servers.iter().any(|s| s.enabled) {
                    item.status = "connected".to_string();
                } else if !local_servers.is_empty() {
                    item.status = "disabled".to_string();
                } else {
                    item.status = "available".to_string();
                }
            } else if !accounts_for_item.is_empty() {
                if accounts_for_item.iter().any(|a| a.status == "connected") {
                    item.status = "connected".to_string();
                } else if accounts_for_item.iter().any(|a| a.status == "error") {
                    item.status = "error".to_string();
                } else if accounts_for_item.iter().any(|a| a.status == "needs_reauth") {
                    item.status = "needs_reauth".to_string();
                } else {
                    item.status = "disconnected".to_string();
                }
            } else if !item.is_available {
                item.status = "catalog_only".to_string();
            } else {
                item.status = "available".to_string();
            }
        }

        Ok(catalog)
    }

    /// List connected accounts with safe metadata (no tokens/secrets)
    pub fn list_connected_accounts(&self) -> Result<Vec<ConnectedAccountSummary>, String> {
        let accounts = self
            .db
            .list_connected_accounts()
            .map_err(|e| e.to_string())?;
        let mut summaries = Vec::new();

        for a in accounts {
            let scopes: Vec<String> =
                serde_json::from_str(&a.granted_scopes_json).unwrap_or_default();
            let config: Value = serde_json::from_str(&a.config_json).unwrap_or(json!({}));

            let tool_count = if let Some(adapter) = self.native_adapters.get(&a.connector_id) {
                adapter.list_tools(&a).len()
            } else {
                0
            };

            summaries.push(ConnectedAccountSummary {
                id: a.id,
                connector_id: a.connector_id,
                account_identifier: a.account_identifier,
                display_name: a.display_name,
                status: a.status,
                status_message: a.status_message,
                granted_scopes: scopes,
                config,
                created_at: a.created_at,
                updated_at: a.updated_at,
                last_connected_at: a.last_connected_at,
                last_error: a.last_error,
                tool_count,
            });
        }

        Ok(summaries)
    }

    /// Connect an account via Personal Access Token or API Token
    pub async fn connect_account_token(
        &self,
        connector_id: &str,
        display_name: &str,
        account_identifier: &str,
        token_secret: &str,
        config: Option<Value>,
    ) -> Result<ConnectedAccountRecord, String> {
        let trimmed_token = token_secret.trim();
        if trimmed_token.is_empty() {
            return Err("Token secret cannot be empty".to_string());
        }

        // Strictly verify that the requested connector is implemented
        if connector_id != "github" && connector_id != "remote_mcp" {
            return Err(format!(
                "Connector '{}' is not implemented in this release of Kyro",
                connector_id
            ));
        }

        let account_id = format!("conn_{}_{}", connector_id, uuid::Uuid::new_v4());
        let now = chrono::Utc::now().timestamp_millis();
        let config_val = config.unwrap_or(json!({}));

        let secret_payload = json!({
            "token": trimmed_token,
            "created_at": now
        });

        // 1. Store credentials strictly in secure backend Vault
        self.vault
            .store_account_secret(&account_id, &secret_payload.to_string())?;

        let mut record = ConnectedAccountRecord {
            id: account_id.clone(),
            connector_id: connector_id.to_string(),
            account_identifier: account_identifier.to_string(),
            display_name: display_name.to_string(),
            status: "connecting".to_string(),
            status_message: Some("Validating credentials with provider...".to_string()),
            granted_scopes_json: "[]".to_string(),
            credential_key: format!("connector:account:{}", account_id),
            config_json: config_val.to_string(),
            created_at: now,
            updated_at: now,
            last_connected_at: None,
            last_error: None,
            expires_at: None,
        };

        // 2. Persist account record initially
        self.db
            .create_connected_account(&record)
            .map_err(|e| format!("Failed to save account record: {}", e))?;

        // 3. Test verification with provider adapter
        if let Some(adapter) = self.native_adapters.get(connector_id) {
            match adapter.test_connection(&record, &self.vault).await {
                Ok(msg) => {
                    record.status = "connected".to_string();
                    record.status_message = Some(msg);
                    record.last_connected_at = Some(chrono::Utc::now().timestamp_millis());
                    let _ = self.db.update_connected_account(&record);
                }
                Err(err) => {
                    // Cleanup secret on verification failure
                    let _ = self.vault.delete_account_secret(&account_id);
                    record.status = "error".to_string();
                    record.status_message = Some("Authentication check failed".to_string());
                    record.last_error = Some(err.clone());
                    let _ = self.db.update_connected_account(&record);
                    return Err(format!("Credential verification failed: {}", err));
                }
            }
        } else if connector_id == "remote_mcp" {
            // Test Remote MCP endpoint
            let url = config_val
                .get("server_url")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if url.is_empty() {
                let _ = self.vault.delete_account_secret(&account_id);
                record.status = "error".to_string();
                record.last_error =
                    Some("Missing 'server_url' in remote MCP configuration".to_string());
                let _ = self.db.update_connected_account(&record);
                return Err("Missing 'server_url' in configuration".to_string());
            }

            let allow_private = config_val
                .get("allow_private_network")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let allow_insecure = config_val
                .get("allow_insecure_http")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let client = match RemoteMcpClient::new_with_trust(
                url,
                display_name,
                Some(trimmed_token.to_string()),
                allow_private,
                allow_insecure,
            ) {
                Ok(c) => c,
                Err(e) => {
                    let _ = self.vault.delete_account_secret(&account_id);
                    record.status = "error".to_string();
                    record.last_error = Some(e.clone());
                    let _ = self.db.update_connected_account(&record);
                    return Err(format!("Invalid Remote MCP endpoint: {}", e));
                }
            };

            match client.connect_or_verify().await {
                Ok(msg) => {
                    record.status = "connected".to_string();
                    record.status_message = Some(msg);
                    record.last_connected_at = Some(chrono::Utc::now().timestamp_millis());
                    let _ = self.db.update_connected_account(&record);
                    let mut lock = self.remote_clients.lock().await;
                    lock.insert(account_id.clone(), client);
                }
                Err(err) => {
                    let _ = self.vault.delete_account_secret(&account_id);
                    record.status = "error".to_string();
                    record.status_message = Some("Remote MCP handshake failed".to_string());
                    record.last_error = Some(err.clone());
                    let _ = self.db.update_connected_account(&record);
                    return Err(format!("Remote MCP connection failed: {}", err));
                }
            }
        }

        Ok(record)
    }

    /// Initialize OAuth PKCE authorization flow with external browser loopback
    pub async fn start_oauth_flow(
        &self,
        connector_id: &str,
        client_id_opt: Option<String>,
        client_secret_opt: Option<String>,
    ) -> Result<OAuthFlowInit, String> {
        if connector_id != "github" && connector_id != "google" && connector_id != "google_calendar"
        {
            return Err(format!(
                "OAuth authorization is not supported or not implemented for provider '{}'",
                connector_id
            ));
        }

        let client_secret_opt = if connector_id == "github" {
            client_secret_opt
                .filter(|secret| !secret.trim().is_empty())
                .or_else(|| {
                    std::env::var("KYRO_GITHUB_CLIENT_SECRET")
                        .ok()
                        .filter(|secret| !secret.trim().is_empty())
                })
                .map(|secret| secret.trim().to_string())
        } else {
            client_secret_opt
        };

        let (state, code_verifier, code_challenge) = OAuthManager::generate_pkce();
        let session_id = format!("oauth_{}", uuid::Uuid::new_v4());

        // Bind ephemeral loopback listener to 127.0.0.1:0
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("Failed to bind loopback TCP listener for OAuth: {}", e))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("Failed to read loopback port: {}", e))?
            .port();

        let redirect_uri = format!("http://127.0.0.1:{}/callback", port);

        let (auth_url, token_endpoint, client_id) = if connector_id == "github" {
            let cid = client_id_opt
                .unwrap_or_else(|| std::env::var("KYRO_GITHUB_CLIENT_ID").unwrap_or_default());
            if cid.trim().is_empty() {
                return Err("GitHub OAuth requires a Client ID. Set KYRO_GITHUB_CLIENT_ID in the process environment or enter it in the connection modal.".to_string());
            }
            if client_secret_opt.is_none() {
                return Err("GitHub OAuth authorization-code flow requires a Client Secret. Enter it in the connection modal or configure KYRO_GITHUB_CLIENT_SECRET in the process environment. Alternatively, connect using a Personal Access Token.".to_string());
            }
            let mut url = reqwest::Url::parse("https://github.com/login/oauth/authorize")
                .map_err(|e| format!("Failed to parse GitHub OAuth URL: {}", e))?;
            {
                let mut query = url.query_pairs_mut();
                query.append_pair("client_id", cid.trim());
                query.append_pair("redirect_uri", &redirect_uri);
                query.append_pair("state", &state);
                // oauth app scopes are space-delimited; repo covers actions reads for private repos.
                query.append_pair("scope", "repo read:user");
                query.append_pair("code_challenge", &code_challenge);
                query.append_pair("code_challenge_method", "S256");
            }
            (
                url.to_string(),
                "https://github.com/login/oauth/access_token".to_string(),
                cid.trim().to_string(),
            )
        } else {
            let cid = client_id_opt
                .unwrap_or_else(|| std::env::var("KYRO_GOOGLE_CLIENT_ID").unwrap_or_default());
            if cid.trim().is_empty() {
                return Err("Google OAuth requires a Client ID. Configure one in Kyro Settings or provide KYRO_GOOGLE_CLIENT_ID.".to_string());
            }
            let scopes = "https://www.googleapis.com/auth/gmail.readonly https://www.googleapis.com/auth/gmail.compose https://www.googleapis.com/auth/calendar.readonly https://www.googleapis.com/auth/calendar.events https://www.googleapis.com/auth/userinfo.email";
            let mut url = reqwest::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
                .map_err(|e| format!("Failed to parse Google OAuth URL: {}", e))?;
            {
                let mut q = url.query_pairs_mut();
                q.append_pair("client_id", &cid);
                q.append_pair("redirect_uri", &redirect_uri);
                q.append_pair("response_type", "code");
                q.append_pair("scope", scopes);
                q.append_pair("access_type", "offline");
                q.append_pair("prompt", "consent");
                q.append_pair("state", &state);
                q.append_pair("code_challenge", &code_challenge);
                q.append_pair("code_challenge_method", "S256");
            }
            (
                url.to_string(),
                "https://oauth2.googleapis.com/token".to_string(),
                cid,
            )
        };

        let now = chrono::Utc::now().timestamp_millis();
        let expires_at = now + 120_000; // 2 minutes expiration

        let session = PendingOAuthSession {
            session_id: session_id.clone(),
            provider_id: connector_id.to_string(),
            client_id,
            client_secret: client_secret_opt,
            code_verifier,
            state: state.clone(),
            redirect_uri: redirect_uri.clone(),
            token_endpoint,
            status: OAuthSessionStatus::Pending,
            created_at: now,
            expires_at,
        };

        self.oauth_manager.register_session(session).await;

        // Register cancellation sender for early abort
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
        self.oauth_manager
            .register_cancellation_sender(&session_id, cancel_tx)
            .await;

        // Spawn background task to wait for loopback callback with timeout
        let oauth_mgr = self.oauth_manager.clone();
        let sid = session_id.clone();
        let expected_st = state;
        tokio::spawn(async move {
            tokio::select! {
                _ = &mut cancel_rx => {
                    log::info!("OAuth loopback listener cancelled for session {}", sid);
                }
                callback_res = OAuthManager::listen_for_callback(listener, expected_st, Duration::from_secs(120)) => {
                    match callback_res {
                        Ok(code) => {
                            log::info!("Captured OAuth callback code for session {}", sid);
                            if let Ok(()) = oauth_mgr.record_callback(&sid, code).await {
                                if let Ok((
                                    endpoint,
                                    client_id,
                                    client_secret,
                                    code,
                                    redirect_uri,
                                    verifier,
                                )) = oauth_mgr.claim_exchange(&sid).await
                                {
                                    let http = reqwest::Client::new();
                                    let res = OAuthManager::exchange_code(
                                        &http,
                                        &endpoint,
                                        &client_id,
                                        client_secret.as_deref(),
                                        &code,
                                        &redirect_uri,
                                        &verifier,
                                    )
                                    .await;
                                    oauth_mgr.finish_exchange(&sid, res).await;
                                }
                            }
                        }
                        Err(e) => {
                            log::warn!("OAuth loopback error for session {}: {}", sid, e);
                            oauth_mgr.finish_exchange(&sid, Err(e)).await;
                        }
                    }
                }
            }
        });

        Ok(OAuthFlowInit {
            session_id,
            auth_url,
            redirect_uri,
            expires_at,
        })
    }

    /// Cancel a pending or active OAuth flow
    pub async fn cancel_oauth_flow(&self, session_id: &str) -> Result<(), String> {
        self.oauth_manager.cancel_session(session_id).await
    }

    /// Retrieve current OAuth session status non-destructively
    pub async fn get_oauth_status(
        &self,
        session_id: &str,
    ) -> Result<crate::connector::auth::OAuthSessionStatusView, String> {
        self.oauth_manager
            .get_session_status_view(session_id)
            .await
            .ok_or_else(|| "OAuth session not found, expired, or already completed".to_string())
    }

    /// Retrieve canonical registered tools isolated to a specific account
    pub async fn get_tools_for_account(&self, account_id: &str) -> Vec<AccountToolView> {
        let reg = self.tool_registry.lock().await;
        let mut tools: Vec<AccountToolView> = reg
            .values()
            .filter(|t| t.account_id == account_id)
            .map(|t| AccountToolView {
                name: t.scoped_name.clone(),
                original_name: t.original_tool_name.clone(),
                description: t.definition.description.clone(),
                is_read_only: t.is_read_only,
                risk_level: match t.risk_level {
                    RiskLevel::Safe => "safe",
                    RiskLevel::Moderate => "moderate",
                    RiskLevel::Destructive => "destructive",
                }
                .to_string(),
                default_policy: if t.can_auto_approve
                    && t.is_read_only
                    && t.risk_level == RiskLevel::Safe
                {
                    "allow".to_string()
                } else {
                    "ask".to_string()
                },
                can_auto_approve: t.can_auto_approve,
                schema_fingerprint: t.schema_fingerprint.clone(),
            })
            .collect();

        // If tools are not yet populated in the dynamic registry (e.g. before initial full refresh or GitHub native),
        // inspect account type and provide canonical tool definitions
        if tools.is_empty() {
            if let Ok(Some(acc)) = self.db.get_connected_account(account_id) {
                if acc.connector_id == "github" {
                    for def in GitHubAdapter::default_tools() {
                        let scoped_name = format!("{}__{}", account_id, def.name);
                        let fp = compute_schema_fingerprint(&def.parameters);
                        let can_auto = def.risk_level != RiskLevel::Destructive;
                        tools.push(AccountToolView {
                            name: scoped_name,
                            original_name: def.name.clone(),
                            description: def.description.clone(),
                            is_read_only: def.is_read_only,
                            risk_level: match def.risk_level {
                                RiskLevel::Safe => "safe",
                                RiskLevel::Moderate => "moderate",
                                RiskLevel::Destructive => "destructive",
                            }
                            .to_string(),
                            default_policy: if def.is_read_only && def.risk_level == RiskLevel::Safe
                            {
                                "allow".to_string()
                            } else {
                                "ask".to_string()
                            },
                            can_auto_approve: can_auto,
                            schema_fingerprint: fp,
                        });
                    }
                } else if acc.connector_id == "google" || acc.connector_id == "google_calendar" {
                    for def in GoogleAdapter::default_tools() {
                        let scoped_name = format!("{}__{}", account_id, def.name);
                        let fp = compute_schema_fingerprint(&def.parameters);
                        let can_auto = def.risk_level != RiskLevel::Destructive;
                        tools.push(AccountToolView {
                            name: scoped_name,
                            original_name: def.name.clone(),
                            description: def.description.clone(),
                            is_read_only: def.is_read_only,
                            risk_level: match def.risk_level {
                                RiskLevel::Safe => "safe",
                                RiskLevel::Moderate => "moderate",
                                RiskLevel::Destructive => "destructive",
                            }
                            .to_string(),
                            default_policy: if def.is_read_only && def.risk_level == RiskLevel::Safe
                            {
                                "allow".to_string()
                            } else {
                                "ask".to_string()
                            },
                            can_auto_approve: can_auto,
                            schema_fingerprint: fp,
                        });
                    }
                }
            }
        }

        tools.sort_by(|a, b| a.original_name.cmp(&b.original_name));
        tools
    }

    /// Classify whether an OAuth token refresh error is permanent (revocation, expired token, invalid scope)
    /// versus transient (network glitch, connection reset, timeout, rate limit).
    pub fn is_permanent_refresh_error(err_msg: &str) -> bool {
        let err_lower = err_msg.to_lowercase();
        (err_lower.contains("invalid_grant")
            || err_lower.contains("invalid_client")
            || err_lower.contains("unauthorized_client")
            || err_lower.contains("access_denied")
            || err_lower.contains("invalid_scope")
            || err_lower.contains("revoked")
            || err_lower.contains("token has been expired")
            || err_lower.contains("status 400")
            || err_lower.contains("status: 400")
            || err_lower.contains("status 401")
            || err_lower.contains("status: 401")
            || err_lower.contains("status 403")
            || err_lower.contains("status: 403")
            || err_lower.contains("status 404")
            || err_lower.contains("status: 404")
            || err_lower.contains("not found")
            || err_lower.contains("bad_verification_code"))
            && !err_lower.contains("rate limit")
            && !err_lower.contains("429")
    }

    /// Perform serialized refresh token rotation for an account
    pub async fn refresh_account_token(&self, account_id: &str) -> Result<String, String> {
        let lock = {
            let mut locks = self.account_refresh_locks.lock().await;
            locks
                .entry(account_id.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };

        let _guard = lock.lock().await;

        let secret_raw = self
            .vault
            .get_account_secret(account_id)?
            .ok_or_else(|| format!("No vault credentials found for account '{}'", account_id))?;

        let secret_val: Value =
            serde_json::from_str(&secret_raw).map_err(|e| format!("Invalid secret JSON: {}", e))?;

        let now = chrono::Utc::now().timestamp_millis();
        let account = self
            .db
            .get_connected_account(account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Account '{}' not found", account_id))?;

        // Concurrency guard: check if another concurrent turn already refreshed the token in the last 15 seconds
        // or if expires_at indicates token is valid well into the future
        if let Some(updated_at) = secret_val.get("updated_at").and_then(|u| u.as_i64()) {
            if now.saturating_sub(updated_at) < 15_000 {
                if let Some(tok) = secret_val.get("access_token").and_then(|t| t.as_str()) {
                    if !tok.is_empty() {
                        log::info!(
                            "Account '{}' token was refreshed recently by concurrent turn; reusing access token",
                            account_id
                        );
                        return Ok(tok.to_string());
                    }
                }
            }
        }
        if let Some(expires_at) = account.expires_at {
            if expires_at > now + 60_000 {
                if let Some(tok) = secret_val.get("access_token").and_then(|t| t.as_str()) {
                    if !tok.is_empty() {
                        return Ok(tok.to_string());
                    }
                }
            }
        }

        let refresh_tok = match secret_val.get("refresh_token").and_then(|t| t.as_str()) {
            Some(t) if !t.is_empty() => t,
            _ => return Err("Account credentials do not include a refresh token".to_string()),
        };

        let config: Value = serde_json::from_str(&account.config_json).unwrap_or_default();
        let env_cid = match account.connector_id.as_str() {
            "github" => std::env::var("KYRO_GITHUB_CLIENT_ID").unwrap_or_default(),
            "google" | "google_calendar" => {
                std::env::var("KYRO_GOOGLE_CLIENT_ID").unwrap_or_default()
            }
            _ => std::env::var(format!(
                "KYRO_{}_CLIENT_ID",
                account.connector_id.to_uppercase()
            ))
            .unwrap_or_default(),
        };
        let client_id = config
            .get("client_id")
            .and_then(|c| c.as_str())
            .unwrap_or(&env_cid);

        let env_sec = match account.connector_id.as_str() {
            "github" => std::env::var("KYRO_GITHUB_CLIENT_SECRET").ok(),
            "google" | "google_calendar" => std::env::var("KYRO_GOOGLE_CLIENT_SECRET").ok(),
            _ => std::env::var(format!(
                "KYRO_{}_CLIENT_SECRET",
                account.connector_id.to_uppercase()
            ))
            .ok(),
        };
        let client_secret = secret_val
            .get("client_secret")
            .and_then(|s| s.as_str())
            .or_else(|| config.get("client_secret").and_then(|s| s.as_str()))
            .or(env_sec.as_deref());

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_default();

        let token_endpoint = match account.connector_id.as_str() {
            "github" => "https://github.com/login/oauth/access_token",
            "google" | "google_calendar" => "https://oauth2.googleapis.com/token",
            other => {
                return Err(format!(
                    "Unsupported provider '{}' for token refresh",
                    other
                ));
            }
        };

        let refresh_result = OAuthManager::refresh_access_token(
            &http,
            token_endpoint,
            client_id,
            client_secret,
            refresh_tok,
        )
        .await;

        let refreshed = match refresh_result {
            Ok(r) => r,
            Err(e) => {
                let mut err_msg = format!("OAuth token refresh failed: {}", e);
                // Sanitize secrets from error message
                if let Some(sec) = client_secret {
                    if !sec.is_empty() {
                        err_msg = err_msg.replace(sec, "<REDACTED_SECRET>");
                    }
                }
                if !refresh_tok.is_empty() {
                    err_msg = err_msg.replace(refresh_tok, "<REDACTED_TOKEN>");
                }

                let is_permanent = Self::is_permanent_refresh_error(&err_msg);

                if is_permanent {
                    let _ = self.db.update_connected_account_status(
                        account_id,
                        "needs_reauth",
                        Some("Reauthorization required: OAuth refresh token has expired or was revoked"),
                        Some(&err_msg),
                        Some(now),
                    );
                } else {
                    let _ = self.db.update_connected_account_status(
                        account_id,
                        "error",
                        Some("Transient failure refreshing OAuth token"),
                        Some(&err_msg),
                        Some(now),
                    );
                }
                return Err(err_msg);
            }
        };

        let new_secret_payload = json!({
            "access_token": refreshed.access_token,
            "refresh_token": refreshed.refresh_token.or_else(|| Some(refresh_tok.to_string())),
            "client_secret": client_secret,
            "updated_at": now
        });

        self.vault
            .store_account_secret(account_id, &new_secret_payload.to_string())?;

        let expires_at_opt = refreshed.expires_in_seconds.map(|secs| now + (secs * 1000));
        let mut updated_account = account;
        updated_account.status = "connected".to_string();
        updated_account.status_message = Some("OAuth token refreshed successfully".to_string());
        updated_account.last_error = None;
        updated_account.updated_at = now;
        if expires_at_opt.is_some() {
            updated_account.expires_at = expires_at_opt;
        }
        self.db
            .update_connected_account(&updated_account)
            .map_err(|e| {
                format!(
                    "Failed to update account in database after token refresh: {}",
                    e
                )
            })?;

        Ok(refreshed.access_token)
    }

    /// Complete an OAuth session after loopback or explicit code submission
    pub async fn complete_oauth_flow(
        &self,
        session_id: &str,
        explicit_code: Option<String>,
    ) -> Result<ConnectedAccountRecord, String> {
        let (provider_id, token_res, client_id, client_secret) = if let Some(code) = explicit_code {
            let sess = self
                .oauth_manager
                .get_session_for_manual_exchange(session_id)
                .await?;
            let http = reqwest::Client::new();
            let res = OAuthManager::exchange_code(
                &http,
                &sess.token_endpoint,
                &sess.client_id,
                sess.client_secret.as_deref(),
                &code,
                &sess.redirect_uri,
                &sess.code_verifier,
            )
            .await?;
            let _ = self
                .oauth_manager
                .mark_finalizing(session_id, res.clone())
                .await;
            (sess.provider_id, res, sess.client_id, sess.client_secret)
        } else {
            self.oauth_manager.claim_finalizing(session_id).await?
        };

        // Query real authenticated identity from provider
        let (real_username, real_display, scopes) = match provider_id.as_str() {
            "github" => {
                let http = reqwest::Client::builder()
                    .timeout(Duration::from_secs(15))
                    .build()
                    .unwrap_or_default();
                let resp = match http
                    .get("https://api.github.com/user")
                    .header("User-Agent", "Kyro-Desktop-Agent/0.1.2")
                    .header("Accept", "application/vnd.github+json")
                    .header(
                        "Authorization",
                        format!("Bearer {}", token_res.access_token),
                    )
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        self.oauth_manager
                            .finalize_failure(session_id, e.to_string())
                            .await;
                        return Err(format!(
                            "Failed to reach GitHub API to verify user profile: {}",
                            e
                        ));
                    }
                };

                if !resp.status().is_success() {
                    let st = resp.status();
                    let err_msg =
                        format!("GitHub user profile verification failed with HTTP {}", st);
                    self.oauth_manager
                        .finalize_failure(session_id, err_msg.clone())
                        .await;
                    return Err(err_msg);
                }

                // Extract granted scopes from header if available
                let header_scopes: Vec<String> = resp
                    .headers()
                    .get("x-oauth-scopes")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| {
                        s.split(',')
                            .map(|p| p.trim().to_string())
                            .filter(|p| !p.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();

                let final_scopes = if !header_scopes.is_empty() {
                    header_scopes
                } else {
                    token_res.granted_scopes.clone()
                };

                let user_json: Value = match resp.json().await {
                    Ok(j) => j,
                    Err(e) => {
                        let err_msg =
                            format!("Failed to parse GitHub /user profile response: {}", e);
                        self.oauth_manager
                            .finalize_failure(session_id, err_msg.clone())
                            .await;
                        return Err(err_msg);
                    }
                };
                let login = match user_json["login"].as_str() {
                    Some(l) => l.to_string(),
                    None => {
                        let err_msg = "GitHub profile missing login".to_string();
                        self.oauth_manager
                            .finalize_failure(session_id, err_msg.clone())
                            .await;
                        return Err(err_msg);
                    }
                };

                (login.clone(), format!("GitHub (@{})", login), final_scopes)
            }
            "google" | "google_calendar" => {
                let http = reqwest::Client::builder()
                    .timeout(Duration::from_secs(15))
                    .build()
                    .unwrap_or_default();
                let resp = match http
                    .get("https://www.googleapis.com/oauth2/v2/userinfo")
                    .header(
                        "Authorization",
                        format!("Bearer {}", token_res.access_token),
                    )
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        let err_msg = format!("Google userinfo verification failed: {}", e);
                        self.oauth_manager
                            .finalize_failure(session_id, err_msg.clone())
                            .await;
                        return Err(err_msg);
                    }
                };

                let user_json: Value = match resp.json().await {
                    Ok(j) => j,
                    Err(e) => {
                        let err_msg = format!("Failed to parse Google userinfo response: {}", e);
                        self.oauth_manager
                            .finalize_failure(session_id, err_msg.clone())
                            .await;
                        return Err(err_msg);
                    }
                };

                let email = user_json["email"]
                    .as_str()
                    .unwrap_or("google_user@gmail.com")
                    .to_string();
                let name = user_json["name"].as_str().unwrap_or(&email).to_string();

                (
                    email.clone(),
                    format!("Google ({})", name),
                    token_res.granted_scopes.clone(),
                )
            }
            other => {
                let err_msg = format!("Unsupported OAuth provider '{}'", other);
                self.oauth_manager
                    .finalize_failure(session_id, err_msg.clone())
                    .await;
                return Err(err_msg);
            }
        };

        let account_id = format!("conn_{}_{}", provider_id, uuid::Uuid::new_v4());
        let now = chrono::Utc::now().timestamp_millis();

        // Save secret in vault
        let secret_payload = json!({
            "access_token": token_res.access_token,
            "refresh_token": token_res.refresh_token,
            "client_secret": client_secret,
            "created_at": now
        });
        if let Err(e) = self
            .vault
            .store_account_secret(&account_id, &secret_payload.to_string())
        {
            let err_msg = format!("Failed to securely store OAuth credentials in vault: {}", e);
            self.oauth_manager
                .finalize_failure(session_id, err_msg.clone())
                .await;
            return Err(err_msg);
        }

        let config_obj = json!({
            "client_id": client_id,
            "has_client_secret": client_secret.is_some()
        });

        let record = ConnectedAccountRecord {
            id: account_id.clone(),
            connector_id: provider_id,
            account_identifier: real_username.clone(),
            display_name: real_display,
            status: "connected".to_string(),
            status_message: Some(format!(
                "Successfully authenticated via GitHub OAuth as @{}",
                real_username
            )),
            granted_scopes_json: serde_json::to_string(&scopes)
                .unwrap_or_else(|_| "[]".to_string()),
            credential_key: format!("connector:account:{}", account_id),
            config_json: config_obj.to_string(),
            created_at: now,
            updated_at: now,
            last_connected_at: Some(now),
            last_error: None,
            expires_at: token_res.expires_in_seconds.map(|secs| now + (secs * 1000)),
        };

        if let Err(e) = self.db.create_connected_account(&record) {
            let _ = self.vault.delete_account_secret(&account_id);
            let err_msg = format!("Failed to save account record: {}", e);
            self.oauth_manager
                .finalize_failure(session_id, err_msg.clone())
                .await;
            return Err(err_msg);
        }

        self.oauth_manager.finalize_success(session_id).await;
        let _ = self.get_all_connector_tools().await;

        Ok(record)
    }

    /// Disconnect an account safely:
    /// - Verifies existence
    /// - Marks status as disconnected to prevent in-flight dispatch
    /// - Purges in-memory client and active tool registrations
    /// - Deletes tool policies
    /// - Deletes credentials from Vault (with explicit error checking)
    /// - Deletes record from SQLite
    pub async fn disconnect_account(&self, account_id: &str) -> Result<(), String> {
        // 1. Verify account exists
        let _ = self
            .db
            .get_connected_account(account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Account '{}' not found", account_id))?;

        // 2. Mark account disconnected
        let _ = self.db.update_connected_account_status(
            account_id,
            "disconnected",
            Some("Account disconnected by user"),
            None,
            None,
        );

        // 3. Remove from remote clients and registry
        {
            let mut lock = self.remote_clients.lock().await;
            lock.remove(account_id);
        }
        {
            let mut reg = self.tool_registry.lock().await;
            reg.retain(|_, t| t.account_id != account_id);
        }

        // 4. Delete policies
        self.db
            .delete_connector_tool_policies(account_id)
            .map_err(|e| format!("Failed to delete connector tool policies: {}", e))?;

        // 5. Delete credentials from Vault
        self.vault
            .delete_account_secret(account_id)
            .map_err(|e| format!("Failed to delete credentials from vault: {}", e))?;

        // 6. Delete account record from DB
        self.db
            .delete_connected_account(account_id)
            .map_err(|e| format!("Failed to delete connected account: {}", e))?;

        Ok(())
    }

    /// Test an account's live connection
    pub async fn test_connection(&self, account_id: &str) -> Result<String, String> {
        let account = self
            .db
            .get_connected_account(account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Account '{}' not found", account_id))?;

        if let Some(adapter) = self.native_adapters.get(&account.connector_id) {
            match adapter.test_connection(&account, &self.vault).await {
                Ok(msg) => {
                    let _ = self.db.update_connected_account_status(
                        account_id,
                        "connected",
                        Some(&msg),
                        None,
                        Some(chrono::Utc::now().timestamp_millis()),
                    );
                    Ok(msg)
                }
                Err(e) => {
                    let _ = self.db.update_connected_account_status(
                        account_id,
                        "error",
                        Some("Connection test failed"),
                        Some(&e),
                        None,
                    );
                    Err(e)
                }
            }
        } else if account.connector_id == "remote_mcp" {
            let secret_raw = self
                .vault
                .get_account_secret(account_id)?
                .unwrap_or_default();
            let token = if secret_raw.starts_with('{') {
                serde_json::from_str::<Value>(&secret_raw)
                    .ok()
                    .and_then(|v| {
                        v.get("token")
                            .or_else(|| v.get("access_token"))
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string())
                    })
            } else if !secret_raw.is_empty() {
                Some(secret_raw)
            } else {
                None
            };

            let cfg: Value = serde_json::from_str(&account.config_json).unwrap_or(json!({}));
            let url = cfg.get("server_url").and_then(|v| v.as_str()).unwrap_or("");
            if url.is_empty() {
                return Err("Missing 'server_url' in configuration".to_string());
            }

            let allow_private = cfg
                .get("allow_private_network")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let allow_insecure = cfg
                .get("allow_insecure_http")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let client = RemoteMcpClient::new_with_trust(
                url,
                &account.display_name,
                token,
                allow_private,
                allow_insecure,
            )?;
            match client.connect_or_verify().await {
                Ok(msg) => {
                    let _ = self.db.update_connected_account_status(
                        account_id,
                        "connected",
                        Some(&msg),
                        None,
                        Some(chrono::Utc::now().timestamp_millis()),
                    );
                    let mut lock = self.remote_clients.lock().await;
                    lock.insert(account_id.to_string(), client);
                    Ok(msg)
                }
                Err(e) => {
                    let _ = self.db.update_connected_account_status(
                        account_id,
                        "error",
                        Some("Remote MCP handshake failed"),
                        Some(&e),
                        None,
                    );
                    Err(e)
                }
            }
        } else {
            Err(format!(
                "No connection tester available for '{}'",
                account.connector_id
            ))
        }
    }

    /// Refresh and register all discoverable tools from enabled connectors:
    /// 1. Native API connectors (Type A)
    /// 2. Remote MCP servers (Type B)
    /// 3. Local stdio MCP servers (Type C)
    pub async fn get_all_connector_tools(&self) -> Vec<ToolDefinition> {
        let mut exposed_tools = Vec::new();
        let mut new_registry = HashMap::new();
        let mut raw_name_counts: HashMap<String, usize> = HashMap::new();

        // 1. Discover tools from connected Native accounts
        if let Ok(accounts) = self.db.list_connected_accounts() {
            for acc in accounts {
                if acc.status != "connected" {
                    continue;
                }

                if let Some(adapter) = self.native_adapters.get(&acc.connector_id) {
                    let tools = adapter.list_tools(&acc);
                    for t in tools {
                        let scoped_name = format!("{}__{}", acc.id, t.name);
                        *raw_name_counts.entry(t.name.clone()).or_insert(0) += 1;

                        let fp = compute_schema_fingerprint(&t.parameters);
                        let can_auto = t.risk_level != RiskLevel::Destructive;

                        let entry = RegisteredConnectorTool {
                            scoped_name: scoped_name.clone(),
                            alias_name: None,
                            account_id: acc.id.clone(),
                            connector_id: acc.connector_id.clone(),
                            original_tool_name: t.name.clone(),
                            definition: ToolDefinition {
                                name: scoped_name.clone(),
                                description: format!("[{}] {}", acc.display_name, t.description),
                                parameters: t.parameters,
                            },
                            risk_level: t.risk_level,
                            is_read_only: t.is_read_only,
                            kind: ConnectorKind::Native,
                            can_auto_approve: can_auto,
                            schema_fingerprint: fp,
                        };

                        new_registry.insert(scoped_name.clone(), entry.clone());
                        exposed_tools.push(entry.definition.clone());
                    }
                } else if acc.connector_id == "remote_mcp" {
                    // Type B Remote MCP
                    let client_opt = {
                        let lock = self.remote_clients.lock().await;
                        lock.get(&acc.id).cloned()
                    };

                    let client = match client_opt {
                        Some(c) => c,
                        None => {
                            let secret_raw = self
                                .vault
                                .get_account_secret(&acc.id)
                                .unwrap_or(None)
                                .unwrap_or_default();
                            let token = if secret_raw.starts_with('{') {
                                serde_json::from_str::<Value>(&secret_raw)
                                    .ok()
                                    .and_then(|v| {
                                        v.get("token")
                                            .or_else(|| v.get("access_token"))
                                            .and_then(|t| t.as_str())
                                            .map(|s| s.to_string())
                                    })
                            } else if !secret_raw.is_empty() {
                                Some(secret_raw)
                            } else {
                                None
                            };

                            let cfg: Value =
                                serde_json::from_str(&acc.config_json).unwrap_or(json!({}));
                            let url = cfg.get("server_url").and_then(|v| v.as_str()).unwrap_or("");
                            if url.is_empty() {
                                continue;
                            }
                            let allow_private = cfg
                                .get("allow_private_network")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            let allow_insecure = cfg
                                .get("allow_insecure_http")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);

                            let c = match RemoteMcpClient::new_with_trust(
                                url,
                                &acc.display_name,
                                token,
                                allow_private,
                                allow_insecure,
                            ) {
                                Ok(client) => client,
                                Err(e) => {
                                    log::warn!(
                                        "Invalid Remote MCP endpoint for account '{}': {}",
                                        acc.display_name,
                                        e
                                    );
                                    continue;
                                }
                            };
                            let mut lock = self.remote_clients.lock().await;
                            lock.insert(acc.id.clone(), c.clone());
                            c
                        }
                    };

                    if let Ok(tools) = client.list_tools().await {
                        for t in tools {
                            let scoped_name = format!("{}__{}", acc.id, t.name);
                            *raw_name_counts.entry(t.name.clone()).or_insert(0) += 1;

                            let (risk_level, is_read_only, can_auto) =
                                classify_external_tool(&t.name, &t.description, &t.parameters);
                            let fp = compute_schema_fingerprint(&t.parameters);

                            let entry = RegisteredConnectorTool {
                                scoped_name: scoped_name.clone(),
                                alias_name: None,
                                account_id: acc.id.clone(),
                                connector_id: acc.connector_id.clone(),
                                original_tool_name: t.name.clone(),
                                definition: ToolDefinition {
                                    name: scoped_name.clone(),
                                    description: format!(
                                        "[{}] {}",
                                        acc.display_name, t.description
                                    ),
                                    parameters: t.parameters,
                                },
                                risk_level,
                                is_read_only,
                                kind: ConnectorKind::RemoteMcp,
                                can_auto_approve: can_auto,
                                schema_fingerprint: fp,
                            };

                            new_registry.insert(scoped_name.clone(), entry.clone());
                            exposed_tools.push(entry.definition.clone());
                        }
                    }
                }
            }
        }

        // 2. Discover tools from Local MCP servers (Type C) via McpManager
        let local_tools = self.local_mcp.get_all_mcp_tools().await;
        for t in local_tools {
            *raw_name_counts.entry(t.name.clone()).or_insert(0) += 1;
            let (risk_level, is_read_only, can_auto) =
                classify_external_tool(&t.name, &t.description, &t.parameters);
            let fp = compute_schema_fingerprint(&t.parameters);

            let entry = RegisteredConnectorTool {
                scoped_name: t.name.clone(),
                alias_name: None,
                account_id: "local_mcp".to_string(),
                connector_id: "local_mcp".to_string(),
                original_tool_name: t.name.clone(),
                definition: t.clone(),
                risk_level,
                is_read_only,
                kind: ConnectorKind::LocalMcp,
                can_auto_approve: can_auto,
                schema_fingerprint: fp,
            };
            new_registry.insert(t.name.clone(), entry);
            exposed_tools.push(t);
        }

        // 3. Register unambiguous aliases for single-account setups
        let mut aliases = Vec::new();
        for entry in new_registry.values() {
            if raw_name_counts.get(&entry.original_tool_name) == Some(&1)
                && entry.scoped_name != entry.original_tool_name
            {
                let mut alias_entry = entry.clone();
                alias_entry.alias_name = Some(entry.original_tool_name.clone());
                aliases.push((entry.original_tool_name.clone(), alias_entry));
            }
        }
        for (alias, entry) in aliases {
            new_registry.insert(alias, entry);
        }

        let mut reg_lock = self.tool_registry.lock().await;
        *reg_lock = new_registry;

        exposed_tools
    }

    /// Resolve a tool by scoped or alias name
    pub async fn resolve_tool(&self, name: &str) -> Option<RegisteredConnectorTool> {
        let reg = self.tool_registry.lock().await;
        reg.get(name).cloned()
    }

    /// Evaluates centralized governance policy for a connector tool immediately before execution
    pub async fn evaluate_tool_authorization(
        &self,
        tool_name: &str,
        args: &Value,
    ) -> Option<AuthDecision> {
        let entry = self.resolve_tool(tool_name).await?;

        // 1. Account existence and status check
        if entry.kind != ConnectorKind::LocalMcp {
            let account = match self.db.get_connected_account(&entry.account_id) {
                Ok(Some(a)) => a,
                _ => {
                    return Some(AuthDecision::Deny {
                        reason: format!("Account '{}' not found or was removed", entry.account_id),
                    });
                }
            };

            if account.status != "connected" {
                return Some(AuthDecision::Deny {
                    reason: format!(
                        "Account '{}' is not in connected state (status: {})",
                        account.display_name, account.status
                    ),
                });
            }
        }

        // 2. Validate arguments against schema
        if let Err(schema_err) = crate::mcp::validate_schema(&entry.definition.parameters, args) {
            return Some(AuthDecision::Deny {
                reason: format!("Tool arguments failed schema validation: {}", schema_err),
            });
        }

        // 3. Explicit connector tool policy check
        if let Ok(Some(policy_rec)) = self
            .db
            .get_connector_tool_policy(&entry.account_id, &entry.original_tool_name)
        {
            // Verify schema fingerprint if tool policy has one
            if let Some(expected_fp) = &policy_rec.schema_fingerprint {
                if expected_fp != &entry.schema_fingerprint {
                    return Some(AuthDecision::RequireApproval {
                        reason: format!(
                            "Tool definition for '{}' has changed since authorization policy was configured. Re-authorization required.",
                            entry.original_tool_name
                        ),
                        risk: entry.risk_level,
                        summary: format!(
                            "Execute modified [{}] action '{}' with args: {}",
                            entry.connector_id, entry.original_tool_name, args
                        ),
                    });
                }
            }

            match policy_rec.policy.trim().to_lowercase().as_str() {
                "deny" => {
                    return Some(AuthDecision::Deny {
                        reason: format!(
                            "Connector action '{}' for account '{}' is blocked by security policy",
                            entry.original_tool_name, entry.account_id
                        ),
                    });
                }
                "ask" => {
                    return Some(AuthDecision::RequireApproval {
                        reason: format!(
                            "Connector action '{}' requires explicit user authorization",
                            entry.original_tool_name
                        ),
                        risk: entry.risk_level,
                        summary: format!(
                            "Run [{}] action '{}' with args: {}",
                            entry.connector_id, entry.original_tool_name, args
                        ),
                    });
                }
                "allow" => {
                    // Cannot auto-approve if can_auto_approve is false or destructive!
                    if !entry.can_auto_approve || entry.risk_level == RiskLevel::Destructive {
                        return Some(AuthDecision::RequireApproval {
                            reason: format!(
                                "Action '{}' cannot be auto-approved and requires explicit user authorization",
                                entry.original_tool_name
                            ),
                            risk: entry.risk_level,
                            summary: format!(
                                "Execute [{}] action '{}' with args: {}",
                                entry.connector_id, entry.original_tool_name, args
                            ),
                        });
                    }
                    return Some(AuthDecision::Allow);
                }
                _ => {}
            }
        }

        // 4. Default conservative policy:
        // - Safe Read-only operations on connected accounts can run automatically if can_auto_approve is true
        // - Modifying write operations require human approval
        if entry.can_auto_approve && entry.is_read_only && entry.risk_level == RiskLevel::Safe {
            Some(AuthDecision::Allow)
        } else {
            Some(AuthDecision::RequireApproval {
                reason: format!(
                    "External write/modifying action '{}' ({}) requires user authorization",
                    entry.original_tool_name, entry.connector_id
                ),
                risk: entry.risk_level,
                summary: format!(
                    "Execute [{}] action '{}' with parameters: {}",
                    entry.connector_id, entry.original_tool_name, args
                ),
            })
        }
    }

    /// Execute a connector tool safely through its registered adapter/client.
    /// Convenience wrapper defaulting to unapproved call (safe/allowed tools succeed; ask/deny blocked).
    pub async fn call_tool(&self, name: &str, args: Value) -> Option<Result<String, String>> {
        self.call_tool_with_auth(name, args, None).await
    }

    /// Execute a connector tool with verified invocation authorization proof.
    /// Strictly verifies:
    /// - Account existence and active connected state
    /// - Schema validity
    /// - Per-account and connector policy
    /// - Authorization token binding (tool name, args hash, atomic DB consumption to prevent replay)
    /// - Automatic token refresh on 401 with retry
    pub async fn call_tool_with_auth(
        &self,
        name: &str,
        args: Value,
        auth_proof: Option<&InvocationAuthProof>,
    ) -> Option<Result<String, String>> {
        let entry = {
            let reg = self.tool_registry.lock().await;
            reg.get(name).cloned()?
        };

        // 1. Re-verify account existence and connected status
        if entry.kind != ConnectorKind::LocalMcp {
            let account = match self.db.get_connected_account(&entry.account_id) {
                Ok(Some(a)) => a,
                _ => {
                    return Some(Err(format!(
                        "Connected account '{}' not found or was removed",
                        entry.account_id
                    )));
                }
            };

            if account.status != "connected" {
                return Some(Err(format!(
                    "Connected account '{}' is not in active connected state (current status: '{}')",
                    account.display_name, account.status
                )));
            }
        }

        // 2. Evaluate centralized authorization decision
        let decision = self.evaluate_tool_authorization(name, &args).await?;
        match decision {
            AuthDecision::Deny { reason } => {
                return Some(Err(format!(
                    "Security policy denied action '{}': {}",
                    entry.original_tool_name, reason
                )));
            }
            AuthDecision::RequireApproval { reason, .. } => {
                // Must provide a valid, verifiable approval proof
                let proof = match auth_proof {
                    Some(p) => p,
                    None => {
                        return Some(Err(format!(
                            "Action '{}' requires human authorization before dispatch: {}",
                            entry.original_tool_name, reason
                        )));
                    }
                };

                // Validate proof binding
                if proof.tool_name != name && proof.tool_name != entry.original_tool_name {
                    return Some(Err(format!(
                        "Authorization proof mismatch: token issued for '{}', attempted execution of '{}'",
                        proof.tool_name, name
                    )));
                }

                let current_args_hash = InvocationAuthProof::compute_args_hash(&args);
                if proof.args_hash != current_args_hash {
                    return Some(Err(
                        "Authorization proof argument hash mismatch: arguments have been modified since approval"
                            .to_string(),
                    ));
                }

                // Verify and atomically consume approval request from DB
                let now = chrono::Utc::now().timestamp_millis();
                let expected_tool_names = [name, entry.original_tool_name.as_str()];
                if let Err(e) = self.db.consume_approval_request_bound(
                    &proof.approval_id,
                    &proof.run_id,
                    &expected_tool_names,
                    &args,
                    now,
                ) {
                    return Some(Err(e));
                }
            }
            AuthDecision::Allow | AuthDecision::AllowSandboxOnly { .. } => {
                // Authorized to proceed directly
            }
            AuthDecision::RequireTakeover { reason } => {
                return Some(Err(format!(
                    "Action '{}' requires human takeover: {}",
                    entry.original_tool_name, reason
                )));
            }
        }

        // 3. Dispatch through adapter/client
        match entry.kind {
            ConnectorKind::Native => {
                let adapter = self.native_adapters.get(&entry.connector_id)?;
                let account = match self.db.get_connected_account(&entry.account_id) {
                    Ok(Some(a)) => a,
                    _ => return Some(Err("Account record missing".to_string())),
                };
                let res = adapter
                    .execute_tool(
                        &account,
                        &entry.original_tool_name,
                        args.clone(),
                        &self.vault,
                    )
                    .await;

                // Handle 401 Unauthorized / Token Expiration for OAuth accounts
                if let Err(ref e) = res {
                    let err_low = e.to_lowercase();
                    let is_unauthorized = err_low.contains("401")
                        || err_low.contains("bad credentials")
                        || err_low.contains("token expired")
                        || err_low.contains("unauthorized")
                        || err_low.contains("invalid credentials")
                        || err_low.contains("autherror");
                    if is_unauthorized {
                        log::warn!(
                            "Native connector adapter received 401 Unauthorized for account '{}'. Attempting token refresh.",
                            entry.account_id
                        );
                        if let Ok(_new_token) = self.refresh_account_token(&entry.account_id).await
                        {
                            // Retry once with refreshed token
                            let retry_res = adapter
                                .execute_tool(
                                    &account,
                                    &entry.original_tool_name,
                                    args,
                                    &self.vault,
                                )
                                .await;
                            return Some(retry_res);
                        } else {
                            // Mark account as needs_reauth
                            let _ = self.db.update_connected_account_status(
                                &entry.account_id,
                                "needs_reauth",
                                Some("OAuth token expired and could not be refreshed. Please re-authorize."),
                                None,
                                None,
                            );
                            return Some(Err(format!(
                                "Account '{}' credentials expired and refresh failed. Re-authorization required.",
                                account.display_name
                            )));
                        }
                    }
                }
                Some(res)
            }
            ConnectorKind::RemoteMcp => {
                let client_opt = {
                    let lock = self.remote_clients.lock().await;
                    lock.get(&entry.account_id).cloned()
                };
                let client = match client_opt {
                    Some(c) => c,
                    None => {
                        return Some(Err(format!(
                            "Remote MCP client for account '{}' is disconnected",
                            entry.account_id
                        )));
                    }
                };

                let res = client
                    .call_tool(
                        &entry.original_tool_name,
                        &entry.definition.parameters,
                        args,
                    )
                    .await;
                Some(res)
            }
            ConnectorKind::LocalMcp => {
                // Dispatch through local McpManager
                self.local_mcp.call_mcp_tool(name, args).await
            }
        }
    }
}
