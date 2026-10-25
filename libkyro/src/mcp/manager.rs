use super::client::McpClient;
use super::schema::validate_schema;
use crate::agent::ToolDefinition;
use crate::storage::{Database, McpServerRecord, Vault, VAULT_SECRET_SENTINEL};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct RegisteredMcpTool {
    pub server_id: String,
    pub server_name: String,
    pub raw_name: String,
    pub scoped_name: String,
    pub definition: ToolDefinition,
}

#[derive(Clone)]
pub struct McpManager {
    clients: Arc<Mutex<HashMap<String, McpClient>>>,
    server_status: Arc<Mutex<HashMap<String, String>>>,
    tool_registry: Arc<Mutex<HashMap<String, RegisteredMcpTool>>>,
    db: Database,
    vault: Vault,
}

impl McpManager {
    pub fn new(db: Database, vault: Vault) -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            server_status: Arc::new(Mutex::new(HashMap::new())),
            tool_registry: Arc::new(Mutex::new(HashMap::new())),
            db,
            vault,
        }
    }

    pub fn new_with_db(db: Database) -> Self {
        let vault = Vault::new_encrypted_fallback(db.clone());
        Self::new(db, vault)
    }

    /// Load enabled MCP servers from the database, refresh tool discovery, and return all available tools
    pub async fn get_all_mcp_tools(&self) -> Vec<ToolDefinition> {
        let mut exposed_tools = Vec::new();
        let mut new_registry = HashMap::new();
        let mut raw_name_counts: HashMap<String, usize> = HashMap::new();

        if let Ok(servers) = self.db.list_mcp_servers() {
            let mut clients_guard = self.clients.lock().await;
            let mut status_guard = self.server_status.lock().await;

            for s in servers {
                if !s.enabled {
                    status_guard.insert(s.id.clone(), "disabled".to_string());
                    clients_guard.remove(&s.id);
                    continue;
                }

                // If not already connected, attempt spawn
                if !clients_guard.contains_key(&s.id) {
                    let args: Vec<String> = match serde_json::from_str(&s.args_json) {
                        Ok(a) => a,
                        Err(_) => shell_words::split(&s.args_json).unwrap_or_default(),
                    };
                    let raw_env: HashMap<String, String> =
                        serde_json::from_str(&s.env_json).unwrap_or_default();

                    let mut env_vars = HashMap::new();
                    let mut secret_error = None;
                    for (k, v) in raw_env {
                        if v == VAULT_SECRET_SENTINEL {
                            match self.vault.get_mcp_env_secret(&s.id, &k) {
                                Ok(Some(secret)) if !secret.is_empty() => {
                                    env_vars.insert(k, secret);
                                }
                                Ok(Some(_)) => {
                                    let msg =
                                        format!("Required secret for '{}' is empty in vault", k);
                                    log::error!("MCP server '{}': {}", s.name, msg);
                                    secret_error = Some(msg);
                                    break;
                                }
                                Ok(None) => {
                                    let msg = format!(
                                        "Required secret for '{}' could not be resolved from vault",
                                        k
                                    );
                                    log::error!("MCP server '{}': {}", s.name, msg);
                                    secret_error = Some(msg);
                                    break;
                                }
                                Err(e) => {
                                    let msg = format!(
                                        "Failed to retrieve vault secret for '{}': {}",
                                        k, e
                                    );
                                    log::error!("MCP server '{}': {}", s.name, msg);
                                    secret_error = Some(msg);
                                    break;
                                }
                            }
                        } else {
                            env_vars.insert(k, v);
                        }
                    }

                    if let Some(err_msg) = secret_error {
                        status_guard.insert(s.id.clone(), format!("failed: {}", err_msg));
                        continue;
                    }

                    match McpClient::spawn(&s.name, &s.command, &args, &env_vars).await {
                        Ok(client) => {
                            status_guard.insert(s.id.clone(), "connected".to_string());
                            clients_guard.insert(s.id.clone(), client);
                        }
                        Err(e) => {
                            log::warn!("Failed to connect to MCP server '{}': {}", s.name, e);
                            status_guard.insert(s.id.clone(), format!("failed: {}", e));
                        }
                    }
                }

                if let Some(client) = clients_guard.get_mut(&s.id) {
                    if !client.is_alive() {
                        status_guard.insert(s.id.clone(), "stopped (process exited)".to_string());
                        clients_guard.remove(&s.id);
                        continue;
                    }
                    if let Ok(mcp_tools) = client.list_tools().await {
                        for t in mcp_tools {
                            let scoped_name = format!("mcp__{}__{}", s.id, t.name);
                            *raw_name_counts.entry(t.name.clone()).or_insert(0) += 1;

                            let entry = RegisteredMcpTool {
                                server_id: s.id.clone(),
                                server_name: s.name.clone(),
                                raw_name: t.name.clone(),
                                scoped_name: scoped_name.clone(),
                                definition: t.clone(),
                            };

                            new_registry.insert(scoped_name.clone(), entry.clone());
                            // Also expose tool under scoped name
                            exposed_tools.push(ToolDefinition {
                                name: scoped_name,
                                description: format!("[{}] {}", s.name, t.description),
                                parameters: t.parameters,
                            });
                        }
                    }
                }
            }
        }

        // Add unambiguous aliases to registry — but never a name reserved by
        // a platform built-in tool (Bug 13): built-in dispatch always wins
        // the name, so such an alias would be authorised as connector policy
        // while the built-in implementation executes. The tool remains
        // reachable through its namespaced `mcp__{server}__{tool}` identity.
        let mut aliases = Vec::new();
        for entry in new_registry.values() {
            if raw_name_counts.get(&entry.raw_name) == Some(&1)
                && !crate::agent::tools::is_reserved_builtin_tool_name(&entry.raw_name)
            {
                aliases.push((entry.raw_name.clone(), entry.clone()));
            }
        }
        for (alias, entry) in aliases {
            new_registry.insert(alias, entry);
        }

        let mut reg_guard = self.tool_registry.lock().await;
        *reg_guard = new_registry;

        exposed_tools
    }

    /// Call an MCP tool if it matches any registered tool definition, validating schema strictly
    pub async fn call_mcp_tool(&self, name: &str, args: Value) -> Option<Result<String, String>> {
        let (server_id, raw_name, schema) = {
            let reg = self.tool_registry.lock().await;
            let entry = reg.get(name)?;
            (
                entry.server_id.clone(),
                entry.raw_name.clone(),
                entry.definition.parameters.clone(),
            )
        };

        // 1. Strictly validate arguments against declared schema
        if let Err(schema_err) = validate_schema(&schema, &args) {
            return Some(Err(format!(
                "MCP schema validation failure for '{}': {}",
                name, schema_err
            )));
        }

        // 2. Dispatch to connected server
        let mut clients_guard = self.clients.lock().await;
        if let Some(client) = clients_guard.get_mut(&server_id) {
            if !client.is_alive() {
                clients_guard.remove(&server_id);
                let mut status_guard = self.server_status.lock().await;
                status_guard.insert(server_id.clone(), "stopped (process exited)".to_string());
                return Some(Err(format!(
                    "MCP server '{}' has stopped (process exited)",
                    server_id
                )));
            }
            Some(client.call_tool(&raw_name, args).await)
        } else {
            Some(Err(format!(
                "MCP server '{}' is disconnected or unavailable",
                server_id
            )))
        }
    }

    /// Disconnect an MCP server by ID
    pub async fn disconnect_server(&self, server_id: &str) {
        let mut clients_guard = self.clients.lock().await;
        let mut status_guard = self.server_status.lock().await;
        clients_guard.remove(server_id);
        status_guard.insert(server_id.to_string(), "disconnected".to_string());

        let mut reg_guard = self.tool_registry.lock().await;
        reg_guard.retain(|_, entry| entry.server_id != server_id);
    }

    /// Get current connection status for all servers, checking live subprocess state
    pub async fn get_server_statuses(&self) -> HashMap<String, String> {
        let mut clients_guard = self.clients.lock().await;
        let mut status_guard = self.server_status.lock().await;
        let mut dead_servers = Vec::new();
        for (id, client) in clients_guard.iter_mut() {
            if !client.is_alive() {
                dead_servers.push(id.clone());
            }
        }
        for dead_id in dead_servers {
            clients_guard.remove(&dead_id);
            status_guard.insert(dead_id, "stopped (process exited)".to_string());
        }
        status_guard.clone()
    }

    /// Save or update an MCP server configuration safely:
    /// - Stores any sensitive environment variables strictly into the Vault
    /// - Enforces that recognized credentials cannot be downgraded to plaintext
    /// - Propagates vault cleanup errors and verifies readback before persisting to SQLite
    /// - Restarts the subprocess if currently active so changes take effect immediately
    pub async fn save_server(&self, mut server: McpServerRecord) -> Result<(), String> {
        let trimmed_id = server.id.trim();
        if trimmed_id.is_empty() {
            return Err("MCP server ID cannot be empty".to_string());
        }
        if server.name.trim().is_empty() {
            return Err("MCP server name cannot be empty".to_string());
        }
        if server.command.trim().is_empty() {
            return Err("MCP server command cannot be empty".to_string());
        }

        // Validate and normalize args_json
        let parsed_args: Vec<String> = match serde_json::from_str::<Vec<String>>(&server.args_json)
        {
            Ok(arr) => arr,
            Err(_) => shell_words::split(&server.args_json)
                .map_err(|e| format!("Invalid server arguments: {}", e))?,
        };
        server.args_json = serde_json::to_string(&parsed_args).unwrap_or_else(|_| "[]".to_string());

        // Retrieve existing server configuration to identify removed or updated keys
        let old_env_map: HashMap<String, String> = self
            .db
            .list_mcp_servers()
            .ok()
            .and_then(|list| list.into_iter().find(|srv| srv.id == server.id))
            .and_then(|srv| serde_json::from_str(&srv.env_json).ok())
            .unwrap_or_default();

        // Validate and process env_json, routing secrets to Vault
        let raw_env_val: Value = serde_json::from_str(&server.env_json)
            .map_err(|e| format!("Invalid environment variables JSON: {}", e))?;

        let raw_entries: Vec<(String, String, Option<bool>)> = match raw_env_val {
            Value::Object(map) => {
                let mut entries = Vec::new();
                for (k, v) in map {
                    let trimmed_k = k.trim().to_string();
                    if trimmed_k.is_empty() {
                        continue;
                    }
                    match v {
                        Value::String(s) => entries.push((trimmed_k, s, None)),
                        Value::Object(obj) => {
                            let val_str = obj
                                .get("value")
                                .and_then(|x| x.as_str())
                                .unwrap_or("")
                                .to_string();
                            let is_sec = obj.get("is_secret").and_then(|x| x.as_bool());
                            entries.push((trimmed_k, val_str, is_sec));
                        }
                        other => entries.push((trimmed_k, other.to_string(), None)),
                    }
                }
                entries
            }
            _ => return Err("Environment variables must be a JSON object".to_string()),
        };

        // 1. Identify keys that were removed or emptied, and purge from Vault (propagating errors)
        let new_keys: std::collections::HashSet<String> = raw_entries
            .iter()
            .filter(|(_, v, _)| !v.is_empty())
            .map(|(k, _, _)| k.clone())
            .collect();

        for old_k in old_env_map.keys() {
            if !new_keys.contains(old_k) {
                self.vault.delete_mcp_env_secret(&server.id, old_k)?;
            }
        }

        // 2. Process active entries
        let mut sanitized_env = HashMap::new();
        for (k, v, explicit_is_secret) in raw_entries {
            if v.is_empty() {
                // Empty value securely purges any existing secret and is omitted
                self.vault.delete_mcp_env_secret(&server.id, &k)?;
                continue;
            }

            // Backend rule: User preference cannot downgrade recognized credentials to plaintext
            let backend_requires_secret =
                Vault::is_secret_env_key(&k) || Vault::is_sensitive_env_value(&v);
            let is_secret = backend_requires_secret || explicit_is_secret.unwrap_or(false);

            if v == VAULT_SECRET_SENTINEL {
                // Existing vault reference: verify it actually exists in Vault
                if !self.vault.verify_mcp_env_secret(&server.id, &k) {
                    return Err(format!(
                        "Vault secret for '{}' is missing or unverified. Please re-enter the secret.",
                        k
                    ));
                }
                sanitized_env.insert(k, VAULT_SECRET_SENTINEL.to_string());
            } else if is_secret {
                // Store secret in secure vault with verified readback
                self.vault.store_mcp_env_secret(&server.id, &k, &v)?;
                let verified = self.vault.get_mcp_env_secret(&server.id, &k)?;
                if verified.as_deref() != Some(&v) {
                    return Err(format!(
                        "Secure storage verification failed for secret key '{}'",
                        k
                    ));
                }
                sanitized_env.insert(k, VAULT_SECRET_SENTINEL.to_string());
            } else {
                // Non-secret: purge any old vault entry for this key
                self.vault.delete_mcp_env_secret(&server.id, &k)?;
                sanitized_env.insert(k, v);
            }
        }

        server.env_json =
            serde_json::to_string(&sanitized_env).unwrap_or_else(|_| "{}".to_string());

        // Save sanitized record to SQLite
        self.db
            .create_mcp_server(&server)
            .map_err(|e| e.to_string())?;

        // Disconnect running process so it can restart cleanly with updated config/secrets
        self.disconnect_server(&server.id).await;

        Ok(())
    }

    /// Delete an MCP server configuration safely:
    /// - Terminates running child process
    /// - Purges all associated environment secrets from the Vault
    /// - Surfaces any Vault cleanup errors
    /// - Removes record from SQLite
    pub async fn delete_server(&self, server_id: &str) -> Result<(), String> {
        // 1. Terminate subprocess and clean status/registry
        self.disconnect_server(server_id).await;

        // 2. Remove all namespaced secrets from Vault and surface errors
        if let Ok(servers) = self.db.list_mcp_servers() {
            if let Some(srv) = servers.into_iter().find(|s| s.id == server_id) {
                if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&srv.env_json) {
                    for k in map.keys() {
                        self.vault
                            .delete_mcp_env_secret(server_id, k)
                            .map_err(|e| {
                                format!("Failed to clean up vault secret for '{}': {}", k, e)
                            })?;
                    }
                }
            }
        }

        // 3. Delete from DB
        self.db
            .delete_mcp_server(server_id)
            .map_err(|e| e.to_string())
    }
}
