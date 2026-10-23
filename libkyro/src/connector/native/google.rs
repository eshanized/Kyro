use super::NativeConnectorAdapter;
use crate::connector::ConnectorToolDefinition;
use crate::execution::permissions::RiskLevel;
use crate::storage::{ConnectedAccountRecord, Vault};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Client, StatusCode, Url};
use serde_json::{json, Value};
use std::time::Duration;

pub const DEFAULT_GOOGLE_API_BASE: &str = "https://www.googleapis.com";
pub const DEFAULT_GMAIL_API_BASE: &str = "https://gmail.googleapis.com";
pub const MAX_GOOGLE_RESPONSE_BYTES: usize = 256 * 1024; // 256 KB

pub struct GoogleAdapter {
    client: Client,
}

impl Default for GoogleAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl GoogleAdapter {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(25))
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
        let adapter = Self::new();
        let dummy = ConnectedAccountRecord {
            id: String::new(),
            connector_id: "google".into(),
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

    pub fn resolve_base_urls(&self, account: &ConnectedAccountRecord) -> (String, String) {
        if let Ok(cfg) = serde_json::from_str::<Value>(&account.config_json) {
            if let Some(b) = cfg.get("base_url").and_then(|v| v.as_str()) {
                let trimmed = b.trim();
                if !trimmed.is_empty() {
                    return (trimmed.to_string(), trimmed.to_string());
                }
            }
        }
        if let Ok(mock_url) = std::env::var("KYRO_GOOGLE_MOCK_URL") {
            let trimmed = mock_url.trim();
            if !trimmed.is_empty() {
                return (trimmed.to_string(), trimmed.to_string());
            }
        }
        (
            DEFAULT_GOOGLE_API_BASE.to_string(),
            DEFAULT_GMAIL_API_BASE.to_string(),
        )
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
            let chunk =
                chunk_res.map_err(|e| format!("Failed to read Google API response: {}", e))?;
            if buffer.len() + chunk.len() > max_bytes {
                return Err(format!(
                    "Google API response exceeded maximum allowed size limit of {} bytes",
                    max_bytes
                ));
            }
            buffer.extend_from_slice(&chunk);
        }

        let body_str = String::from_utf8(buffer)
            .map_err(|e| format!("Google API response is not valid UTF-8: {}", e))?;
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
            .map_err(|e| format!("Google API request failed: {}", e))?;

        let (status, headers, body_str) =
            Self::read_bounded_response(resp, MAX_GOOGLE_RESPONSE_BYTES).await?;

        if !status.is_success() {
            return Err(Self::map_error_status(
                status,
                &body_str,
                Some(&headers),
                resource,
            ));
        }

        serde_json::from_str::<Value>(&body_str)
            .map_err(|e| format!("Failed to parse Google JSON response: {}", e))
    }

    fn map_error_status(
        status: StatusCode,
        body: &str,
        _headers: Option<&HeaderMap>,
        resource: &str,
    ) -> String {
        match status {
            StatusCode::UNAUTHORIZED => {
                "Google authentication failed (401 Unauthorized): Access token is invalid or expired. Please reauthorize your Google account.".to_string()
            }
            StatusCode::FORBIDDEN => {
                format!(
                    "Google permission denied (403 Forbidden) for '{}': The account lacks necessary scopes (e.g. Gmail or Calendar permissions).",
                    resource
                )
            }
            StatusCode::NOT_FOUND => {
                format!(
                    "Google resource not found (404 Not Found): '{}' does not exist or is inaccessible.",
                    resource
                )
            }
            StatusCode::TOO_MANY_REQUESTS => {
                "Google API rate limit exceeded (429 Too Many Requests): Please retry shortly.".to_string()
            }
            other => {
                let bounded: String = body.chars().take(300).collect();
                format!("Google API error (HTTP {}): {}", other.as_u16(), bounded)
            }
        }
    }

    pub fn extract_token(&self, secret_raw: &str) -> Result<String, String> {
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
            Err("No Google authentication token found in secure vault for this account".to_string())
        } else {
            Ok(trimmed.to_string())
        }
    }

    /// Validates an individual email address (strict RFC 5322 subset).
    /// Rejects control characters, invalid chars, missing domain/dot, multiple @ symbols.
    pub fn validate_email_address(addr: &str) -> Result<String, String> {
        let trimmed = addr.trim();
        if trimmed.is_empty() {
            return Err("Email address cannot be empty".to_string());
        }
        if trimmed.chars().any(|c| {
            c.is_control()
                || c.is_whitespace()
                || c == '<'
                || c == '>'
                || c == '"'
                || c == ':'
                || c == ';'
        }) {
            return Err(format!(
                "Email address '{}' contains invalid characters or control characters",
                trimmed
            ));
        }
        let parts: Vec<&str> = trimmed.split('@').collect();
        if parts.len() != 2 {
            return Err(format!(
                "Email address '{}' must contain exactly one '@'",
                trimmed
            ));
        }
        let local = parts[0];
        let domain = parts[1];
        if local.is_empty() {
            return Err(format!("Email address '{}' has empty local part", trimmed));
        }
        if domain.is_empty() || !domain.contains('.') {
            return Err(format!(
                "Email address '{}' has invalid domain part",
                trimmed
            ));
        }
        if domain.starts_with('.') || domain.ends_with('.') || domain.contains("..") {
            return Err(format!(
                "Email address '{}' has malformed domain dots",
                trimmed
            ));
        }
        for c in domain.chars() {
            if !c.is_alphanumeric() && c != '.' && c != '-' {
                return Err(format!(
                    "Email address domain '{}' contains invalid character '{}'",
                    domain, c
                ));
            }
        }
        Ok(trimmed.to_string())
    }

    /// Validates and sanitizes a single recipient, which may be `user@example.com` or `Display Name <user@example.com>`.
    /// RFC 2047 encodes display names containing non-ASCII characters or special symbols.
    pub fn validate_and_sanitize_recipient(entry: &str) -> Result<String, String> {
        use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
        use base64::Engine;

        if entry
            .chars()
            .any(|c| c == '\r' || c == '\n' || c == '\0' || (c.is_control() && c != '\t'))
        {
            return Err(
                "Recipient contains carriage return, newline, or control characters".to_string(),
            );
        }

        let trimmed = entry.trim();
        if trimmed.is_empty() {
            return Err("Recipient cannot be empty".to_string());
        }

        if trimmed.contains('<') || trimmed.contains('>') {
            let left_idx = trimmed
                .find('<')
                .ok_or_else(|| "Malformed recipient: missing '<'".to_string())?;
            let right_idx = trimmed
                .rfind('>')
                .ok_or_else(|| "Malformed recipient: missing '>'".to_string())?;
            if left_idx >= right_idx {
                return Err("Malformed recipient: invalid angle brackets order".to_string());
            }
            if right_idx != trimmed.len() - 1 {
                return Err("Malformed recipient: extra characters after '>'".to_string());
            }
            let display_name = trimmed[..left_idx].trim();
            let raw_addr = &trimmed[left_idx + 1..right_idx];
            let clean_addr = Self::validate_email_address(raw_addr)?;

            if display_name.is_empty() {
                Ok(clean_addr)
            } else {
                let sanitized_name = if !display_name.is_ascii()
                    || display_name.contains('"')
                    || display_name.contains(',')
                    || display_name.contains(':')
                {
                    format!(
                        "=?UTF-8?B?{}?=",
                        BASE64_STANDARD.encode(display_name.as_bytes())
                    )
                } else {
                    display_name.to_string()
                };
                Ok(format!("{} <{}>", sanitized_name, clean_addr))
            }
        } else {
            let clean_addr = Self::validate_email_address(trimmed)?;
            Ok(clean_addr)
        }
    }

    /// Standards-compliant RFC 2822 MIME message construction.
    /// Strictly rejects CRLF header injection across all untrusted inputs.
    /// Applies RFC 2047 encoding for non-ASCII subjects and display names.
    /// Safely structures threading headers (In-Reply-To and References).
    pub fn encode_rfc2822_message(
        to: &str,
        subject: &str,
        body: &str,
        in_reply_to: Option<&str>,
    ) -> Result<String, String> {
        use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
        use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL;
        use base64::Engine;

        // 1. Validate and sanitize recipient(s)
        if to
            .chars()
            .any(|c| c == '\r' || c == '\n' || c == '\0' || (c.is_control() && c != '\t'))
        {
            return Err("Header injection attempt: recipient field contains carriage return or newline characters".to_string());
        }
        let recipient_parts: Vec<&str> = to.split(',').collect();
        let mut sanitized_recipients = Vec::new();
        for r in recipient_parts {
            let s = r.trim();
            if !s.is_empty() {
                sanitized_recipients.push(Self::validate_and_sanitize_recipient(s)?);
            }
        }
        if sanitized_recipients.is_empty() {
            return Err("At least one valid recipient email address is required".to_string());
        }
        let clean_to = sanitized_recipients.join(", ");

        // 2. Validate and RFC 2047 encode Subject
        if subject
            .chars()
            .any(|c| c == '\r' || c == '\n' || c == '\0' || (c.is_control() && c != '\t'))
        {
            return Err("Header injection attempt: subject field contains carriage return or newline characters".to_string());
        }
        let clean_subject = if !subject.is_ascii() {
            format!("=?UTF-8?B?{}?=", BASE64_STANDARD.encode(subject.as_bytes()))
        } else {
            subject.to_string()
        };

        // 3. Construct base headers
        let mut raw = format!(
            "To: {}\r\nSubject: {}\r\nContent-Type: text/plain; charset=UTF-8\r\nMIME-Version: 1.0\r\n",
            clean_to, clean_subject
        );

        // 4. Safely validate and append In-Reply-To / References if present
        if let Some(irt) = in_reply_to {
            let irt_trimmed = irt.trim();
            if !irt_trimmed.is_empty() {
                if irt_trimmed.chars().any(|c| {
                    c == '\r' || c == '\n' || c == '\0' || c == ':' || (c.is_control() && c != '\t')
                }) {
                    return Err("Header injection attempt: in_reply_to contains invalid characters or newlines".to_string());
                }
                let clean_msg_id = irt_trimmed
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .trim();
                if clean_msg_id.is_empty() {
                    return Err("Invalid in_reply_to: empty message ID".to_string());
                }
                for c in clean_msg_id.chars() {
                    if !c.is_alphanumeric()
                        && c != '.'
                        && c != '-'
                        && c != '@'
                        && c != '+'
                        && c != '_'
                        && c != '%'
                    {
                        return Err(format!(
                            "Invalid character '{}' in in_reply_to message ID",
                            c
                        ));
                    }
                }
                raw.push_str(&format!(
                    "In-Reply-To: <{}>\r\nReferences: <{}>\r\n",
                    clean_msg_id, clean_msg_id
                ));
            }
        }

        raw.push_str("\r\n");
        raw.push_str(body);

        Ok(BASE64_URL.encode(raw.as_bytes()))
    }

    pub fn decode_base64url(s: &str) -> Result<Vec<u8>, String> {
        use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
        use base64::Engine;

        let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        URL_SAFE_NO_PAD
            .decode(&clean)
            .or_else(|_| URL_SAFE.decode(&clean))
            .map_err(|e| format!("Base64url decode error: {}", e))
    }

    pub fn html_to_plain_text(html: &str) -> String {
        let mut text = html.to_string();
        if let Ok(re_script) = regex::Regex::new(r"(?is)<script[^>]*>.*?</script>") {
            text = re_script.replace_all(&text, "").to_string();
        }
        if let Ok(re_style) = regex::Regex::new(r"(?is)<style[^>]*>.*?</style>") {
            text = re_style.replace_all(&text, "").to_string();
        }
        if let Ok(re_br) = regex::Regex::new(r"(?i)<br\s*/?>") {
            text = re_br.replace_all(&text, "\n").to_string();
        }
        if let Ok(re_p) = regex::Regex::new(r"(?i)</(?:p|div|tr|li|h[1-6])>") {
            text = re_p.replace_all(&text, "\n").to_string();
        }
        if let Ok(re_tags) = regex::Regex::new(r"<[^>]+>") {
            text = re_tags.replace_all(&text, "").to_string();
        }

        text = text
            .replace("&nbsp;", " ")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&apos;", "'");

        let mut cleaned = String::new();
        let mut prev_empty = false;
        for line in text.lines() {
            let tr = line.trim();
            if tr.is_empty() {
                if !prev_empty {
                    cleaned.push('\n');
                    prev_empty = true;
                }
            } else {
                cleaned.push_str(tr);
                cleaned.push('\n');
                prev_empty = false;
            }
        }
        cleaned.trim().to_string()
    }

    pub fn extract_mime_content(
        part: &Value,
        text_plain: &mut Option<String>,
        text_html: &mut Option<String>,
        attachments: &mut Vec<String>,
    ) {
        let mime_type = part["mimeType"].as_str().unwrap_or("").to_lowercase();
        let filename = part["filename"].as_str().unwrap_or("");

        if !filename.is_empty() {
            let size = part["body"]["size"].as_u64().unwrap_or(0);
            attachments.push(format!("{} ({}, {} bytes)", filename, mime_type, size));
            return;
        }

        if let Some(data_str) = part["body"]["data"].as_str() {
            if let Ok(bytes) = Self::decode_base64url(data_str) {
                let decoded_str = String::from_utf8_lossy(&bytes).to_string();
                if mime_type.starts_with("text/plain") && text_plain.is_none() {
                    *text_plain = Some(decoded_str);
                } else if mime_type.starts_with("text/html") && text_html.is_none() {
                    *text_html = Some(decoded_str);
                }
            }
        }

        if let Some(subparts) = part["parts"].as_array() {
            for sub in subparts {
                Self::extract_mime_content(sub, text_plain, text_html, attachments);
            }
        }
    }

    /// Resolves a NaiveDateTime in the given timezone deterministically, handling daylight saving transitions:
    /// - Exact matches (`LocalResult::Single`) return immediately.
    /// - Ambiguous times (e.g. fall-back where the same local time occurs twice) deterministically select
    ///   the earlier offset (`earliest`).
    /// - Missing times (e.g. spring-forward where a local time does not exist) advance in 15-minute intervals
    ///   up to 3 hours until the next valid local time is found.
    pub fn resolve_local_datetime_deterministic(
        tz: chrono_tz::Tz,
        naive: &chrono::NaiveDateTime,
    ) -> Result<chrono::DateTime<chrono_tz::Tz>, String> {
        use chrono::TimeZone;
        match tz.from_local_datetime(naive) {
            chrono::LocalResult::Single(dt) => Ok(dt),
            chrono::LocalResult::Ambiguous(earliest, _) => Ok(earliest),
            chrono::LocalResult::None => {
                for step in 1..=12 {
                    let shifted = naive
                        .checked_add_signed(chrono::Duration::minutes(step * 15))
                        .ok_or_else(|| "Datetime arithmetic overflow".to_string())?;
                    match tz.from_local_datetime(&shifted) {
                        chrono::LocalResult::Single(dt) => return Ok(dt),
                        chrono::LocalResult::Ambiguous(earliest, _) => return Ok(earliest),
                        chrono::LocalResult::None => continue,
                    }
                }
                Err(format!(
                    "Failed to resolve local time '{}' in timezone '{}' across DST transition gap",
                    naive, tz
                ))
            }
        }
    }
}

