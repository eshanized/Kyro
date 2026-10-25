use crate::agent::loop_engine::AgentLoopEngine;
use crate::agent::router::{ChatMessage, MessageContent, ModelRouter, DEFAULT_NEMOTRON_MODEL};
use crate::computer_use::{capture_screen, InputController};
use crate::governance::TakeoverController;
use crate::mcp::McpManager;
use crate::storage::{AgentRun, Database, Vault};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

pub struct TelegramBridge;

impl TelegramBridge {
    pub fn api_base() -> String {
        std::env::var("KYRO_TELEGRAM_API_BASE")
            .unwrap_or_else(|_| "https://api.telegram.org".to_string())
    }

    pub fn start_worker(
        vault: Vault,
        db: Database,
        engine: Arc<AgentLoopEngine>,
        mcp_manager: Option<McpManager>,
    ) {
        tauri::async_runtime::spawn(async move {
            let client = reqwest::Client::new();
            let mut last_update_id: i64 = 0;

            loop {
                sleep(Duration::from_secs(3)).await;

                // Load Telegram credentials from Vault
                let (token, allowed_chat_id, nvidia_key) = match vault.get_all_keys() {
                    Ok(keys) => {
                        let t = keys.telegram_bot_token.unwrap_or_default();
                        let c = keys.telegram_chat_id.unwrap_or_default();
                        let n = keys.nvidia;
                        if t.is_empty() {
                            continue;
                        }
                        (t, c, n)
                    }
                    Err(_) => continue,
                };

                // FAIL CLOSED: If allowed_chat_id is empty, refuse all traffic
                if allowed_chat_id.is_empty() {
                    log::warn!("Telegram bridge: telegram_chat_id is not configured. Rejecting all incoming Telegram messages (fail-closed).");
                    continue;
                }

                let url = format!(
                    "{}/bot{}/getUpdates?offset={}&timeout=2",
                    Self::api_base(),
                    token,
                    last_update_id + 1
                );

                let res = match client.get(&url).send().await {
                    Ok(r) => r,
                    Err(e) => {
                        log::warn!("Telegram bridge poll error: {}", e);
                        continue;
                    }
                };

                if let Ok(data) = res.json::<serde_json::Value>().await {
                    if let Some(updates) = data["result"].as_array() {
                        for u in updates {
                            if let Some(uid) = u["update_id"].as_i64() {
                                if uid > last_update_id {
                                    last_update_id = uid;
                                }
                            }

                            let _ = Self::handle_update(
                                u,
                                &token,
                                &allowed_chat_id,
                                nvidia_key.as_deref(),
                                &client,
                                &db,
                                engine.clone(),
                                mcp_manager.clone(),
                            )
                            .await;
                        }
                    }
                }
            }
        });
    }

