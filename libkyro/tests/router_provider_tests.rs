use kyro_lib::agent::cancellation::CancellationToken;
use kyro_lib::agent::router::{
    determine_provider_protocol, ChatMessage, MessageContent, ModelRouter, ProviderProtocol,
    ToolCall, ToolDefinition,
};
use kyro_lib::storage::ApiKeys;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

struct CapturedRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

struct MockProviderServer {
    pub base_url: String,
    pub requests: Arc<Mutex<Vec<CapturedRequest>>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

type MockResponse = (u16, Vec<(&'static str, &'static str)>, String);

impl MockProviderServer {
    async fn start(responses: VecDeque<MockResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind free port");
        let local_addr = listener.local_addr().expect("local addr");
        let base_url = format!("http://{}", local_addr);

        let requests = Arc::new(Mutex::new(Vec::new()));
        let responses = Arc::new(Mutex::new(responses));
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let reqs_clone = requests.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accept_res = listener.accept() => {
                        let (mut stream, _) = match accept_res {
                            Ok(s) => s,
                            Err(_) => break,
                        };

                        let reqs = reqs_clone.clone();
                        let resps = responses.clone();
                        tokio::spawn(async move {
                            let mut buf = Vec::new();
                            let mut tmp = [0u8; 4096];
                            let header_end = loop {
                                let n = stream.read(&mut tmp).await.unwrap_or(0);
                                if n == 0 {
                                    break buf.len();
                                }
                                buf.extend_from_slice(&tmp[..n]);
                                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                                    break pos + 4;
                                }
                            };

                            let header_str = String::from_utf8_lossy(&buf[..header_end.min(buf.len())]);
                            let mut lines = header_str.lines();
                            let req_line = lines.next().unwrap_or("");
                            let mut parts = req_line.split_whitespace();
                            let method = parts.next().unwrap_or("").to_string();
                            let path = parts.next().unwrap_or("").to_string();

                            let mut headers = Vec::new();
                            let mut content_length: usize = 0;
                            for line in lines {
                                if let Some((k, v)) = line.split_once(':') {
                                    let key = k.trim().to_lowercase();
                                    let val = v.trim().to_string();
                                    if key == "content-length" {
                                        content_length = val.parse().unwrap_or(0);
                                    }
                                    headers.push((key, val));
                                }
                            }

                            while buf.len() < header_end + content_length {
                                let n = stream.read(&mut tmp).await.unwrap_or(0);
                                if n == 0 {
                                    break;
                                }
                                buf.extend_from_slice(&tmp[..n]);
                            }

                            let body = if header_end < buf.len() {
                                String::from_utf8_lossy(&buf[header_end..]).to_string()
                            } else {
                                String::new()
                            };

                            reqs.lock().await.push(CapturedRequest {
                                method,
                                path,
                                headers,
                                body,
                            });

                            let maybe_resp = resps.lock().await.pop_front();
                            if let Some((status, hdrs, resp_body)) = maybe_resp {
                                let status_text = match status {
                                    200 => "OK",
                                    400 => "Bad Request",
                                    401 => "Unauthorized",
                                    429 => "Too Many Requests",
                                    500 => "Internal Server Error",
                                    _ => "Unknown",
                                };
                                let mut raw = format!("HTTP/1.1 {} {}\r\n", status, status_text);
                                let mut has_cl = false;
                                for (k, v) in hdrs {
                                    if k.eq_ignore_ascii_case("content-length") {
                                        has_cl = true;
                                    }
                                    raw.push_str(&format!("{}: {}\r\n", k, v));
                                }
                                if !has_cl && !raw.to_lowercase().contains("transfer-encoding") {
                                    raw.push_str(&format!("Content-Length: {}\r\n", resp_body.len()));
                                }
                                raw.push_str("\r\n");
                                raw.push_str(&resp_body);
                                let _ = stream.write_all(raw.as_bytes()).await;
                                let _ = stream.flush().await;
                            }
                        });
                    }
                }
            }
        });

        Self {
            base_url,
            requests,
            shutdown_tx: Some(shutdown_tx),
        }
    }
}

