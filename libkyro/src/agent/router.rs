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

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Verified capabilities of an LLM provider and model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub provider: String,
    pub model_id: String,
    pub text_input: bool,
    pub text_output: bool,
    pub vision: bool,
    #[serde(default)]
    pub image_input: bool,
    pub tool_calling: bool,
    pub supported_formats: Vec<String>,
    pub max_image_size_bytes: Option<usize>,
    pub max_tokens: Option<usize>,
    pub capability_status: String, // "verified", "configured", "unknown"
    pub configuration_error: Option<String>,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            provider: "custom".to_string(),
            model_id: "default".to_string(),
            text_input: true,
            text_output: true,
            vision: false,
            image_input: false,
            tool_calling: true,
            supported_formats: vec![],
            max_image_size_bytes: None,
            max_tokens: None,
            capability_status: "unknown".to_string(),
            configuration_error: None,
        }
    }
}

/// Validates an image payload against model capabilities before transmission.
pub fn validate_image_payload(
    raw_payload: &str,
    caps: &ModelCapabilities,
) -> Result<(String, usize, u32, u32), String> {
    if !caps.vision && !caps.image_input {
        return Err(format!(
            "Destination model '{}' (provider: {}) does not support image or multimodal inputs. Please route to a verified vision-capable model.",
            caps.model_id, caps.provider
        ));
    }

    let b64_clean = if let Some(idx) = raw_payload.find(";base64,") {
        &raw_payload[idx + 8..]
    } else if let Some(stripped) = raw_payload.strip_prefix("data:") {
        if let Some(idx) = stripped.find(',') {
            &stripped[idx + 1..]
        } else {
            raw_payload
        }
    } else {
        raw_payload.trim()
    };

    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64_clean)
        .map_err(|e| format!("Invalid base64 encoding in image payload: {}", e))?;

    let max_allowed = caps.max_image_size_bytes.unwrap_or(20 * 1024 * 1024);
    if bytes.len() > max_allowed {
        return Err(format!(
            "Image payload size ({} bytes) exceeds the maximum allowed limit ({} bytes) for model '{}'.",
            bytes.len(),
            max_allowed,
            caps.model_id
        ));
    }

    if bytes.is_empty() {
        return Err("Image payload is empty (0 bytes).".to_string());
    }

    let mime_type = if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else {
        return Err(
            "Unsupported image format. Supported formats are PNG, JPEG, WEBP, and GIF.".to_string(),
        );
    };

    if !caps.supported_formats.is_empty() && !caps.supported_formats.iter().any(|f| f == mime_type)
    {
        return Err(format!(
            "Image format '{}' is not supported by model '{}'. Supported formats: {:?}",
            mime_type, caps.model_id, caps.supported_formats
        ));
    }

    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("Failed to parse image dimensions or corrupted data: {}", e))?;
    let (width, height) = (img.width(), img.height());
    if width == 0 || height == 0 {
        return Err("Image dimensions are zero (invalid image).".to_string());
    }

    Ok((mime_type.to_string(), bytes.len(), width, height))
}