    /// Processes an individual incoming Telegram update with strict authorization enforcement.
    #[allow(clippy::too_many_arguments)]
    pub async fn handle_update(
        u: &serde_json::Value,
        token: &str,
        allowed_chat_id: &str,
        nvidia_key: Option<&str>,
        client: &reqwest::Client,
        db: &Database,
        engine: Arc<AgentLoopEngine>,
        mcp_manager: Option<McpManager>,
    ) -> Result<bool, String> {
        let message = &u["message"];
        let chat_id = message["chat"]["id"].to_string();
        let text = message["text"].as_str().unwrap_or_default().trim();

        // FAIL CLOSED: Verify authorization before processing any instruction
        if chat_id != allowed_chat_id {
            log::warn!("Telegram bridge: rejected unauthorized message from unknown chat_id");
            return Err(format!("Unauthorized chat_id: {}", chat_id));
        }

        if text.is_empty() {
            return Ok(false);
        }

        // Process commands
        if text == "/start" || text == "/help" {
            let msg = "🤖 *Kyro Desktop Agent Connected*\n\nAvailable commands:\n• `/status` - Check desktop task state & pending approvals\n• `/screenshot` - View current desktop screen\n• `/takeover_resume` - Resume after human takeover\n• `/stop` - Emergency stop desktop automation\n• `/approve <id>` - Approve pending action\n• `/deny <id>` - Deny pending action\n• Send any other text to run a task on your PC!";
            let _ = Self::send_message(client, token, &chat_id, msg).await;
            Ok(true)
        } else if text == "/status" {
            let takeover = TakeoverController::get_status();
            let mut status_text = if takeover.is_active {
                format!("⚠️ *HUMAN TAKEOVER REQUIRED*\nReason: {}\nReply `/takeover_resume` once done.\n", takeover.reason)
            } else {
                "🟢 Kyro Desktop is idle and ready.\n".to_string()
            };

            if let Ok(pending) = db.list_pending_approvals() {
                if !pending.is_empty() {
                    status_text
                        .push_str(&format!("\n⏳ *Pending Approvals ({})*:\n", pending.len()));
                    for p in pending.iter().take(3) {
                        status_text.push_str(&format!(
                            "• `{}`: {}\n  `/approve {}` | `/deny {}`\n",
                            p.id, p.tool_name, p.id, p.id
                        ));
                    }
                }
            }

            let _ = Self::send_message(client, token, &chat_id, &status_text).await;
            Ok(true)
        } else if text == "/screenshot" {
            let _ =
                Self::send_message(client, token, &chat_id, "📸 Capturing desktop screen...").await;
            if let Ok(shot) = capture_screen(0) {
                use base64::Engine;
                if let Ok(bytes) =
                    base64::engine::general_purpose::STANDARD.decode(&shot.base64_png)
                {
                    let _ = Self::send_photo(client, token, &chat_id, bytes).await;
                }
            }
            Ok(true)
        } else if text == "/takeover_resume" {
            TakeoverController::resume();
            InputController::reset_abort();
            let _ = Self::send_message(
                client,
                token,
                &chat_id,
                "✅ Takeover resolved. Autonomous execution resumed.",
            )
            .await;
            Ok(true)
        } else if text == "/stop" {
            InputController::trigger_abort();
            let _ = Self::send_message(
                client,
                token,
                &chat_id,
                "🛑 Emergency stop triggered on Kyro desktop.",
            )
            .await;
            Ok(true)
        } else if text.starts_with("/approve ") {
            let app_id = text.trim_start_matches("/approve ").trim();
            match db.resolve_approval_request(app_id, "approved", "telegram_user") {
                Ok(true) => {
                    let _ = Self::send_message(
                        client,
                        token,
                        &chat_id,
                        &format!("✅ Approval `{}` granted.", app_id),
                    )
                    .await;
                }
                Ok(false) => {
                    let _ = Self::send_message(
                        client,
                        token,
                        &chat_id,
                        &format!(
                            "⚠️ Approval `{}` not found, expired, or already resolved.",
                            app_id
                        ),
                    )
                    .await;
                }
                Err(e) => {
                    let _ = Self::send_message(
                        client,
                        token,
                        &chat_id,
                        &format!("❌ Failed to resolve approval: {}", e),
                    )
                    .await;
                }
            }
            Ok(true)
        } else if text.starts_with("/deny ") {
            let app_id = text.trim_start_matches("/deny ").trim();
            match db.resolve_approval_request(app_id, "denied", "telegram_user") {
                Ok(true) => {
                    let _ = Self::send_message(
                        client,
                        token,
                        &chat_id,
                        &format!("🛑 Approval `{}` denied.", app_id),
                    )
                    .await;
                }
                Ok(false) => {
                    let _ = Self::send_message(
                        client,
                        token,
                        &chat_id,
                        &format!(
                            "⚠️ Approval `{}` not found, expired, or already resolved.",
                            app_id
                        ),
                    )
                    .await;
                }
                Err(e) => {
                    let _ = Self::send_message(
                        client,
                        token,
                        &chat_id,
                        &format!("❌ Failed to resolve approval: {}", e),
                    )
                    .await;
                }
            }
            Ok(true)
        } else {
            // Authorized incoming task: dispatch to agent loop engine
            let api_key = match nvidia_key {
                Some(k) if !k.is_empty() => k.to_string(),
                _ => {
                    let _ = Self::send_message(
                        client,
                        token,
                        &chat_id,
                        "❌ NVIDIA API key is not configured in Kyro desktop settings. Cannot run task.",
                    ).await;
                    return Ok(false);
                }
            };

            let run_id = format!("run_tg_{}", uuid::Uuid::new_v4());
            let now = chrono::Utc::now().timestamp_millis();
            let run_record = AgentRun {
                id: run_id.clone(),
                conversation_id: None,
                agent_id: "telegram-bot".to_string(),
                model: DEFAULT_NEMOTRON_MODEL.to_string(),
                prompt: text.to_string(),
                status: "running".to_string(),
                created_at: now,
                started_at: Some(now),
                completed_at: None,
                error: None,
                config_json: None,
                parent_run_id: None,
            };

            if let Err(e) = db.create_run(&run_record) {
                let _ = Self::send_message(
                    client,
                    token,
                    &chat_id,
                    &format!("❌ Failed to record agent run: {}", e),
                )
                .await;
                return Err(e.to_string());
            }

            let _ = Self::send_message(
                client,
                token,
                &chat_id,
                &format!(
                    "⚡ Task received: \"{}\". Executing on desktop with NVIDIA Nemotron...",
                    text
                ),
            )
            .await;

            let engine_cloned = engine.clone();
            let db_cloned = db.clone();
            let mcp_cloned = mcp_manager.clone();
            let client_cloned = client.clone();
            let token_cloned = token.to_string();
            let chat_id_cloned = chat_id.clone();
            let run_id_cloned = run_id.clone();
            let prompt_cloned = text.to_string();

            tauri::async_runtime::spawn(async move {
                let history = vec![ChatMessage {
                    role: "user".to_string(),
                    content: MessageContent::from_text(&prompt_cloned),
                    tool_calls: None,
                    tool_call_id: None,
                }];
                let sys = "You are Kyro, executing an authorized mobile task received via Telegram. Execute the objective step by step safely.";
                // Bind the endpoint explicitly via the authoritative route
                // resolver instead of relying on the implicit NVIDIA fallback
                // inside the router (Bug 15).
                let telegram_route = ModelRouter::resolve_model_route(DEFAULT_NEMOTRON_MODEL)
                    .map(|r| r.base_url)
                    .unwrap_or_else(|_| crate::agent::router::NVIDIA_BASE_URL.to_string());
                let result = engine_cloned
                    .run_step(
                        &run_id_cloned,
                        DEFAULT_NEMOTRON_MODEL,
                        &api_key,
                        Some(&telegram_route),
                        sys,
                        history,
                        db_cloned.clone(),
                        mcp_cloned,
                        None,
                        |_| {},
                    )
                    .await;

                let finish_time = chrono::Utc::now().timestamp_millis();
                match result {
                    Ok(reply) => {
                        let _ = db_cloned.update_run_status(
                            &run_id_cloned,
                            "succeeded",
                            None,
                            Some(finish_time),
                            None,
                        );
                        let msg = format!("✅ *Task completed:*\n{}", reply);
                        let _ = Self::send_message(
                            &client_cloned,
                            &token_cloned,
                            &chat_id_cloned,
                            &msg,
                        )
                        .await;
                    }
                    Err(err) => {
                        let _ = db_cloned.update_run_status(
                            &run_id_cloned,
                            "failed",
                            None,
                            Some(finish_time),
                            Some(&err),
                        );
                        let msg = format!("❌ *Task failed:*\n{}", err);
                        let _ = Self::send_message(
                            &client_cloned,
                            &token_cloned,
                            &chat_id_cloned,
                            &msg,
                        )
                        .await;
                    }
                }
            });
            Ok(true)
        }
    }

    pub async fn send_message(
        client: &reqwest::Client,
        token: &str,
        chat_id: &str,
        text: &str,
    ) -> Result<(), String> {
        let url = format!("{}/bot{}/sendMessage", Self::api_base(), token);
        let payload = serde_json::json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "Markdown"
        });
        client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Telegram sendMessage failed: {}", e))?;
        Ok(())
    }

    pub async fn send_photo(
        client: &reqwest::Client,
        token: &str,
        chat_id: &str,
        photo_bytes: Vec<u8>,
    ) -> Result<(), String> {
        let url = format!("{}/bot{}/sendPhoto", Self::api_base(), token);
        let part = reqwest::multipart::Part::bytes(photo_bytes)
            .file_name("screen.jpg")
            .mime_str("image/jpeg")
            .map_err(|e| format!("Multipart error: {}", e))?;

        let form = reqwest::multipart::Form::new()
            .text("chat_id", chat_id.to_string())
            .part("photo", part);

        client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("Telegram sendPhoto failed: {}", e))?;
        Ok(())
    }
}
