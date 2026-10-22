use kyro_lib::connector::auth::{
    html_escape, OAuthManager, PendingOAuthSession, TokenExchangeResult,
};
use kyro_lib::connector::manager::{
    canonicalize_json_value, classify_external_tool, compute_schema_fingerprint,
    ConnectorManager, InvocationAuthProof,
};
use kyro_lib::connector::remote_mcp::{RemoteMcpClient, ValidatedRemoteEndpoint};
use kyro_lib::execution::permissions::RiskLevel;
use kyro_lib::governance::auth::AuthDecision;
use kyro_lib::mcp::{validate_schema, McpManager};
use kyro_lib::storage::{
    ConnectedAccountRecord, ConnectorToolPolicyRecord, Database, Vault,
};
use serde_json::json;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

struct TestDir {
    path: std::path::PathBuf,
}

impl TestDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{}_{}", prefix, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn create_test_db_and_vault() -> (Database, Vault, TestDir) {
    let td = TestDir::new("kyro_conn_test");
    let db = Database::init(td.path().to_path_buf()).unwrap();
    let vault = Vault::new_encrypted_fallback(db.clone());
    (db, vault, td)
}

#[test]
fn test_connector_canonicalize_and_hash() {
    // 1. canonicalize_json_value sorting
    let unsorted = json!({
        "zebra": 1,
        "apple": 2,
        "nested": {
            "zeta": true,
            "alpha": [3, 2, 1]
        }
    });

    let canonical = canonicalize_json_value(&unsorted);
    let s = canonical.to_string();
    assert!(s.find("\"apple\"").unwrap() < s.find("\"zebra\"").unwrap());
    assert!(s.find("\"alpha\"").unwrap() < s.find("\"zeta\"").unwrap());

    // 2. compute_args_hash determinism
    let hash1 = InvocationAuthProof::compute_args_hash(&json!({"b": 2, "a": 1}));
    let hash2 = InvocationAuthProof::compute_args_hash(&json!({"a": 1, "b": 2}));
    assert_eq!(hash1, hash2, "Hashes must be identical regardless of key order");
    assert_eq!(hash1.len(), 64, "SHA-256 hex string should be 64 characters");

    // 3. compute_schema_fingerprint
    let schema1 = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "age": {"type": "integer"}
        }
    });
    let schema2 = json!({
        "properties": {
            "age": {"type": "integer"},
            "name": {"type": "string"}
        },
        "type": "object"
    });
    assert_eq!(
        compute_schema_fingerprint(&schema1),
        compute_schema_fingerprint(&schema2)
    );
}

#[test]
fn test_connector_tool_classification() {
    let empty_schema = json!({"type": "object"});

    let (risk, is_ro, _auto) = classify_external_tool("delete_database", "Destroys a database", &empty_schema);
    assert_eq!(risk, RiskLevel::Destructive);
    assert!(!is_ro);

    let (risk, is_ro, _auto) = classify_external_tool("destroy_cluster", "Deletes cluster", &empty_schema);
    assert_eq!(risk, RiskLevel::Destructive);
    assert!(!is_ro);

    let (risk, is_ro, _auto) = classify_external_tool("drop_table", "Drops SQL table", &empty_schema);
    assert_eq!(risk, RiskLevel::Destructive);
    assert!(!is_ro);

    let (risk, is_ro, _auto) = classify_external_tool("list_repositories", "Lists GitHub repos", &empty_schema);
    assert_eq!(risk, RiskLevel::Safe);
    assert!(is_ro);

    let (risk, is_ro, _auto) = classify_external_tool("get_user_profile", "Fetches user info", &empty_schema);
    assert_eq!(risk, RiskLevel::Safe);
    assert!(is_ro);

    let (risk, is_ro, _auto) = classify_external_tool("create_issue", "Creates a new issue", &empty_schema);
    assert_eq!(risk, RiskLevel::Moderate);
    assert!(!is_ro);
}