impl Drop for MockProviderServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

#[tokio::test]
async fn test_bug_15_anthropic_non_streaming_protocol_wire_contract() {
    let anthropic_response = serde_json::json!({
        "id": "msg_01X9z8anthropic",
        "type": "message",
        "role": "assistant",
        "content": [
            {
                "type": "text",
                "text": "I will examine the code now."
            },
            {
                "type": "tool_use",
                "id": "toolu_019876",
                "name": "read_file",
                "input": {
                    "path": "src/main.rs"
                }
            }
        ],
        "model": "claude-3-5-sonnet-20241022",
        "stop_reason": "tool_use"
    });

    let mut responses = VecDeque::new();
    responses.push_back((
        200,
        vec![("Content-Type", "application/json")],
        anthropic_response.to_string(),
    ));

    let server = MockProviderServer::start(responses).await;

    let router = ModelRouter::new();
    let messages = vec![
        ChatMessage {
            role: "user".to_string(),
            content: MessageContent::from_text("Please check main.rs"),
            tool_calls: None,
            tool_call_id: None,
        },
        ChatMessage {
            role: "assistant".to_string(),
            content: MessageContent::from_text("Running tool"),
            tool_calls: Some(vec![ToolCall {
                id: "toolu_prev".to_string(),
                name: "list_files".to_string(),
                arguments: serde_json::json!({"dir": "."}),
            }]),
            tool_call_id: None,
        },
        ChatMessage {
            role: "tool".to_string(),
            content: MessageContent::from_text("src/main.rs\nCargo.toml"),
            tool_calls: None,
            tool_call_id: Some("toolu_prev".to_string()),
        },
    ];

    let tools = vec![ToolDefinition {
        name: "read_file".to_string(),
        description: "Read file contents".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"]
        }),
    }];

    let resp = router
        .complete(
            "claude-3-5-sonnet",
            "sk-ant-test-key-12345",
            Some(&server.base_url),
            "You are a helpful coding assistant.",
            &messages,
            &tools,
            None,
        )
        .await
        .expect("Anthropic completion must succeed");

    // Verify response was parsed into standard LlmResponse
    assert_eq!(resp.content, "I will examine the code now.");
    assert_eq!(resp.tool_calls.len(), 1);
    assert_eq!(resp.tool_calls[0].id, "toolu_019876");
    assert_eq!(resp.tool_calls[0].name, "read_file");
    assert_eq!(resp.tool_calls[0].arguments["path"], "src/main.rs");

    // Verify wire contract sent to the server
    let reqs = server.requests.lock().await;
    assert_eq!(reqs.len(), 1);
    let req = &reqs[0];
    assert_eq!(req.method, "POST");
    assert_eq!(req.path, "/messages");

    // Check headers
    let has_api_key = req
        .headers
        .iter()
        .any(|(k, v)| k == "x-api-key" && v == "sk-ant-test-key-12345");
    assert!(has_api_key, "Request must include x-api-key header");

    let has_version = req
        .headers
        .iter()
        .any(|(k, v)| k == "anthropic-version" && v == "2023-06-01");
    assert!(has_version, "Request must include anthropic-version header");

    // Check payload structure
    let body_json: serde_json::Value =
        serde_json::from_str(&req.body).expect("body must be valid JSON");
    assert_eq!(body_json["model"], "claude-3-5-sonnet");
    assert_eq!(body_json["system"], "You are a helpful coding assistant.");

    // Tools must use Anthropic's input_schema parameter
    let tools_json = body_json["tools"].as_array().expect("tools array");
    assert_eq!(tools_json.len(), 1);
    assert_eq!(tools_json[0]["name"], "read_file");
    assert!(
        tools_json[0].get("input_schema").is_some(),
        "Anthropic tools must use input_schema instead of parameters"
    );

    // Messages must have converted tool results to tool_result blocks in user role
    let msgs_json = body_json["messages"].as_array().expect("messages array");
    let has_tool_result = msgs_json.iter().any(|m| {
        m["role"] == "user"
            && m["content"]
                .as_array()
                .map(|blocks| {
                    blocks
                        .iter()
                        .any(|b| b["type"] == "tool_result" && b["tool_use_id"] == "toolu_prev")
                })
                .unwrap_or(false)
    });
    assert!(
        has_tool_result,
        "Tool results must be converted to user message tool_result content blocks"
    );
}

