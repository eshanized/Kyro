use crate::agent::ToolDefinition;
use crate::mcp::validate_schema;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

const MAX_RESPONSE_BYTES: usize = 512 * 1024; // 512 KB limit
pub const MCP_SPEC_2026_07_28: &str = "2026-07-28";
pub const MCP_SPEC_2024_11_05: &str = "2024-11-05";

/// Strict, validated Remote MCP endpoint preventing SSRF, userinfo injection,
/// credentials over plain HTTP, and unintended protocol access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedRemoteEndpoint {
    raw_url: String,
    parsed_url: reqwest::Url,
    is_loopback: bool,
    is_private_network: bool,
}

impl ValidatedRemoteEndpoint {
    pub fn parse_and_validate(
        raw_url: &str,
        has_credentials: bool,
        allow_private_network: bool,
        allow_insecure_http: bool,
    ) -> Result<Self, String> {
        let trimmed = raw_url.trim();
        if trimmed.is_empty() {
            return Err("Remote MCP server URL cannot be empty".to_string());
        }

        let parsed = reqwest::Url::parse(trimmed)
            .map_err(|e| format!("Invalid Remote MCP server URL '{}': {}", trimmed, e))?;

        // 1. Strict scheme validation: only http and https permitted
        let scheme = parsed.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(format!(
                "Unsupported URL scheme '{}': Remote MCP requires 'https' (or 'http' on loopback)",
                scheme
            ));
        }

        // 2. Reject URL user-information (e.g. http://user:pass@host/)
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(
                "URL user-information (username/password) is prohibited in Remote MCP endpoint URLs"
                    .to_string(),
            );
        }

        // 3. Must have a valid host
        let host_str = parsed.host_str().ok_or_else(|| {
            format!(
                "Remote MCP endpoint '{}' is missing a valid hostname",
                trimmed
            )
        })?;

        // 4. Loopback detection
        let is_loopback = host_str == "localhost"
            || host_str == "127.0.0.1"
            || host_str == "::1"
            || host_str.starts_with("127.");

        // 5. Private network detection (RFC1918 / unique local)
        let is_private = if let Ok(ip) = host_str.parse::<std::net::IpAddr>() {
            match ip {
                std::net::IpAddr::V4(v4) => {
                    v4.is_loopback() || v4.is_private() || v4.is_link_local()
                }
                std::net::IpAddr::V6(v6) => {
                    v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00
                }
            }
        } else {
            host_str.ends_with(".local")
                || host_str.ends_with(".internal")
                || host_str.ends_with(".lan")
                || host_str.ends_with(".home")
        };

        // 5b. Cloud metadata and link-local protection: unconditionally prohibited
        let is_metadata_or_link_local = if let Ok(ip) = host_str.parse::<std::net::IpAddr>() {
            match ip {
                std::net::IpAddr::V4(v4) => {
                    v4.is_link_local() || v4.octets() == [169, 254, 169, 254]
                }
                std::net::IpAddr::V6(v6) => (v6.segments()[0] & 0xffc0) == 0xfe80,
            }
        } else {
            host_str.eq_ignore_ascii_case("metadata.google.internal")
                || host_str.eq_ignore_ascii_case("169.254.169.254")
        };

        if is_metadata_or_link_local {
            return Err(format!(
                "SSRF protection: Link-local and cloud metadata address '{}' is strictly prohibited.",
                host_str
            ));
        }

        // 6. Explicit private network authorization rule
        if is_private && !is_loopback && !allow_private_network {
            return Err(format!(
                "Private IP / intranet network host '{}' requires explicit authorization (allow_private_network = true).",
                host_str
            ));
        }

        // 7. Enforce transport security policy
        if scheme == "http" {
            if is_loopback {
                // Loopback HTTP is explicitly permitted for local development/sidecars
            } else if is_private && allow_private_network {
                if !allow_insecure_http {
                    return Err(format!(
                        "Plain HTTP is not allowed for private network host '{}' without explicit configuration. Use HTTPS or enable allow_insecure_http.",
                        host_str
                    ));
                }
            } else {
                return Err(format!(
                    "Plain HTTP is prohibited for public remote host '{}'. HTTPS is strictly required for public Remote MCP endpoints.",
                    host_str
                ));
            }
        }

        // 8. Prevent transmitting credentials over unencrypted public HTTP
        if has_credentials && scheme == "http" && !is_loopback && !allow_insecure_http {
            return Err(format!(
                "Transmitting bearer authentication credentials over plain HTTP to '{}' is prohibited. HTTPS is required.",
                host_str
            ));
        }

        Ok(Self {
            raw_url: trimmed.trim_end_matches('/').to_string(),
            parsed_url: parsed,
            is_loopback,
            is_private_network: is_private,
        })
    }

    pub fn url(&self) -> &str {
        &self.raw_url
    }

    pub fn parsed(&self) -> &reqwest::Url {
        &self.parsed_url
    }

    pub fn is_loopback(&self) -> bool {
        self.is_loopback
    }

    pub fn is_private_network(&self) -> bool {
        self.is_private_network
    }
}

