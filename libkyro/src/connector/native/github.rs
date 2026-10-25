use super::NativeConnectorAdapter;
use crate::connector::ConnectorToolDefinition;
use crate::execution::permissions::RiskLevel;
use crate::storage::{ConnectedAccountRecord, Vault};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::{Client, StatusCode, Url};
use serde_json::{json, Value};
use std::time::Duration;

pub const DEFAULT_GITHUB_API_BASE: &str = "https://api.github.com";
pub const MAX_GITHUB_RESPONSE_BYTES: usize = 256 * 1024; // 256 KB limit

pub struct GitHubAdapter {
    client: Client,
}

impl Default for GitHubAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GitHubAdapter {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(25))
            // Prevent credentials from being forwarded across origin or protocol changes
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
            .unwrap_or_default();
        Self { client }
    }

    pub fn default_tools() -> Vec<ConnectorToolDefinition> {
        use crate::connector::NativeConnectorAdapter;
        let adapter = Self::new();
        let dummy = ConnectedAccountRecord {
            id: String::new(),
            connector_id: "github".into(),
            account_identifier: String::new(),
            display_name: String::new(),
            status: "connected".into(),
            status_message: None,
            granted_scopes_json: "[]".into(),
            credential_key: String::new(),
            config_json: "{}".into(),
            created_at: 0,
            updated_at: 0,
            last_connected_at: None,
            last_error: None,
            expires_at: None,
        };
        adapter.list_tools(&dummy)
    }

    /// Resolve and strictly validate API origin.
    /// - Default is official GitHub API (https://api.github.com)
    /// - Custom Enterprise hosts require explicit, backend-validated configuration (is_enterprise: true)
    /// - Normal accounts cannot silently use loopback as a trusted origin without explicit development setup (allow_dev_loopback: true)
    /// - Enforces HTTPS for non-loopback hosts
    /// - Rejects userinfo and malformed URLs
    pub fn resolve_base_url(&self, account: &ConnectedAccountRecord) -> Result<String, String> {
        if let Ok(cfg) = serde_json::from_str::<Value>(&account.config_json) {
            if let Some(b) = cfg.get("base_url").and_then(|v| v.as_str()) {
                let trimmed = b.trim();
                if !trimmed.is_empty() {
                    let parsed = Url::parse(trimmed)
                        .map_err(|e| format!("Invalid GitHub API base_url '{}': {}", trimmed, e))?;

                    let scheme = parsed.scheme();
                    if scheme != "https" && scheme != "http" {
                        return Err(format!(
                            "Unsupported URL scheme '{}' in GitHub API base_url. Only 'https' (or 'http' on authorized loopback) is permitted.",
                            scheme
                        ));
                    }

                    let host = parsed
                        .host_str()
                        .ok_or_else(|| "Missing hostname in GitHub API base_url".to_string())?;

                    if !parsed.username().is_empty() || parsed.password().is_some() {
                        return Err(
                            "Userinfo in GitHub API base_url is strictly prohibited".to_string()
                        );
                    }

                    if host == "api.github.com" {
                        if scheme != "https" {
                            return Err(
                                "Official GitHub API requires HTTPS (https://api.github.com)"
                                    .to_string(),
                            );
                        }
                        return Ok("https://api.github.com".to_string());
                    }

                    let is_loopback = host == "localhost"
                        || host == "127.0.0.1"
                        || host == "::1"
                        || host.starts_with("127.");

                    // SSRF Protection: strictly reject link-local and cloud metadata endpoints
                    let is_metadata_or_link_local = if let Ok(ip) = host.parse::<std::net::IpAddr>()
                    {
                        match ip {
                            std::net::IpAddr::V4(v4) => {
                                v4.is_link_local() || v4.octets() == [169, 254, 169, 254]
                            }
                            std::net::IpAddr::V6(v6) => (v6.segments()[0] & 0xffc0) == 0xfe80,
                        }
                    } else {
                        host.eq_ignore_ascii_case("metadata.google.internal")
                            || host.eq_ignore_ascii_case("169.254.169.254")
                    };

                    if is_metadata_or_link_local {
                        return Err(format!(
                            "SSRF protection: Link-local and cloud metadata addresses are strictly prohibited in GitHub API base_url: '{}'",
                            host
                        ));
                    }

                    let is_enterprise = cfg
                        .get("is_enterprise")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let allow_dev_loopback = cfg
                        .get("allow_dev_loopback")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    if is_loopback {
                        if !allow_dev_loopback && !is_enterprise {
                            return Err(format!(
                                "Untrusted loopback GitHub API host '{}'. Loopback origin requires explicit configuration (allow_dev_loopback: true or is_enterprise: true).",
                                host
                            ));
                        }
                        if scheme == "http" && !allow_dev_loopback {
                            return Err(format!(
                                "Plain HTTP is prohibited on loopback host '{}' without explicit development authorization (allow_dev_loopback: true)",
                                host
                            ));
                        }
                    } else {
                        if scheme != "https" {
                            return Err(format!(
                                "GitHub API base URL must use HTTPS for non-loopback hosts. Rejected host: '{}'",
                                host
                            ));
                        }

                        if !is_enterprise {
                            return Err(format!(
                                "Untrusted GitHub API host '{}'. Custom GitHub Enterprise Server hosts require explicit configuration (is_enterprise: true).",
                                host
                            ));
                        }
                    }

                    return Ok(trimmed.trim_end_matches('/').to_string());
                }
            }
        }
        Ok(DEFAULT_GITHUB_API_BASE.to_string())
    }

    /// Safely construct an API endpoint URL with path segments and query parameters
    fn build_url(
        base: &str,
        path_segments: &[&str],
        query_params: &[(&str, &str)],
    ) -> Result<Url, String> {
        let mut url =
            Url::parse(base).map_err(|e| format!("Failed to parse base URL '{}': {}", base, e))?;
        {
            let mut segs = url
                .path_segments_mut()
                .map_err(|_| "Base URL cannot be a base for path segments".to_string())?;
            for seg in path_segments {
                segs.push(seg);
            }
        }
        if !query_params.is_empty() {
            let mut query = url.query_pairs_mut();
            for (k, v) in query_params {
                query.append_pair(k, v);
            }
        }
        Ok(url)
    }

    pub fn validate_github_ident(name: &str, field_name: &str) -> Result<(), String> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(format!("'{}' cannot be empty", field_name));
        }
        if trimmed.len() > 100 {
            return Err(format!(
                "'{}' exceeds maximum allowed length of 100 characters",
                field_name
            ));
        }
        if trimmed.contains('/')
            || trimmed.contains('\\')
            || trimmed.contains("..")
            || trimmed.contains('\0')
        {
            return Err(format!(
                "'{}' contains prohibited path characters or traversal sequences",
                field_name
            ));
        }
        if !trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            return Err(format!(
                "'{}' contains invalid characters. Only alphanumeric, '-', '_', '.' are permitted.",
                field_name
            ));
        }
        Ok(())
    }

    pub fn validate_positive_int(val: i64, field_name: &str) -> Result<i64, String> {
        if val <= 0 {
            return Err(format!(
                "'{}' must be a positive integer greater than zero",
                field_name
            ));
        }
        Ok(val)
    }

    fn extract_token(&self, secret_raw: &str) -> Result<String, String> {
        let trimmed = secret_raw.trim();
        if trimmed.starts_with('{') {
            if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
                if let Some(tok) = v.get("access_token").and_then(|t| t.as_str()) {
                    return Ok(tok.trim().to_string());
                }
                if let Some(tok) = v.get("token").and_then(|t| t.as_str()) {
                    return Ok(tok.trim().to_string());
                }
            }
        }
        if trimmed.is_empty() {
            Err("No GitHub authentication token found in secure vault for this account".to_string())
        } else {
            Ok(trimmed.to_string())
        }
    }

    fn build_headers(&self, token: &str) -> Result<HeaderMap, String> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("Kyro-Autonomous-Desktop-Agent/0.1.3"),
        );
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        let auth_val = format!("Bearer {}", token);
        let mut auth_header = HeaderValue::from_str(&auth_val)
            .map_err(|e| format!("Invalid authorization header value: {}", e))?;
        auth_header.set_sensitive(true);
        headers.insert(AUTHORIZATION, auth_header);
        Ok(headers)
    }

    pub async fn read_bounded_response(
        resp: reqwest::Response,
        max_bytes: usize,
    ) -> Result<(StatusCode, HeaderMap, String), String> {
        let status = resp.status();
        let headers = resp.headers().clone();
        let mut stream = resp.bytes_stream();
        let mut buffer = Vec::new();

        while let Some(chunk_res) = stream.next().await {
            let chunk = chunk_res.map_err(|e| format!("Failed to read response body: {}", e))?;
            if buffer.len() + chunk.len() > max_bytes {
                return Err(format!(
                    "GitHub API response exceeded maximum allowed size limit of {} bytes",
                    max_bytes
                ));
            }
            buffer.extend_from_slice(&chunk);
        }

        let body_str = String::from_utf8(buffer)
            .map_err(|e| format!("GitHub API response is not valid UTF-8: {}", e))?;
        Ok((status, headers, body_str))
    }

    pub async fn send_and_read_bounded(
        &self,
        req: reqwest::RequestBuilder,
        resource: &str,
    ) -> Result<Value, String> {
        let resp = req
            .send()
            .await
            .map_err(|e| format!("GitHub request failed: {}", e))?;

        let (status, headers, body_str) =
            Self::read_bounded_response(resp, MAX_GITHUB_RESPONSE_BYTES).await?;

        if !status.is_success() {
            return Err(Self::map_error_status(
                status,
                &body_str,
                Some(&headers),
                resource,
            ));
        }

        serde_json::from_str::<Value>(&body_str)
            .map_err(|e| format!("Failed to parse GitHub JSON response: {}", e))
    }

    fn map_error_status(
        status: StatusCode,
        body: &str,
        headers: Option<&HeaderMap>,
        resource: &str,
    ) -> String {
        let retry_msg = headers
            .and_then(|h| h.get("retry-after"))
            .and_then(|v| v.to_str().ok())
            .map(|s| format!(" (Retry-After: {} seconds)", s))
            .unwrap_or_default();

        match status {
            StatusCode::UNAUTHORIZED => {
                "GitHub authentication failed (401 Unauthorized): Access token is invalid or expired. Please reauthorize account.".to_string()
            }
            StatusCode::FORBIDDEN => {
                if body.contains("rate limit") {
                    format!("GitHub rate limit exceeded (403 Forbidden){}: Please wait before making more requests.", retry_msg)
                } else {
                    format!("GitHub permission denied (403 Forbidden) for '{}': The account lacks necessary scopes or repository permissions.", resource)
                }
            }
            StatusCode::NOT_FOUND => {
                format!("GitHub resource not found (404 Not Found): '{}' does not exist or account lacks visibility permissions.", resource)
            }
            StatusCode::TOO_MANY_REQUESTS => {
                format!("GitHub rate limit exceeded (429 Too Many Requests){}: Please retry later.", retry_msg)
            }
            StatusCode::UNPROCESSABLE_ENTITY => {
                let bounded_body: String = body.chars().take(300).collect();
                format!("GitHub validation error (422 Unprocessable Entity): {}", bounded_body)
            }
            other => {
                let bounded_body: String = body.chars().take(300).collect();
                format!("GitHub API error (HTTP {}): {}", other.as_u16(), bounded_body)
            }
        }
    }

    fn bound_output(text: String) -> String {
        const MAX_OUTPUT_BYTES: usize = 256 * 1024; // 256 KB limit
        if text.len() > MAX_OUTPUT_BYTES {
            let mut truncated = text;
            truncated.truncate(MAX_OUTPUT_BYTES);
            truncated.push_str("\n...[GitHub output truncated at 256KB size limit]");
            truncated
        } else {
            text
        }
    }
}