#[test]
fn test_connector_permanent_vs_transient_refresh_errors() {
    assert!(ConnectorManager::is_permanent_refresh_error("invalid_grant: refresh token expired"));
    assert!(ConnectorManager::is_permanent_refresh_error("unauthorized_client"));
    assert!(ConnectorManager::is_permanent_refresh_error("token has been revoked by user"));
    assert!(ConnectorManager::is_permanent_refresh_error("status: 401 unauthorized"));
    assert!(ConnectorManager::is_permanent_refresh_error("status: 403 forbidden"));

    assert!(!ConnectorManager::is_permanent_refresh_error("503 Service Unavailable"));
    assert!(!ConnectorManager::is_permanent_refresh_error("timed out"));
    assert!(!ConnectorManager::is_permanent_refresh_error("connection reset by peer"));
    assert!(!ConnectorManager::is_permanent_refresh_error("Network is unreachable"));
}

#[tokio::test]
async fn test_connector_catalog_and_account_management() {
    let (db, vault, _td) = create_test_db_and_vault();
    let local_mcp = McpManager::new(db.clone(), vault.clone());
    let conn_mgr = ConnectorManager::new(db.clone(), vault.clone(), local_mcp);

    // 1. List catalog
    let catalog = conn_mgr.list_catalog().unwrap();
    assert!(catalog.len() >= 4);
    assert!(catalog.iter().any(|c| c.id == "github"));
    assert!(catalog.iter().any(|c| c.id == "google"));

    // 2. Initial connected accounts empty
    let initial_accounts = conn_mgr.list_connected_accounts().unwrap();
    assert!(initial_accounts.is_empty());

    // 3. Add connected account record
    let acc = ConnectedAccountRecord {
        id: "acc_gh_1".to_string(),
        connector_id: "github".to_string(),
        account_identifier: "octocat".to_string(),
        display_name: "octocat".to_string(),
        status: "connected".to_string(),
        status_message: None,
        granted_scopes_json: "[\"repo\", \"read:user\"]".to_string(),
        credential_key: "account:acc_gh_1:token".to_string(),
        config_json: "{}".to_string(),
        created_at: 1000,
        updated_at: 1000,
        last_connected_at: Some(1000),
        last_error: None,
        expires_at: Some(chrono::Utc::now().timestamp_millis() + 3600_000),
    };
    db.create_connected_account(&acc).unwrap();
    vault.store_account_secret("acc_gh_1", "gho_test_token_12345").unwrap();

    let accounts = conn_mgr.list_connected_accounts().unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].display_name, "octocat");
    assert_eq!(accounts[0].status, "connected");

    // 4. Tools for account
    let tools = conn_mgr.get_tools_for_account("acc_gh_1").await;
    assert!(!tools.is_empty());

    // 5. Update tool policy in DB
    let policy = ConnectorToolPolicyRecord {
        id: "pol_1".to_string(),
        account_id: "acc_gh_1".to_string(),
        tool_name: "github_delete_repository".to_string(),
        policy: "deny".to_string(),
        risk_level: "destructive".to_string(),
        updated_at: 2000,
        schema_fingerprint: None,
    };
    db.set_connector_tool_policy(&policy).unwrap();

    let updated_tools = conn_mgr.get_tools_for_account("acc_gh_1").await;
    assert!(!updated_tools.is_empty());
    let tool_names: Vec<_> = updated_tools.iter().map(|t| t.original_name.clone()).collect();
    assert!(tool_names.iter().any(|n| n.contains("repository") || n.contains("user")));

    // 6. Disconnect account
    conn_mgr.disconnect_account("acc_gh_1").await.unwrap();

    // Verify record was deleted from DB and secret was cleared
    assert!(db.get_connected_account("acc_gh_1").unwrap().is_none());
    let secret = vault.get_account_secret("acc_gh_1").unwrap();
    assert!(secret.is_none(), "Token secret must be deleted on disconnect");
}