#[derive(Clone, Debug)]
pub struct RemoteMcpClient {
    pub endpoint: ValidatedRemoteEndpoint,
    pub server_name: String,
    pub auth_token: Option<String>,
    client: Client,
    request_id: Arc<AtomicU64>,
    session_id: Arc<Mutex<Option<String>>>,
    negotiated_protocol_version: Arc<Mutex<String>>,
}

impl RemoteMcpClient {
    pub fn new(
        server_url: &str,
        server_name: &str,
        auth_token: Option<String>,
    ) -> Result<Self, String> {
        Self::new_with_trust(server_url, server_name, auth_token, false, false)
    }

    pub fn new_with_version(
        server_url: &str,
        server_name: &str,
        auth_token: Option<String>,
        version: &str,
    ) -> Result<Self, String> {
        let mut inst = Self::new_with_trust(server_url, server_name, auth_token, false, false)?;
        inst.negotiated_protocol_version = Arc::new(Mutex::new(version.to_string()));
        Ok(inst)
    }

    pub fn new_with_trust(
        server_url: &str,
        server_name: &str,
        auth_token: Option<String>,
        allow_private_network: bool,
        allow_insecure_http: bool,
    ) -> Result<Self, String> {
        let has_token = auth_token
            .as_ref()
            .map(|t| !t.trim().is_empty())
            .unwrap_or(false);
        let endpoint = ValidatedRemoteEndpoint::parse_and_validate(
            server_url,
            has_token,
            allow_private_network,
            allow_insecure_http,
        )?;

        // Build client with strict redirect policy:
        // Follows redirects ONLY within the exact same origin.
        // Strips credentials and aborts on cross-origin redirects.
        let client = Client::builder()
            .timeout(Duration::from_secs(35))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().is_empty() {
                    return attempt.follow();
                }
                let prev = &attempt.previous()[0];
                let next = attempt.url();
                if prev.origin() == next.origin() {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
            .map_err(|e| format!("Failed to build HTTP client for Remote MCP: {}", e))?;

        Ok(Self {
            endpoint,
            server_name: server_name.to_string(),
            auth_token,
            client,
            request_id: Arc::new(AtomicU64::new(1)),
            session_id: Arc::new(Mutex::new(None)),
            negotiated_protocol_version: Arc::new(Mutex::new(MCP_SPEC_2026_07_28.to_string())),
        })
    }

    async fn build_headers(
        &self,
        method: &str,
        tool_name: Option<&str>,
        extra_headers: &[(String, String)],
    ) -> Result<HeaderMap, String> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("Kyro-Desktop-RemoteMCP/0.1.0"),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/json, text/event-stream"),
        );

        let proto_ver = {
            let version_guard = self.negotiated_protocol_version.lock().await;
            version_guard.clone()
        };

        if let Ok(h) = HeaderValue::from_str(&proto_ver) {
            headers.insert("MCP-Protocol-Version", h);
        }

        if let Ok(h) = HeaderValue::from_str(method) {
            headers.insert("Mcp-Method", h);
        }

        if let Some(name) = tool_name {
            if let Ok(h) = HeaderValue::from_str(name) {
                headers.insert("Mcp-Name", h);
            }
        }

        for (k, v) in extra_headers {
            if let (Ok(hk), Ok(hv)) = (
                reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                HeaderValue::from_str(v),
            ) {
                headers.insert(hk, hv);
            }
        }

        if let Some(ref tok) = self.auth_token {
            let trimmed = tok.trim();
            if !trimmed.is_empty() {
                let auth_val = format!("Bearer {}", trimmed);
                if let Ok(mut h) = HeaderValue::from_str(&auth_val) {
                    h.set_sensitive(true);
                    headers.insert(AUTHORIZATION, h);
                }
            }
        }

        // Attach Mcp-Session-Id only for legacy 2024-11-05 stateful transport if present
        if proto_ver == MCP_SPEC_2024_11_05 {
            let session_guard = self.session_id.lock().await;
            if let Some(ref sid) = *session_guard {
                if let Ok(h) = HeaderValue::from_str(sid) {
                    headers.insert("Mcp-Session-Id", h);
                }
            }
        }