#[derive(Clone)]
pub struct ModelRouter {
    client: reqwest::Client,
    capabilities: Arc<RwLock<HashMap<String, ModelCapabilities>>>,
    fallback_vision_model: Arc<RwLock<Option<String>>>,
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
        Self {
            client,
            capabilities: Arc::new(RwLock::new(HashMap::new())),
            fallback_vision_model: Arc::new(RwLock::new(Some(DEFAULT_VISION_MODEL.to_string()))),
        }
    }

    /// Explicitly register capabilities for a model identifier
    pub async fn register_capability(&self, model: &str, caps: ModelCapabilities) {
        let mut map = self.capabilities.write().await;
        map.insert(model.to_string(), caps);
    }

    /// Set fallback vision model to use when primary model is text-only
    pub async fn set_fallback_vision_model(&self, model: Option<String>) {
        let mut guard = self.fallback_vision_model.write().await;
        *guard = model;
    }

    /// Get configured fallback vision model
    pub async fn get_fallback_vision_model(&self) -> Option<String> {
        let guard = self.fallback_vision_model.read().await;
        guard.clone()
    }

    /// Retrieve model capabilities using explicit registry, then provider/model capability metadata
    pub async fn capabilities_for(&self, model: &str) -> ModelCapabilities {
        let guard = self.capabilities.read().await;
        if let Some(caps) = guard.get(model) {
            return caps.clone();
        }
        Self::resolve_capabilities(model)
    }

    /// Statically resolve capabilities based on verified provider/model metadata
    pub fn resolve_capabilities(model: &str) -> ModelCapabilities {
        let lower = model.to_lowercase();
        let provider = if lower.contains("nvidia") || lower.contains("nemotron") {
            "nvidia".to_string()
        } else if lower.contains("gpt-") || lower.contains("openai") {
            "openai".to_string()
        } else if lower.contains("claude") || lower.contains("anthropic") {
            "anthropic".to_string()
        } else if lower.contains("gemini") || lower.contains("google") {
            "google".to_string()
        } else if lower.contains("llama") || lower.contains("meta/") {
            "meta".to_string()
        } else if lower.contains("mistral") || lower.contains("mixtral") {
            "mistral".to_string()
        } else if lower.contains("deepseek") {
            "deepseek".to_string()
        } else if lower.contains("qwen") {
            "qwen".to_string()
        } else if lower.contains("ollama") {
            "ollama".to_string()
        } else {
            "custom".to_string()
        };

        // Explicit text-only models MUST be rejected for vision
        let text_only =
            (lower.contains("instruct") && !lower.contains("vision") && !lower.contains("vl"))
                || lower.contains("deepseek-coder")
                || lower.contains("gpt-3.5")
                || lower.contains("llama-3-8b")
                || lower.contains("llama-3-70b")
                || lower.contains("llama-3.1-8b")
                || lower.contains("llama-3.1-70b")
                || lower.contains("mistral-7b")
                || lower.contains("mixtral-8x7b")
                || lower.contains("claude-2");

        if text_only {
            return ModelCapabilities {
                provider,
                model_id: model.to_string(),
                text_input: true,
                text_output: true,
                vision: false,
                image_input: false,
                tool_calling: true,
                supported_formats: vec![],
                max_image_size_bytes: None,
                max_tokens: Some(32768),
                capability_status: "verified".to_string(),
                configuration_error: None,
            };
        }

        let is_vision = lower.contains("vision")
            || lower.contains("neva")
            || lower.contains("vl")
            || lower.contains("gpt-4o")
            || lower.contains("claude-3")
            || lower.contains("claude-3-5")
            || lower.contains("gemini")
            || lower.contains("llava")
            || lower.contains("kosmos")
            || lower.contains("pixtral");

        let supported_formats = if is_vision {
            vec![
                "image/png".to_string(),
                "image/jpeg".to_string(),
                "image/webp".to_string(),
                "image/gif".to_string(),
            ]
        } else {
            vec![]
        };

        let max_image_size_bytes = if is_vision {
            Some(20 * 1024 * 1024) // 20 MB default limit
        } else {
            None
        };

        let max_tokens = if lower.contains("gemini") {
            Some(1000000)
        } else if lower.contains("gpt-4o")
            || lower.contains("claude-3")
            || lower.contains("llama-3")
        {
            Some(128000)
        } else {
            Some(32768)
        };

        let capability_status = if is_vision || lower.contains("nemotron") || lower.contains("gpt")
        {
            "verified".to_string()
        } else {
            "unknown".to_string()
        };

        ModelCapabilities {
            provider,
            model_id: model.to_string(),
            text_input: true,
            text_output: true,
            vision: is_vision,
            image_input: is_vision,
            tool_calling: true,
            supported_formats,
            max_image_size_bytes,
            max_tokens,
            capability_status,
            configuration_error: None,
        }
    }

    /// List known standard models with authoritative capability metadata
    pub fn list_known_models() -> Vec<ModelCapabilities> {
        vec![
            Self::resolve_capabilities(DEFAULT_NEMOTRON_MODEL),
            Self::resolve_capabilities(DEFAULT_VISION_MODEL),
            Self::resolve_capabilities("meta/llama-3.2-90b-vision-instruct"),
            Self::resolve_capabilities("openai/gpt-4o"),
            Self::resolve_capabilities("openai/gpt-4o-mini"),
            Self::resolve_capabilities("anthropic/claude-3-5-sonnet"),
            Self::resolve_capabilities("google/gemini-1.5-pro"),
            Self::resolve_capabilities("google/gemini-1.5-flash"),
            Self::resolve_capabilities("deepseek-ai/deepseek-coder-33b-instruct"),
            Self::resolve_capabilities("mistralai/mistral-large"),
        ]
    }

    /// Check if a model identifier indicates multimodal / vision capabilities
    pub fn is_vision_model(model: &str) -> bool {
        Self::resolve_capabilities(model).vision
    }

    /// Validate vision support for a model, returning an error if unsupported
    pub fn validate_vision_support(model: &str) -> Result<(), String> {
        if !Self::is_vision_model(model) {
            Err(format!(
                "Selected model '{}' does not support multimodal/vision inputs. Please configure a verified vision-capable model to process images.",
                model
            ))
        } else {
            Ok(())
        }
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

        if has_image_content {
            let caps = self.capabilities_for(model).await;
            if !caps.vision && !caps.image_input {
                return Err(format!(
                    "Selected model '{}' does not support multimodal/vision inputs. Please select a vision-capable model (e.g., Nemotron-Vision, Claude-3, or GPT-4o) to process images.",
                    model
                ));
            }
            // Validate all image payloads for size, MIME type, and dimension corruption
            for m in messages {
                if let MessageContent::Parts(parts) = &m.content {
                    for part in parts {
                        if let ContentPart::ImageUrl { image_url } = part {
                            validate_image_payload(&image_url.url, &caps)?;
                        }
                    }
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;

    #[test]
    fn test_resolve_capabilities_metadata() {
        let nemotron = ModelRouter::resolve_capabilities(DEFAULT_NEMOTRON_MODEL);
        assert_eq!(nemotron.provider, "nvidia");
        assert_eq!(nemotron.capability_status, "verified");
        assert!(nemotron.tool_calling);

        let llama_vision = ModelRouter::resolve_capabilities(DEFAULT_VISION_MODEL);
        assert!(llama_vision.vision);
        assert!(llama_vision.image_input);
        assert_eq!(llama_vision.provider, "meta");
        assert_eq!(
            llama_vision.supported_formats,
            vec!["image/png", "image/jpeg", "image/webp", "image/gif"]
        );

        let deepseek = ModelRouter::resolve_capabilities("deepseek-coder");
        assert!(!deepseek.vision);
        assert!(!deepseek.image_input);
        assert_eq!(deepseek.provider, "deepseek");

        let claude = ModelRouter::resolve_capabilities("claude-3-5-sonnet");
        assert!(claude.vision);
        assert_eq!(claude.provider, "anthropic");
    }

    #[test]
    fn test_validate_image_payload_rejection_and_acceptance() {
        let vision_caps = ModelRouter::resolve_capabilities(DEFAULT_VISION_MODEL);
        let text_caps = ModelRouter::resolve_capabilities("deepseek-coder");

        // Create a 1x1 valid PNG in base64
        let img = image::RgbaImage::new(1, 1);
        let mut png_bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png_bytes)
            .write_image(&img, 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        use base64::Engine;
        let b64_png = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        let data_url = format!("data:image/png;base64,{}", b64_png);

        // 1. Text caps must reject vision input
        let err_text = validate_image_payload(&data_url, &text_caps);
        assert!(err_text.is_err());
        assert!(err_text
            .unwrap_err()
            .contains("does not support image or multimodal inputs"));

        // 2. Vision caps must accept valid PNG data URL
        let ok_res = validate_image_payload(&data_url, &vision_caps);
        assert!(ok_res.is_ok());
        let (mime, bytes_len, w, h) = ok_res.unwrap();
        assert_eq!(mime, "image/png");
        assert!(bytes_len > 0);
        assert_eq!(w, 1);
        assert_eq!(h, 1);

        // 3. Invalid / corrupt payload must be rejected
        let corrupt = "data:image/png;base64,aW52YWxpZGJ5dGVz";
        let err_corrupt = validate_image_payload(corrupt, &vision_caps);
        assert!(err_corrupt.is_err());

        // 4. Empty payload must be rejected
        let empty = "data:image/png;base64,";
        let err_empty = validate_image_payload(empty, &vision_caps);
        assert!(err_empty.is_err());
    }

    #[test]
    fn test_list_known_models() {
        let models = ModelRouter::list_known_models();
        assert!(models.len() >= 8);
        assert!(models.iter().any(|m| m.model_id == DEFAULT_NEMOTRON_MODEL));
        assert!(models.iter().any(|m| m.model_id == DEFAULT_VISION_MODEL));
        assert!(models.iter().any(|m| m.model_id == "openai/gpt-4o"));
    }
}