#[tokio::test]
async fn test_connector_tool_authorization_evaluation() {
    let (db, vault, _td) = create_test_db_and_vault();
    let local_mcp = McpManager::new(db.clone(), vault.clone());
    let conn_mgr = Arc::new(ConnectorManager::new(db.clone(), vault.clone(), local_mcp));

    // Case 1: Unregistered tool returns None (falls back to core policy)
    let dec = conn_mgr
        .evaluate_tool_authorization("unknown_tool", &json!({}))
        .await;
    assert!(dec.is_none());

    // Case 2: Register connected account and discover tools
    let acc = ConnectedAccountRecord {
        id: "acc_conn_1".to_string(),
        connector_id: "github".to_string(),
        account_identifier: "dev_user".to_string(),
        display_name: "dev_user".to_string(),
        status: "connected".to_string(),
        status_message: None,
        granted_scopes_json: "[]".to_string(),
        credential_key: "account:acc_conn_1:token".to_string(),
        config_json: "{}".to_string(),
        created_at: 1000,
        updated_at: 1000,
        last_connected_at: None,
        last_error: None,
        expires_at: None,
    };
    db.create_connected_account(&acc).unwrap();

    // Discover tools to populate internal tool registry
    let tools = conn_mgr.get_all_connector_tools().await;
    assert!(!tools.is_empty());

    let target_tool = tools.iter().find(|t| t.name.contains("acc_conn_1__")).unwrap();
    let scoped_name = &target_tool.name;

    // Default policy for github read tool is allow
    let dec = conn_mgr
        .evaluate_tool_authorization(scoped_name, &json!({}))
        .await;
    assert!(dec.is_some());

    // Case 3: Set explicit policy to deny
    let raw_tool_name = scoped_name.strip_prefix("acc_conn_1__").unwrap();
    let pol_deny = ConnectorToolPolicyRecord {
        id: "pol_deny".to_string(),
        account_id: "acc_conn_1".to_string(),
        tool_name: raw_tool_name.to_string(),
        policy: "deny".to_string(),
        risk_level: "safe".to_string(),
        updated_at: 1000,
        schema_fingerprint: None,
    };
    db.set_connector_tool_policy(&pol_deny).unwrap();

    let dec = conn_mgr
        .evaluate_tool_authorization(scoped_name, &json!({}))
        .await
        .unwrap();
    assert!(matches!(dec, AuthDecision::Deny { .. }));

    // Case 4: Policy set to ask requires explicit approval
    let pol_ask = ConnectorToolPolicyRecord {
        id: "pol_ask".to_string(),
        account_id: "acc_conn_1".to_string(),
        tool_name: raw_tool_name.to_string(),
        policy: "ask".to_string(),
        risk_level: "moderate".to_string(),
        updated_at: 2000,
        schema_fingerprint: None,
    };
    db.set_connector_tool_policy(&pol_ask).unwrap();

    let dec = conn_mgr
        .evaluate_tool_authorization(scoped_name, &json!({}))
        .await
        .unwrap();
    assert!(matches!(dec, AuthDecision::RequireApproval { .. }));
}

