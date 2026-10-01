use crate::agent::cancellation::CancellationToken;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;

pub const NVIDIA_BASE_URL: &str = "https://integrate.api.nvidia.com/v1";
pub const DEFAULT_NEMOTRON_MODEL: &str = "nvidia/nemotron-3-ultra-550b-a55b";
pub const DEFAULT_VISION_MODEL: &str = "meta/llama-3.2-11b-vision-instruct";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrlDetail },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrlDetail {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

impl MessageContent {
    pub fn from_text(s: impl Into<String>) -> Self {
        MessageContent::Text(s.into())
    }

    pub fn from_text_and_image(text: &str, base64_png: &str) -> Self {
        MessageContent::Parts(vec![
            ContentPart::Text {
                text: text.to_string(),
            },
            ContentPart::ImageUrl {
                image_url: ImageUrlDetail {
                    url: format!("data:image/png;base64,{}", base64_png),
                },
            },
        ])
    }

    pub fn as_text_lossy(&self) -> String {
        match self {
            MessageContent::Text(t) => t.clone(),
            MessageContent::Parts(parts) => {
                let mut out = String::new();
                for p in parts {
                    if let ContentPart::Text { text } = p {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(text);
                    }
                }
                out
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: MessageContent,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

pub struct ModelRouter {
    client: reqwest::Client,
}

impl Default for ModelRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelRouter {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(90))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client }
    }

    /// Check if a model identifier indicates multimodal / vision capabilities
    pub fn is_vision_model(model: &str) -> bool {
        let lower = model.to_lowercase();
        lower.contains("vision")
            || lower.contains("neva")
            || lower.contains("vl")
            || lower.contains("gpt-4o")
            || lower.contains("claude-3")
            || lower.contains("gemini")
            || lower.contains("llava")
            || lower.contains("kosmos")
            || lower.contains("pixtral")
    }

    /// Complete using NVIDIA NIM or OpenAI-compatible endpoint with cancellation support
    #[allow(clippy::too_many_arguments)]
    pub async fn complete(
        &self,
        model: &str,
        api_key: &str,
        base_url: Option<&str>,
        system_prompt: &str,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        cancel_token: Option<&CancellationToken>,
    ) -> Result<LlmResponse, String> {
        let endpoint = base_url.unwrap_or(NVIDIA_BASE_URL);
        let selected_model = if model.is_empty() {
            DEFAULT_NEMOTRON_MODEL
        } else {
            model
        };

        self.complete_with_retries(
            endpoint,
            api_key,
            selected_model,
            system_prompt,
            messages,
            tools,
            cancel_token,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn complete_with_retries(
        &self,
        endpoint: &str,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        cancel_token: Option<&CancellationToken>,
    ) -> Result<LlmResponse, String> {
        let max_retries = 3;
        let mut last_error = String::new();

        for attempt in 0..max_retries {
            if let Some(token) = cancel_token {
                if token.is_cancelled() {
                    return Err("Agent run cancelled before model completion".to_string());
                }
            }

            match self
                .complete_openai_compatible(
                    endpoint,
                    api_key,
                    model,
                    system_prompt,
                    messages,
                    tools,
                    cancel_token,
                )
                .await
            {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    let is_retryable = e.contains("429")
                        || e.contains("503")
                        || e.contains("502")
                        || e.contains("timeout")
                        || e.contains("connection");

                    if is_retryable && attempt + 1 < max_retries {
                        let delay = Duration::from_millis(500 * (1 << attempt));
                        log::warn!("Model request attempt {} failed with retryable error: {}. Retrying in {:?}...", attempt + 1, e, delay);
                        if let Some(token) = cancel_token {
                            tokio::select! {
                                _ = token.cancelled() => {
                                    return Err("Model completion cancelled during retry delay".to_string());
                                }
                                _ = sleep(delay) => {}
                            }
                        } else {
                            sleep(delay).await;
                        }
                        last_error = e;
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(format!(
            "Model completion failed after {} retries: {}",
            max_retries, last_error
        ))
    }

    #[allow(clippy::too_many_arguments)]
    async fn complete_openai_compatible(
        &self,
        endpoint: &str,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        cancel_token: Option<&CancellationToken>,
    ) -> Result<LlmResponse, String> {
        // Explicitly reject multimodal inputs when calling non-vision models to prevent silent dropping
        let has_image_content = messages.iter().any(|m| match &m.content {
            MessageContent::Parts(parts) => parts
                .iter()
                .any(|p| matches!(p, ContentPart::ImageUrl { .. })),
            _ => false,
        });

        if has_image_content && !Self::is_vision_model(model) {
            return Err(format!(
                "Selected model '{}' does not support multimodal/vision inputs. Please select a vision-capable model (e.g., Nemotron-Vision, Claude-3, or GPT-4o) to process images.",
                model
            ));
        }

        let mut oai_messages = Vec::new();
        if !system_prompt.is_empty() {
            oai_messages.push(serde_json::json!({
                "role": "system",
                "content": system_prompt
            }));
        }

        for m in messages {
            let content_val = match &m.content {
                MessageContent::Text(t) => serde_json::Value::String(t.clone()),
                MessageContent::Parts(parts) => {
                    serde_json::to_value(parts).unwrap_or(serde_json::json!([]))
                }
            };

            if let Some(ref tcs) = m.tool_calls {
                let oai_tcs: Vec<serde_json::Value> = tcs
                    .iter()
                    .map(|tc| {
                        serde_json::json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": tc.arguments.to_string()
                            }
                        })
                    })
                    .collect();
                oai_messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": content_val,
                    "tool_calls": oai_tcs
                }));
            } else if m.role == "tool" {
                oai_messages.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": m.tool_call_id.clone().unwrap_or_default(),
                    "content": content_val
                }));
            } else {
                oai_messages.push(serde_json::json!({
                    "role": m.role,
                    "content": content_val
                }));
            }
        }

        let mut payload = serde_json::json!({
            "model": model,
            "messages": oai_messages,
            "temperature": 0.5,
            "max_tokens": 4096,
        });

        if !tools.is_empty() {
            let oai_tools: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters
                        }
                    })
                })
                .collect();
            payload["tools"] = serde_json::json!(oai_tools);
            payload["tool_choice"] = serde_json::json!("auto");
        }

        let url = format!("{}/chat/completions", endpoint.trim_end_matches('/'));
        let mut req = self
            .client
            .post(&url)
            .header("content-type", "application/json");

        if !api_key.is_empty() {
            req = req.header("authorization", format!("Bearer {}", api_key));
        }

        let res = if let Some(token) = cancel_token {
            tokio::select! {
                _ = token.cancelled() => {
                    return Err("Model completion cancelled by user".to_string());
                }
                res_result = req.json(&payload).send() => {
                    res_result.map_err(|e| format!("HTTP request error to model provider at {}: {}", url, e))?
                }
            }
        } else {
            req.json(&payload)
                .send()
                .await
                .map_err(|e| format!("HTTP request error to model provider at {}: {}", url, e))?
        };

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            return Err(format!("Model provider API error ({}): {}", status, text));
        }

        let data: serde_json::Value = res
            .json()
            .await
            .map_err(|e| format!("Failed to parse response JSON: {}", e))?;

        let choices = data["choices"]
            .as_array()
            .ok_or_else(|| "Response contains no 'choices' array".to_string())?;

        if choices.is_empty() {
            return Err("Model response choices list is empty".to_string());
        }

        let choice = &choices[0]["message"];
        let content = choice["content"].as_str().unwrap_or_default().to_string();

        let mut tool_calls = Vec::new();
        if let Some(tcs) = choice["tool_calls"].as_array() {
            for tc in tcs {
                let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                // Validate tool arguments strictly; never fallback silently to null
                let parsed_args: serde_json::Value =
                    serde_json::from_str(args_str).map_err(|e| {
                        format!(
                            "Model returned malformed tool-call JSON for '{}': {}",
                            tc["function"]["name"], e
                        )
                    })?;

                tool_calls.push(ToolCall {
                    id: tc["id"].as_str().unwrap_or_default().to_string(),
                    name: tc["function"]["name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    arguments: parsed_args,
                });
            }
        }

        Ok(LlmResponse {
            content,
            tool_calls,
        })
    }
}
