use crate::agent::cancellation::CancellationToken;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;

pub const NVIDIA_BASE_URL: &str = "https://integrate.api.nvidia.com/v1";
pub const DEFAULT_NEMOTRON_MODEL: &str = "nvidia/nemotron-3-ultra-550b-a55b";
pub const DEFAULT_VISION_MODEL: &str = "meta/llama-3.2-11b-vision-instruct";

/// One authoritative provider/model resolution.
///
/// The resolver binds a selected model identifier to
/// 1. the provider identity that actually serves the model,
/// 2. the endpoint/base URL to call,
/// 3. the credential reference that must be used.
///
/// Every caller (interactive turn, routine scheduler, delegated child agent,
/// vision fallback) MUST resolve the route through this function instead of
/// hard-coding an endpoint or silently inheriting the NVIDIA default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRoute {
    pub provider: String,
    pub base_url: String,
    /// Vault key name that holds the credential for this provider, or
    /// `None` when the provider does not need a key (e.g. a local Ollama).
    pub credential_key: Option<String>,
}

fn default_base_for_provider(provider: &str) -> &'static str {
    match provider {
        "openai" => "https://api.openai.com/v1",
        "anthropic" => "https://api.anthropic.com/v1",
        "google" => "https://generativelanguage.googleapis.com/v1beta/openai",
        "groq" => "https://api.groq.com/openai/v1",
        "mistral" => "https://api.mistral.ai/v1",
        "deepseek" => "https://api.deepseek.com/v1",
        "openrouter" => "https://openrouter.ai/api/v1",
        "together" => "https://api.together.xyz/v1",
        "ollama" => "http://127.0.0.1:11434/v1",
        _ => NVIDIA_BASE_URL,
    }
}

impl ModelRouter {
    /// Resolves the trusted provider route for a model identifier.
    ///
    /// Model identifiers hosted on NVIDIA NIM (`nvidia/...`, `nvidia/`,
    /// `meta/...`, `mistralai/...`, `qwen/...`, `deepseek-ai/...`) route to
    /// NVIDIA.  Model identifiers belonging to a different hosted provider
    /// (for example `openai/gpt-4o`) are NOT assumed to be reachable through
    /// NVIDIA's OpenAI-compatible schema — the request would target the wrong
    /// endpoint and credential.  Such models resolve to their own provider
    /// endpoint and credentials.
    pub fn resolve_model_route(model: &str) -> Result<ModelRoute, String> {
        let lower = model.trim().to_lowercase();

        let provider: &str =
            if lower.is_empty() || lower.starts_with("nvidia") || lower.contains("nemotron") {
                "nvidia"
            } else if lower.starts_with("ollama") || lower.contains("-ollama") {
                // Explicit local-deployment tags (`ollama*`, `*-ollama`) resolve
                // to the local Ollama endpoint BEFORE family prefixes are
                // considered: `mistral-7b-ollama` is an Ollama model tag that
                // api.mistral.ai does not serve, and the local instance is the
                // only endpoint that does.
                "ollama"
            } else if lower.starts_with("openai/")
                || lower.starts_with("gpt-")
                || lower.starts_with("o1-")
                || lower.starts_with("o3-")
                || lower.starts_with("o4-")
            {
                "openai"
            } else if lower.starts_with("anthropic/") || lower.starts_with("claude-") {
                "anthropic"
            } else if lower.starts_with("google/") || lower.starts_with("gemini") {
                "google"
            } else if lower.starts_with("groq/") {
                "groq"
            } else if lower.starts_with("mistralai/") {
                // `mistralai/...` identifiers are served verbatim on NVIDIA NIM;
                // Mistral's own API does not serve these HF-namespace IDs, so
                // routing them to api.mistral.ai would hit a nonexistent model.
                "nvidia"
            } else if lower.starts_with("mistral-") {
                "mistral"
            } else if lower.starts_with("deepseek-ai/") {
                // `deepseek-ai/...` identifiers are served verbatim on NVIDIA NIM;
                // DeepSeek's own API expects `deepseek-chat`/`deepseek-reasoner`.
                "nvidia"
            } else if lower.starts_with("deepseek") {
                "deepseek"
            } else if lower.starts_with("qwen")
                || lower.starts_with("meta/")
                || lower.starts_with("llama")
                || lower.contains("llava")
            {
                "nvidia"
            } else if lower.starts_with("openrouter/") {
                "openrouter"
            } else if lower.starts_with("together/") {
                "together"
            } else {
                // Unknown model families stay on the NVIDIA-compatible gateway,
                // which is the historical default of this platform.
                "nvidia"
            };

        // Environment overrides allow self-hosted proxies or regional
        // endpoints without source changes.
        let env_base = std::env::var(format!("KYRO_{}_BASE_URL", provider.to_uppercase()))
            .ok()
            .filter(|v| !v.trim().is_empty());

        let env_key_var = format!("KYRO_{}_API_KEY", provider.to_uppercase());

        Ok(ModelRoute {
            provider: provider.to_string(),
            base_url: env_base.unwrap_or_else(|| default_base_for_provider(provider).to_string()),
            credential_key: if provider == "ollama" {
                None
            } else {
                Some(env_key_var)
            },
        })
    }