#[tokio::test]
async fn test_oauth_manager_pkce_and_session_state_machine() {
    let oauth = OAuthManager::new();

    // 1. Generate PKCE
    let (verifier, challenge, state) = OAuthManager::generate_pkce();
    assert!(!verifier.is_empty());
    assert!(!challenge.is_empty());
    assert!(!state.is_empty());

    // 2. Register session
    let now = chrono::Utc::now().timestamp_millis();
    let session = PendingOAuthSession {
        session_id: "sess_100".to_string(),
        provider_id: "github".to_string(),
        client_id: "github_client_id_123".to_string(),
        client_secret: Some("github_secret".to_string()),
        code_verifier: verifier.clone(),
        state: state.clone(),
        redirect_uri: "http://localhost:18492/callback".to_string(),
        token_endpoint: "https://github.com/login/oauth/access_token".to_string(),
        status: kyro_lib::connector::auth::OAuthSessionStatus::Pending,
        created_at: now,
        expires_at: now + 600_000,
    };
    oauth.register_session(session).await;

    let status = oauth.get_session_status("sess_100").await.unwrap();
    assert!(matches!(status, kyro_lib::connector::auth::OAuthSessionStatus::Pending));

    // 3. Record callback code
    oauth.record_callback("sess_100", "auth_code_xyz123".to_string()).await.unwrap();
    let status = oauth.get_session_status("sess_100").await.unwrap();
    assert!(matches!(status, kyro_lib::connector::auth::OAuthSessionStatus::CallbackReceived { .. }));

    // 4. Claim and finish exchange
    let claimed = oauth.claim_exchange("sess_100").await.unwrap();
    assert_eq!(claimed.3, "auth_code_xyz123");

    let token_res = TokenExchangeResult {
        access_token: "gho_access_token_ok".to_string(),
        refresh_token: None,
        token_type: "bearer".to_string(),
        granted_scopes: vec!["repo".to_string()],
        expires_in_seconds: Some(3600),
    };

    oauth.finish_exchange("sess_100", Ok(token_res)).await;
    let status = oauth.get_session_status("sess_100").await.unwrap();
    assert!(matches!(status, kyro_lib::connector::auth::OAuthSessionStatus::Completed { .. }));

    // 5. Take completed session
    let (provider, completed_token) = oauth.take_completed_session("sess_100").await.unwrap();
    assert_eq!(provider, "github");
    assert_eq!(completed_token.access_token, "gho_access_token_ok");
    assert!(oauth.get_session_status("sess_100").await.is_none());

    // 6. Test cancel session
    let session2 = PendingOAuthSession {
        session_id: "sess_200".to_string(),
        provider_id: "google".to_string(),
        client_id: "google_client_id".to_string(),
        client_secret: None,
        code_verifier: "verifier_200".to_string(),
        state: "state_200".to_string(),
        redirect_uri: "http://localhost:18492/callback".to_string(),
        token_endpoint: "https://oauth2.googleapis.com/token".to_string(),
        status: kyro_lib::connector::auth::OAuthSessionStatus::Pending,
        created_at: now,
        expires_at: now + 600_000,
    };
    oauth.register_session(session2).await;
    oauth.cancel_session("sess_200").await;
    let status2 = oauth.get_session_status("sess_200").await.unwrap();
    assert!(matches!(status2, kyro_lib::connector::auth::OAuthSessionStatus::Cancelled));

    // 7. HTML escape
    let safe = html_escape("<script>alert('xss & attack');</script> \"quotes\"");
    assert!(!safe.contains('<'));
    assert!(!safe.contains('>'));
    assert!(safe.contains("&lt;script&gt;"));
    assert!(safe.contains("&quot;quotes&quot;"));
}

#[test]
fn test_remote_mcp_validated_endpoint_ssrf_protection() {
    // 1. Private IPv4 ranges rejected when allow_private is false
    assert!(ValidatedRemoteEndpoint::parse_and_validate("http://10.0.0.1:3000/mcp", false, false, false).is_err());
    assert!(ValidatedRemoteEndpoint::parse_and_validate("http://192.168.1.100/mcp", false, false, false).is_err());
    assert!(ValidatedRemoteEndpoint::parse_and_validate("http://172.16.0.1/mcp", false, false, false).is_err());

    // 2. Link-local cloud metadata rejected unconditionally
    assert!(ValidatedRemoteEndpoint::parse_and_validate("http://169.254.169.254/latest/meta-data", false, false, false).is_err());

    // 3. Plain HTTP rejected for public remote hosts
    assert!(ValidatedRemoteEndpoint::parse_and_validate("http://api.github.com/mcp", false, false, false).is_err());

    // 4. Invalid protocols rejected
    assert!(ValidatedRemoteEndpoint::parse_and_validate("ftp://example.com/mcp", false, false, false).is_err());
    assert!(ValidatedRemoteEndpoint::parse_and_validate("file:///etc/passwd", false, false, false).is_err());

    // 5. Public HTTPS endpoint allowed
    let ep = ValidatedRemoteEndpoint::parse_and_validate("https://api.github.com/mcp", false, false, false).unwrap();
    assert_eq!(ep.url(), "https://api.github.com/mcp");
    assert!(!ep.is_loopback());
    assert!(!ep.is_private_network());

    // 6. Loopback allowed for local sidecars
    let local_ep = ValidatedRemoteEndpoint::parse_and_validate("http://127.0.0.1:9090/mcp", false, false, false).unwrap();
    assert!(local_ep.is_loopback());
}

