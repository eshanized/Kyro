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

    fn encode_rfc2822_message(
        to: &str,
        subject: &str,
        body: &str,
        in_reply_to: Option<&str>,
    ) -> String {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL;
        use base64::Engine;

        let mut raw = format!(
            "To: {}\r\nSubject: {}\r\nContent-Type: text/plain; charset=UTF-8\r\n",
            to, subject
        );
        if let Some(irt) = in_reply_to {
            raw.push_str(&format!("In-Reply-To: {}\r\nReferences: {}\r\n", irt, irt));
        }
        raw.push_str("\r\n");
        raw.push_str(body);

        BASE64_URL.encode(raw.as_bytes())
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
        let token = vault
            .get_secret(&account.credential_key)
            .map_err(|e| format!("Failed to read credential from vault: {}", e))?
            .ok_or_else(|| {
                format!(
                    "Google account '{}' is not authenticated. Please connect your Google account in Kyro.",
                    account.display_name
                )
            })?;

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
                let result = json!({
                    "total_found": messages.len(),
                    "messages": messages,
                    "result_size_estimate": res["resultSizeEstimate"],
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

                // Extract friendly headers and snippet
                let snippet = res["snippet"].as_str().unwrap_or_default();
                let headers_list = res["payload"]["headers"].as_array();
                let mut subject = "";
                let mut from = "";
                let mut date = "";
                if let Some(hdrs) = headers_list {
                    for h in hdrs {
                        let name = h["name"].as_str().unwrap_or_default();
                        let val = h["value"].as_str().unwrap_or_default();
                        if name.eq_ignore_ascii_case("subject") {
                            subject = val;
                        } else if name.eq_ignore_ascii_case("from") {
                            from = val;
                        } else if name.eq_ignore_ascii_case("date") {
                            date = val;
                        }
                    }
                }

                let result = json!({
                    "id": res["id"],
                    "thread_id": res["threadId"],
                    "subject": subject,
                    "from": from,
                    "date": date,
                    "snippet": snippet,
                    "labels": res["labelIds"],
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

                let encoded_msg = Self::encode_rfc2822_message(to, subject, body, in_reply_to);

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

                let encoded_msg = Self::encode_rfc2822_message(to, subject, body, None);

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

                let res = self
                    .send_and_read_bounded(
                        self.client.get(url).headers(headers),
                        "Calendar events list",
                    )
                    .await?;

                let items = res["items"].as_array().cloned().unwrap_or_default();
                let events: Vec<Value> = items
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

                let result = json!({
                    "total_events": events.len(),
                    "events": events,
                });
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
                    .ok_or("Missing required 'date' parameter")?;
                let duration_mins = args["duration_minutes"]
                    .as_i64()
                    .unwrap_or(30)
                    .clamp(15, 240);

                let day_start = format!("{}T09:00:00Z", date_str);
                let day_end = format!("{}T17:00:00Z", date_str);

                let mut url = Url::parse(&format!(
                    "{}/calendar/v3/calendars/primary/events",
                    google_base
                ))
                .map_err(|e| format!("Failed to build Calendar URL: {}", e))?;
                {
                    let mut q = url.query_pairs_mut();
                    q.append_pair("singleEvents", "true");
                    q.append_pair("orderBy", "startTime");
                    q.append_pair("timeMin", &day_start);
                    q.append_pair("timeMax", &day_end);
                }

                let res = self
                    .send_and_read_bounded(
                        self.client.get(url).headers(headers),
                        "Find meeting times",
                    )
                    .await?;

                let items = res["items"].as_array().cloned().unwrap_or_default();
                let busy_slots: Vec<Value> = items
                    .iter()
                    .map(|it| {
                        json!({
                            "summary": it["summary"].as_str().unwrap_or("Busy"),
                            "start": it["start"]["dateTime"],
                            "end": it["end"]["dateTime"]
                        })
                    })
                    .collect();

                let result = json!({
                    "date": date_str,
                    "duration_minutes": duration_mins,
                    "working_hours": "09:00 - 17:00",
                    "busy_slots": busy_slots,
                    "available_slots_suggestion": [
                        format!("{}T10:00:00Z to {}T10:{:02}:00Z", date_str, date_str, duration_mins),
                        format!("{}T14:00:00Z to {}T14:{:02}:00Z", date_str, date_str, duration_mins),
                        format!("{}T15:30:00Z to {}T16:00:00Z", date_str, date_str)
                    ]
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
        let token = vault
            .get_secret(&account.credential_key)
            .map_err(|e| format!("Failed to read credential: {}", e))?
            .ok_or_else(|| "No credential found in vault for this Google account".to_string())?;

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