    /// Selects the credential for a resolved route.
    ///
    /// This function refuses to silently substitute another provider's key:
    /// an absent or `Option::None` credential for the resolved provider is a
    /// hard error before any HTTP request is sent, so the request can never
    /// reach the wrong endpoint with the wrong authorization.
    pub fn select_api_key_for_route(
        route: &ModelRoute,
        vault_keys: &crate::storage::ApiKeys,
    ) -> Result<String, String> {
        match route.provider.as_str() {
            "ollama" => return Ok(String::new()),
            "nvidia" => {
                if let Some(k) = vault_keys.nvidia.as_ref().filter(|k| !k.trim().is_empty()) {
                    return Ok(k.clone());
                }
            }
            other => {
                if let Some(env_key) =
                    std::env::var(format!("KYRO_{}_API_KEY", other.to_uppercase()))
                        .ok()
                        .filter(|v| !v.trim().is_empty())
                {
                    return Ok(env_key);
                }
            }
        }

        Err(format!(
            "No API key configured for provider '{}' (service key '{}'). \
             The model requires this provider's endpoint and credential; \
             Kyro will not route it to a different provider. \
             Add the key in Settings or set the '{}' environment variable.",
            route.provider,
            route.credential_key.as_deref().unwrap_or("<vault>"),
            route.credential_key.as_deref().unwrap_or("<vault>")
        ))
    }
}

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
            Self::resolve_capabilities("meta/llama-3.3-70b-instruct"),
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

    /// Complete using streamed SSE with live reasoning extraction and tool-call assembly
    #[allow(clippy::too_many_arguments)]
    pub async fn complete_streamed<F>(
        &self,
        model: &str,
        api_key: &str,
        base_url: Option<&str>,
        system_prompt: &str,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        cancel_token: Option<&CancellationToken>,
        on_reasoning_delta: F,
    ) -> Result<LlmResponse, String>
    where
        F: FnMut(&str) + Send,
    {
        let endpoint = base_url.unwrap_or(NVIDIA_BASE_URL);
        let selected_model = if model.is_empty() {
            DEFAULT_NEMOTRON_MODEL
        } else {
            model
        };

        self.complete_streamed_with_retries(
            endpoint,
            api_key,
            selected_model,
            system_prompt,
            messages,
            tools,
            cancel_token,
            on_reasoning_delta,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn complete_streamed_with_retries<F>(
        &self,
        endpoint: &str,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        cancel_token: Option<&CancellationToken>,
        mut on_reasoning_delta: F,
    ) -> Result<LlmResponse, String>
    where
        F: FnMut(&str) + Send,
    {
        let max_retries = 3;
        let mut last_error = String::new();

        for attempt in 0..max_retries {
            if let Some(token) = cancel_token {
                if token.is_cancelled() {
                    return Err("Agent run cancelled before model completion".to_string());
                }
            }

            let mut deltas_emitted_this_attempt = false;
            let mut tracking_delta_sink = |delta: &str| {
                deltas_emitted_this_attempt = true;
                on_reasoning_delta(delta);
            };

            match self
                .complete_streamed_openai_compatible(
                    endpoint,
                    api_key,
                    model,
                    system_prompt,
                    messages,
                    tools,
                    cancel_token,
                    &mut tracking_delta_sink,
                )
                .await
            {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    // Do not duplicate partial reasoning after a retry.
                    // Once user-visible deltas have been emitted, abort further retries
                    // so partial reasoning is never replayed or duplicated.
                    if deltas_emitted_this_attempt {
                        log::warn!(
                            "Streamed model request failed mid-stream after emitting visible tokens: {}. Halting retries to prevent reasoning duplication.",
                            e
                        );
                        return Err(e);
                    }

                    let is_retryable = e.contains("429")
                        || e.contains("503")
                        || e.contains("502")
                        || e.contains("timeout")
                        || e.contains("connection");

                    if is_retryable && attempt + 1 < max_retries {
                        let delay = Duration::from_millis(500 * (1 << attempt));
                        log::warn!(
                            "Streamed model request attempt {} failed with retryable error: {}. Retrying in {:?}...",
                            attempt + 1, e, delay
                        );
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
            "Streamed model completion failed after {} retries: {}",
            max_retries, last_error
        ))
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

        let payload = build_chat_payload(model, system_prompt, messages, tools, false);

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

    #[allow(clippy::too_many_arguments)]
    async fn complete_streamed_openai_compatible<F>(
        &self,
        endpoint: &str,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        cancel_token: Option<&CancellationToken>,
        on_reasoning_delta: &mut F,
    ) -> Result<LlmResponse, String>
    where
        F: FnMut(&str) + Send,
    {
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

        let payload = build_chat_payload(model, system_prompt, messages, tools, true);

        let url = format!("{}/chat/completions", endpoint.trim_end_matches('/'));
        let mut req = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .header("accept", "text/event-stream");

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

        use futures_util::StreamExt;
        let mut stream = res.bytes_stream();
        let mut sse_buffer = SseBuffer::new();
        let mut accumulator = StreamAccumulator::default();

        let mut done = false;
        while !done {
            let next_chunk = if let Some(token) = cancel_token {
                tokio::select! {
                    _ = token.cancelled() => {
                        return Err("Model completion cancelled by user".to_string());
                    }
                    chunk = stream.next() => chunk
                }
            } else {
                stream.next().await
            };

            match next_chunk {
                Some(Ok(bytes)) => {
                    sse_buffer.push_bytes(&bytes);
                    while let Some(line) = sse_buffer.next_line() {
                        if accumulator.process_sse_line(&line, |rd| on_reasoning_delta(rd))? {
                            done = true;
                            break;
                        }
                        if let Some(token) = cancel_token {
                            if token.is_cancelled() {
                                return Err("Model completion cancelled by user".to_string());
                            }
                        }
                    }
                }
                Some(Err(e)) => {
                    return Err(format!(
                        "Network error while streaming model response: {}",
                        e
                    ));
                }
                None => {
                    if let Some(line) = sse_buffer.flush_remaining() {
                        if accumulator.process_sse_line(&line, |rd| on_reasoning_delta(rd))? {
                            done = true;
                        }
                    }
                    if !done && accumulator.finish_reason.is_none() {
                        return Err(
                            "Network stream ended prematurely before model completed response"
                                .to_string(),
                        );
                    }
                    break;
                }
            }
        }

        if let Some(token) = cancel_token {
            if token.is_cancelled() {
                return Err("Model completion cancelled by user".to_string());
            }
        }

        accumulator.finalize()
    }
}

pub fn build_chat_payload(
    model: &str,
    system_prompt: &str,
    messages: &[ChatMessage],
    tools: &[ToolDefinition],
    stream: bool,
) -> serde_json::Value {
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

    if stream {
        payload["stream"] = serde_json::json!(true);
    }

    let lower = model.to_lowercase();
    if lower.contains("nemotron") {
        payload["chat_template_kwargs"] = serde_json::json!({
            "enable_thinking": true
        });
    }

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

    payload
}

#[derive(Default, Debug, Clone)]
pub struct PartialStreamedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Default, Debug, Clone)]
pub struct StreamAccumulator {
    pub content: String,
    pub reasoning: String,
    pub tool_calls: HashMap<usize, PartialStreamedToolCall>,
    pub finish_reason: Option<String>,
}

impl StreamAccumulator {
    pub fn process_sse_line<F>(&mut self, line: &str, mut on_reasoning: F) -> Result<bool, String>
    where
        F: FnMut(&str),
    {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(':') {
            return Ok(false);
        }

        let data = if let Some(stripped) = trimmed.strip_prefix("data:") {
            stripped.trim()
        } else {
            trimmed
        };

        if data == "[DONE]" {
            return Ok(true);
        }

        let val: serde_json::Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(e) => {
                log::debug!("Non-JSON SSE data ignored: {} ({})", data, e);
                return Ok(false);
            }
        };

        if let Some(err) = val.get("error") {
            let msg = if let Some(m) = err.get("message").and_then(|m| m.as_str()) {
                m.to_string()
            } else {
                err.to_string()
            };
            return Err(format!("Model provider stream error: {}", msg));
        }

        if let Some(choices) = val.get("choices").and_then(|c| c.as_array()) {
            if choices.is_empty() {
                return Ok(false);
            }
            let choice = &choices[0];
            if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                self.finish_reason = Some(fr.to_string());
            }

            if let Some(delta) = choice.get("delta") {
                // Extract reasoning tokens
                let reasoning_delta = delta
                    .get("reasoning_content")
                    .or_else(|| delta.get("reasoning"))
                    .or_else(|| delta.get("thinking"))
                    .and_then(|r| r.as_str());

                if let Some(rd) = reasoning_delta {
                    if !rd.is_empty() {
                        self.reasoning.push_str(rd);
                        on_reasoning(rd);
                    }
                }

                // Extract ordinary assistant content
                if let Some(c) = delta.get("content").and_then(|c| c.as_str()) {
                    if !c.is_empty() {
                        self.content.push_str(c);
                    }
                }

                // Assemble streamed tool calls by index
                if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                    for tc in tcs {
                        let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                        let entry = self.tool_calls.entry(idx).or_default();
                        if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                            entry.id.push_str(id);
                        }
                        if let Some(func) = tc.get("function") {
                            if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                                entry.name.push_str(name);
                            }
                            if let Some(args) = func.get("arguments").and_then(|a| a.as_str()) {
                                entry.arguments.push_str(args);
                            }
                        }
                    }
                }
            } else if let Some(msg) = choice.get("message") {
                // Support full message format (fallback for non-streaming mock responses / proxies)
                let reasoning_delta = msg
                    .get("reasoning_content")
                    .or_else(|| msg.get("reasoning"))
                    .or_else(|| msg.get("thinking"))
                    .and_then(|r| r.as_str());

                if let Some(rd) = reasoning_delta {
                    if !rd.is_empty() {
                        self.reasoning.push_str(rd);
                        on_reasoning(rd);
                    }
                }

                if let Some(c) = msg.get("content").and_then(|c| c.as_str()) {
                    if !c.is_empty() {
                        self.content.push_str(c);
                    }
                }

                if let Some(tcs) = msg.get("tool_calls").and_then(|t| t.as_array()) {
                    for (i, tc) in tcs.iter().enumerate() {
                        let idx = tc
                            .get("index")
                            .and_then(|idx| idx.as_u64())
                            .unwrap_or(i as u64) as usize;
                        let entry = self.tool_calls.entry(idx).or_default();
                        if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                            entry.id = id.to_string();
                        }
                        if let Some(func) = tc.get("function") {
                            if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                                entry.name = name.to_string();
                            }
                            if let Some(args) = func.get("arguments") {
                                if let Some(s) = args.as_str() {
                                    entry.arguments = s.to_string();
                                } else {
                                    entry.arguments = args.to_string();
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(false)
    }

    pub fn finalize(mut self) -> Result<LlmResponse, String> {
        let mut sorted_indices: Vec<usize> = self.tool_calls.keys().cloned().collect();
        sorted_indices.sort_unstable();

        let mut final_tool_calls = Vec::new();
        for idx in sorted_indices {
            if let Some(tc) = self.tool_calls.remove(&idx) {
                let args_str = if tc.arguments.trim().is_empty() {
                    "{}"
                } else {
                    tc.arguments.trim()
                };
                let parsed_args: serde_json::Value =
                    serde_json::from_str(args_str).map_err(|e| {
                        format!(
                            "Model returned malformed tool-call JSON for '{}': {}",
                            tc.name, e
                        )
                    })?;
                final_tool_calls.push(ToolCall {
                    id: tc.id,
                    name: tc.name,
                    arguments: parsed_args,
                });
            }
        }

        Ok(LlmResponse {
            content: self.content,
            tool_calls: final_tool_calls,
        })
    }
}

#[derive(Default, Debug)]
pub struct SseBuffer {
    buffer: String,
    byte_buffer: Vec<u8>,
}

impl SseBuffer {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            byte_buffer: Vec::new(),
        }
    }

    /// Push an already-decoded text chunk (backward-compatibility).
    pub fn push_chunk(&mut self, chunk_str: &str) {
        self.buffer.push_str(chunk_str);
    }

    /// Incrementally push raw bytes from transport stream, safely handling
    /// multi-byte UTF-8 code points split across chunk boundaries without loss.
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        self.byte_buffer.extend_from_slice(bytes);
        self.drain_valid_utf8();
    }

    fn drain_valid_utf8(&mut self) {
        let mut start = 0;
        loop {
            if start >= self.byte_buffer.len() {
                self.byte_buffer.clear();
                break;
            }
            match std::str::from_utf8(&self.byte_buffer[start..]) {
                Ok(valid_str) => {
                    self.buffer.push_str(valid_str);
                    self.byte_buffer.clear();
                    break;
                }
                Err(err) => {
                    let valid_len = err.valid_up_to();
                    if valid_len > 0 {
                        let valid_str =
                            std::str::from_utf8(&self.byte_buffer[start..start + valid_len])
                                .expect("valid UTF-8 prefix verified by from_utf8");
                        self.buffer.push_str(valid_str);
                        start += valid_len;
                    }

                    match err.error_len() {
                        Some(invalid_len) => {
                            // Truly invalid byte sequence: emit replacement char and skip invalid bytes
                            self.buffer.push(std::char::REPLACEMENT_CHARACTER);
                            start += invalid_len;
                        }
                        None => {
                            // Incomplete multi-byte sequence at buffer tail: retain for next chunk
                            if start > 0 {
                                self.byte_buffer.drain(..start);
                            }
                            break;
                        }
                    }
                }
            }
        }
    }

    pub fn next_line(&mut self) -> Option<String> {
        if let Some(idx) = self.buffer.find('\n') {
            let line = self.buffer[..idx].trim_end_matches('\r').to_string();
            self.buffer.drain(..=idx);
            Some(line)
        } else {
            None
        }
    }

    pub fn flush_remaining(&mut self) -> Option<String> {
        if !self.byte_buffer.is_empty() {
            let lossy = String::from_utf8_lossy(&self.byte_buffer);
            self.buffer.push_str(&lossy);
            self.byte_buffer.clear();
        }

        if !self.buffer.trim().is_empty() {
            let remaining = std::mem::take(&mut self.buffer);
            Some(remaining.trim_end_matches('\r').to_string())
        } else {
            self.buffer.clear();
            None
        }
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

    #[test]
    fn test_sse_buffer_split_chunks_and_flush() {
        let mut buffer = SseBuffer::new();
        buffer.push_chunk("data: {\"choices\": [{\"delta\": {\"reasoning_content\": \"Let");
        assert!(buffer.next_line().is_none());

        buffer.push_chunk("'s think\"}}]}\n\ndata: [DONE]\n");
        let line1 = buffer.next_line().unwrap();
        assert_eq!(
            line1,
            "data: {\"choices\": [{\"delta\": {\"reasoning_content\": \"Let's think\"}}]}"
        );

        let line2 = buffer.next_line().unwrap();
        assert_eq!(line2, "");

        let line3 = buffer.next_line().unwrap();
        assert_eq!(line3, "data: [DONE]");

        assert!(buffer.next_line().is_none());

        // Test flush remaining without trailing newline
        buffer.push_chunk("data: {\"remaining\": true}");
        assert!(buffer.next_line().is_none());
        assert_eq!(
            buffer.flush_remaining(),
            Some("data: {\"remaining\": true}".to_string())
        );
        assert!(buffer.flush_remaining().is_none());
    }

    #[test]
    fn test_stream_accumulator_reasoning_and_content_separation() {
        let mut accumulator = StreamAccumulator::default();
        let mut emitted_reasoning = Vec::new();

        let chunk1 =
            "data: {\"choices\": [{\"delta\": {\"reasoning_content\": \"Thinking step 1. \"}}]}\n";
        let chunk2 =
            "data: {\"choices\": [{\"delta\": {\"reasoning_content\": \"Thinking step 2.\"}}]}\n";
        let chunk3 = "data: {\"choices\": [{\"delta\": {\"content\": \"Here is \"}}]}\n";
        let chunk4 = "data: {\"choices\": [{\"delta\": {\"content\": \"the actual answer.\"}}]}\n";
        let chunk_done = "data: [DONE]\n";

        for chunk in [chunk1, chunk2, chunk3, chunk4] {
            let is_done = accumulator
                .process_sse_line(chunk, |rd| emitted_reasoning.push(rd.to_string()))
                .unwrap();
            assert!(!is_done);
        }

        let is_done = accumulator
            .process_sse_line(chunk_done, |rd| emitted_reasoning.push(rd.to_string()))
            .unwrap();
        assert!(is_done);

        // Reasoning verified
        assert_eq!(
            emitted_reasoning,
            vec!["Thinking step 1. ", "Thinking step 2."]
        );
        assert_eq!(accumulator.reasoning, "Thinking step 1. Thinking step 2.");

        // Assistant content verified separate
        assert_eq!(accumulator.content, "Here is the actual answer.");

        let finalized = accumulator.finalize().unwrap();
        // Crucial: finalized content must ONLY contain the assistant content, never reasoning!
        assert_eq!(finalized.content, "Here is the actual answer.");
        assert!(finalized.tool_calls.is_empty());
    }

    #[test]
    fn test_stream_accumulator_tool_call_reassembly() {
        let mut accumulator = StreamAccumulator::default();

        let line1 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_shell_1","function":{"name":"execute_shell","arguments":""}}]}}]}"#;
        let line2 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"command\": "}}]}}]}"#;
        let line3 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"cargo test\"}"}}]}}]}"#;
        let line4 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":1,"id":"call_read_2","function":{"name":"read_file","arguments":"{\"path\":\"Cargo.toml\"}"}}]}}]}"#;

        for line in [line1, line2, line3, line4] {
            accumulator.process_sse_line(line, |_| {}).unwrap();
        }

        let resp = accumulator.finalize().unwrap();
        assert_eq!(resp.tool_calls.len(), 2);

        assert_eq!(resp.tool_calls[0].id, "call_shell_1");
        assert_eq!(resp.tool_calls[0].name, "execute_shell");
        assert_eq!(
            resp.tool_calls[0].arguments,
            serde_json::json!({ "command": "cargo test" })
        );

        assert_eq!(resp.tool_calls[1].id, "call_read_2");
        assert_eq!(resp.tool_calls[1].name, "read_file");
        assert_eq!(
            resp.tool_calls[1].arguments,
            serde_json::json!({ "path": "Cargo.toml" })
        );
    }

    #[test]
    fn test_stream_accumulator_provider_error() {
        let mut accumulator = StreamAccumulator::default();
        let error_chunk =
            r#"data: {"error":{"message":"Rate limit exceeded for endpoint","type":"rate_limit"}}"#;
        let res = accumulator.process_sse_line(error_chunk, |_| {});
        assert!(res.is_err());
        assert!(res
            .unwrap_err()
            .contains("Model provider stream error: Rate limit exceeded for endpoint"));
    }

    #[test]
    fn test_build_chat_payload_nemotron_vs_other_models() {
        let msg = vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::from_text("Hello"),
            tool_calls: None,
            tool_call_id: None,
        }];

        // 1. Nemotron gets chat_template_kwargs.enable_thinking
        let nemotron_payload =
            build_chat_payload(DEFAULT_NEMOTRON_MODEL, "sys prompt", &msg, &[], true);
        assert_eq!(nemotron_payload["stream"], true);
        assert_eq!(
            nemotron_payload["chat_template_kwargs"]["enable_thinking"],
            true
        );

        // 2. Other models (e.g. GPT-4o) do not get chat_template_kwargs
        let gpt_payload = build_chat_payload("openai/gpt-4o", "sys prompt", &msg, &[], true);
        assert_eq!(gpt_payload["stream"], true);
        assert!(gpt_payload.get("chat_template_kwargs").is_none());

        // 3. Non-streaming doesn't have stream: true
        let non_streaming = build_chat_payload("openai/gpt-4o", "sys", &msg, &[], false);
        assert!(non_streaming.get("stream").is_none());
    }

    #[test]
    fn test_sse_buffer_split_utf8_multibyte_characters() {
        let mut buffer = SseBuffer::new();

        // Rocket emoji 🚀 is 4 bytes: [0xF0, 0x9F, 0x9A, 0x80]
        // Euro symbol € is 3 bytes: [0xE2, 0x82, 0xAC]
        // Japanese text 'こんにちは' is 3 bytes per char
        let payload = "data: {\"reasoning\": \"Planning: 🚀 launch €100 こんにちは\"}\n";
        let bytes = payload.as_bytes();

        // Split right through the middle of the 4-byte rocket emoji:
        // Find position of 🚀 in bytes
        let rocket_idx = payload.find('🚀').unwrap();
        let split_pos_1 = rocket_idx + 2; // Split 2 bytes into the 4-byte sequence

        let chunk1 = &bytes[..split_pos_1];
        let chunk2 = &bytes[split_pos_1..];

        buffer.push_bytes(chunk1);
        // The line is not finished yet, and the incomplete UTF-8 bytes must not be corrupted
        assert!(buffer.next_line().is_none());

        buffer.push_bytes(chunk2);
        let line = buffer
            .next_line()
            .expect("full line decoded after second chunk");
        assert_eq!(
            line,
            "data: {\"reasoning\": \"Planning: 🚀 launch €100 こんにちは\"}"
        );
        assert!(
            !line.contains('\u{FFFD}'),
            "Split UTF-8 sequence must never be corrupted with replacement characters"
        );
    }

    #[tokio::test]
    async fn test_complete_streamed_retry_does_not_duplicate_reasoning() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let connection_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let conn_count_clone = connection_count.clone();

        tokio::spawn(async move {
            loop {
                if let Ok((mut socket, _)) = listener.accept().await {
                    let count = conn_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    tokio::spawn(async move {
                        // Read request headers until \r\n\r\n
                        let mut buf = [0u8; 1024];
                        let mut received = Vec::new();
                        loop {
                            let n = socket.read(&mut buf).await.unwrap_or(0);
                            if n == 0 {
                                break;
                            }
                            received.extend_from_slice(&buf[..n]);
                            if received.windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                        }

                        if count == 0 {
                            // Emit HTTP response and first delta, then drop connection prematurely
                            let sse_part = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
                            let _ = socket.write_all(sse_part.as_bytes()).await;
                            let line = "data: {\"choices\": [{\"delta\": {\"reasoning_content\": \"Initial partial reasoning... \"}}]}\n\n";
                            let _ = socket.write_all(line.as_bytes()).await;
                            let _ = socket.flush().await;
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            // Terminate stream abruptly without [DONE]
                            let _ = socket.shutdown().await;
                        } else {
                            // If a buggy client retried after reasoning was emitted:
                            let sse_part = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
                            let _ = socket.write_all(sse_part.as_bytes()).await;
                            let line = "data: {\"choices\": [{\"delta\": {\"reasoning_content\": \"Duplicated reasoning!\"}}]}\n\n";
                            let _ = socket.write_all(line.as_bytes()).await;
                            let line2 = "data: [DONE]\n\n";
                            let _ = socket.write_all(line2.as_bytes()).await;
                            let _ = socket.flush().await;
                        }
                    });
                }
            }
        });

        let router = ModelRouter::new();
        let mut emitted_reasoning = Vec::new();

        let endpoint = format!("http://{}", addr);
        let res = router
            .complete_streamed(
                "test-model",
                "test-key",
                Some(&endpoint),
                "sys",
                &[],
                &[],
                None,
                |delta| {
                    emitted_reasoning.push(delta.to_string());
                },
            )
            .await;

        // The stream emitted partial reasoning before failing abruptly.
        // It must NOT retry, preserving the partial stream state without duplication.
        assert!(
            res.is_err(),
            "Stream that failed mid-stream after emission must return Err"
        );
        assert_eq!(emitted_reasoning, vec!["Initial partial reasoning... "]);
        assert_eq!(
            connection_count.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
    }

    #[tokio::test]
    async fn test_complete_streamed_cancellation_during_reasoning() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let mut received = Vec::new();
                loop {
                    let n = socket.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    received.extend_from_slice(&buf[..n]);
                    if received.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let sse_part = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: keep-alive\r\n\r\n";
                let _ = socket.write_all(sse_part.as_bytes()).await;
                let line = "data: {\"choices\": [{\"delta\": {\"reasoning_content\": \"First delta. \"}}]}\n\n";
                let _ = socket.write_all(line.as_bytes()).await;
                let _ = socket.flush().await;
                // Wait indefinitely to simulate an active, long-running streaming response
                tokio::time::sleep(Duration::from_secs(15)).await;
            }
        });

        let router = ModelRouter::new();
        let cancel_token = CancellationToken::new();
        let cancel_clone = cancel_token.clone();

        let mut emitted = Vec::new();
        let endpoint = format!("http://{}", addr);

        let res = router
            .complete_streamed(
                "test-model",
                "test-key",
                Some(&endpoint),
                "sys",
                &[],
                &[],
                Some(&cancel_token),
                |delta| {
                    emitted.push(delta.to_string());
                    cancel_clone.cancel();
                },
            )
            .await;

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(
            err.to_lowercase().contains("cancel"),
            "Expected cancellation error, got: {}",
            err
        );
        assert_eq!(emitted, vec!["First delta. "]);
    }
}