        Ok(headers)
    }

    /// Redact access token from any text or error payload
    fn sanitize_error(&self, err_text: &str) -> String {
        if let Some(ref tok) = self.auth_token {
            let trimmed = tok.trim();
            if !trimmed.is_empty() && trimmed.len() > 4 {
                return err_text.replace(trimmed, "[REDACTED_BEARER_TOKEN]");
            }
        }
        err_text.to_string()
    }

    /// Extract Mcp-Param-* headers for schema properties marked with x-mcp-header
    pub fn extract_mcp_param_headers(schema: &Value, args: &Value) -> Vec<(String, String)> {
        let mut headers = Vec::new();
        if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
            for (prop_name, prop_def) in props {
                if let Some(ann) = prop_def.get("x-mcp-header") {
                    let header_name = match ann {
                        Value::String(custom_hdr) if !custom_hdr.trim().is_empty() => {
                            custom_hdr.trim().to_string()
                        }
                        Value::Bool(true) => {
                            format!("Mcp-Param-{}", prop_name)
                        }
                        _ => continue,
                    };
                    if let Some(val) = args.get(prop_name) {
                        let val_str = match val {
                            Value::String(s) => s.clone(),
                            Value::Number(n) => n.to_string(),
                            Value::Bool(b) => b.to_string(),
                            other => other.to_string(),
                        };
                        headers.push((header_name, val_str));
                    }
                }
            }
        }
        headers
    }

    async fn send_json_rpc(
        &self,
        method: &str,
        params: Value,
        timeout_dur: Duration,
        tool_name: Option<&str>,
        extra_headers: &[(String, String)],
    ) -> Result<Value, String> {
        let req_id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let proto_ver = {
            let version_guard = self.negotiated_protocol_version.lock().await;
            version_guard.clone()
        };

        let mut enriched_params = params;
        if proto_ver == MCP_SPEC_2026_07_28 {
            if let Value::Object(ref mut map) = enriched_params {
                let meta_entry = map.entry("_meta".to_string()).or_insert_with(|| json!({}));
                if let Value::Object(ref mut meta_obj) = meta_entry {
                    if !meta_obj.contains_key("io.modelcontextprotocol/protocolVersion") {
                        meta_obj.insert(
                            "io.modelcontextprotocol/protocolVersion".to_string(),
                            json!(MCP_SPEC_2026_07_28),
                        );
                    }
                    if !meta_obj.contains_key("io.modelcontextprotocol/clientInfo") {
                        meta_obj.insert(
                            "io.modelcontextprotocol/clientInfo".to_string(),
                            json!({
                                "name": "kyro-desktop",
                                "version": "0.1.0"
                            }),
                        );
                    }
                    if !meta_obj.contains_key("io.modelcontextprotocol/clientCapabilities") {
                        meta_obj.insert(
                            "io.modelcontextprotocol/clientCapabilities".to_string(),
                            json!({
                                "tools": { "listChanged": true }
                            }),
                        );
                    }
                }
            }
        }

        let payload = json!({
            "jsonrpc": "2.0",
            "id": req_id,
            "method": method,
            "params": enriched_params
        });

        let headers = self.build_headers(method, tool_name, extra_headers).await?;
        let mut resp = tokio::time::timeout(
            timeout_dur,
            self.client
                .post(self.endpoint.url())
                .headers(headers)
                .json(&payload)
                .send(),
        )
        .await
        .map_err(|_| {
            format!(
                "Remote MCP server '{}' timed out after {:?}",
                self.server_name, timeout_dur
            )
        })?
        .map_err(|e| {
            self.sanitize_error(&format!(
                "Remote MCP network error reaching '{}': {}",
                self.endpoint.url(),
                e
            ))
        })?;

        // Capture session ID header if provided by server (legacy transport support)
        if let Some(sid_val) = resp.headers().get("Mcp-Session-Id") {
            if let Ok(sid_str) = sid_val.to_str() {
                let mut session_guard = self.session_id.lock().await;
                *session_guard = Some(sid_str.to_string());
            }
        }

        let status = resp.status();
        let content_type = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        // Enforce body-size limits while streaming network response chunks — NOT after allocating
        let mut body_bytes = Vec::new();
        while let Some(chunk_res) = resp
            .chunk()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?
        {
            if body_bytes.len() + chunk_res.len() > MAX_RESPONSE_BYTES {
                return Err(format!(
                    "Remote MCP server response exceeded maximum size limit of {} bytes",
                    MAX_RESPONSE_BYTES
                ));
            }
            body_bytes.extend_from_slice(&chunk_res);
        }

        let body_str = String::from_utf8_lossy(&body_bytes).to_string();

        if !status.is_success() {
            let sanitized_body = self.sanitize_error(&body_str);
            let bounded_body: String = sanitized_body.chars().take(500).collect();
            return Err(match status {
                reqwest::StatusCode::UNAUTHORIZED => {
                    "Remote MCP authentication failed (401 Unauthorized): Bearer token is invalid or expired.".to_string()
                }
                reqwest::StatusCode::FORBIDDEN => {
                    "Remote MCP authorization denied (403 Forbidden): Token lacks required server permissions.".to_string()
                }
                reqwest::StatusCode::NOT_FOUND => {
                    format!("Remote MCP endpoint not found (404) at '{}'. Check server URL path.", self.endpoint.url())
                }
                reqwest::StatusCode::TOO_MANY_REQUESTS => {
                    "Remote MCP rate limit exceeded (429). Please retry later.".to_string()
                }
                _ => {
                    format!("Remote MCP server '{}' returned HTTP {}: {}", self.server_name, status, bounded_body)
                }
            });
        }

        // Handle Server-Sent Events (SSE) format
        if content_type.contains("text/event-stream")
            || body_str.starts_with("data:")
            || body_str.contains("\ndata:")
        {
            for line in body_str.lines() {
                let trimmed_line = line.trim();
                if let Some(data_str) = trimmed_line.strip_prefix("data:") {
                    let json_candidate = data_str.trim();
                    if let Ok(val) = serde_json::from_str::<Value>(json_candidate) {
                        if let Some(id_val) = val.get("id") {
                            let id_matches = (id_val.is_u64() && id_val.as_u64() == Some(req_id))
                                || (id_val.is_i64() && id_val.as_i64() == Some(req_id as i64));
                            if id_matches {
                                if val.get("jsonrpc").and_then(|v| v.as_str()) != Some("2.0") {
                                    return Err(format!(
                                        "Invalid JSON-RPC version in SSE response from '{}'",
                                        self.server_name
                                    ));
                                }
                                if let Some(err) = val.get("error") {
                                    return Err(self.sanitize_error(&format!(
                                        "Remote MCP error from '{}': {:?}",
                                        self.server_name, err
                                    )));
                                }
                                if let Some(res) = val.get("result") {
                                    return Ok(res.clone());
                                } else {
                                    return Err(format!(
                                        "Invalid JSON-RPC SSE response from '{}': missing result and error",
                                        self.server_name
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            return Err(format!(
                "Remote MCP server '{}' SSE stream did not contain a response matching request ID {}",
                self.server_name, req_id
            ));
        }

        // Standard JSON-RPC parse and validation
        let resp_json: Value = serde_json::from_str(&body_str).map_err(|e| {
            format!(
                "Failed to parse JSON-RPC response from '{}': {}",
                self.server_name, e
            )
        })?;

        if !resp_json.is_object() {
            return Err(format!(
                "Malformed JSON-RPC response from '{}': root is not an object",
                self.server_name
            ));
        }

        // 1. Validate JSON-RPC version
        match resp_json.get("jsonrpc").and_then(|v| v.as_str()) {
            Some("2.0") => {}
            Some(v) => {
                return Err(format!(
                    "Unsupported JSON-RPC version '{}' from '{}': expected '2.0'",
                    v, self.server_name
                ));
            }
            None => {
                return Err(format!(
                    "Missing 'jsonrpc' version field in response from '{}'",
                    self.server_name
                ));
            }
        }

        // 2. Validate response ID type and correlation
        let resp_id = match resp_json.get("id") {
            Some(val) if val.is_u64() => val.as_u64().unwrap(),
            Some(val) if val.is_i64() && val.as_i64().unwrap() >= 0 => val.as_i64().unwrap() as u64,
            Some(_) => {
                return Err(format!(
                    "Malformed JSON-RPC response ID from '{}': expected integer {}, got {:?}",
                    self.server_name,
                    req_id,
                    resp_json.get("id")
                ));
            }
            None => {
                return Err(format!(
                    "Missing JSON-RPC response ID from '{}': expected {}",
                    self.server_name, req_id
                ));
            }
        };

        if resp_id != req_id {
            return Err(format!(
                "JSON-RPC response ID mismatch from '{}': expected {}, received {}",
                self.server_name, req_id, resp_id
            ));
        }

        // 3. Validate result or error presence
        let has_error = resp_json.get("error").is_some();
        let has_result = resp_json.get("result").is_some();

        if has_error && has_result {
            return Err(format!(
                "Invalid JSON-RPC response from '{}': contains both 'result' and 'error'",
                self.server_name
            ));
        }

        if !has_error && !has_result {
            return Err(format!(
                "Invalid JSON-RPC response from '{}': missing both 'result' and 'error'",
                self.server_name
            ));
        }

        if let Some(err) = resp_json.get("error") {
            return Err(self.sanitize_error(&format!(
                "Remote MCP error from '{}': {:?}",
                self.server_name, err
            )));
        }

        Ok(resp_json.get("result").cloned().unwrap())
    }

    /// Send a JSON-RPC notification (no ID, does not expect a response body)
    async fn send_notification(&self, method: &str, params: Value) -> Result<(), String> {
        let payload = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let headers = self.build_headers(method, None, &[]).await?;
        let _ = self
            .client
            .post(self.endpoint.url())
            .headers(headers)
            .json(&payload)
            .send()
            .await;
        Ok(())
    }

    /// Perform protocol verification or legacy handshake
    pub async fn connect_or_verify(&self) -> Result<String, String> {
        let proto_ver = {
            let guard = self.negotiated_protocol_version.lock().await;
            guard.clone()
        };

        if proto_ver == MCP_SPEC_2024_11_05 {
            // Legacy 2024-11-05 protocol: stateful handshake
            let params = json!({
                "protocolVersion": MCP_SPEC_2024_11_05,
                "capabilities": {
                    "roots": { "listChanged": true }
                },
                "clientInfo": {
                    "name": "kyro-desktop",
                    "version": "0.1.0"
                }
            });

            let res = self
                .send_json_rpc("initialize", params, Duration::from_secs(12), None, &[])
                .await?;

            let server_title = res["serverInfo"]["name"]
                .as_str()
                .unwrap_or(&self.server_name);

            let _ = self
                .send_notification("notifications/initialized", json!({}))
                .await;

            Ok(format!(
                "Connected to {} (protocol: {})",
                server_title, MCP_SPEC_2024_11_05
            ))
        } else {
            // Modern 2026-07-28 protocol: stateless, no initialize handshake!
            // Directly test discovery via tools/list with modern headers and _meta
            let tools = self.list_tools().await?;
            Ok(format!(
                "Connected to {} (protocol: {}, {} tools discovered)",
                self.server_name,
                MCP_SPEC_2026_07_28,
                tools.len()
            ))
        }
    }

    /// Explicit initialize call (valid only for legacy protocol)
    pub async fn initialize(&self) -> Result<String, String> {
        let proto_ver = {
            let guard = self.negotiated_protocol_version.lock().await;
            guard.clone()
        };
        if proto_ver != MCP_SPEC_2024_11_05 {
            return Err(format!(
                "The 'initialize' handshake has been removed in MCP specification '{}'. Modern MCP connections are stateless.",
                proto_ver
            ));
        }
        self.connect_or_verify().await
    }

    pub async fn list_tools(&self) -> Result<Vec<ToolDefinition>, String> {
        let res = self
            .send_json_rpc("tools/list", json!({}), Duration::from_secs(12), None, &[])
            .await?;

        let mut list = Vec::new();
        if let Some(tools) = res["tools"].as_array() {
            for t in tools {
                let name = t["name"].as_str().unwrap_or_default().to_string();
                let description = t["description"].as_str().unwrap_or_default().to_string();
                let schema = t["inputSchema"].clone();

                if !name.is_empty() {
                    list.push(ToolDefinition {
                        name,
                        description,
                        parameters: schema,
                    });
                }
            }
        }
        Ok(list)
    }

    pub async fn call_tool(
        &self,
        tool_name: &str,
        schema: &Value,
        args: Value,
    ) -> Result<String, String> {
        // Strictly validate arguments against declared schema before sending over network
        validate_schema(schema, &args)?;

        let param_headers = Self::extract_mcp_param_headers(schema, &args);

        let params = json!({
            "name": tool_name,
            "arguments": args
        });

        let res = self
            .send_json_rpc(
                "tools/call",
                params,
                Duration::from_secs(30),
                Some(tool_name),
                &param_headers,
            )
            .await?;

        let mut text_out = if let Some(content) = res["content"].as_array() {
            let mut buf = String::new();
            for c in content {
                if let Some(t) = c["text"].as_str() {
                    buf.push_str(t);
                }
            }
            buf
        } else {
            res.to_string()
        };

        const MAX_OUTPUT_BYTES: usize = 256 * 1024;
        if text_out.len() > MAX_OUTPUT_BYTES {
            text_out.truncate(MAX_OUTPUT_BYTES);
            text_out.push_str("\n...[Remote MCP Output truncated at 256KB limit]");
        }

        Ok(text_out)
    }
}