#[tokio::test]
async fn test_bug_15_anthropic_streaming_sse_with_thinking_and_tools() {
    let sse_body = [
        "event: message_start\n",
        "data: {\"type\": \"message_start\", \"message\": {\"id\": \"msg_stream\", \"type\": \"message\", \"role\": \"assistant\"}}\n\n",
        "event: content_block_start\n",
        "data: {\"type\": \"content_block_start\", \"index\": 0, \"content_block\": {\"type\": \"thinking\", \"thinking\": \"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\": \"content_block_delta\", \"index\": 0, \"delta\": {\"type\": \"thinking_delta\", \"thinking\": \"Let me inspect the workspace first.\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\": \"content_block_stop\", \"index\": 0}\n\n",
        "event: content_block_start\n",
        "data: {\"type\": \"content_block_start\", \"index\": 1, \"content_block\": {\"type\": \"text\", \"text\": \"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\": \"content_block_delta\", \"index\": 1, \"delta\": {\"type\": \"text_delta\", \"text\": \"Starting analysis now...\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\": \"content_block_stop\", \"index\": 1}\n\n",
        "event: content_block_start\n",
        "data: {\"type\": \"content_block_start\", \"index\": 2, \"content_block\": {\"type\": \"tool_use\", \"id\": \"toolu_abc123\", \"name\": \"read_file\", \"input\": {}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\": \"content_block_delta\", \"index\": 2, \"delta\": {\"type\": \"input_json_delta\", \"partial_json\": \"{\\\"path\\\": \\\"Cargo.toml\\\"}\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\": \"content_block_stop\", \"index\": 2}\n\n",
        "event: message_delta\n",
        "data: {\"type\": \"message_delta\", \"delta\": {\"stop_reason\": \"tool_use\"}}\n\n",
        "event: message_stop\n",
        "data: {\"type\": \"message_stop\"}\n\n",
    ]
    .concat();

    let mut responses = VecDeque::new();
    responses.push_back((200, vec![("Content-Type", "text/event-stream")], sse_body));

    let server = MockProviderServer::start(responses).await;

    let router = ModelRouter::new();
    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Analyze workspace"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let tools = vec![ToolDefinition {
        name: "read_file".to_string(),
        description: "Read a file".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string" } }
        }),
    }];

    let mut captured_reasoning = String::new();
    let resp = router
        .complete_streamed(
            "claude-3-5-sonnet",
            "sk-ant-test-key-stream",
            Some(&server.base_url),
            "System prompt",
            &messages,
            &tools,
            None,
            |delta| {
                captured_reasoning.push_str(delta);
            },
        )
        .await
        .expect("Streamed Anthropic completion must succeed");

    assert_eq!(
        captured_reasoning, "Let me inspect the workspace first.",
        "Reasoning/thinking deltas must be streamed to the callback in real time"
    );
    assert_eq!(resp.content, "Starting analysis now...");
    assert_eq!(resp.tool_calls.len(), 1);
    assert_eq!(resp.tool_calls[0].id, "toolu_abc123");
    assert_eq!(resp.tool_calls[0].name, "read_file");
    assert_eq!(resp.tool_calls[0].arguments["path"], "Cargo.toml");
}