#[async_trait]
impl NativeConnectorAdapter for GoogleAdapter {
    fn provider_id(&self) -> &str {
        "google"
    }

    fn list_tools(&self, _account: &ConnectedAccountRecord) -> Vec<ConnectorToolDefinition> {
        vec![
            ConnectorToolDefinition {
                name: "gmail_list_messages".to_string(),
                description: "Search and list messages in Gmail with query filtering (e.g., 'is:unread', 'from:support', 'important').".to_string(),
                is_read_only: true,
                risk_level: RiskLevel::Safe,
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Gmail search query (e.g. 'is:unread', 'label:INBOX', 'after:2026/10/01')"
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Maximum number of messages to return (default 10, max 50)",
                            "minimum": 1,
                            "maximum": 50
                        }
                    }
                }),
            },
            ConnectorToolDefinition {
                name: "gmail_get_message".to_string(),
                description: "Retrieve details of a specific Gmail message including sender, subject, date, snippet, and body content.".to_string(),
                is_read_only: true,
                risk_level: RiskLevel::Safe,
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "message_id": {
                            "type": "string",
                            "description": "The unique Gmail message ID"
                        }
                    },
                    "required": ["message_id"]
                }),
            },
            ConnectorToolDefinition {
                name: "gmail_create_draft".to_string(),
                description: "Draft a new email or reply in Gmail for user review without sending it.".to_string(),
                is_read_only: false,
                risk_level: RiskLevel::Moderate,
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "to": {
                            "type": "string",
                            "description": "Recipient email address"
                        },
                        "subject": {
                            "type": "string",
                            "description": "Email subject line"
                        },
                        "body": {
                            "type": "string",
                            "description": "Email body text"
                        },
                        "in_reply_to": {
                            "type": "string",
                            "description": "Optional Message-ID or Thread-ID being replied to"
                        }
                    },
                    "required": ["to", "subject", "body"]
                }),
            },
            ConnectorToolDefinition {
                name: "gmail_send_email".to_string(),
                description: "Send an email through Gmail. Consequential action requiring explicit user confirmation.".to_string(),
                is_read_only: false,
                risk_level: RiskLevel::Destructive,
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "to": {
                            "type": "string",
                            "description": "Recipient email address"
                        },
                        "subject": {
                            "type": "string",
                            "description": "Email subject line"
                        },
                        "body": {
                            "type": "string",
                            "description": "Email body content"
                        }
                    },
                    "required": ["to", "subject", "body"]
                }),
            },
            ConnectorToolDefinition {
                name: "calendar_list_events".to_string(),
                description: "List upcoming calendar events from Google Calendar with optional time range and search query.".to_string(),
                is_read_only: true,
                risk_level: RiskLevel::Safe,
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "time_min": {
                            "type": "string",
                            "description": "Start of time range in ISO 8601 / RFC 3339 format (e.g. '2026-10-22T00:00:00Z')"
                        },
                        "time_max": {
                            "type": "string",
                            "description": "End of time range in ISO 8601 / RFC 3339 format"
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "Maximum number of events to return (default 10, max 50)",
                            "minimum": 1,
                            "maximum": 50
                        }
                    }
                }),
            },
            ConnectorToolDefinition {
                name: "calendar_create_event".to_string(),
                description: "Schedule a new event on Google Calendar. Consequential action requiring user confirmation.".to_string(),
                is_read_only: false,
                risk_level: RiskLevel::Moderate,
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "summary": {
                            "type": "string",
                            "description": "Title of the calendar event"
                        },
                        "start_time": {
                            "type": "string",
                            "description": "Start time in ISO 8601 / RFC 3339 format"
                        },
                        "end_time": {
                            "type": "string",
                            "description": "End time in ISO 8601 / RFC 3339 format"
                        },
                        "description": {
                            "type": "string",
                            "description": "Optional event description or notes"
                        },
                        "location": {
                            "type": "string",
                            "description": "Optional event location or meeting URL"
                        }
                    },
                    "required": ["summary", "start_time", "end_time"]
                }),
            },
            ConnectorToolDefinition {
                name: "calendar_find_meeting_times".to_string(),
                description: "Find available free time slots for a meeting on a given date from Google Calendar.".to_string(),
                is_read_only: true,
                risk_level: RiskLevel::Safe,
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "date": {
                            "type": "string",
                            "description": "Target date in YYYY-MM-DD format (e.g. '2026-10-23')"
                        },
                        "duration_minutes": {
                            "type": "integer",
                            "description": "Desired meeting duration in minutes (default 30)",
                            "minimum": 15,
                            "maximum": 240
                        },
                        "timezone": {
                            "type": "string",
                            "description": "IANA timezone identifier (e.g., 'America/New_York', 'UTC', 'Europe/London'). If omitted, uses UTC."
                        },
                        "work_start_hour": {
                            "type": "integer",
                            "description": "Start of working hours (0-23, default 9)",
                            "minimum": 0,
                            "maximum": 23
                        },
                        "work_end_hour": {
                            "type": "integer",
                            "description": "End of working hours (1-24, default 17)",
                            "minimum": 1,
                            "maximum": 24
                        }
                    },
                    "required": ["date"]
                }),
            },
        ]
    }

    async fn execute_tool(
        &self,
        account: &ConnectedAccountRecord,
        tool_name: &str,
        args: Value,
        vault: &Vault,
    ) -> Result<String, String> {
        let secret_raw = vault
            .get_secret(&account.credential_key)
            .map_err(|e| format!("Failed to read credential from vault: {}", e))?
            .ok_or_else(|| {
                format!(
                    "Google account '{}' is not authenticated. Please connect your Google account in Kyro.",
                    account.display_name
                )
            })?;
        let token = self.extract_token(&secret_raw)?;

        let (google_base, gmail_base) = self.resolve_base_urls(account);

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token))
                .map_err(|_| "Invalid authorization token characters".to_string())?,
        );
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

        match tool_name {
            "gmail_list_messages" => {
                let query = args["query"].as_str().unwrap_or("").trim();
                let max_results = args["max_results"].as_u64().unwrap_or(10).clamp(1, 50);

                let mut url = Url::parse(&format!("{}/gmail/v1/users/me/messages", gmail_base))
                    .map_err(|e| format!("Failed to build Gmail URL: {}", e))?;
                {
                    let mut q = url.query_pairs_mut();
                    q.append_pair("maxResults", &max_results.to_string());
                    if !query.is_empty() {
                        q.append_pair("q", query);
                    }
                }

                let res = self
                    .send_and_read_bounded(
                        self.client.get(url).headers(headers),
                        "Gmail messages list",
                    )
                    .await?;

                let messages = res["messages"].as_array().cloned().unwrap_or_default();
                let result_size_estimate = res["resultSizeEstimate"].as_u64();
                let total_found = result_size_estimate.unwrap_or(messages.len() as u64);
                let result = json!({
                    "total_found": total_found,
                    "messages_returned": messages.len(),
                    "messages": messages,
                    "result_size_estimate": res["resultSizeEstimate"],
                    "next_page_token": res["nextPageToken"],
                });
                Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
            }
            "gmail_get_message" => {
                let msg_id = args["message_id"]
                    .as_str()
                    .ok_or("Missing required 'message_id' parameter")?;
                if msg_id.trim().is_empty() {
                    return Err("'message_id' cannot be empty".to_string());
                }

                let url = format!(
                    "{}/gmail/v1/users/me/messages/{}?format=full",
                    gmail_base, msg_id
                );
                let res = self
                    .send_and_read_bounded(
                        self.client.get(&url).headers(headers),
                        &format!("Gmail message {}", msg_id),
                    )
                    .await?;

                let payload = &res["payload"];
                let mut text_plain = None;
                let mut text_html = None;
                let mut attachments = Vec::new();

                Self::extract_mime_content(
                    payload,
                    &mut text_plain,
                    &mut text_html,
                    &mut attachments,
                );

                let (body_text, body_format) = if let Some(plain) = text_plain {
                    (plain, "plain")
                } else if let Some(html) = text_html {
                    (Self::html_to_plain_text(&html), "html_converted")
                } else {
                    (
                        res["snippet"].as_str().unwrap_or("").to_string(),
                        "snippet_fallback",
                    )
                };

                let bounded_body: String = body_text.chars().take(40_000).collect();

                let mut from = "";
                let mut to = "";
                let mut cc = "";
                let mut subject = "";
                let mut date = "";
                let mut message_id_hdr = "";
                let mut in_reply_to_hdr = "";

                if let Some(hdrs) = payload["headers"].as_array() {
                    for h in hdrs {
                        let name = h["name"].as_str().unwrap_or_default();
                        let val = h["value"].as_str().unwrap_or_default();
                        if name.eq_ignore_ascii_case("subject") {
                            subject = val;
                        } else if name.eq_ignore_ascii_case("from") {
                            from = val;
                        } else if name.eq_ignore_ascii_case("to") {
                            to = val;
                        } else if name.eq_ignore_ascii_case("cc") {
                            cc = val;
                        } else if name.eq_ignore_ascii_case("date") {
                            date = val;
                        } else if name.eq_ignore_ascii_case("message-id") {
                            message_id_hdr = val;
                        } else if name.eq_ignore_ascii_case("in-reply-to") {
                            in_reply_to_hdr = val;
                        }
                    }
                }

                let result = json!({
                    "id": res["id"],
                    "thread_id": res["threadId"],
                    "subject": subject,
                    "from": from,
                    "to": to,
                    "cc": cc,
                    "date": date,
                    "message_id_header": message_id_hdr,
                    "in_reply_to": in_reply_to_hdr,
                    "snippet": res["snippet"],
                    "body": bounded_body,
                    "body_format": body_format,
                    "labels": res["labelIds"],
                    "attachments": attachments,
                });
                Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
            }
            "gmail_create_draft" => {
                let to = args["to"]
                    .as_str()
                    .ok_or("Missing required 'to' parameter")?;
                let subject = args["subject"]
                    .as_str()
                    .ok_or("Missing required 'subject' parameter")?;
                let body = args["body"]
                    .as_str()
                    .ok_or("Missing required 'body' parameter")?;
                let in_reply_to = args["in_reply_to"].as_str();

                let encoded_msg = Self::encode_rfc2822_message(to, subject, body, in_reply_to)?;

                let url = format!("{}/gmail/v1/users/me/drafts", gmail_base);
                let payload = json!({
                    "message": {
                        "raw": encoded_msg
                    }
                });

                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                let res = self
                    .send_and_read_bounded(
                        self.client.post(&url).headers(headers).json(&payload),
                        "Create Gmail draft",
                    )
                    .await?;

                let result = json!({
                    "draft_id": res["id"],
                    "message_id": res["message"]["id"],
                    "status": "draft_created",
                    "recipient": to,
                    "subject": subject,
                });
                Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
            }
            "gmail_send_email" => {
                let to = args["to"]
                    .as_str()
                    .ok_or("Missing required 'to' parameter")?;
                let subject = args["subject"]
                    .as_str()
                    .ok_or("Missing required 'subject' parameter")?;
                let body = args["body"]
                    .as_str()
                    .ok_or("Missing required 'body' parameter")?;

                let encoded_msg = Self::encode_rfc2822_message(to, subject, body, None)?;

                let url = format!("{}/gmail/v1/users/me/messages/send", gmail_base);
                let payload = json!({
                    "raw": encoded_msg
                });

                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                let res = self
                    .send_and_read_bounded(
                        self.client.post(&url).headers(headers).json(&payload),
                        "Send Gmail message",
                    )
                    .await?;

                let result = json!({
                    "sent_message_id": res["id"],
                    "thread_id": res["threadId"],
                    "status": "sent",
                    "recipient": to,
                    "subject": subject,
                });
                Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
            }
            "calendar_list_events" => {
                let max_results = args["max_results"].as_u64().unwrap_or(15).clamp(1, 50);
                let time_min = args["time_min"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

                let mut url = Url::parse(&format!(
                    "{}/calendar/v3/calendars/primary/events",
                    google_base
                ))
                .map_err(|e| format!("Failed to build Calendar URL: {}", e))?;
                {
                    let mut q = url.query_pairs_mut();
                    q.append_pair("singleEvents", "true");
                    q.append_pair("orderBy", "startTime");
                    q.append_pair("timeMin", &time_min);
                    q.append_pair("maxResults", &max_results.to_string());
                    if let Some(t_max) = args["time_max"].as_str() {
                        q.append_pair("timeMax", t_max);
                    }
                }

                // Paginate events to ensure full coverage up to the safety limit
                let mut page_token: Option<String> = None;
                let mut all_items: Vec<Value> = Vec::new();
                let mut page_count = 0;
                const MAX_CALENDAR_PAGES: usize = 10;
                let mut pagination_truncated = false;

                loop {
                    let mut page_url = url.clone();
                    if let Some(ref pt) = page_token {
                        page_url.query_pairs_mut().append_pair("pageToken", pt);
                    }

                    let res = self
                        .send_and_read_bounded(
                            self.client.get(page_url).headers(headers.clone()),
                            "Calendar events list",
                        )
                        .await?;

                    if let Some(arr) = res["items"].as_array() {
                        all_items.extend(arr.iter().cloned());
                    }

                    page_token = res["nextPageToken"]
                        .as_str()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                    page_count += 1;

                    if page_token.is_none() {
                        break;
                    }
                    if page_count >= MAX_CALENDAR_PAGES {
                        pagination_truncated = true;
                        break;
                    }
                }

                let events: Vec<Value> = all_items
                    .iter()
                    .map(|item| {
                        json!({
                            "id": item["id"],
                            "summary": item["summary"].as_str().unwrap_or("(No Title)"),
                            "description": item["description"],
                            "start": item["start"]["dateTime"].as_str().or_else(|| item["start"]["date"].as_str()).unwrap_or(""),
                            "end": item["end"]["dateTime"].as_str().or_else(|| item["end"]["date"].as_str()).unwrap_or(""),
                            "location": item["location"],
                            "html_link": item["htmlLink"],
                        })
                    })
                    .collect();

                let mut result = json!({
                    "total_events": events.len(),
                    "events": events,
                    "pages_retrieved": page_count,
                    "has_more": pagination_truncated,
                });
                if pagination_truncated {
                    result["warning"] = json!(format!(
                        "Calendar events list safety limit reached ({} pages) with additional events remaining.",
                        MAX_CALENDAR_PAGES
                    ));
                }
                Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
            }
            "calendar_create_event" => {
                let summary = args["summary"]
                    .as_str()
                    .ok_or("Missing required 'summary' parameter")?;
                let start_time = args["start_time"]
                    .as_str()
                    .ok_or("Missing required 'start_time' parameter")?;
                let end_time = args["end_time"]
                    .as_str()
                    .ok_or("Missing required 'end_time' parameter")?;
                let description = args["description"].as_str().unwrap_or("");
                let location = args["location"].as_str().unwrap_or("");

                // Validate ISO 8601 timestamps
                let start_dt = chrono::DateTime::parse_from_rfc3339(start_time).map_err(|e| {
                    format!(
                        "Invalid 'start_time' format '{}': {}. Expected RFC 3339 format.",
                        start_time, e
                    )
                })?;
                let end_dt = chrono::DateTime::parse_from_rfc3339(end_time).map_err(|e| {
                    format!(
                        "Invalid 'end_time' format '{}': {}. Expected RFC 3339 format.",
                        end_time, e
                    )
                })?;

                if start_dt >= end_dt {
                    return Err(format!(
                        "Validation error: 'start_time' ({}) must be earlier than 'end_time' ({}).",
                        start_time, end_time
                    ));
                }

                let url = format!("{}/calendar/v3/calendars/primary/events", google_base);
                let payload = json!({
                    "summary": summary,
                    "description": description,
                    "location": location,
                    "start": { "dateTime": start_time },
                    "end": { "dateTime": end_time }
                });

                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                let res = self
                    .send_and_read_bounded(
                        self.client.post(&url).headers(headers).json(&payload),
                        "Create Calendar event",
                    )
                    .await?;

                let result = json!({
                    "event_id": res["id"],
                    "summary": res["summary"],
                    "start": res["start"]["dateTime"],
                    "end": res["end"]["dateTime"],
                    "html_link": res["htmlLink"],
                    "status": "created",
                });
                Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
            }
            "calendar_find_meeting_times" => {
                let date_str = args["date"]
                    .as_str()
                    .ok_or("Missing required 'date' parameter (expected YYYY-MM-DD)")?;
                let duration_mins = args["duration_minutes"]
                    .as_i64()
                    .unwrap_or(30)
                    .clamp(15, 240);

                let tz_str = args["timezone"].as_str().unwrap_or("UTC");
                let tz: chrono_tz::Tz = if let Some(tz_raw) =
                    args.get("timezone").and_then(|t| t.as_str())
                {
                    let trimmed = tz_raw.trim();
                    if trimmed.is_empty() {
                        chrono_tz::UTC
                    } else {
                        trimmed.parse::<chrono_tz::Tz>().map_err(|_| {
                            format!(
                                "Invalid timezone '{}'. Expected a valid IANA timezone identifier (e.g. 'America/New_York', 'Europe/London', 'UTC').",
                                trimmed
                            )
                        })?
                    }
                } else {
                    chrono_tz::UTC
                };

                let start_hour: u32 =
                    args["work_start_hour"].as_u64().unwrap_or(9).clamp(0, 23) as u32;
                let end_hour: u32 =
                    args["work_end_hour"].as_u64().unwrap_or(17).clamp(1, 24) as u32;

                if start_hour >= end_hour {
                    return Err(format!(
                        "Invalid working hours: work_start_hour ({}) must be less than work_end_hour ({}).",
                        start_hour, end_hour
                    ));
                }

                let naive_date =
                    chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").map_err(|e| {
                        format!(
                            "Invalid date format '{}': {}. Expected YYYY-MM-DD.",
                            date_str, e
                        )
                    })?;

                let start_ndt = naive_date
                    .and_hms_opt(start_hour, 0, 0)
                    .ok_or_else(|| format!("Invalid time {}:00", start_hour))?;
                let end_ndt = naive_date
                    .and_hms_opt(end_hour, 0, 0)
                    .ok_or_else(|| format!("Invalid time {}:00", end_hour))?;

                let work_start_local = Self::resolve_local_datetime_deterministic(tz, &start_ndt)?;
                let work_end_local = Self::resolve_local_datetime_deterministic(tz, &end_ndt)?;

                let time_min = work_start_local.with_timezone(&chrono::Utc).to_rfc3339();
                let time_max = work_end_local.with_timezone(&chrono::Utc).to_rfc3339();

                let mut url = Url::parse(&format!(
                    "{}/calendar/v3/calendars/primary/events",
                    google_base
                ))
                .map_err(|e| format!("Failed to build Calendar URL: {}", e))?;
                {
                    let mut q = url.query_pairs_mut();
                    q.append_pair("singleEvents", "true");
                    q.append_pair("orderBy", "startTime");
                    q.append_pair("timeMin", &time_min);
                    q.append_pair("timeMax", &time_max);
                }

                // Paginate events to ensure full coverage of the time window
                let mut page_token: Option<String> = None;
                let mut items: Vec<Value> = Vec::new();
                let mut page_count = 0;

                loop {
                    let mut page_url = url.clone();
                    if let Some(ref pt) = page_token {
                        page_url.query_pairs_mut().append_pair("pageToken", pt);
                    }
                    let res = self
                        .send_and_read_bounded(
                            self.client.get(page_url).headers(headers.clone()),
                            "Find meeting times",
                        )
                        .await?;

                    if let Some(arr) = res["items"].as_array() {
                        items.extend(arr.iter().cloned());
                    }
                    page_token = res["nextPageToken"].as_str().map(|s| s.to_string());
                    page_count += 1;
                    if page_token.is_none() || page_count >= 10 {
                        break;
                    }
                }

                if page_token.is_some() && page_count >= 10 {
                    return Err("Calendar pagination safety limit reached (10 pages) with additional events remaining. Availability cannot be definitively calculated from incomplete calendar data.".to_string());
                }

                let work_start_ts = work_start_local.timestamp_millis();
                let work_end_ts = work_end_local.timestamp_millis();

                let mut busy_intervals: Vec<(i64, i64, String)> = Vec::new();

                for it in &items {
                    // Ignore cancelled events
                    if it["status"].as_str() == Some("cancelled") {
                        continue;
                    }
                    // Ignore events marked as transparent ("Show as Free" / available)
                    if it["transparency"].as_str() == Some("transparent") {
                        continue;
                    }

                    let summary = it["summary"].as_str().unwrap_or("Busy").to_string();
                    let start_val = &it["start"];
                    let end_val = &it["end"];

                    let start_ts = if let Some(dt_str) = start_val["dateTime"].as_str() {
                        chrono::DateTime::parse_from_rfc3339(dt_str)
                            .map(|d| d.timestamp_millis())
                            .ok()
                    } else if let Some(d_str) = start_val["date"].as_str() {
                        chrono::NaiveDate::parse_from_str(d_str, "%Y-%m-%d")
                            .ok()
                            .and_then(|nd| nd.and_hms_opt(0, 0, 0))
                            .and_then(|ndt| {
                                Self::resolve_local_datetime_deterministic(tz, &ndt).ok()
                            })
                            .map(|d| d.timestamp_millis())
                    } else {
                        None
                    };

                    let end_ts = if let Some(dt_str) = end_val["dateTime"].as_str() {
                        chrono::DateTime::parse_from_rfc3339(dt_str)
                            .map(|d| d.timestamp_millis())
                            .ok()
                    } else if let Some(d_str) = end_val["date"].as_str() {
                        // Google Calendar all-day event end dates are exclusive (midnight of end date)
                        chrono::NaiveDate::parse_from_str(d_str, "%Y-%m-%d")
                            .ok()
                            .and_then(|nd| nd.and_hms_opt(0, 0, 0))
                            .and_then(|ndt| {
                                Self::resolve_local_datetime_deterministic(tz, &ndt).ok()
                            })
                            .map(|d| d.timestamp_millis())
                    } else {
                        None
                    };

                    if let (Some(s), Some(e)) = (start_ts, end_ts) {
                        let clamped_s = s.max(work_start_ts);
                        let clamped_e = e.min(work_end_ts);
                        if clamped_s < clamped_e {
                            busy_intervals.push((clamped_s, clamped_e, summary));
                        }
                    }
                }

                busy_intervals.sort_by_key(|i| i.0);

                let mut merged_busy: Vec<(i64, i64)> = Vec::new();
                for (s, e, _) in &busy_intervals {
                    if let Some(last) = merged_busy.last_mut() {
                        if *s <= last.1 {
                            last.1 = last.1.max(*e);
                        } else {
                            merged_busy.push((*s, *e));
                        }
                    } else {
                        merged_busy.push((*s, *e));
                    }
                }

                let mut free_intervals: Vec<(i64, i64)> = Vec::new();
                let mut cursor = work_start_ts;
                for (bs, be) in &merged_busy {
                    if *bs > cursor {
                        free_intervals.push((cursor, *bs));
                    }
                    cursor = cursor.max(*be);
                }
                if cursor < work_end_ts {
                    free_intervals.push((cursor, work_end_ts));
                }

                let req_duration_ms = duration_mins * 60 * 1000;
                let mut candidate_slots: Vec<Value> = Vec::new();
                let now_ts = chrono::Utc::now().timestamp_millis();
                let min_notice_ms = 15 * 60 * 1000; // Require 15 mins lead time
                let earliest_slot_ts = now_ts + min_notice_ms;

                for (fs, fe) in &free_intervals {
                    let mut slot_start = *fs;
                    while slot_start + req_duration_ms <= *fe {
                        let slot_end = slot_start + req_duration_ms;
                        if slot_start >= earliest_slot_ts {
                            let dt_s = chrono::DateTime::from_timestamp_millis(slot_start)
                                .unwrap_or_default()
                                .with_timezone(&tz);
                            let dt_e = chrono::DateTime::from_timestamp_millis(slot_end)
                                .unwrap_or_default()
                                .with_timezone(&tz);
                            candidate_slots.push(json!({
                                "start": dt_s.to_rfc3339(),
                                "end": dt_e.to_rfc3339(),
                                "display": format!("{} – {} ({})", dt_s.format("%I:%M %p"), dt_e.format("%I:%M %p"), tz_str),
                                "duration_minutes": duration_mins,
                            }));
                        }
                        slot_start += 30 * 60 * 1000;
                    }
                }

                let busy_slots_display: Vec<Value> = busy_intervals
                    .iter()
                    .map(|(s, e, sum)| {
                        let dt_s = chrono::DateTime::from_timestamp_millis(*s).unwrap_or_default().with_timezone(&tz);
                        let dt_e = chrono::DateTime::from_timestamp_millis(*e).unwrap_or_default().with_timezone(&tz);
                        json!({
                            "summary": sum,
                            "start": dt_s.to_rfc3339(),
                            "end": dt_e.to_rfc3339(),
                            "display": format!("{} – {}", dt_s.format("%I:%M %p"), dt_e.format("%I:%M %p")),
                        })
                    })
                    .collect();

                let result = json!({
                    "date": date_str,
                    "timezone": tz_str,
                    "duration_minutes": duration_mins,
                    "working_hours": format!("{:02}:00 - {:02}:00", start_hour, end_hour),
                    "busy_slots": busy_slots_display,
                    "available_slots_suggestion": candidate_slots,
                    "has_availability": !candidate_slots.is_empty(),
                });
                Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
            }
            other => Err(format!("Unknown tool '{}' for Google connector", other)),
        }
    }

    async fn test_connection(
        &self,
        account: &ConnectedAccountRecord,
        vault: &Vault,
    ) -> Result<String, String> {
        let secret_raw = vault
            .get_secret(&account.credential_key)
            .map_err(|e| format!("Failed to read credential: {}", e))?
            .ok_or_else(|| "No credential found in vault for this Google account".to_string())?;
        let token = self.extract_token(&secret_raw)?;

        let (google_base, _) = self.resolve_base_urls(account);
        let url = format!("{}/oauth2/v2/userinfo", google_base);

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token))
                .map_err(|_| "Invalid authorization token format".to_string())?,
        );
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

        let res = self
            .send_and_read_bounded(self.client.get(&url).headers(headers), "Google userinfo")
            .await?;

        let email = res["email"].as_str().unwrap_or("connected user");
        let name = res["name"].as_str().unwrap_or("");
        Ok(format!(
            "Successfully connected to Google Workspace as {} <{}>. Gmail and Calendar tools are active.",
            name, email
        ))
    }
}