#[async_trait]
impl NativeConnectorAdapter for GitHubAdapter {
    fn provider_id(&self) -> &str {
        "github"
    }

    fn list_tools(&self, _account: &ConnectedAccountRecord) -> Vec<ConnectorToolDefinition> {
        vec![
            ConnectorToolDefinition {
                name: "github_get_user".to_string(),
                description: "Retrieve authenticated GitHub user profile, username, email, and accessible account metadata.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {}
                }),
                risk_level: RiskLevel::Safe,
                is_read_only: true,
            },
            ConnectorToolDefinition {
                name: "github_list_repositories".to_string(),
                description: "List accessible GitHub repositories for the authenticated user or organization.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "org": { "type": "string", "description": "Optional organization login. If omitted, lists authenticated user repositories." },
                        "visibility": { "type": "string", "enum": ["all", "public", "private"], "description": "Repository visibility filter" },
                        "sort": { "type": "string", "enum": ["created", "updated", "pushed", "full_name"], "description": "Sort order" },
                        "limit": { "type": "integer", "description": "Max repositories to return (default 30, max 100)" }
                    }
                }),
                risk_level: RiskLevel::Safe,
                is_read_only: true,
            },
            ConnectorToolDefinition {
                name: "github_get_repository".to_string(),
                description: "Retrieve detailed metadata for a specific GitHub repository.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "owner": { "type": "string", "description": "Repository owner (user or organization)" },
                        "repo": { "type": "string", "description": "Repository name" }
                    },
                    "required": ["owner", "repo"]
                }),
                risk_level: RiskLevel::Safe,
                is_read_only: true,
            },
            ConnectorToolDefinition {
                name: "github_list_issues".to_string(),
                description: "List issues for a GitHub repository with status filtering.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "owner": { "type": "string", "description": "Repository owner" },
                        "repo": { "type": "string", "description": "Repository name" },
                        "state": { "type": "string", "enum": ["open", "closed", "all"], "description": "Issue state filter" },
                        "labels": { "type": "string", "description": "Comma-separated list of label names" },
                        "limit": { "type": "integer", "description": "Max issues to return (default 30, max 100)" }
                    },
                    "required": ["owner", "repo"]
                }),
                risk_level: RiskLevel::Safe,
                is_read_only: true,
            },
            ConnectorToolDefinition {
                name: "github_get_issue".to_string(),
                description: "Get full details, body text, state, and author of a specific GitHub issue.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "owner": { "type": "string", "description": "Repository owner" },
                        "repo": { "type": "string", "description": "Repository name" },
                        "issue_number": { "type": "integer", "description": "Issue number" }
                    },
                    "required": ["owner", "repo", "issue_number"]
                }),
                risk_level: RiskLevel::Safe,
                is_read_only: true,
            },
            ConnectorToolDefinition {
                name: "github_list_pull_requests".to_string(),
                description: "List pull requests in a repository with state filtering.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "owner": { "type": "string", "description": "Repository owner" },
                        "repo": { "type": "string", "description": "Repository name" },
                        "state": { "type": "string", "enum": ["open", "closed", "all"], "description": "PR state filter" },
                        "limit": { "type": "integer", "description": "Max pull requests to return (default 30, max 100)" }
                    },
                    "required": ["owner", "repo"]
                }),
                risk_level: RiskLevel::Safe,
                is_read_only: true,
            },
            ConnectorToolDefinition {
                name: "github_get_pull_request".to_string(),
                description: "Get full details of a specific pull request, including mergeable status, commit counts, and diff stats.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "owner": { "type": "string", "description": "Repository owner" },
                        "repo": { "type": "string", "description": "Repository name" },
                        "pull_number": { "type": "integer", "description": "Pull request number" }
                    },
                    "required": ["owner", "repo", "pull_number"]
                }),
                risk_level: RiskLevel::Safe,
                is_read_only: true,
            },
            ConnectorToolDefinition {
                name: "github_list_workflow_runs".to_string(),
                description: "Inspect GitHub Actions CI/CD workflow runs and statuses for a repository.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "owner": { "type": "string", "description": "Repository owner" },
                        "repo": { "type": "string", "description": "Repository name" },
                        "branch": { "type": "string", "description": "Optional branch filter" },
                        "status": { "type": "string", "enum": ["completed", "in_progress", "queued", "failure", "success"], "description": "Workflow run status" },
                        "limit": { "type": "integer", "description": "Max runs to inspect (default 20, max 50)" }
                    },
                    "required": ["owner", "repo"]
                }),
                risk_level: RiskLevel::Safe,
                is_read_only: true,
            },
            ConnectorToolDefinition {
                name: "github_get_workflow_run_jobs".to_string(),
                description: "Inspect jobs, individual step results, and failures for a specific GitHub Actions workflow run.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "owner": { "type": "string", "description": "Repository owner" },
                        "repo": { "type": "string", "description": "Repository name" },
                        "run_id": { "type": "integer", "description": "GitHub Actions workflow run ID" }
                    },
                    "required": ["owner", "repo", "run_id"]
                }),
                risk_level: RiskLevel::Safe,
                is_read_only: true,
            },
            // Write operations
            ConnectorToolDefinition {
                name: "github_create_issue".to_string(),
                description: "Create a new issue in a GitHub repository. Requires explicit policy approval.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "owner": { "type": "string", "description": "Repository owner" },
                        "repo": { "type": "string", "description": "Repository name" },
                        "title": { "type": "string", "description": "Title of the issue" },
                        "body": { "type": "string", "description": "Description / content of the issue" },
                        "labels": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Labels to apply to the issue"
                        }
                    },
                    "required": ["owner", "repo", "title"]
                }),
                risk_level: RiskLevel::Moderate,
                is_read_only: false,
            },
            ConnectorToolDefinition {
                name: "github_add_issue_comment".to_string(),
                description: "Add a comment to an existing GitHub issue or pull request. Requires explicit policy approval.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "owner": { "type": "string", "description": "Repository owner" },
                        "repo": { "type": "string", "description": "Repository name" },
                        "issue_number": { "type": "integer", "description": "Issue or PR number" },
                        "body": { "type": "string", "description": "Comment markdown text" }
                    },
                    "required": ["owner", "repo", "issue_number", "body"]
                }),
                risk_level: RiskLevel::Moderate,
                is_read_only: false,
            },
        ]
    }

    async fn test_connection(
        &self,
        account: &ConnectedAccountRecord,
        vault: &Vault,
    ) -> Result<String, String> {
        let secret_raw = vault.get_account_secret(&account.id)?.ok_or_else(|| {
            format!(
                "No credentials found in vault for account '{}'",
                account.display_name
            )
        })?;
        let token = self.extract_token(&secret_raw)?;
        let base_url = self.resolve_base_url(account)?;
        let url = Self::build_url(&base_url, &["user"], &[])?;

        let headers = self.build_headers(&token)?;
        let val = self
            .send_and_read_bounded(self.client.get(url).headers(headers), "authenticated user")
            .await?;

        let login = val["login"].as_str().unwrap_or("unknown");
        let name = val["name"].as_str().unwrap_or(login);
        Ok(format!(
            "Successfully connected to GitHub as @{} ({})",
            login, name
        ))
    }

    async fn execute_tool(
        &self,
        account: &ConnectedAccountRecord,
        tool_name: &str,
        args: Value,
        vault: &Vault,
    ) -> Result<String, String> {
        let secret_raw = vault.get_account_secret(&account.id)?.ok_or_else(|| {
            format!(
                "No credentials found in vault for account '{}'",
                account.display_name
            )
        })?;
        let token = self.extract_token(&secret_raw)?;
        let base_url = self.resolve_base_url(account)?;
        let headers = self.build_headers(&token)?;

        match tool_name {
            "github_get_user" => {
                let url = Self::build_url(&base_url, &["user"], &[])?;
                let val = self
                    .send_and_read_bounded(self.client.get(url).headers(headers), "user profile")
                    .await?;
                let out = json!({
                    "login": val["login"],
                    "id": val["id"],
                    "name": val["name"],
                    "email": val["email"],
                    "public_repos": val["public_repos"],
                    "followers": val["followers"],
                    "html_url": val["html_url"],
                    "created_at": val["created_at"],
                });
                Ok(Self::bound_output(
                    serde_json::to_string_pretty(&out).unwrap_or_default(),
                ))
            }
            "github_list_repositories" => {
                let limit = args["limit"].as_u64().unwrap_or(30).clamp(1, 100);
                let sort = args["sort"].as_str().unwrap_or("updated");
                if !["created", "updated", "pushed", "full_name"].contains(&sort) {
                    return Err(format!("Invalid sort option '{}'", sort));
                }
                let limit_str = limit.to_string();

                let (path_segs, query_params) = if let Some(org) = args["org"].as_str() {
                    Self::validate_github_ident(org, "org")?;
                    (
                        vec!["orgs", org, "repos"],
                        vec![("per_page", limit_str.as_str()), ("sort", sort)],
                    )
                } else {
                    let visibility = args["visibility"].as_str().unwrap_or("all");
                    if !["all", "public", "private"].contains(&visibility) {
                        return Err(format!("Invalid visibility option '{}'", visibility));
                    }
                    (
                        vec!["user", "repos"],
                        vec![
                            ("per_page", limit_str.as_str()),
                            ("sort", sort),
                            ("visibility", visibility),
                        ],
                    )
                };

                let url = Self::build_url(&base_url, &path_segs, &query_params)?;
                let val = self
                    .send_and_read_bounded(
                        self.client.get(url).headers(headers),
                        "repositories list",
                    )
                    .await?;
                let repos: Vec<Value> = val
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .take(limit as usize)
                    .map(|r| {
                        json!({
                            "name": r["name"],
                            "full_name": r["full_name"],
                            "private": r["private"],
                            "description": r["description"],
                            "html_url": r["html_url"],
                            "stargazers_count": r["stargazers_count"],
                            "forks_count": r["forks_count"],
                            "open_issues_count": r["open_issues_count"],
                            "default_branch": r["default_branch"],
                            "updated_at": r["updated_at"]
                        })
                    })
                    .collect();

                Ok(Self::bound_output(
                    serde_json::to_string_pretty(&repos).unwrap_or_default(),
                ))
            }
            "github_get_repository" => {
                let owner = args["owner"]
                    .as_str()
                    .ok_or("Missing required 'owner' parameter")?;
                let repo = args["repo"]
                    .as_str()
                    .ok_or("Missing required 'repo' parameter")?;
                Self::validate_github_ident(owner, "owner")?;
                Self::validate_github_ident(repo, "repo")?;

                let url = Self::build_url(&base_url, &["repos", owner, repo], &[])?;
                let r = self
                    .send_and_read_bounded(
                        self.client.get(url).headers(headers),
                        &format!("{}/{}", owner, repo),
                    )
                    .await?;
                let out = json!({
                    "full_name": r["full_name"],
                    "description": r["description"],
                    "private": r["private"],
                    "html_url": r["html_url"],
                    "default_branch": r["default_branch"],
                    "open_issues_count": r["open_issues_count"],
                    "stargazers_count": r["stargazers_count"],
                    "language": r["language"],
                    "created_at": r["created_at"],
                    "updated_at": r["updated_at"]
                });
                Ok(Self::bound_output(
                    serde_json::to_string_pretty(&out).unwrap_or_default(),
                ))
            }
            "github_list_issues" => {
                let owner = args["owner"]
                    .as_str()
                    .ok_or("Missing required 'owner' parameter")?;
                let repo = args["repo"]
                    .as_str()
                    .ok_or("Missing required 'repo' parameter")?;
                Self::validate_github_ident(owner, "owner")?;
                Self::validate_github_ident(repo, "repo")?;

                let state = args["state"].as_str().unwrap_or("open");
                if !["open", "closed", "all"].contains(&state) {
                    return Err(format!("Invalid state filter '{}'", state));
                }
                let limit = args["limit"].as_u64().unwrap_or(30).clamp(1, 100);
                let limit_str = limit.to_string();

                let mut q_params = vec![("state", state), ("per_page", limit_str.as_str())];
                let labels_val = args["labels"].as_str();
                if let Some(lbls) = labels_val {
                    q_params.push(("labels", lbls));
                }

                let url = Self::build_url(&base_url, &["repos", owner, repo, "issues"], &q_params)?;
                let val = self
                    .send_and_read_bounded(
                        self.client.get(url).headers(headers),
                        &format!("{}/{} issues", owner, repo),
                    )
                    .await?;
                let issues: Vec<Value> = val
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .take(limit as usize)
                    .map(|i| {
                        json!({
                            "number": i["number"],
                            "title": i["title"],
                            "user": i["user"]["login"],
                            "state": i["state"],
                            "comments": i["comments"],
                            "created_at": i["created_at"],
                            "html_url": i["html_url"],
                            "is_pull_request": i.get("pull_request").is_some()
                        })
                    })
                    .collect();

                Ok(Self::bound_output(
                    serde_json::to_string_pretty(&issues).unwrap_or_default(),
                ))
            }
            "github_get_issue" => {
                let owner = args["owner"]
                    .as_str()
                    .ok_or("Missing required 'owner' parameter")?;
                let repo = args["repo"]
                    .as_str()
                    .ok_or("Missing required 'repo' parameter")?;
                let num = args["issue_number"]
                    .as_i64()
                    .ok_or("Missing required 'issue_number' parameter")?;
                Self::validate_github_ident(owner, "owner")?;
                Self::validate_github_ident(repo, "repo")?;
                Self::validate_positive_int(num, "issue_number")?;

                let num_str = num.to_string();
                let url = Self::build_url(
                    &base_url,
                    &["repos", owner, repo, "issues", num_str.as_str()],
                    &[],
                )?;
                let i = self
                    .send_and_read_bounded(
                        self.client.get(url).headers(headers),
                        &format!("{}/{} issue #{}", owner, repo, num),
                    )
                    .await?;
                let out = json!({
                    "number": i["number"],
                    "title": i["title"],
                    "body": i["body"],
                    "user": i["user"]["login"],
                    "state": i["state"],
                    "comments": i["comments"],
                    "created_at": i["created_at"],
                    "updated_at": i["updated_at"],
                    "html_url": i["html_url"],
                });
                Ok(Self::bound_output(
                    serde_json::to_string_pretty(&out).unwrap_or_default(),
                ))
            }
            "github_list_pull_requests" => {
                let owner = args["owner"]
                    .as_str()
                    .ok_or("Missing required 'owner' parameter")?;
                let repo = args["repo"]
                    .as_str()
                    .ok_or("Missing required 'repo' parameter")?;
                Self::validate_github_ident(owner, "owner")?;
                Self::validate_github_ident(repo, "repo")?;

                let state = args["state"].as_str().unwrap_or("open");
                if !["open", "closed", "all"].contains(&state) {
                    return Err(format!("Invalid state filter '{}'", state));
                }
                let limit = args["limit"].as_u64().unwrap_or(30).clamp(1, 100);
                let limit_str = limit.to_string();

                let url = Self::build_url(
                    &base_url,
                    &["repos", owner, repo, "pulls"],
                    &[("state", state), ("per_page", limit_str.as_str())],
                )?;
                let val = self
                    .send_and_read_bounded(
                        self.client.get(url).headers(headers),
                        &format!("{}/{} pull requests", owner, repo),
                    )
                    .await?;
                let prs: Vec<Value> = val
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .take(limit as usize)
                    .map(|p| {
                        json!({
                            "number": p["number"],
                            "title": p["title"],
                            "user": p["user"]["login"],
                            "state": p["state"],
                            "created_at": p["created_at"],
                            "html_url": p["html_url"],
                            "head_branch": p["head"]["ref"],
                            "base_branch": p["base"]["ref"],
                        })
                    })
                    .collect();

                Ok(Self::bound_output(
                    serde_json::to_string_pretty(&prs).unwrap_or_default(),
                ))
            }
            "github_get_pull_request" => {
                let owner = args["owner"]
                    .as_str()
                    .ok_or("Missing required 'owner' parameter")?;
                let repo = args["repo"]
                    .as_str()
                    .ok_or("Missing required 'repo' parameter")?;
                let num = args["pull_number"]
                    .as_i64()
                    .ok_or("Missing required 'pull_number' parameter")?;
                Self::validate_github_ident(owner, "owner")?;
                Self::validate_github_ident(repo, "repo")?;
                Self::validate_positive_int(num, "pull_number")?;

                let num_str = num.to_string();
                let url = Self::build_url(
                    &base_url,
                    &["repos", owner, repo, "pulls", num_str.as_str()],
                    &[],
                )?;
                let p = self
                    .send_and_read_bounded(
                        self.client.get(url).headers(headers),
                        &format!("{}/{} PR #{}", owner, repo, num),
                    )
                    .await?;
                let out = json!({
                    "number": p["number"],
                    "title": p["title"],
                    "body": p["body"],
                    "user": p["user"]["login"],
                    "state": p["state"],
                    "mergeable": p["mergeable"],
                    "commits": p["commits"],
                    "additions": p["additions"],
                    "deletions": p["deletions"],
                    "changed_files": p["changed_files"],
                    "head_branch": p["head"]["ref"],
                    "base_branch": p["base"]["ref"],
                    "html_url": p["html_url"],
                });
                Ok(Self::bound_output(
                    serde_json::to_string_pretty(&out).unwrap_or_default(),
                ))
            }
            "github_list_workflow_runs" => {
                let owner = args["owner"]
                    .as_str()
                    .ok_or("Missing required 'owner' parameter")?;
                let repo = args["repo"]
                    .as_str()
                    .ok_or("Missing required 'repo' parameter")?;
                Self::validate_github_ident(owner, "owner")?;
                Self::validate_github_ident(repo, "repo")?;

                let limit = args["limit"].as_u64().unwrap_or(20).clamp(1, 50);
                let limit_str = limit.to_string();

                let mut q_params = vec![("per_page", limit_str.as_str())];
                let branch_val = args["branch"].as_str();
                if let Some(branch) = branch_val {
                    if branch.contains('/') && (branch.contains("..") || branch.contains('\0')) {
                        return Err("Invalid branch name filter".to_string());
                    }
                    q_params.push(("branch", branch));
                }
                let status_val = args["status"].as_str();
                if let Some(status) = status_val {
                    if !["completed", "in_progress", "queued", "failure", "success"]
                        .contains(&status)
                    {
                        return Err(format!("Invalid workflow status filter '{}'", status));
                    }
                    q_params.push(("status", status));
                }

                let url = Self::build_url(
                    &base_url,
                    &["repos", owner, repo, "actions", "runs"],
                    &q_params,
                )?;
                let val = self
                    .send_and_read_bounded(
                        self.client.get(url).headers(headers),
                        &format!("{}/{} workflow runs", owner, repo),
                    )
                    .await?;
                let runs: Vec<Value> = val["workflow_runs"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .take(limit as usize)
                    .map(|r| {
                        json!({
                            "id": r["id"],
                            "name": r["name"],
                            "head_branch": r["head_branch"],
                            "event": r["event"],
                            "status": r["status"],
                            "conclusion": r["conclusion"],
                            "created_at": r["created_at"],
                            "html_url": r["html_url"]
                        })
                    })
                    .collect();

                Ok(Self::bound_output(
                    serde_json::to_string_pretty(&runs).unwrap_or_default(),
                ))
            }
            "github_get_workflow_run_jobs" => {
                let owner = args["owner"]
                    .as_str()
                    .ok_or("Missing required 'owner' parameter")?;
                let repo = args["repo"]
                    .as_str()
                    .ok_or("Missing required 'repo' parameter")?;
                let run_id = args["run_id"]
                    .as_i64()
                    .ok_or("Missing required 'run_id' parameter")?;
                Self::validate_github_ident(owner, "owner")?;
                Self::validate_github_ident(repo, "repo")?;
                Self::validate_positive_int(run_id, "run_id")?;

                let run_id_str = run_id.to_string();
                let url = Self::build_url(
                    &base_url,
                    &[
                        "repos",
                        owner,
                        repo,
                        "actions",
                        "runs",
                        run_id_str.as_str(),
                        "jobs",
                    ],
                    &[],
                )?;
                let val = self
                    .send_and_read_bounded(
                        self.client.get(url).headers(headers),
                        &format!("{}/{} workflow run #{} jobs", owner, repo, run_id),
                    )
                    .await?;
                let jobs: Vec<Value> = val["jobs"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .map(|j| {
                        json!({
                            "id": j["id"],
                            "name": j["name"],
                            "status": j["status"],
                            "conclusion": j["conclusion"],
                            "started_at": j["started_at"],
                            "completed_at": j["completed_at"],
                            "steps": j["steps"].as_array().unwrap_or(&vec![]).iter().map(|s| {
                                json!({
                                    "name": s["name"],
                                    "status": s["status"],
                                    "conclusion": s["conclusion"],
                                    "number": s["number"]
                                })
                            }).collect::<Vec<Value>>()
                        })
                    })
                    .collect();

                Ok(Self::bound_output(
                    serde_json::to_string_pretty(&jobs).unwrap_or_default(),
                ))
            }
            "github_create_issue" => {
                let owner = args["owner"]
                    .as_str()
                    .ok_or("Missing required 'owner' parameter")?;
                let repo = args["repo"]
                    .as_str()
                    .ok_or("Missing required 'repo' parameter")?;
                let title = args["title"]
                    .as_str()
                    .ok_or("Missing required 'title' parameter")?;
                Self::validate_github_ident(owner, "owner")?;
                Self::validate_github_ident(repo, "repo")?;

                let trimmed_title = title.trim();
                if trimmed_title.is_empty() {
                    return Err("Issue title cannot be empty".to_string());
                }
                if trimmed_title.len() > 1024 {
                    return Err("Issue title exceeds maximum length of 1024 characters".to_string());
                }

                let mut payload = json!({ "title": trimmed_title });
                if let Some(b) = args["body"].as_str() {
                    if b.len() > 65536 {
                        return Err(
                            "Issue body exceeds maximum size limit of 65536 characters".to_string()
                        );
                    }
                    payload["body"] = json!(b);
                }
                if let Some(labels) = args["labels"].as_array() {
                    let mut safe_labels = Vec::new();
                    for lbl in labels {
                        if let Some(lbl_str) = lbl.as_str() {
                            let clean_lbl = lbl_str.trim();
                            if !clean_lbl.is_empty() && clean_lbl.len() <= 50 {
                                safe_labels.push(clean_lbl);
                            }
                        }
                    }
                    payload["labels"] = json!(safe_labels);
                }

                let url = Self::build_url(&base_url, &["repos", owner, repo, "issues"], &[])?;
                let i = self
                    .send_and_read_bounded(
                        self.client.post(url).headers(headers).json(&payload),
                        &format!("create issue in {}/{}", owner, repo),
                    )
                    .await?;
                let out = json!({
                    "success": true,
                    "issue_number": i["number"],
                    "title": i["title"],
                    "html_url": i["html_url"],
                    "message": format!("Successfully created issue #{} in {}/{}", i["number"], owner, repo)
                });
                Ok(serde_json::to_string_pretty(&out).unwrap_or_default())
            }
            "github_add_issue_comment" => {
                let owner = args["owner"]
                    .as_str()
                    .ok_or("Missing required 'owner' parameter")?;
                let repo = args["repo"]
                    .as_str()
                    .ok_or("Missing required 'repo' parameter")?;
                let num = args["issue_number"]
                    .as_i64()
                    .ok_or("Missing required 'issue_number' parameter")?;
                let body = args["body"]
                    .as_str()
                    .ok_or("Missing required 'body' parameter")?;

                Self::validate_github_ident(owner, "owner")?;
                Self::validate_github_ident(repo, "repo")?;
                Self::validate_positive_int(num, "issue_number")?;

                let trimmed_body = body.trim();
                if trimmed_body.is_empty() {
                    return Err("Comment body cannot be empty".to_string());
                }
                if trimmed_body.len() > 65536 {
                    return Err(
                        "Comment body exceeds maximum size limit of 65536 characters".to_string(),
                    );
                }

                let payload = json!({ "body": trimmed_body });
                let num_str = num.to_string();
                let url = Self::build_url(
                    &base_url,
                    &["repos", owner, repo, "issues", num_str.as_str(), "comments"],
                    &[],
                )?;

                let c = self
                    .send_and_read_bounded(
                        self.client.post(url).headers(headers).json(&payload),
                        &format!("comment on {}/{} #{}", owner, repo, num),
                    )
                    .await?;
                let out = json!({
                    "success": true,
                    "comment_id": c["id"],
                    "html_url": c["html_url"],
                    "message": format!("Successfully added comment to {}/{} #{}", owner, repo, num)
                });
                Ok(serde_json::to_string_pretty(&out).unwrap_or_default())
            }
            other => Err(format!("Unknown GitHub connector operation '{}'", other)),
        }
    }
}