#[tokio::test]
async fn test_bug_15_cancellation_during_anthropic_streaming() {
    let cancel_token = CancellationToken::new();
    cancel_token.cancel();

    let router = ModelRouter::new();
    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::from_text("Cancelled request"),
        tool_calls: None,
        tool_call_id: None,
    }];

    let err = router
        .complete_streamed(
            "claude-3-5-sonnet",
            "sk-ant-key",
            Some("http://127.0.0.1:9"),
            "System prompt",
            &messages,
            &[],
            Some(&cancel_token),
            |_| {},
        )
        .await
        .expect_err("Cancelled token must abort immediately");

    assert!(
        err.contains("cancelled") || err.contains("Cancelled"),
        "Error message must indicate cancellation: {}",
        err
    );
}

#[test]
fn test_bug_15_protocol_determination() {
    // Anthropic official endpoints
    assert_eq!(
        determine_provider_protocol("https://api.anthropic.com/v1", "claude-3-5-sonnet"),
        ProviderProtocol::AnthropicMessages
    );
    assert_eq!(
        determine_provider_protocol("https://api.anthropic.com/v1/messages", "claude-3-haiku"),
        ProviderProtocol::AnthropicMessages
    );

    // Mock/local endpoint for Claude model
    assert_eq!(
        determine_provider_protocol("http://127.0.0.1:8080", "claude-3-5-sonnet"),
        ProviderProtocol::AnthropicMessages
    );
    assert_eq!(
        determine_provider_protocol("http://127.0.0.1:8080", "anthropic/claude-3-opus"),
        ProviderProtocol::AnthropicMessages
    );

    // Explicit OpenAI-compatible proxy (OpenRouter / Together / local proxy with chat completions)
    assert_eq!(
        determine_provider_protocol("https://openrouter.ai/api/v1", "anthropic/claude-3-sonnet"),
        ProviderProtocol::OpenAiCompatible
    );
    assert_eq!(
        determine_provider_protocol("https://api.openai.com/v1", "gpt-4o"),
        ProviderProtocol::OpenAiCompatible
    );
    assert_eq!(
        determine_provider_protocol(
            "https://integrate.api.nvidia.com/v1",
            "nvidia/nemotron-3-ultra-550b-a55b"
        ),
        ProviderProtocol::OpenAiCompatible
    );
}

#[test]
fn test_bug_15_missing_provider_credentials_fail_closed() {
    let empty_vault = ApiKeys::default();

    // Anthropic model route requires its own key
    let anthropic_route = ModelRouter::resolve_model_route("anthropic/claude-3-5-sonnet")
        .expect("resolution succeeds");
    assert_eq!(anthropic_route.provider, "anthropic");
    assert_eq!(
        anthropic_route.credential_key.as_deref(),
        Some("KYRO_ANTHROPIC_API_KEY")
    );

    let err = ModelRouter::select_api_key_for_route(&anthropic_route, &empty_vault)
        .expect_err("Missing Anthropic key must fail closed");
    assert!(
        err.contains("No API key configured for provider 'anthropic'"),
        "Error must clearly state missing provider key: {}",
        err
    );
}

#[test]
fn test_bug_15_mixed_provider_parent_child_delegation_resolution() {
    // Parent run uses NVIDIA model
    let parent_model = "nvidia/nemotron-3-ultra-550b-a55b";
    let parent_route = ModelRouter::resolve_model_route(parent_model).expect("parent route");
    assert_eq!(parent_route.provider, "nvidia");
    assert_eq!(
        parent_route.credential_key.as_deref(),
        Some("KYRO_NVIDIA_API_KEY")
    );

    // Child delegated run uses Anthropic model
    let child_model = "anthropic/claude-3-5-sonnet";
    let child_route = ModelRouter::resolve_model_route(child_model).expect("child route");
    assert_eq!(child_route.provider, "anthropic");
    assert_eq!(
        child_route.credential_key.as_deref(),
        Some("KYRO_ANTHROPIC_API_KEY")
    );
    assert_eq!(child_route.base_url, "https://api.anthropic.com/v1");

    // Child does NOT inherit parent's provider or endpoint
    assert_ne!(parent_route.provider, child_route.provider);
    assert_ne!(parent_route.base_url, child_route.base_url);
    assert_ne!(parent_route.credential_key, child_route.credential_key);
}
