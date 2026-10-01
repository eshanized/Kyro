use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::time::timeout;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthFlowInit {
    pub session_id: String,
    pub auth_url: String,
    pub redirect_uri: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OAuthSessionStatus {
    Pending,
    CallbackReceived { code: String },
    Exchanging,
    Finalizing { token: TokenExchangeResult },
    Completed { token: TokenExchangeResult },
    Failed { error: String },
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthSessionStatusView {
    pub session_id: String,
    pub status: String, // "pending", "callback_received", "exchanging", "completed", "failed", "cancelled", "expired"
    pub error: Option<String>,
}

pub fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

#[derive(Debug, Clone)]
pub struct PendingOAuthSession {
    pub session_id: String,
    pub provider_id: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub code_verifier: String,
    pub state: String,
    pub redirect_uri: String,
    pub token_endpoint: String,
    pub status: OAuthSessionStatus,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenExchangeResult {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: String,
    pub granted_scopes: Vec<String>,
    pub expires_in_seconds: Option<i64>,
}

#[derive(Clone, Default)]
pub struct OAuthManager {
    sessions: Arc<Mutex<HashMap<String, PendingOAuthSession>>>,
}

impl OAuthManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Generate cryptographically secure state and PKCE verifier/challenge (RFC 7636)
    pub fn generate_pkce() -> (String, String, String) {
        // 32-byte secure state
        let mut state_bytes = [0u8; 32];
        rand::fill(&mut state_bytes);
        let state = BASE64_URL.encode(state_bytes);

        // 32-byte code verifier
        let mut verifier_bytes = [0u8; 32];
        rand::fill(&mut verifier_bytes);
        let verifier = BASE64_URL.encode(verifier_bytes);

        // SHA-256 S256 code challenge
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let challenge = BASE64_URL.encode(hasher.finalize());

        (state, verifier, challenge)
    }

    /// Register a pending OAuth session
    pub async fn register_session(&self, session: PendingOAuthSession) {
        let mut lock = self.sessions.lock().await;
        let now = chrono::Utc::now().timestamp_millis();
        // Purge expired sessions
        lock.retain(|_, s| now < s.expires_at && s.status != OAuthSessionStatus::Expired);
        lock.insert(session.session_id.clone(), session);
    }

    /// Check the current status of an OAuth session
    pub async fn get_session_status(&self, session_id: &str) -> Option<OAuthSessionStatus> {
        let lock = self.sessions.lock().await;
        lock.get(session_id).map(|s| s.status.clone())
    }

    /// Check the current status view of an OAuth session
    pub async fn get_session_status_view(
        &self,
        session_id: &str,
    ) -> Option<OAuthSessionStatusView> {
        let mut lock = self.sessions.lock().await;
        let now = chrono::Utc::now().timestamp_millis();
        if let Some(session) = lock.get_mut(session_id) {
            if now > session.expires_at && session.status != OAuthSessionStatus::Cancelled {
                session.status = OAuthSessionStatus::Expired;
            }
            let (status_str, err) = match &session.status {
                OAuthSessionStatus::Pending => ("pending".to_string(), None),
                OAuthSessionStatus::CallbackReceived { .. } => {
                    ("callback_received".to_string(), None)
                }
                OAuthSessionStatus::Exchanging => ("exchanging".to_string(), None),
                OAuthSessionStatus::Finalizing { .. } => ("exchanging".to_string(), None),
                OAuthSessionStatus::Completed { .. } => ("completed".to_string(), None),
                OAuthSessionStatus::Failed { error } => ("failed".to_string(), Some(error.clone())),
                OAuthSessionStatus::Cancelled => ("cancelled".to_string(), None),
                OAuthSessionStatus::Expired => ("expired".to_string(), None),
            };
            Some(OAuthSessionStatusView {
                session_id: session.session_id.clone(),
                status: status_str,
                error: err,
            })
        } else {
            None
        }
    }

    /// Atomically record callback receipt.
    /// Only succeeds if the session is currently in `Pending` state.
    pub async fn record_callback(&self, session_id: &str, code: String) -> Result<(), String> {
        let mut lock = self.sessions.lock().await;
        let session = lock
            .get_mut(session_id)
            .ok_or_else(|| "OAuth session not found".to_string())?;

        let now = chrono::Utc::now().timestamp_millis();
        if now > session.expires_at {
            session.status = OAuthSessionStatus::Expired;
            return Err("OAuth session has expired".to_string());
        }

        match &session.status {
            OAuthSessionStatus::Pending => {
                session.status = OAuthSessionStatus::CallbackReceived { code };
                Ok(())
            }
            OAuthSessionStatus::CallbackReceived { .. } | OAuthSessionStatus::Exchanging => {
                // Ignore duplicate callback deliveries to prevent duplicate exchange races
                Ok(())
            }
            OAuthSessionStatus::Completed { .. } | OAuthSessionStatus::Finalizing { .. } => Ok(()),
            OAuthSessionStatus::Cancelled => Err("OAuth session was cancelled".to_string()),
            OAuthSessionStatus::Expired => Err("OAuth session has expired".to_string()),
            OAuthSessionStatus::Failed { error } => Err(error.clone()),
        }
    }

    /// Atomically claim the code exchange operation.
    /// Transitions state from `CallbackReceived` to `Exchanging`.
    pub async fn claim_exchange(
        &self,
        session_id: &str,
    ) -> Result<
        (
            String,         // token_endpoint
            String,         // client_id
            Option<String>, // client_secret
            String,         // code
            String,         // redirect_uri
            String,         // code_verifier
        ),
        String,
    > {
        let mut lock = self.sessions.lock().await;
        let session = lock
            .get_mut(session_id)
            .ok_or_else(|| "OAuth session not found".to_string())?;

        let now = chrono::Utc::now().timestamp_millis();
        if now > session.expires_at {
            session.status = OAuthSessionStatus::Expired;
            return Err("OAuth session has expired".to_string());
        }

        match &session.status {
            OAuthSessionStatus::CallbackReceived { code } => {
                let code_clone = code.clone();
                session.status = OAuthSessionStatus::Exchanging;
                Ok((
                    session.token_endpoint.clone(),
                    session.client_id.clone(),
                    session.client_secret.clone(),
                    code_clone,
                    session.redirect_uri.clone(),
                    session.code_verifier.clone(),
                ))
            }
            OAuthSessionStatus::Exchanging => Err("Token exchange already in progress".to_string()),
            OAuthSessionStatus::Completed { .. } | OAuthSessionStatus::Finalizing { .. } => {
                Err("Token exchange already completed".to_string())
            }
            OAuthSessionStatus::Pending => {
                Err("Authorization code not yet received from callback".to_string())
            }
            OAuthSessionStatus::Cancelled => Err("OAuth session was cancelled".to_string()),
            OAuthSessionStatus::Expired => Err("OAuth session has expired".to_string()),
            OAuthSessionStatus::Failed { error } => Err(error.clone()),
        }
    }

    /// Complete session with exchange result
    pub async fn finish_exchange(
        &self,
        session_id: &str,
        result: Result<TokenExchangeResult, String>,
    ) {
        let mut lock = self.sessions.lock().await;
        if let Some(session) = lock.get_mut(session_id) {
            // Do not overwrite user cancellation or expiry with exchange completion
            if session.status == OAuthSessionStatus::Cancelled
                || session.status == OAuthSessionStatus::Expired
            {
                return;
            }
            match result {
                Ok(token) => {
                    session.status = OAuthSessionStatus::Completed { token };
                }
                Err(error) => {
                    session.status = OAuthSessionStatus::Failed { error };
                }
            }
        }
    }

    /// Claim completed session for finalization without removing from map yet.
    /// Transitions state from Completed to Finalizing.
    /// Guarantees that at most one completion attempt runs concurrently and the session is consumed once.
    pub async fn claim_finalizing(
        &self,
        session_id: &str,
    ) -> Result<(String, TokenExchangeResult, String, Option<String>), String> {
        let mut lock = self.sessions.lock().await;
        let session = lock
            .get_mut(session_id)
            .ok_or_else(|| "OAuth session not found, expired, or already completed".to_string())?;

        let now = chrono::Utc::now().timestamp_millis();
        if now > session.expires_at {
            session.status = OAuthSessionStatus::Expired;
            return Err("OAuth session has expired. Please restart the authorization.".to_string());
        }

        match &session.status {
            OAuthSessionStatus::Completed { token } => {
                let provider_id = session.provider_id.clone();
                let token_res = token.clone();
                let client_id = session.client_id.clone();
                let client_secret = session.client_secret.clone();
                session.status = OAuthSessionStatus::Finalizing {
                    token: token.clone(),
                };
                Ok((provider_id, token_res, client_id, client_secret))
            }
            OAuthSessionStatus::Finalizing { .. } => {
                Err("OAuth token finalization is already in progress".to_string())
            }
            OAuthSessionStatus::Failed { error } => {
                let err_clone = error.clone();
                lock.remove(session_id);
                Err(format!("OAuth authorization failed: {}", err_clone))
            }
            OAuthSessionStatus::Exchanging => {
                Err("OAuth token exchange is currently in progress".to_string())
            }
            OAuthSessionStatus::CallbackReceived { .. } => {
                Err("OAuth code received, exchange in progress".to_string())
            }
            OAuthSessionStatus::Pending => {
                Err("OAuth authorization is pending user completion in browser".to_string())
            }
            OAuthSessionStatus::Cancelled => {
                lock.remove(session_id);
                Err("OAuth session was cancelled".to_string())
            }
            OAuthSessionStatus::Expired => {
                lock.remove(session_id);
                Err("OAuth session has expired. Please restart the authorization.".to_string())
            }
        }
    }

    /// Mark finalization as successful and clean up temporary session data
    pub async fn finalize_success(&self, session_id: &str) {
        let mut lock = self.sessions.lock().await;
        lock.remove(session_id);
    }

    /// Mark finalization as failed
    pub async fn finalize_failure(&self, session_id: &str, error: String) {
        let mut lock = self.sessions.lock().await;
        if let Some(session) = lock.get_mut(session_id) {
            session.status = OAuthSessionStatus::Failed { error };
        }
    }

    /// Retrieve and remove completed token result
    pub async fn take_completed_session(
        &self,
        session_id: &str,
    ) -> Result<(String, TokenExchangeResult), String> {
        let mut lock = self.sessions.lock().await;
        let session = lock
            .get(session_id)
            .ok_or_else(|| "OAuth session not found, expired, or already completed".to_string())?;

        match &session.status {
            OAuthSessionStatus::Completed { token } => {
                let provider_id = session.provider_id.clone();
                let token_res = token.clone();
                lock.remove(session_id);
                Ok((provider_id, token_res))
            }
            OAuthSessionStatus::Finalizing { .. } => {
                Err("OAuth token finalization is already in progress".to_string())
            }
            OAuthSessionStatus::Failed { error } => {
                let err_clone = error.clone();
                lock.remove(session_id);
                Err(format!("OAuth authorization failed: {}", err_clone))
            }
            OAuthSessionStatus::Exchanging => {
                Err("OAuth token exchange is currently in progress".to_string())
            }
            OAuthSessionStatus::CallbackReceived { .. } => {
                Err("OAuth code received, exchange in progress".to_string())
            }
            OAuthSessionStatus::Pending => {
                Err("OAuth authorization is pending user completion in browser".to_string())
            }
            OAuthSessionStatus::Cancelled => {
                lock.remove(session_id);
                Err("OAuth session was cancelled".to_string())
            }
            OAuthSessionStatus::Expired => {
                lock.remove(session_id);
                Err("OAuth session has expired. Please restart the authorization.".to_string())
            }
        }
    }

    /// Retrieve session for manual code exchange
    pub async fn get_session_for_manual_exchange(
        &self,
        session_id: &str,
    ) -> Result<PendingOAuthSession, String> {
        let lock = self.sessions.lock().await;
        let session = lock
            .get(session_id)
            .ok_or_else(|| "OAuth session not found or expired".to_string())?;

        let now = chrono::Utc::now().timestamp_millis();
        if now > session.expires_at {
            return Err("OAuth session has expired".to_string());
        }

        Ok(session.clone())
    }

    /// Cancel a session and mark status as Cancelled
    pub async fn cancel_session(&self, session_id: &str) {
        let mut lock = self.sessions.lock().await;
        if let Some(s) = lock.get_mut(session_id) {
            s.status = OAuthSessionStatus::Cancelled;
        }
    }

    /// Start a local loopback listener on an ephemeral port (127.0.0.1:0)
    /// to capture the OAuth redirect callback with secure state validation and timeout.
    pub async fn listen_for_callback(
        listener: TcpListener,
        expected_state: String,
        timeout_duration: Duration,
    ) -> Result<String, String> {
        let accept_fut = async {
            loop {
                let (mut socket, _) = listener
                    .accept()
                    .await
                    .map_err(|e| format!("Loopback accept error: {}", e))?;

                let mut buffer = [0u8; 4096];
                let n = socket
                    .read(&mut buffer)
                    .await
                    .map_err(|e| format!("Loopback read error: {}", e))?;

                let request_str = String::from_utf8_lossy(&buffer[..n]);
                let first_line = request_str.lines().next().unwrap_or_default();

                // Ignore browser favicon/asset requests
                if first_line.contains("/favicon.ico") {
                    let not_found =
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    let _ = socket.write_all(not_found.as_bytes()).await;
                    continue;
                }

                // Expect: GET /callback?code=...&state=... HTTP/1.1
                if let Some(path_and_query) = first_line.split_whitespace().nth(1) {
                    let path = path_and_query.split('?').next().unwrap_or("");
                    if path != "/callback" {
                        let not_found =
                            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = socket.write_all(not_found.as_bytes()).await;
                        continue;
                    }

                    if let Some(query) = path_and_query.split('?').nth(1) {
                        let query_map: HashMap<String, String> = query
                            .split('&')
                            .filter_map(|pair| {
                                let mut parts = pair.split('=');
                                let k = parts.next()?;
                                let v = parts.next().unwrap_or("");
                                Some((k.to_string(), v.to_string()))
                            })
                            .collect();

                        // 1. Verify state parameter strictly (CSRF defense)
                        let callback_state = query_map.get("state").cloned().unwrap_or_default();
                        if callback_state.is_empty() || callback_state != expected_state {
                            let response = "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n<html><body style='font-family:sans-serif;background:#141724;color:#f87171;padding:40px;text-align:center;'><h2>Security Error: State parameter mismatch or missing</h2><p>Authentication rejected to protect against CSRF attacks.</p></body></html>";
                            let _ = socket.write_all(response.as_bytes()).await;
                            return Err("OAuth state parameter mismatch or missing (potential CSRF / callback spoofing)".to_string());
                        }

                        // 2. Extract error if provider sent an error (HTML-escaped to prevent reflection/XSS)
                        if let Some(err) = query_map.get("error") {
                            let err_desc = query_map
                                .get("error_description")
                                .cloned()
                                .unwrap_or_default();
                            let safe_err = html_escape(err);
                            let safe_desc = html_escape(&err_desc);
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n<html><body style='font-family:sans-serif;background:#141724;color:#f87171;padding:40px;text-align:center;'><h2>Authorization Cancelled: {}</h2><p>{}</p><p>You can close this tab and return to Kyro.</p></body></html>",
                                safe_err, safe_desc
                            );
                            let _ = socket.write_all(response.as_bytes()).await;
                            return Err(format!(
                                "OAuth error from provider: {} - {}",
                                err, err_desc
                            ));
                        }

                        // 3. Extract code
                        if let Some(code) = query_map.get("code") {
                            let html_body = "<html><body style='font-family:sans-serif;background:#141724;color:#e2e8f0;padding:40px;text-align:center;'><h2 style='color:#34d399;'>Kyro Connected Successfully!</h2><p>Authorization code received. You may close this tab and return to the Kyro desktop application.</p></body></html>";
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                html_body.len(),
                                html_body
                            );
                            let _ = socket.write_all(response.as_bytes()).await;
                            let _ = socket.flush().await;
                            return Ok(code.to_string());
                        }
                    }
                }

                // Unrecognized request path
                let not_found =
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = socket.write_all(not_found.as_bytes()).await;
            }
        };

        timeout(timeout_duration, accept_fut).await.map_err(|_| {
            "OAuth loopback callback timed out waiting for user completion in browser".to_string()
        })?
    }

    /// Exchange authorization code for access token via HTTP POST.
    /// Supports both JSON and Form-Urlencoded token endpoint responses.
    pub async fn exchange_code(
        client: &reqwest::Client,
        token_endpoint: &str,
        client_id: &str,
        client_secret: Option<&str>,
        code: &str,
        redirect_uri: &str,
        code_verifier: &str,
    ) -> Result<TokenExchangeResult, String> {
        let mut form_params = HashMap::new();
        form_params.insert("client_id", client_id);
        form_params.insert("code", code);
        form_params.insert("redirect_uri", redirect_uri);
        form_params.insert("grant_type", "authorization_code");
        form_params.insert("code_verifier", code_verifier);

        if let Some(sec) = client_secret {
            let trimmed = sec.trim();
            if !trimmed.is_empty() {
                form_params.insert("client_secret", trimmed);
            }
        }

        let resp = client
            .post(token_endpoint)
            .header("Accept", "application/json")
            .header("User-Agent", "Kyro-Desktop-Agent/0.1.0")
            .form(&form_params)
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(|e| format!("Failed to reach token endpoint: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let bounded_body: String = body.chars().take(300).collect();
            return Err(format!(
                "Token exchange failed with status {}: {}",
                status, bounded_body
            ));
        }

        let body_text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read token response: {}", e))?;

        // 1. Try parsing JSON token response
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body_text) {
            if let Some(err) = val.get("error") {
                let desc = val
                    .get("error_description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");
                return Err(format!("Token exchange error: {:?} - {}", err, desc));
            }

            if let Some(access_token) = val.get("access_token").and_then(|t| t.as_str()) {
                let refresh_token = val
                    .get("refresh_token")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());
                let token_type = val
                    .get("token_type")
                    .and_then(|s| s.as_str())
                    .unwrap_or("Bearer")
                    .to_string();
                let expires_in = val.get("expires_in").and_then(|i| i.as_i64());

                let scopes = if let Some(scope_str) = val.get("scope").and_then(|s| s.as_str()) {
                    scope_str
                        .split(',')
                        .flat_map(|s| s.split_whitespace())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                } else {
                    vec![]
                };

                return Ok(TokenExchangeResult {
                    access_token: access_token.to_string(),
                    refresh_token,
                    token_type,
                    granted_scopes: scopes,
                    expires_in_seconds: expires_in,
                });
            }
        }

        // 2. Try parsing form-urlencoded (e.g. access_token=...&scope=...)
        let mut parsed_map: HashMap<String, String> = HashMap::new();
        for pair in body_text.split('&') {
            let mut parts = pair.split('=');
            if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
                parsed_map.insert(k.to_string(), v.to_string());
            }
        }

        if let Some(access_token) = parsed_map.get("access_token") {
            let refresh_token = parsed_map.get("refresh_token").cloned();
            let token_type = parsed_map
                .get("token_type")
                .cloned()
                .unwrap_or_else(|| "Bearer".to_string());
            let expires_in = parsed_map
                .get("expires_in")
                .and_then(|s| s.parse::<i64>().ok());

            let scopes = if let Some(scope_str) = parsed_map.get("scope") {
                scope_str
                    .split(',')
                    .flat_map(|s| s.split_whitespace())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
                vec![]
            };

            return Ok(TokenExchangeResult {
                access_token: access_token.clone(),
                refresh_token,
                token_type,
                granted_scopes: scopes,
                expires_in_seconds: expires_in,
            });
        }

        Err("Token response did not contain a valid 'access_token'".to_string())
    }

    /// Refresh an OAuth access token using a refresh token via HTTP POST.
    /// Supports refresh token rotation and scope preservation.
    pub async fn refresh_access_token(
        client: &reqwest::Client,
        token_endpoint: &str,
        client_id: &str,
        client_secret: Option<&str>,
        refresh_token: &str,
    ) -> Result<TokenExchangeResult, String> {
        let mut form_params = HashMap::new();
        form_params.insert("client_id", client_id);
        form_params.insert("grant_type", "refresh_token");
        form_params.insert("refresh_token", refresh_token);

        if let Some(sec) = client_secret {
            let trimmed = sec.trim();
            if !trimmed.is_empty() {
                form_params.insert("client_secret", trimmed);
            }
        }

        let resp = client
            .post(token_endpoint)
            .header("Accept", "application/json")
            .header("User-Agent", "Kyro-Desktop-Agent/0.1.0")
            .form(&form_params)
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(|e| format!("Failed to reach token refresh endpoint: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let bounded_body: String = body.chars().take(300).collect();
            return Err(format!(
                "Token refresh failed with status {}: {}",
                status, bounded_body
            ));
        }

        let body_text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read refresh response: {}", e))?;

        // 1. Try parsing JSON
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body_text) {
            if let Some(err) = val.get("error") {
                let desc = val
                    .get("error_description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");
                return Err(format!("Token refresh error: {:?} - {}", err, desc));
            }

            if let Some(access_token) = val.get("access_token").and_then(|t| t.as_str()) {
                let new_refresh = val
                    .get("refresh_token")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| Some(refresh_token.to_string()));
                let token_type = val
                    .get("token_type")
                    .and_then(|s| s.as_str())
                    .unwrap_or("Bearer")
                    .to_string();
                let expires_in = val.get("expires_in").and_then(|i| i.as_i64());

                let scopes = if let Some(scope_str) = val.get("scope").and_then(|s| s.as_str()) {
                    scope_str
                        .split(',')
                        .flat_map(|s| s.split_whitespace())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                } else {
                    vec![]
                };

                return Ok(TokenExchangeResult {
                    access_token: access_token.to_string(),
                    refresh_token: new_refresh,
                    token_type,
                    granted_scopes: scopes,
                    expires_in_seconds: expires_in,
                });
            }
        }

        // 2. Try parsing form-urlencoded
        let mut parsed_map: HashMap<String, String> = HashMap::new();
        for pair in body_text.split('&') {
            let mut parts = pair.split('=');
            if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
                parsed_map.insert(k.to_string(), v.to_string());
            }
        }

        if let Some(access_token) = parsed_map.get("access_token") {
            let new_refresh = parsed_map
                .get("refresh_token")
                .cloned()
                .or_else(|| Some(refresh_token.to_string()));
            let token_type = parsed_map
                .get("token_type")
                .cloned()
                .unwrap_or_else(|| "Bearer".to_string());
            let expires_in = parsed_map
                .get("expires_in")
                .and_then(|s| s.parse::<i64>().ok());

            let scopes = if let Some(scope_str) = parsed_map.get("scope") {
                scope_str
                    .split(',')
                    .flat_map(|s| s.split_whitespace())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
                vec![]
            };

            return Ok(TokenExchangeResult {
                access_token: access_token.clone(),
                refresh_token: new_refresh,
                token_type,
                granted_scopes: scopes,
                expires_in_seconds: expires_in,
            });
        }

        Err("Token refresh response did not contain a valid 'access_token'".to_string())
    }
}