#[tokio::test]
async fn test_remote_mcp_client_rpc_protocol() {
    // Start mock MCP server
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(conn) => conn,
                Err(_) => break,
            };

            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let mut total_read = 0;
                loop {
                    let n = socket.read(&mut buf[total_read..]).await.unwrap_or(0);
                    if n == 0 { break; }
                    total_read += n;
                    if buf[..total_read].windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }

                let req_text = String::from_utf8_lossy(&buf[..total_read]);
                let body_start = req_text.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
                let body = &req_text[body_start..];
                let rpc_req: serde_json::Value = serde_json::from_str(body).unwrap_or(json!({}));
                let method = rpc_req["method"].as_str().unwrap_or_default();
                let id = &rpc_req["id"];

                let rpc_resp = match method {
                    "tools/list" => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "tools": [{
                                "name": "mock_calculator",
                                "description": "Performs addition",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "a": { "type": "number" },
                                        "b": { "type": "number" }
                                    },
                                    "required": ["a", "b"]
                                }
                            }]
                        }
                    }),
                    "tools/call" => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{
                                "type": "text",
                                "text": "Calculation result: 42"
                            }]
                        }
                    }),
                    _ => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32601, "message": "Method not found" }
                    }),
                };

                let resp_body = rpc_resp.to_string();
                let http_resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    resp_body.len(),
                    resp_body
                );
                let _ = socket.write_all(http_resp.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });

    let endpoint_url = format!("http://127.0.0.1:{}/mcp", addr.port());
    let client = RemoteMcpClient::new_with_trust(
        &endpoint_url,
        "mock_server",
        Some("test_secret_token".to_string()),
        true,
        true,
    )
    .unwrap();

    // 1. Connect or verify
    let verify_res = client.connect_or_verify().await;
    assert!(verify_res.is_ok(), "connect_or_verify failed: {:?}", verify_res);

    // 2. List tools
    let tools = client.list_tools().await.unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "mock_calculator");

    // 3. Call tool
    let schema = json!({
        "type": "object",
        "properties": {
            "a": {"type": "number"},
            "b": {"type": "number"}
        }
    });
    let call_res = client
        .call_tool("mock_calculator", &schema, json!({"a": 20, "b": 22}))
        .await;
    assert!(call_res.is_ok());
    assert_eq!(call_res.unwrap(), "Calculation result: 42");
}

#[test]
fn test_mcp_json_schema_validation() {
    let schema = json!({
        "type": "object",
        "properties": {
            "query": { "type": "string" },
            "limit": { "type": "integer" }
        },
        "required": ["query"]
    });

    // Valid arguments
    let valid_args = json!({
        "query": "SELECT * FROM users",
        "limit": 10
    });
    assert!(validate_schema(&schema, &valid_args).is_ok());

    // Missing required field
    let missing_req = json!({
        "limit": 5
    });
    let err = validate_schema(&schema, &missing_req);
    assert!(err.is_err());
    assert!(err.unwrap_err().contains("required"));

    // Type mismatch: limit is string instead of integer
    let wrong_type = json!({
        "query": "SELECT 1",
        "limit": "not_a_number"
    });
    let err = validate_schema(&schema, &wrong_type);
    assert!(err.is_err());
    assert!(err.unwrap_err().contains("integer"));
}
