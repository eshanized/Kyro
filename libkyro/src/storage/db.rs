use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub model: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub tool_calls_json: Option<String>,
    pub observations_json: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub steps_json: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineRecord {
    pub id: String,
    pub name: String,
    pub cron_expression: String,
    pub action_type: String, // "skill" or "prompt"
    pub target_id: String,
    pub enabled: bool,
    pub last_run: Option<i64>,
    pub next_run: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizedFolderRecord {
    pub id: String,
    pub path: String,
    pub display_name: String,
    pub added_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentIndexRecord {
    pub path: String,
    pub file_name: String,
    pub format: String,
    pub size_bytes: u64,
    pub modified_at: i64,
    pub sha256: String,
    pub extracted_text: Option<String>,
    pub indexed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineRunRecord {
    pub id: String,
    pub routine_id: String,
    pub routine_name: String,
    pub status: String, // "running", "succeeded", "failed", "cancelled"
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub summary: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTransactionRecord {
    pub transaction_id: String,
    pub target_directory: String,
    pub status: String, // "completed", "partially_failed", "undone"
    pub created_at: i64,
    pub payload_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotRecord {
    pub id: String,
    pub name: String,
    pub avatar: String,
    pub role: String,
    pub system_prompt: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerRecord {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args_json: String,
    pub env_json: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RunStatus {
    Queued,
    Running,
    WaitingApproval,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

impl RunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RunStatus::Queued => "queued",
            RunStatus::Running => "running",
            RunStatus::WaitingApproval => "waiting_approval",
            RunStatus::Succeeded => "succeeded",
            RunStatus::Failed => "failed",
            RunStatus::Cancelled => "cancelled",
            RunStatus::Interrupted => "interrupted",
        }
    }
}

impl std::str::FromStr for RunStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        std::result::Result::Ok(match s {
            "queued" => RunStatus::Queued,
            "running" => RunStatus::Running,
            "waiting_approval" => RunStatus::WaitingApproval,
            "succeeded" => RunStatus::Succeeded,
            "failed" => RunStatus::Failed,
            "cancelled" => RunStatus::Cancelled,
            _ => RunStatus::Interrupted,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRun {
    pub id: String,
    pub conversation_id: Option<String>,
    pub agent_id: String,
    pub model: String,
    pub prompt: String,
    pub status: String,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub error: Option<String>,
    pub config_json: Option<String>,
    pub parent_run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEvent {
    pub id: String,
    pub run_id: String,
    pub seq: i64,
    pub event_type: String,
    pub payload_json: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub run_id: String,
    pub tool_name: String,
    pub summary: String,
    pub args_json: String,
    pub risk_level: String,
    pub reason: String,
    pub status: String, // "pending", "approved", "denied", "expired"
    pub created_at: i64,
    pub expires_at: i64,
    pub resolved_at: Option<i64>,
    pub resolved_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub scope: String, // "preference", "agent", "project", "summary", "fact"
    pub owner_id: String,
    pub key: String,
    pub value: String,
    pub source: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineStepRecord {
    pub id: String,
    pub run_id: Option<String>,
    pub step: usize,
    pub action_name: String,
    pub click_x: Option<i32>,
    pub click_y: Option<i32>,
    pub before_base64: String,
    pub after_base64: String,
    pub diff_percent: f32,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultRecord {
    pub key: String,
    pub encrypted_data: String,
    pub nonce: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub id: String,
    pub run_id: Option<String>,
    pub conversation_id: Option<String>,
    pub name: String,
    pub file_path: String,
    pub artifact_type: String, // "code", "markdown", "image", "json", "log", "document"
    pub mime_type: String,
    pub size_bytes: i64,
    pub created_at: i64,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub category: String, // "conversation", "message", "memory", "skill", "routine", "artifact"
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub score: f32,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorDefinitionRecord {
    pub id: String,
    pub name: String,
    pub connector_type: String, // "native", "remote_mcp", "local_mcp"
    pub description: String,
    pub icon: String,
    pub auth_type: String, // "pat_or_oauth", "oauth_browser", "pat_token", "api_key", "bearer_token", "none"
    pub capabilities_json: String,
    pub required_scopes_json: String,
    pub is_builtin: bool,
    pub is_available: bool,
    pub doc_url: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedAccountRecord {
    pub id: String,
    pub connector_id: String,
    pub account_identifier: String,
    pub display_name: String,
    pub status: String, // "connected", "needs_auth", "needs_reauth", "disconnected", "error", "disabled"
    pub status_message: Option<String>,
    pub granted_scopes_json: String,
    pub credential_key: String, // Backend vault key reference
    pub config_json: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_connected_at: Option<i64>,
    pub last_error: Option<String>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorToolPolicyRecord {
    pub id: String,
    pub account_id: String,
    pub tool_name: String,
    pub policy: String,     // "allow", "ask", "deny"
    pub risk_level: String, // "safe", "moderate", "destructive"
    pub updated_at: i64,
    #[serde(default)]
    pub schema_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HabitSettingsRecord {
    pub key: String,
    pub enabled: bool,
    pub min_occurrences: usize,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HabitSuggestionRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub signature_hash: String,
    pub occurrences: usize,
    pub confidence: f32,
    pub proposed_action_type: String, // "skill" | "prompt"
    pub proposed_payload_json: String,
    pub status: String, // "pending" | "accepted" | "dismissed" | "snoozed"
    pub evidence_json: String,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunkRecord {
    pub id: String,
    pub document_path: String,
    pub chunk_index: usize,
    pub heading: Option<String>,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub content: String,
    pub content_hash: String,
    pub format: String,
    pub chunk_version: u32,
    pub indexed_at: i64,
    pub embedding_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagSettingsRecord {
    pub key: String,
    pub embedding_mode: String, // "lexical_only" | "local_endpoint"
    pub embedding_endpoint: Option<String>,
    pub embedding_model: Option<String>,
    pub max_candidates: usize,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeExperimentRecord {
    pub id: String,
    pub name: String,
    pub run_id: Option<String>,
    pub runtime: String,
    pub image: String,
    pub script: Option<String>,
    pub argv_json: String,
    pub workspace_path: Option<String>,
    pub output_dir: Option<String>,
    pub network_mode: String,
    pub cpu_limit: f32,
    pub memory_limit_mb: u64,
    pub timeout_seconds: u64,
    pub status: String,
    pub exit_code: Option<i32>,
    pub stdout_tail: Option<String>,
    pub stderr_tail: Option<String>,
    pub result_summary: Option<String>,
    pub artifact_ids_json: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn init(app_data_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&app_data_dir).ok();
        let db_path = app_data_dir.join("kyro.db");
        let conn = Connection::open(db_path)?;

        // Configure PRAGMAs for concurrency & resilience
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA synchronous = NORMAL;",
        )?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        db.run_migrations()?;
        db.seed_default_bots().ok();
        db.seed_default_connectors().ok();

        // Perform safe startup reconciliation
        db.reconcile_active_runs_on_startup().ok();

        Ok(db)
    }

    fn run_migrations(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Create migration ledger
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at INTEGER NOT NULL
            );",
            [],
        )?;

        // Migration 1: Core MVP tables
        let mut stmt = conn.prepare("SELECT 1 FROM schema_migrations WHERE version = 1")?;
        let m1_exists = stmt.exists([])?;
        drop(stmt);

        if !m1_exists {
            conn.execute_batch(
                "BEGIN TRANSACTION;
                CREATE TABLE IF NOT EXISTS conversations (
                    id TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    model TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS messages (
                    id TEXT PRIMARY KEY,
                    conversation_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    content TEXT NOT NULL,
                    tool_calls_json TEXT,
                    observations_json TEXT,
                    created_at INTEGER NOT NULL,
                    FOREIGN KEY(conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS skills (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    description TEXT,
                    steps_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS routines (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    cron_expression TEXT NOT NULL,
                    action_type TEXT NOT NULL,
                    target_id TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    last_run INTEGER
                );

                CREATE TABLE IF NOT EXISTS bots (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    avatar TEXT NOT NULL,
                    role TEXT NOT NULL,
                    system_prompt TEXT NOT NULL,
                    is_default INTEGER NOT NULL DEFAULT 0
                );

                CREATE TABLE IF NOT EXISTS mcp_servers (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    command TEXT NOT NULL,
                    args_json TEXT NOT NULL,
                    env_json TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1
                );

                INSERT INTO schema_migrations (version, name, applied_at)
                VALUES (1, 'initial_mvp_schema', strftime('%s', 'now') * 1000);
                COMMIT;",
            )?;
        }

        // Migration 2: Durable runs, events, approvals, timeline, memories, vault
        let mut stmt2 = conn.prepare("SELECT 1 FROM schema_migrations WHERE version = 2")?;
        let m2_exists = stmt2.exists([])?;
        drop(stmt2);

        if !m2_exists {
            conn.execute_batch(
                "BEGIN TRANSACTION;
                CREATE TABLE IF NOT EXISTS agent_runs (
                    id TEXT PRIMARY KEY,
                    conversation_id TEXT,
                    agent_id TEXT NOT NULL,
                    model TEXT NOT NULL,
                    prompt TEXT NOT NULL,
                    status TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    started_at INTEGER,
                    completed_at INTEGER,
                    error TEXT,
                    config_json TEXT,
                    parent_run_id TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_agent_runs_status ON agent_runs(status);
                CREATE INDEX IF NOT EXISTS idx_agent_runs_conv ON agent_runs(conversation_id);

                CREATE TABLE IF NOT EXISTS run_events (
                    id TEXT PRIMARY KEY,
                    run_id TEXT NOT NULL,
                    seq INTEGER NOT NULL,
                    event_type TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    FOREIGN KEY(run_id) REFERENCES agent_runs(id) ON DELETE CASCADE
                );
                CREATE UNIQUE INDEX IF NOT EXISTS idx_run_events_run_seq ON run_events(run_id, seq);

                CREATE TABLE IF NOT EXISTS approval_requests (
                    id TEXT PRIMARY KEY,
                    run_id TEXT NOT NULL,
                    tool_name TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    args_json TEXT NOT NULL,
                    risk_level TEXT NOT NULL,
                    reason TEXT NOT NULL,
                    status TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    expires_at INTEGER NOT NULL,
                    resolved_at INTEGER,
                    resolved_by TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_approvals_status ON approval_requests(status);
                CREATE INDEX IF NOT EXISTS idx_approvals_run ON approval_requests(run_id);

                CREATE TABLE IF NOT EXISTS memories (
                    id TEXT PRIMARY KEY,
                    scope TEXT NOT NULL,
                    owner_id TEXT NOT NULL,
                    key TEXT NOT NULL,
                    value TEXT NOT NULL,
                    source TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_memories_scope_owner ON memories(scope, owner_id);

                CREATE TABLE IF NOT EXISTS timeline_steps (
                    id TEXT PRIMARY KEY,
                    run_id TEXT,
                    step INTEGER NOT NULL,
                    action_name TEXT NOT NULL,
                    click_x INTEGER,
                    click_y INTEGER,
                    before_base64 TEXT NOT NULL,
                    after_base64 TEXT NOT NULL,
                    diff_percent REAL NOT NULL,
                    timestamp INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_timeline_run ON timeline_steps(run_id);

                CREATE TABLE IF NOT EXISTS vault_records (
                    key TEXT PRIMARY KEY,
                    encrypted_data TEXT NOT NULL,
                    nonce TEXT NOT NULL,
                    updated_at INTEGER NOT NULL
                );

                INSERT INTO schema_migrations (version, name, applied_at)
                VALUES (2, 'durable_runs_and_governance', strftime('%s', 'now') * 1000);
                COMMIT;",
            )?;
        }

        // Migration 3: Add next_run to routines if missing
        let mut stmt3 = conn.prepare("SELECT 1 FROM schema_migrations WHERE version = 3")?;
        let m3_exists = stmt3.exists([])?;
        drop(stmt3);

        if !m3_exists {
            // Check if column next_run exists
            let has_col = {
                let mut col_check = conn.prepare("PRAGMA table_info(routines)")?;
                let mut rows = col_check.query([])?;
                let mut found = false;
                while let Some(r) = rows.next()? {
                    let col_name: String = r.get(1)?;
                    if col_name == "next_run" {
                        found = true;
                        break;
                    }
                }
                found
            };

            if !has_col {
                conn.execute("ALTER TABLE routines ADD COLUMN next_run INTEGER;", [])?;
            }

            conn.execute(
                "INSERT INTO schema_migrations (version, name, applied_at)
                 VALUES (3, 'routines_next_run_column', strftime('%s', 'now') * 1000);",
                [],
            )?;
        }

        // Migration 4: Artifacts registry
        let mut stmt4 = conn.prepare("SELECT 1 FROM schema_migrations WHERE version = 4")?;
        let m4_exists = stmt4.exists([])?;
        drop(stmt4);

        if !m4_exists {
            conn.execute_batch(
                "BEGIN TRANSACTION;
                CREATE TABLE IF NOT EXISTS artifacts (
                    id TEXT PRIMARY KEY,
                    run_id TEXT,
                    conversation_id TEXT,
                    name TEXT NOT NULL,
                    file_path TEXT NOT NULL,
                    artifact_type TEXT NOT NULL,
                    mime_type TEXT NOT NULL,
                    size_bytes INTEGER NOT NULL,
                    created_at INTEGER NOT NULL,
                    metadata_json TEXT,
                    FOREIGN KEY(run_id) REFERENCES agent_runs(id) ON DELETE SET NULL,
                    FOREIGN KEY(conversation_id) REFERENCES conversations(id) ON DELETE SET NULL
                );
                CREATE INDEX IF NOT EXISTS idx_artifacts_run ON artifacts(run_id);
                CREATE INDEX IF NOT EXISTS idx_artifacts_conv ON artifacts(conversation_id);

                INSERT INTO schema_migrations (version, name, applied_at)
                VALUES (4, 'artifacts_registry', strftime('%s', 'now') * 1000);
                COMMIT;",
            )?;
        }

        // Migration 5: Connector runtime, definitions, connected accounts, and tool policies
        let mut stmt5 = conn.prepare("SELECT 1 FROM schema_migrations WHERE version = 5")?;
        let m5_exists = stmt5.exists([])?;
        drop(stmt5);

        if !m5_exists {
            conn.execute_batch(
                "BEGIN TRANSACTION;
                CREATE TABLE IF NOT EXISTS connector_definitions (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    connector_type TEXT NOT NULL,
                    description TEXT NOT NULL,
                    icon TEXT NOT NULL,
                    auth_type TEXT NOT NULL,
                    capabilities_json TEXT NOT NULL,
                    required_scopes_json TEXT NOT NULL,
                    is_builtin INTEGER NOT NULL DEFAULT 1,
                    is_available INTEGER NOT NULL DEFAULT 1,
                    doc_url TEXT,
                    created_at INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS connected_accounts (
                    id TEXT PRIMARY KEY,
                    connector_id TEXT NOT NULL,
                    account_identifier TEXT NOT NULL,
                    display_name TEXT NOT NULL,
                    status TEXT NOT NULL,
                    status_message TEXT,
                    granted_scopes_json TEXT NOT NULL,
                    credential_key TEXT NOT NULL,
                    config_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    last_connected_at INTEGER,
                    last_error TEXT,
                    expires_at INTEGER,
                    FOREIGN KEY(connector_id) REFERENCES connector_definitions(id) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_conn_accounts_connector ON connected_accounts(connector_id);
                CREATE INDEX IF NOT EXISTS idx_conn_accounts_status ON connected_accounts(status);

                CREATE TABLE IF NOT EXISTS connector_tool_policies (
                    id TEXT PRIMARY KEY,
                    account_id TEXT NOT NULL,
                    tool_name TEXT NOT NULL,
                    policy TEXT NOT NULL,
                    risk_level TEXT NOT NULL,
                    updated_at INTEGER NOT NULL,
                    FOREIGN KEY(account_id) REFERENCES connected_accounts(id) ON DELETE CASCADE,
                    UNIQUE(account_id, tool_name)
                );
                CREATE INDEX IF NOT EXISTS idx_tool_policies_account ON connector_tool_policies(account_id);

                INSERT INTO schema_migrations (version, name, applied_at)
                VALUES (5, 'connector_runtime_and_connected_accounts', strftime('%s', 'now') * 1000);
                COMMIT;",
            )?;
        }

        // Migration 6: Add schema_fingerprint to connector_tool_policies
        let mut stmt6 = conn.prepare("SELECT 1 FROM schema_migrations WHERE version = 6")?;
        let m6_exists = stmt6.exists([])?;
        drop(stmt6);

        if !m6_exists {
            conn.execute_batch(
                "BEGIN TRANSACTION;
                ALTER TABLE connector_tool_policies ADD COLUMN schema_fingerprint TEXT;
                INSERT INTO schema_migrations (version, name, applied_at)
                VALUES (6, 'connector_tool_policies_schema_fingerprint', strftime('%s', 'now') * 1000);
                COMMIT;",
            )?;
        }

        // Migration 7: Add index on routines(target_id) for task deletion referential integrity
        let mut stmt7 = conn.prepare("SELECT 1 FROM schema_migrations WHERE version = 7")?;
        let m7_exists = stmt7.exists([])?;
        drop(stmt7);

        if !m7_exists {
            conn.execute_batch(
                "BEGIN TRANSACTION;
                CREATE INDEX IF NOT EXISTS idx_routines_target ON routines(target_id);
                INSERT INTO schema_migrations (version, name, applied_at)
                VALUES (7, 'routines_target_id_index', strftime('%s', 'now') * 1000);
                COMMIT;",
            )?;
        }

        // Migration 8: Authorized folders, routine runs, file transactions
        let mut stmt8 = conn.prepare("SELECT 1 FROM schema_migrations WHERE version = 8")?;
        let m8_exists = stmt8.exists([])?;
        drop(stmt8);

        if !m8_exists {
            conn.execute_batch(
                "BEGIN TRANSACTION;
                CREATE TABLE IF NOT EXISTS authorized_folders (
                    id TEXT PRIMARY KEY,
                    path TEXT NOT NULL UNIQUE,
                    display_name TEXT NOT NULL,
                    added_at INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS routine_runs (
                    id TEXT PRIMARY KEY,
                    routine_id TEXT NOT NULL,
                    routine_name TEXT NOT NULL,
                    status TEXT NOT NULL,
                    started_at INTEGER NOT NULL,
                    completed_at INTEGER,
                    summary TEXT,
                    error TEXT,
                    FOREIGN KEY(routine_id) REFERENCES routines(id) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_routine_runs_routine ON routine_runs(routine_id);

                CREATE TABLE IF NOT EXISTS file_transactions (
                    transaction_id TEXT PRIMARY KEY,
                    target_directory TEXT NOT NULL,
                    status TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    payload_json TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_file_transactions_dir ON file_transactions(target_directory);

                INSERT INTO schema_migrations (version, name, applied_at)
                VALUES (8, 'everyday_assistant_schema', strftime('%s', 'now') * 1000);
                COMMIT;",
            )?;
        }

        // Migration 9: Document search index cache
        let mut stmt9 = conn.prepare("SELECT 1 FROM schema_migrations WHERE version = 9")?;
        let m9_exists = stmt9.exists([])?;
        drop(stmt9);

        if !m9_exists {
            conn.execute_batch(
                "BEGIN TRANSACTION;
                CREATE TABLE IF NOT EXISTS document_index (
                    path TEXT PRIMARY KEY,
                    file_name TEXT NOT NULL,
                    format TEXT NOT NULL,
                    size_bytes INTEGER NOT NULL,
                    modified_at INTEGER NOT NULL,
                    sha256 TEXT NOT NULL,
                    extracted_text TEXT,
                    indexed_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_doc_index_mod ON document_index(modified_at);

                INSERT INTO schema_migrations (version, name, applied_at)
                VALUES (9, 'document_index_schema', strftime('%s', 'now') * 1000);
                COMMIT;",
            )?;
        }

        // Migration 10: Local habit learning, Deep RAG chunking, and Sandboxed Code Experiments
        let mut stmt10 = conn.prepare("SELECT 1 FROM schema_migrations WHERE version = 10")?;
        let m10_exists = stmt10.exists([])?;
        drop(stmt10);

        if !m10_exists {
            conn.execute_batch(
                "BEGIN TRANSACTION;
                CREATE TABLE IF NOT EXISTS habit_learning_settings (
                    key TEXT PRIMARY KEY,
                    enabled INTEGER NOT NULL DEFAULT 0,
                    min_occurrences INTEGER NOT NULL DEFAULT 3,
                    updated_at INTEGER NOT NULL
                );

                INSERT OR IGNORE INTO habit_learning_settings (key, enabled, min_occurrences, updated_at)
                VALUES ('default', 0, 3, strftime('%s', 'now') * 1000);

                CREATE TABLE IF NOT EXISTS habit_suggestions (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    description TEXT NOT NULL,
                    signature_hash TEXT NOT NULL UNIQUE,
                    occurrences INTEGER NOT NULL DEFAULT 1,
                    confidence REAL NOT NULL DEFAULT 0.5,
                    proposed_action_type TEXT NOT NULL,
                    proposed_payload_json TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'pending',
                    evidence_json TEXT NOT NULL,
                    first_seen_at INTEGER NOT NULL,
                    last_seen_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_habit_sugg_status ON habit_suggestions(status);

                CREATE TABLE IF NOT EXISTS document_chunks (
                    id TEXT PRIMARY KEY,
                    document_path TEXT NOT NULL,
                    chunk_index INTEGER NOT NULL,
                    heading TEXT,
                    start_line INTEGER,
                    end_line INTEGER,
                    content TEXT NOT NULL,
                    content_hash TEXT NOT NULL,
                    format TEXT NOT NULL,
                    chunk_version INTEGER NOT NULL DEFAULT 1,
                    indexed_at INTEGER NOT NULL,
                    embedding_json TEXT,
                    FOREIGN KEY(document_path) REFERENCES document_index(path) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_doc_chunks_path ON document_chunks(document_path);
                CREATE INDEX IF NOT EXISTS idx_doc_chunks_hash ON document_chunks(content_hash);

                CREATE TABLE IF NOT EXISTS rag_settings (
                    key TEXT PRIMARY KEY,
                    embedding_mode TEXT NOT NULL DEFAULT 'lexical_only',
                    embedding_endpoint TEXT,
                    embedding_model TEXT,
                    max_candidates INTEGER NOT NULL DEFAULT 5,
                    updated_at INTEGER NOT NULL
                );

                INSERT OR IGNORE INTO rag_settings (key, embedding_mode, max_candidates, updated_at)
                VALUES ('default', 'lexical_only', 5, strftime('%s', 'now') * 1000);

                CREATE TABLE IF NOT EXISTS code_experiments (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    run_id TEXT,
                    runtime TEXT NOT NULL,
                    image TEXT NOT NULL,
                    script TEXT,
                    argv_json TEXT NOT NULL,
                    workspace_path TEXT,
                    output_dir TEXT,
                    network_mode TEXT NOT NULL DEFAULT 'none',
                    cpu_limit REAL NOT NULL DEFAULT 1.0,
                    memory_limit_mb INTEGER NOT NULL DEFAULT 512,
                    timeout_seconds INTEGER NOT NULL DEFAULT 60,
                    status TEXT NOT NULL DEFAULT 'queued',
                    exit_code INTEGER,
                    stdout_tail TEXT,
                    stderr_tail TEXT,
                    result_summary TEXT,
                    artifact_ids_json TEXT,
                    created_at INTEGER NOT NULL,
                    started_at INTEGER,
                    completed_at INTEGER,
                    error TEXT,
                    FOREIGN KEY(run_id) REFERENCES agent_runs(id) ON DELETE SET NULL
                );
                CREATE INDEX IF NOT EXISTS idx_experiments_status ON code_experiments(status);
                CREATE INDEX IF NOT EXISTS idx_experiments_run ON code_experiments(run_id);

                INSERT INTO schema_migrations (version, name, applied_at)
                VALUES (10, 'habit_rag_and_experiments_schema', strftime('%s', 'now') * 1000);
                COMMIT;",
            )?;
        }

        Ok(())
    }

    fn seed_default_bots(&self) -> Result<()> {
        let bots = self.list_bots()?;
        if bots.is_empty() {
            self.create_bot(&BotRecord {
                id: "apex-orchestrator".into(),
                name: "Apex".into(),
                avatar: "zap".into(),
                role: "General Workspace & Strategic Orchestrator".into(),
                system_prompt: "You are Apex, the lead strategic orchestrator of Kyro. You break down complex user objectives, plan multi-step workflows, delegate specialized work, and coordinate desktop actions.".into(),
                is_default: true,
            })?;

            self.create_bot(&BotRecord {
                id: "forge-devops".into(),
                name: "Forge".into(),
                avatar: "wrench".into(),
                role: "DevOps, Code & Terminal Specialist".into(),
                system_prompt: "You are Forge, the code and systems engineer of Kyro. You specialize in shell commands, git workflows, Docker containerization, code edits, and debugging.".into(),
                is_default: false,
            })?;

            self.create_bot(&BotRecord {
                id: "scout-web".into(),
                name: "Scout".into(),
                avatar: "globe".into(),
                role: "Browser Automation & Web Researcher".into(),
                system_prompt: "You are Scout, the web intelligence and browser automation agent of Kyro. You visually navigate websites, fill forms, extract data, and handle web workflows.".into(),
                is_default: false,
            })?;
        }
        Ok(())
    }

    // Startup Reconciliation
    pub fn reconcile_active_runs_on_startup(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        let count = conn.execute(
            "UPDATE agent_runs
             SET status = 'interrupted',
                 completed_at = ?1,
                 error = 'Process terminated unexpectedly during execution'
             WHERE status IN ('running', 'queued', 'waiting_approval')",
            params![now],
        )?;

        // Reconcile interrupted routine_runs so they are never left indefinitely in 'running'
        let routine_count = conn.execute(
            "UPDATE routine_runs
             SET status = 'interrupted',
                 completed_at = ?1,
                 error = 'Process terminated unexpectedly during execution'
             WHERE status = 'running'",
            params![now],
        )?;

        // Reconcile interrupted code_experiments so they are never left in 'running' or 'queued'
        let exp_count = conn
            .execute(
                "UPDATE code_experiments
             SET status = 'interrupted',
                 completed_at = ?1,
                 error = 'Experiment terminated unexpectedly during system restart'
             WHERE status IN ('running', 'queued')",
                params![now],
            )
            .unwrap_or(0);

        // Expire any lingering pending approvals from previous session
        conn.execute(
            "UPDATE approval_requests
             SET status = 'expired',
                 resolved_at = ?1,
                 resolved_by = 'system_restart'
             WHERE status = 'pending'",
            params![now],
        )?;

        Ok(count + routine_count + exp_count)
    }

    // Conversations & Messages
    pub fn create_conversation(&self, id: &str, title: &str, model: &str) -> Result<Conversation> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO conversations (id, title, model, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, title, model, now],
        )?;

        Ok(Conversation {
            id: id.to_string(),
            title: title.to_string(),
            model: model.to_string(),
            created_at: now,
        })
    }

    pub fn list_conversations(&self) -> Result<Vec<Conversation>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, model, created_at FROM conversations ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                model: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn get_conversation(&self, id: &str) -> Result<Option<Conversation>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, title, model, created_at FROM conversations WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                model: row.get(2)?,
                created_at: row.get(3)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn delete_conversation(&self, id: &str) -> Result<bool> {
        let id_trimmed = id.trim();
        if id_trimmed.is_empty() {
            return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other("Invalid session identifier: ID cannot be empty"),
            )));
        }

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        // 1. Verify existence before starting mutation
        {
            let mut check_stmt = tx.prepare("SELECT 1 FROM conversations WHERE id = ?1")?;
            if !check_stmt.exists(params![id_trimmed])? {
                return Ok(false);
            }
        }

        // 2. Reject deletion if conversation has an active run (queued, running, waiting_approval)
        {
            let mut active_stmt = tx.prepare(
                "SELECT COUNT(*) FROM agent_runs 
                 WHERE conversation_id = ?1 
                   AND status IN ('queued', 'running', 'waiting_approval')",
            )?;
            let active_count: i64 = active_stmt.query_row(params![id_trimmed], |row| row.get(0))?;
            if active_count > 0 {
                return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                    std::io::Error::other(format!(
                        "Cannot delete session: conversation '{}' has an active task in progress. The task must finish or be stopped before the session can be deleted.",
                        id_trimmed
                    )),
                )));
            }
        }

        // 3. Atomically perform deletion and association cleanup within the transaction
        // Detach historical agent runs: keep execution history and events, detach conversation link
        tx.execute(
            "UPDATE agent_runs SET conversation_id = NULL WHERE conversation_id = ?1",
            params![id_trimmed],
        )?;

        // Preserve artifacts and files: clear conversation association
        tx.execute(
            "UPDATE artifacts SET conversation_id = NULL WHERE conversation_id = ?1",
            params![id_trimmed],
        )?;

        // Remove conversation messages (reliably removes associated messages)
        tx.execute(
            "DELETE FROM messages WHERE conversation_id = ?1",
            params![id_trimmed],
        )?;

        // Delete conversation definition
        let count = tx.execute(
            "DELETE FROM conversations WHERE id = ?1",
            params![id_trimmed],
        )?;

        tx.commit()?;

        Ok(count > 0)
    }

    pub fn add_message(&self, msg: &Message) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO messages (id, conversation_id, role, content, tool_calls_json, observations_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                msg.id,
                msg.conversation_id,
                msg.role,
                msg.content,
                msg.tool_calls_json,
                msg.observations_json,
                msg.created_at
            ],
        )?;
        Ok(())
    }

    pub fn get_messages(&self, conversation_id: &str) -> Result<Vec<Message>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, role, content, tool_calls_json, observations_json, created_at
             FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![conversation_id], |row| {
            Ok(Message {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                tool_calls_json: row.get(4)?,
                observations_json: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    // Settings (non-sensitive)
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn delete_setting(&self, key: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        Ok(affected > 0)
    }

    pub fn list_settings_keys_with_prefix(&self, prefix: &str) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("{}%", prefix);
        let mut stmt = conn.prepare("SELECT key, value FROM settings WHERE key LIKE ?1")?;
        let rows = stmt.query_map(params![pattern], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    // Vault Records (encrypted fallback)
    pub fn set_vault_record(&self, record: &VaultRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO vault_records (key, encrypted_data, nonce, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(key) DO UPDATE SET encrypted_data = ?2, nonce = ?3, updated_at = ?4",
            params![
                record.key,
                record.encrypted_data,
                record.nonce,
                record.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn get_vault_record(&self, key: &str) -> Result<Option<VaultRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT key, encrypted_data, nonce, updated_at FROM vault_records WHERE key = ?1",
        )?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(VaultRecord {
                key: row.get(0)?,
                encrypted_data: row.get(1)?,
                nonce: row.get(2)?,
                updated_at: row.get(3)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn delete_vault_record(&self, key: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute("DELETE FROM vault_records WHERE key = ?1", params![key])?;
        Ok(affected > 0)
    }

    // Agent Runs
    pub fn create_run(&self, run: &AgentRun) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Lifecycle guard: reject creating an active run for a deleted or non-existent conversation
        if let Some(ref conv_id) = run.conversation_id {
            if run.status == "running" || run.status == "queued" || run.status == "waiting_approval"
            {
                let mut stmt = conn.prepare("SELECT 1 FROM conversations WHERE id = ?1")?;
                if !stmt.exists(params![conv_id])? {
                    return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                        std::io::Error::other(format!(
                            "Cannot create active run: conversation '{}' not found or was deleted",
                            conv_id
                        )),
                    )));
                }
            }
        }

        conn.execute(
            "INSERT INTO agent_runs (id, conversation_id, agent_id, model, prompt, status, created_at, started_at, completed_at, error, config_json, parent_run_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                run.id,
                run.conversation_id,
                run.agent_id,
                run.model,
                run.prompt,
                run.status,
                run.created_at,
                run.started_at,
                run.completed_at,
                run.error,
                run.config_json,
                run.parent_run_id
            ],
        )?;
        Ok(())
    }

    pub fn update_run_status(
        &self,
        id: &str,
        status: &str,
        started_at: Option<i64>,
        completed_at: Option<i64>,
        error: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Lifecycle guard: reject transitioning to active state if parent conversation was deleted
        if status == "running" || status == "queued" || status == "waiting_approval" {
            let mut stmt = conn.prepare("SELECT conversation_id FROM agent_runs WHERE id = ?1")?;
            let mut rows = stmt.query(params![id])?;
            if let Some(row) = rows.next()? {
                let conv_id: Option<String> = row.get(0)?;
                if let Some(ref cid) = conv_id {
                    let mut c_stmt = conn.prepare("SELECT 1 FROM conversations WHERE id = ?1")?;
                    if !c_stmt.exists(params![cid])? {
                        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                            std::io::Error::other(format!(
                                "Cannot transition run to active: conversation '{}' was deleted",
                                cid
                            )),
                        )));
                    }
                }
            }
        }

        conn.execute(
            "UPDATE agent_runs
             SET status = ?1,
                 started_at = COALESCE(?2, started_at),
                 completed_at = COALESCE(?3, completed_at),
                 error = COALESCE(?4, error)
             WHERE id = ?5",
            params![status, started_at, completed_at, error, id],
        )?;
        Ok(())
    }

    pub fn get_run(&self, id: &str) -> Result<Option<AgentRun>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, agent_id, model, prompt, status, created_at, started_at, completed_at, error, config_json, parent_run_id
             FROM agent_runs WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(r) = rows.next()? {
            Ok(Some(AgentRun {
                id: r.get(0)?,
                conversation_id: r.get(1)?,
                agent_id: r.get(2)?,
                model: r.get(3)?,
                prompt: r.get(4)?,
                status: r.get(5)?,
                created_at: r.get(6)?,
                started_at: r.get(7)?,
                completed_at: r.get(8)?,
                error: r.get(9)?,
                config_json: r.get(10)?,
                parent_run_id: r.get(11)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Number of runs whose `parent_run_id` is `parent_id`, regardless of
    /// status. Delegation fan-out budgets count every child a run has ever
    /// created (concurrent or sequential), which keeps the accounting durable
    /// across restarts.
    pub fn count_child_runs(&self, parent_id: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM agent_runs WHERE parent_run_id = ?1")?;
        stmt.query_row(params![parent_id], |r| r.get(0))
    }

    /// Length of the delegation chain above `run_id`: 0 for a top-level run,
    /// 1 when the run itself was delegated once, and so on. Walks the durable
    /// `agent_runs` linkage rather than parsing run id strings.
    pub fn delegation_depth(&self, run_id: &str) -> usize {
        let mut depth = 0usize;
        let mut cursor = Some(run_id.to_string());
        while depth < 64 {
            let parent = cursor
                .as_deref()
                .and_then(|id| self.get_run(id).ok().flatten())
                .and_then(|r| r.parent_run_id);
            match parent {
                None => break,
                Some(p) => {
                    depth += 1;
                    cursor = Some(p);
                }
            }
        }
        depth
    }

    pub fn list_runs(&self, limit: usize) -> Result<Vec<AgentRun>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, agent_id, model, prompt, status, created_at, started_at, completed_at, error, config_json, parent_run_id
             FROM agent_runs ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok(AgentRun {
                id: r.get(0)?,
                conversation_id: r.get(1)?,
                agent_id: r.get(2)?,
                model: r.get(3)?,
                prompt: r.get(4)?,
                status: r.get(5)?,
                created_at: r.get(6)?,
                started_at: r.get(7)?,
                completed_at: r.get(8)?,
                error: r.get(9)?,
                config_json: r.get(10)?,
                parent_run_id: r.get(11)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    // Run Events (Ordered Event Timeline)
    pub fn append_run_event(&self, event: &RunEvent) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let mut attempts = 0;
        let mut seq = event.seq;

        loop {
            let tx = conn.transaction()?;
            if seq <= 0 {
                seq = tx.query_row(
                    "SELECT COALESCE(MAX(seq), 0) + 1 FROM run_events WHERE run_id = ?1",
                    params![event.run_id],
                    |r| r.get(0),
                )?;
            }

            let event_id = if event.id.is_empty()
                || event.id == format!("evt_{}_{}", event.run_id, event.seq)
            {
                format!("evt_{}_{}", event.run_id, seq)
            } else {
                event.id.clone()
            };

            let res = tx.execute(
                "INSERT INTO run_events (id, run_id, seq, event_type, payload_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    event_id,
                    event.run_id,
                    seq,
                    event.event_type,
                    event.payload_json,
                    event.created_at
                ],
            );

            match res {
                Ok(_) => {
                    tx.commit()?;
                    return Ok(());
                }
                Err(rusqlite::Error::SqliteFailure(err, _))
                    if err.code == rusqlite::ffi::ErrorCode::ConstraintViolation
                        && attempts < 5 =>
                {
                    drop(tx);
                    attempts += 1;
                    seq = 0;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Atomically allocates the next sequence and appends a run event within a single transaction
    pub fn append_new_run_event(
        &self,
        run_id: &str,
        event_type: &str,
        payload_json: &str,
    ) -> Result<RunEvent> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM run_events WHERE run_id = ?1",
            params![run_id],
            |r| r.get(0),
        )?;
        let now = chrono::Utc::now().timestamp_millis();
        let event = RunEvent {
            id: format!("evt_{}_{}", run_id, seq),
            run_id: run_id.to_string(),
            seq,
            event_type: event_type.to_string(),
            payload_json: payload_json.to_string(),
            created_at: now,
        };

        tx.execute(
            "INSERT INTO run_events (id, run_id, seq, event_type, payload_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event.id,
                event.run_id,
                event.seq,
                event.event_type,
                event.payload_json,
                event.created_at
            ],
        )?;
        tx.commit()?;
        Ok(event)
    }

    pub fn get_run_events(&self, run_id: &str, after_seq: Option<i64>) -> Result<Vec<RunEvent>> {
        let conn = self.conn.lock().unwrap();
        let min_seq = after_seq.unwrap_or(0);
        let mut stmt = conn.prepare(
            "SELECT id, run_id, seq, event_type, payload_json, created_at
             FROM run_events WHERE run_id = ?1 AND seq > ?2 ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map(params![run_id, min_seq], |r| {
            Ok(RunEvent {
                id: r.get(0)?,
                run_id: r.get(1)?,
                seq: r.get(2)?,
                event_type: r.get(3)?,
                payload_json: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn get_next_event_seq(&self, run_id: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT COALESCE(MAX(seq), 0) + 1 FROM run_events WHERE run_id = ?1")?;
        let seq: i64 = stmt.query_row(params![run_id], |r| r.get(0))?;
        Ok(seq)
    }

    // Approval Requests
    pub fn create_approval_request(&self, req: &ApprovalRequest) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO approval_requests (id, run_id, tool_name, summary, args_json, risk_level, reason, status, created_at, expires_at, resolved_at, resolved_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                req.id,
                req.run_id,
                req.tool_name,
                req.summary,
                req.args_json,
                req.risk_level,
                req.reason,
                req.status,
                req.created_at,
                req.expires_at,
                req.resolved_at,
                req.resolved_by
            ],
        )?;
        Ok(())
    }

    pub fn expire_overdue_approvals(&self, now: i64) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute(
            "UPDATE approval_requests
             SET status = 'expired',
                 resolved_at = ?1,
                 resolved_by = 'system_expiry'
             WHERE status = 'pending' AND expires_at <= ?1",
            params![now],
        )?;
        Ok(rows)
    }

    pub fn get_approval_request(&self, id: &str) -> Result<Option<ApprovalRequest>> {
        let now = chrono::Utc::now().timestamp_millis();
        let _ = self.expire_overdue_approvals(now);

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, run_id, tool_name, summary, args_json, risk_level, reason, status, created_at, expires_at, resolved_at, resolved_by
             FROM approval_requests WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(r) = rows.next()? {
            Ok(Some(ApprovalRequest {
                id: r.get(0)?,
                run_id: r.get(1)?,
                tool_name: r.get(2)?,
                summary: r.get(3)?,
                args_json: r.get(4)?,
                risk_level: r.get(5)?,
                reason: r.get(6)?,
                status: r.get(7)?,
                created_at: r.get(8)?,
                expires_at: r.get(9)?,
                resolved_at: r.get(10)?,
                resolved_by: r.get(11)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn resolve_approval_request(
        &self,
        id: &str,
        decision: &str, // "approved", "denied", "cancelled"
        resolved_by: &str,
    ) -> Result<bool> {
        let now = chrono::Utc::now().timestamp_millis();
        let _ = self.expire_overdue_approvals(now);

        let conn = self.conn.lock().unwrap();
        let rows_affected = conn.execute(
            "UPDATE approval_requests
             SET status = ?1,
                 resolved_at = ?2,
                 resolved_by = ?3
             WHERE id = ?4 AND status = 'pending' AND expires_at > ?2",
            params![decision, now, resolved_by, id],
        )?;
        Ok(rows_affected > 0)
    }

    pub fn consume_approval_request(&self, id: &str, run_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let rows_affected = conn.execute(
            "UPDATE approval_requests
             SET status = 'consumed'
             WHERE id = ?1 AND status = 'approved' AND run_id = ?2",
            params![id, run_id],
        )?;
        Ok(rows_affected > 0)
    }

    /// Atomically validates and consumes an approved approval request within a database transaction.
    /// Strictly verifies:
    /// 1. Request exists and is currently in 'approved' status
    /// 2. Request has not already been consumed (rejects replays)
    /// 3. Request has not expired
    /// 4. Run ID matches exactly
    /// 5. Tool name matches expected scoped name or original tool name
    /// 6. Arguments match exact approved arguments using canonical serialization
    pub fn consume_approval_request_bound(
        &self,
        id: &str,
        run_id: &str,
        expected_tool_names: &[&str],
        expected_args: &serde_json::Value,
        now: i64,
    ) -> std::result::Result<(), String> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare("SELECT run_id, tool_name, args_json, status, expires_at FROM approval_requests WHERE id = ?1")
            .map_err(|e| e.to_string())?;

        let mut rows = stmt.query(params![id]).map_err(|e| e.to_string())?;

        let row = match rows.next().map_err(|e| e.to_string())? {
            Some(r) => r,
            None => return Err(format!("Approval request '{}' not found", id)),
        };

        let stored_run_id: String = row.get(0).map_err(|e| e.to_string())?;
        let stored_tool_name: String = row.get(1).map_err(|e| e.to_string())?;
        let stored_args_json: String = row.get(2).map_err(|e| e.to_string())?;
        let stored_status: String = row.get(3).map_err(|e| e.to_string())?;
        let stored_expires_at: i64 = row.get(4).map_err(|e| e.to_string())?;

        drop(rows);
        drop(stmt);

        // 1. Replay check
        if stored_status == "consumed" {
            return Err(
                "Authorization proof has already been consumed or is invalid (replay rejected)"
                    .to_string(),
            );
        }

        // 2. Status check
        if stored_status != "approved" {
            return Err(format!(
                "Approval request '{}' is not approved (current status: '{}')",
                id, stored_status
            ));
        }

        // 3. Expiry check
        if stored_expires_at <= now {
            let _ = conn.execute(
                "UPDATE approval_requests SET status = 'expired' WHERE id = ?1",
                params![id],
            );
            return Err(format!("Approval request '{}' has expired", id));
        }

        // 4. Run ID match check
        if stored_run_id != run_id {
            return Err(format!(
                "Approval request run ID mismatch: approved for run '{}', attempted in run '{}'",
                stored_run_id, run_id
            ));
        }

        // 5. Tool name match check
        if !expected_tool_names
            .iter()
            .any(|&name| name == stored_tool_name)
        {
            return Err(format!(
                "Approval request tool mismatch: approved for '{}', attempted execution of '{}'",
                stored_tool_name,
                expected_tool_names.first().unwrap_or(&"unknown")
            ));
        }

        // 6. Argument match check (canonical serialization)
        let stored_args_val: serde_json::Value = serde_json::from_str(&stored_args_json)
            .map_err(|e| format!("Failed to parse persisted approval args: {}", e))?;

        if !json_values_canonically_equal(&stored_args_val, expected_args) {
            return Err(
                "Approval request argument mismatch: execution arguments differ from persisted approved arguments"
                    .to_string(),
            );
        }

        // 7. Atomic update to 'consumed'
        let affected = conn.execute(
            "UPDATE approval_requests SET status = 'consumed' WHERE id = ?1 AND status = 'approved'",
            params![id],
        ).map_err(|e| e.to_string())?;

        if affected == 0 {
            return Err(
                "Authorization proof has already been consumed (concurrent race/replay rejected)"
                    .to_string(),
            );
        }

        Ok(())
    }

    pub fn list_pending_approvals(&self) -> Result<Vec<ApprovalRequest>> {
        let now = chrono::Utc::now().timestamp_millis();
        let _ = self.expire_overdue_approvals(now);

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, run_id, tool_name, summary, args_json, risk_level, reason, status, created_at, expires_at, resolved_at, resolved_by
             FROM approval_requests WHERE status = 'pending' ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ApprovalRequest {
                id: r.get(0)?,
                run_id: r.get(1)?,
                tool_name: r.get(2)?,
                summary: r.get(3)?,
                args_json: r.get(4)?,
                risk_level: r.get(5)?,
                reason: r.get(6)?,
                status: r.get(7)?,
                created_at: r.get(8)?,
                expires_at: r.get(9)?,
                resolved_at: r.get(10)?,
                resolved_by: r.get(11)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn list_approval_history(&self, run_id: Option<&str>) -> Result<Vec<ApprovalRequest>> {
        let now = chrono::Utc::now().timestamp_millis();
        let _ = self.expire_overdue_approvals(now);

        let conn = self.conn.lock().unwrap();
        let query = if run_id.is_some() {
            "SELECT id, run_id, tool_name, summary, args_json, risk_level, reason, status, created_at, expires_at, resolved_at, resolved_by
             FROM approval_requests WHERE run_id = ?1 ORDER BY created_at DESC"
        } else {
            "SELECT id, run_id, tool_name, summary, args_json, risk_level, reason, status, created_at, expires_at, resolved_at, resolved_by
             FROM approval_requests ORDER BY created_at DESC"
        };

        let mut stmt = conn.prepare(query)?;
        let mut list = Vec::new();

        if let Some(r_id) = run_id {
            let rows = stmt.query_map(params![r_id], |r| {
                Ok(ApprovalRequest {
                    id: r.get(0)?,
                    run_id: r.get(1)?,
                    tool_name: r.get(2)?,
                    summary: r.get(3)?,
                    args_json: r.get(4)?,
                    risk_level: r.get(5)?,
                    reason: r.get(6)?,
                    status: r.get(7)?,
                    created_at: r.get(8)?,
                    expires_at: r.get(9)?,
                    resolved_at: r.get(10)?,
                    resolved_by: r.get(11)?,
                })
            })?;
            for r in rows {
                list.push(r?);
            }
        } else {
            let rows = stmt.query_map([], |r| {
                Ok(ApprovalRequest {
                    id: r.get(0)?,
                    run_id: r.get(1)?,
                    tool_name: r.get(2)?,
                    summary: r.get(3)?,
                    args_json: r.get(4)?,
                    risk_level: r.get(5)?,
                    reason: r.get(6)?,
                    status: r.get(7)?,
                    created_at: r.get(8)?,
                    expires_at: r.get(9)?,
                    resolved_at: r.get(10)?,
                    resolved_by: r.get(11)?,
                })
            })?;
            for r in rows {
                list.push(r?);
            }
        }

        Ok(list)
    }

    // Persistent Memories
    pub fn save_memory(&self, entry: &MemoryEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO memories (id, scope, owner_id, key, value, source, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET value = ?5, updated_at = ?8",
            params![
                entry.id,
                entry.scope,
                entry.owner_id,
                entry.key,
                entry.value,
                entry.source,
                entry.created_at,
                entry.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn get_memories_for_scope(&self, scope: &str, owner_id: &str) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, scope, owner_id, key, value, source, created_at, updated_at
             FROM memories WHERE scope = ?1 AND owner_id = ?2 ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map(params![scope, owner_id], |r| {
            Ok(MemoryEntry {
                id: r.get(0)?,
                scope: r.get(1)?,
                owner_id: r.get(2)?,
                key: r.get(3)?,
                value: r.get(4)?,
                source: r.get(5)?,
                created_at: r.get(6)?,
                updated_at: r.get(7)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn list_all_memories(&self) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, scope, owner_id, key, value, source, created_at, updated_at
             FROM memories ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(MemoryEntry {
                id: r.get(0)?,
                scope: r.get(1)?,
                owner_id: r.get(2)?,
                key: r.get(3)?,
                value: r.get(4)?,
                source: r.get(5)?,
                created_at: r.get(6)?,
                updated_at: r.get(7)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn delete_memory(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    // Visual Timeline Persistence
    pub fn save_timeline_step(&self, step: &TimelineStepRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO timeline_steps (id, run_id, step, action_name, click_x, click_y, before_base64, after_base64, diff_percent, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                step.id,
                step.run_id,
                step.step as i64,
                step.action_name,
                step.click_x,
                step.click_y,
                step.before_base64,
                step.after_base64,
                step.diff_percent,
                step.timestamp
            ],
        )?;
        Ok(())
    }

    pub fn get_timeline_steps(
        &self,
        run_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<TimelineStepRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut list = Vec::new();

        if let Some(rid) = run_id {
            let mut stmt = conn.prepare(
                "SELECT id, run_id, step, action_name, click_x, click_y, before_base64, after_base64, diff_percent, timestamp
                 FROM timeline_steps WHERE run_id = ?1 ORDER BY timestamp ASC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![rid, limit as i64], |r| {
                let step_i64: i64 = r.get(2)?;
                Ok(TimelineStepRecord {
                    id: r.get(0)?,
                    run_id: r.get(1)?,
                    step: step_i64 as usize,
                    action_name: r.get(3)?,
                    click_x: r.get(4)?,
                    click_y: r.get(5)?,
                    before_base64: r.get(6)?,
                    after_base64: r.get(7)?,
                    diff_percent: r.get(8)?,
                    timestamp: r.get(9)?,
                })
            })?;
            for r in rows {
                list.push(r?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, run_id, step, action_name, click_x, click_y, before_base64, after_base64, diff_percent, timestamp
                 FROM timeline_steps ORDER BY timestamp DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit as i64], |r| {
                let step_i64: i64 = r.get(2)?;
                Ok(TimelineStepRecord {
                    id: r.get(0)?,
                    run_id: r.get(1)?,
                    step: step_i64 as usize,
                    action_name: r.get(3)?,
                    click_x: r.get(4)?,
                    click_y: r.get(5)?,
                    before_base64: r.get(6)?,
                    after_base64: r.get(7)?,
                    diff_percent: r.get(8)?,
                    timestamp: r.get(9)?,
                })
            })?;
            for r in rows {
                list.push(r?);
            }
            list.reverse();
        }

        Ok(list)
    }

    pub fn clear_timeline_steps(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM timeline_steps", [])?;
        Ok(())
    }

    // Skills
    pub fn create_skill(&self, skill: &SkillRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO skills (id, name, description, steps_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET name = ?2, description = ?3, steps_json = ?4",
            params![skill.id, skill.name, skill.description, skill.steps_json, skill.created_at],
        )?;
        Ok(())
    }

    pub fn list_skills(&self) -> Result<Vec<SkillRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, description, steps_json, created_at FROM skills ORDER BY created_at DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok(SkillRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                steps_json: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn get_skill(&self, id: &str) -> Result<Option<SkillRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, steps_json, created_at FROM skills WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(SkillRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                steps_json: row.get(3)?,
                created_at: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn delete_skill(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        // Maintain referential integrity: remove associated scheduled routine registrations targeting this skill
        conn.execute(
            "DELETE FROM routines WHERE action_type = 'skill' AND target_id = ?1",
            params![id],
        )?;
        let count = conn.execute("DELETE FROM skills WHERE id = ?1", params![id])?;
        Ok(count > 0)
    }

    // Routines (Cron)
    pub fn create_routine(&self, routine: &RoutineRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let enabled_int = if routine.enabled { 1 } else { 0 };
        conn.execute(
            "INSERT INTO routines (id, name, cron_expression, action_type, target_id, enabled, last_run, next_run)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET name = ?2, cron_expression = ?3, action_type = ?4, target_id = ?5, enabled = ?6, last_run = ?7, next_run = ?8",
            params![routine.id, routine.name, routine.cron_expression, routine.action_type, routine.target_id, enabled_int, routine.last_run, routine.next_run],
        )?;
        Ok(())
    }

    pub fn update_routine(&self, routine: &RoutineRecord) -> Result<()> {
        self.create_routine(routine)
    }

    pub fn list_routines(&self) -> Result<Vec<RoutineRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, name, cron_expression, action_type, target_id, enabled, last_run, next_run FROM routines")?;
        let rows = stmt.query_map([], |row| {
            let enabled_int: i32 = row.get(5)?;
            Ok(RoutineRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                cron_expression: row.get(2)?,
                action_type: row.get(3)?,
                target_id: row.get(4)?,
                enabled: enabled_int == 1,
                last_run: row.get(6)?,
                next_run: row.get(7)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn get_routine(&self, id: &str) -> Result<Option<RoutineRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, cron_expression, action_type, target_id, enabled, last_run, next_run FROM routines WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            let enabled_int: i32 = row.get(5)?;
            Ok(Some(RoutineRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                cron_expression: row.get(2)?,
                action_type: row.get(3)?,
                target_id: row.get(4)?,
                enabled: enabled_int == 1,
                last_run: row.get(6)?,
                next_run: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_routines_for_target(&self, target_id: &str) -> Result<Vec<RoutineRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, cron_expression, action_type, target_id, enabled, last_run, next_run FROM routines WHERE target_id = ?1",
        )?;
        let rows = stmt.query_map(params![target_id], |row| {
            let enabled_int: i32 = row.get(5)?;
            Ok(RoutineRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                cron_expression: row.get(2)?,
                action_type: row.get(3)?,
                target_id: row.get(4)?,
                enabled: enabled_int == 1,
                last_run: row.get(6)?,
                next_run: row.get(7)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn update_routine_execution(
        &self,
        id: &str,
        last_run: i64,
        next_run: Option<i64>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE routines SET last_run = ?1, next_run = ?2 WHERE id = ?3",
            params![last_run, next_run, id],
        )?;
        Ok(())
    }

    pub fn update_routine_last_run(&self, id: &str, timestamp: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE routines SET last_run = ?1 WHERE id = ?2",
            params![timestamp, id],
        )?;
        Ok(())
    }

    pub fn delete_routine(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute("DELETE FROM routines WHERE id = ?1", params![id])?;
        Ok(count > 0)
    }

    // Bots (Multi-Bot)
    pub fn create_bot(&self, bot: &BotRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let is_default_int = if bot.is_default { 1 } else { 0 };
        conn.execute(
            "INSERT INTO bots (id, name, avatar, role, system_prompt, is_default)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET name = ?2, avatar = ?3, role = ?4, system_prompt = ?5, is_default = ?6",
            params![bot.id, bot.name, bot.avatar, bot.role, bot.system_prompt, is_default_int],
        )?;
        Ok(())
    }

    pub fn list_bots(&self) -> Result<Vec<BotRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, name, avatar, role, system_prompt, is_default FROM bots")?;
        let rows = stmt.query_map([], |row| {
            let is_default_int: i32 = row.get(5)?;
            Ok(BotRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                avatar: row.get(2)?,
                role: row.get(3)?,
                system_prompt: row.get(4)?,
                is_default: is_default_int == 1,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn delete_bot(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM bots WHERE id = ?1", params![id])?;
        Ok(())
    }

    // MCP Servers
    pub fn create_mcp_server(&self, srv: &McpServerRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let enabled_int = if srv.enabled { 1 } else { 0 };
        conn.execute(
            "INSERT INTO mcp_servers (id, name, command, args_json, env_json, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET name = ?2, command = ?3, args_json = ?4, env_json = ?5, enabled = ?6",
            params![srv.id, srv.name, srv.command, srv.args_json, srv.env_json, enabled_int],
        )?;
        Ok(())
    }

    pub fn list_mcp_servers(&self) -> Result<Vec<McpServerRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, name, command, args_json, env_json, enabled FROM mcp_servers")?;
        let rows = stmt.query_map([], |row| {
            let enabled_int: i32 = row.get(5)?;
            Ok(McpServerRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                command: row.get(2)?,
                args_json: row.get(3)?,
                env_json: row.get(4)?,
                enabled: enabled_int == 1,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn delete_mcp_server(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM mcp_servers WHERE id = ?1", params![id])?;
        Ok(())
    }

    // Artifacts Registry
    pub fn create_artifact(&self, artifact: &ArtifactRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO artifacts (id, run_id, conversation_id, name, file_path, artifact_type, mime_type, size_bytes, created_at, metadata_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                run_id = ?2, conversation_id = ?3, name = ?4, file_path = ?5,
                artifact_type = ?6, mime_type = ?7, size_bytes = ?8, metadata_json = ?10",
            params![
                artifact.id,
                artifact.run_id,
                artifact.conversation_id,
                artifact.name,
                artifact.file_path,
                artifact.artifact_type,
                artifact.mime_type,
                artifact.size_bytes,
                artifact.created_at,
                artifact.metadata_json,
            ],
        )?;
        Ok(())
    }

    /// Reconciles an unregistered file on disk into the artifact catalog
    pub fn reconcile_file_as_artifact(
        &self,
        file_path: &std::path::Path,
        run_id: Option<&str>,
        conversation_id: Option<&str>,
    ) -> Result<ArtifactRecord> {
        let meta = std::fs::metadata(file_path).map_err(|e| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(format!(
                "Cannot reconcile artifact: failed to read file metadata for '{}': {}",
                file_path.display(),
                e
            ))))
        })?;

        let file_name = file_path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "file.txt".to_string());

        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let art_type = match ext {
            "rs" | "ts" | "tsx" | "js" | "py" | "sh" | "c" | "cpp" | "go" => "code",
            "md" | "txt" => "markdown",
            "json" => "json",
            "png" | "jpg" | "jpeg" => "image",
            _ => "document",
        };

        let rec = ArtifactRecord {
            id: format!("art_{}", uuid::Uuid::new_v4()),
            run_id: run_id.map(|s| s.to_string()),
            conversation_id: conversation_id.map(|s| s.to_string()),
            name: file_name,
            file_path: file_path.to_string_lossy().to_string(),
            artifact_type: art_type.to_string(),
            mime_type: "text/plain".to_string(),
            size_bytes: meta.len() as i64,
            created_at: chrono::Utc::now().timestamp_millis(),
            metadata_json: Some(serde_json::json!({ "reconciled": true }).to_string()),
        };

        self.create_artifact(&rec)?;
        Ok(rec)
    }

    pub fn list_artifacts(
        &self,
        run_id: Option<&str>,
        conversation_id: Option<&str>,
    ) -> Result<Vec<ArtifactRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut sql = "SELECT id, run_id, conversation_id, name, file_path, artifact_type, mime_type, size_bytes, created_at, metadata_json FROM artifacts WHERE 1=1".to_string();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(r_id) = run_id {
            sql.push_str(" AND run_id = ?");
            params_vec.push(Box::new(r_id.to_string()));
        }
        if let Some(c_id) = conversation_id {
            sql.push_str(" AND conversation_id = ?");
            params_vec.push(Box::new(c_id.to_string()));
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT 100");

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(ArtifactRecord {
                id: row.get(0)?,
                run_id: row.get(1)?,
                conversation_id: row.get(2)?,
                name: row.get(3)?,
                file_path: row.get(4)?,
                artifact_type: row.get(5)?,
                mime_type: row.get(6)?,
                size_bytes: row.get(7)?,
                created_at: row.get(8)?,
                metadata_json: row.get(9)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn get_artifact(&self, id: &str) -> Result<Option<ArtifactRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, run_id, conversation_id, name, file_path, artifact_type, mime_type, size_bytes, created_at, metadata_json
             FROM artifacts WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(ArtifactRecord {
                id: row.get(0)?,
                run_id: row.get(1)?,
                conversation_id: row.get(2)?,
                name: row.get(3)?,
                file_path: row.get(4)?,
                artifact_type: row.get(5)?,
                mime_type: row.get(6)?,
                size_bytes: row.get(7)?,
                created_at: row.get(8)?,
                metadata_json: row.get(9)?,
            })
        })?;

        if let Some(item) = rows.next() {
            Ok(Some(item?))
        } else {
            Ok(None)
        }
    }

    pub fn delete_artifact(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute("DELETE FROM artifacts WHERE id = ?1", params![id])?;
        Ok(count > 0)
    }

    pub fn update_artifact_metadata(&self, id: &str, metadata_json: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute(
            "UPDATE artifacts SET metadata_json = ?1 WHERE id = ?2",
            params![metadata_json, id],
        )?;
        Ok(count > 0)
    }

    // Universal Search Across All Subsystems
    pub fn search_all(&self, query: &str) -> Result<Vec<SearchResultItem>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let pattern = format!("%{}%", trimmed);
        let conn = self.conn.lock().unwrap();
        let mut results = Vec::new();

        // 1. Search conversations
        let mut c_stmt = conn.prepare(
            "SELECT id, title, created_at FROM conversations WHERE title LIKE ?1 ORDER BY created_at DESC LIMIT 10",
        )?;
        let c_rows = c_stmt.query_map(params![pattern], |row| {
            Ok(SearchResultItem {
                category: "conversation".to_string(),
                id: row.get(0)?,
                title: row.get(1)?,
                snippet: format!("Session: {}", row.get::<_, String>(1)?),
                score: 1.0,
                timestamp: row.get(2)?,
            })
        })?;
        for r in c_rows {
            results.push(r?);
        }

        // 2. Search messages
        let mut m_stmt = conn.prepare(
            "SELECT id, conversation_id, content, created_at FROM messages WHERE content LIKE ?1 ORDER BY created_at DESC LIMIT 15",
        )?;
        let m_rows = m_stmt.query_map(params![pattern], |row| {
            let content: String = row.get(2)?;
            let snippet = if content.len() > 140 {
                format!("{}...", &content[..140])
            } else {
                content
            };
            Ok(SearchResultItem {
                category: "message".to_string(),
                id: row.get(0)?,
                title: format!("Message in {}", row.get::<_, String>(1)?),
                snippet,
                score: 0.9,
                timestamp: row.get(3)?,
            })
        })?;
        for r in m_rows {
            results.push(r?);
        }

        // 3. Search memories
        let mut mem_stmt = conn.prepare(
            "SELECT id, key, value, scope, updated_at FROM memories WHERE key LIKE ?1 OR value LIKE ?1 ORDER BY updated_at DESC LIMIT 10",
        )?;
        let mem_rows = mem_stmt.query_map(params![pattern], |row| {
            let k: String = row.get(1)?;
            let v: String = row.get(2)?;
            let scope: String = row.get(3)?;
            Ok(SearchResultItem {
                category: "memory".to_string(),
                id: row.get(0)?,
                title: format!("[{}] {}", scope, k),
                snippet: v,
                score: 0.85,
                timestamp: row.get(4)?,
            })
        })?;
        for r in mem_rows {
            results.push(r?);
        }

        // 4. Search skills
        let mut s_stmt = conn.prepare(
            "SELECT id, name, description, created_at FROM skills WHERE name LIKE ?1 OR description LIKE ?1 ORDER BY created_at DESC LIMIT 10",
        )?;
        let s_rows = s_stmt.query_map(params![pattern], |row| {
            Ok(SearchResultItem {
                category: "skill".to_string(),
                id: row.get(0)?,
                title: row.get(1)?,
                snippet: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                score: 0.95,
                timestamp: row.get(3)?,
            })
        })?;
        for r in s_rows {
            results.push(r?);
        }

        // 5. Search routines
        let mut r_stmt = conn.prepare(
            "SELECT id, name, cron_expression, last_run FROM routines WHERE name LIKE ?1 LIMIT 10",
        )?;
        let r_rows = r_stmt.query_map(params![pattern], |row| {
            Ok(SearchResultItem {
                category: "routine".to_string(),
                id: row.get(0)?,
                title: row.get(1)?,
                snippet: format!("Cron: {}", row.get::<_, String>(2)?),
                score: 0.9,
                timestamp: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
            })
        })?;
        for r in r_rows {
            results.push(r?);
        }

        // 6. Search artifacts
        let mut a_stmt = conn.prepare(
            "SELECT id, name, artifact_type, file_path, created_at FROM artifacts WHERE name LIKE ?1 OR file_path LIKE ?1 ORDER BY created_at DESC LIMIT 10",
        )?;
        let a_rows = a_stmt.query_map(params![pattern], |row| {
            Ok(SearchResultItem {
                category: "artifact".to_string(),
                id: row.get(0)?,
                title: row.get(1)?,
                snippet: format!(
                    "Type: {} ({})",
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?
                ),
                score: 0.95,
                timestamp: row.get(4)?,
            })
        })?;
        for r in a_rows {
            results.push(r?);
        }

        results.sort_by_key(|a| std::cmp::Reverse(a.timestamp));
        Ok(results)
    }

    // ==========================================
    // Connector Runtime & Connected Accounts
    // ==========================================

    pub fn seed_default_connectors(&self) -> Result<()> {
        let existing = self.list_connector_definitions()?;
        if existing.is_empty() {
            let now = chrono::Utc::now().timestamp_millis();
            let catalog = vec![
                ConnectorDefinitionRecord {
                    id: "github".into(),
                    name: "GitHub".into(),
                    connector_type: "native".into(),
                    description: "Connect GitHub to inspect repositories, summarize open issues, track pull requests, monitor GitHub Actions, and create issues or comments.".into(),
                    icon: "github".into(),
                    auth_type: "pat_or_oauth".into(),
                    capabilities_json: serde_json::json!([
                        "identity", "repositories", "issues", "pull_requests", "actions", "create_issue", "add_comment"
                    ]).to_string(),
                    required_scopes_json: serde_json::json!(["repo", "read:user", "actions:read"]).to_string(),
                    is_builtin: true,
                    is_available: true,
                    doc_url: Some("https://docs.github.com/en/rest".into()),
                    created_at: now,
                },
                ConnectorDefinitionRecord {
                    id: "gitlab".into(),
                    name: "GitLab".into(),
                    connector_type: "native".into(),
                    description: "Inspect GitLab projects, merge requests, issues, and CI/CD pipelines.".into(),
                    icon: "gitlab".into(),
                    auth_type: "pat_token".into(),
                    capabilities_json: serde_json::json!(["projects", "issues", "merge_requests", "pipelines"]).to_string(),
                    required_scopes_json: serde_json::json!(["read_api", "read_user"]).to_string(),
                    is_builtin: true,
                    is_available: false,
                    doc_url: Some("https://docs.gitlab.com/ee/api/rest/".into()),
                    created_at: now,
                },
                ConnectorDefinitionRecord {
                    id: "linear".into(),
                    name: "Linear".into(),
                    connector_type: "native".into(),
                    description: "Query issues, team boards, cycles, and project roadmaps.".into(),
                    icon: "linear".into(),
                    auth_type: "api_key".into(),
                    capabilities_json: serde_json::json!(["issues", "projects", "teams"]).to_string(),
                    required_scopes_json: serde_json::json!(["read", "write"]).to_string(),
                    is_builtin: true,
                    is_available: false,
                    doc_url: Some("https://developers.linear.app/docs/graphql/working-with-the-graphql-api".into()),
                    created_at: now,
                },
                ConnectorDefinitionRecord {
                    id: "google_drive".into(),
                    name: "Google Drive".into(),
                    connector_type: "native".into(),
                    description: "Search, inspect, and read files and documents in Google Drive.".into(),
                    icon: "google_drive".into(),
                    auth_type: "oauth_browser".into(),
                    capabilities_json: serde_json::json!(["search_files", "read_documents"]).to_string(),
                    required_scopes_json: serde_json::json!(["drive.readonly"]).to_string(),
                    is_builtin: true,
                    is_available: false,
                    doc_url: Some("https://developers.google.com/drive/api".into()),
                    created_at: now,
                },
                ConnectorDefinitionRecord {
                    id: "google_calendar".into(),
                    name: "Google Calendar".into(),
                    connector_type: "native".into(),
                    description: "List upcoming events, check availability, and inspect calendar agendas.".into(),
                    icon: "google_calendar".into(),
                    auth_type: "oauth_browser".into(),
                    capabilities_json: serde_json::json!(["list_events", "check_agenda"]).to_string(),
                    required_scopes_json: serde_json::json!(["calendar.readonly"]).to_string(),
                    is_builtin: true,
                    is_available: false,
                    doc_url: Some("https://developers.google.com/calendar/api".into()),
                    created_at: now,
                },
                ConnectorDefinitionRecord {
                    id: "slack".into(),
                    name: "Slack".into(),
                    connector_type: "native".into(),
                    description: "Search messages, channels, and conversations for team decisions.".into(),
                    icon: "slack".into(),
                    auth_type: "oauth_browser".into(),
                    capabilities_json: serde_json::json!(["search_messages", "list_channels"]).to_string(),
                    required_scopes_json: serde_json::json!(["channels:read", "search:read"]).to_string(),
                    is_builtin: true,
                    is_available: false,
                    doc_url: Some("https://api.slack.com/".into()),
                    created_at: now,
                },
                ConnectorDefinitionRecord {
                    id: "remote_mcp".into(),
                    name: "Remote MCP Server".into(),
                    connector_type: "remote_mcp".into(),
                    description: "Connect to a remote MCP server via HTTP Streamable JSON-RPC 2.0 transport with token authorization.".into(),
                    icon: "network".into(),
                    auth_type: "bearer_token".into(),
                    capabilities_json: serde_json::json!(["dynamic_tools", "http_transport", "remote_execution"]).to_string(),
                    required_scopes_json: serde_json::json!([]).to_string(),
                    is_builtin: true,
                    is_available: true,
                    doc_url: Some("https://modelcontextprotocol.io/".into()),
                    created_at: now,
                },
                ConnectorDefinitionRecord {
                    id: "local_mcp".into(),
                    name: "Local MCP (Stdio)".into(),
                    connector_type: "local_mcp".into(),
                    description: "Standard input/output subprocess MCP tool servers executing on your local machine.".into(),
                    icon: "terminal".into(),
                    auth_type: "none".into(),
                    capabilities_json: serde_json::json!(["dynamic_tools", "stdio_transport", "process_isolation"]).to_string(),
                    required_scopes_json: serde_json::json!([]).to_string(),
                    is_builtin: true,
                    is_available: true,
                    doc_url: Some("https://modelcontextprotocol.io/".into()),
                    created_at: now,
                },
            ];

            for c in catalog {
                self.create_connector_definition(&c)?;
            }
        }
        Ok(())
    }

    pub fn create_connector_definition(&self, def: &ConnectorDefinitionRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let is_builtin_int = if def.is_builtin { 1 } else { 0 };
        let is_available_int = if def.is_available { 1 } else { 0 };
        conn.execute(
            "INSERT INTO connector_definitions (
                id, name, connector_type, description, icon, auth_type,
                capabilities_json, required_scopes_json, is_builtin, is_available, doc_url, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(id) DO UPDATE SET
                name = ?2, connector_type = ?3, description = ?4, icon = ?5, auth_type = ?6,
                capabilities_json = ?7, required_scopes_json = ?8, is_builtin = ?9, is_available = ?10, doc_url = ?11",
            params![
                def.id, def.name, def.connector_type, def.description, def.icon, def.auth_type,
                def.capabilities_json, def.required_scopes_json, is_builtin_int, is_available_int, def.doc_url, def.created_at
            ],
        )?;
        Ok(())
    }

    pub fn get_connector_definition(&self, id: &str) -> Result<Option<ConnectorDefinitionRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, connector_type, description, icon, auth_type,
                    capabilities_json, required_scopes_json, is_builtin, is_available, doc_url, created_at
             FROM connector_definitions WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            let is_builtin_int: i32 = row.get(8)?;
            let is_available_int: i32 = row.get(9)?;
            Ok(Some(ConnectorDefinitionRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                connector_type: row.get(2)?,
                description: row.get(3)?,
                icon: row.get(4)?,
                auth_type: row.get(5)?,
                capabilities_json: row.get(6)?,
                required_scopes_json: row.get(7)?,
                is_builtin: is_builtin_int == 1,
                is_available: is_available_int == 1,
                doc_url: row.get(10)?,
                created_at: row.get(11)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_connector_definitions(&self) -> Result<Vec<ConnectorDefinitionRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, connector_type, description, icon, auth_type,
                    capabilities_json, required_scopes_json, is_builtin, is_available, doc_url, created_at
             FROM connector_definitions ORDER BY is_available DESC, name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let is_builtin_int: i32 = row.get(8)?;
            let is_available_int: i32 = row.get(9)?;
            Ok(ConnectorDefinitionRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                connector_type: row.get(2)?,
                description: row.get(3)?,
                icon: row.get(4)?,
                auth_type: row.get(5)?,
                capabilities_json: row.get(6)?,
                required_scopes_json: row.get(7)?,
                is_builtin: is_builtin_int == 1,
                is_available: is_available_int == 1,
                doc_url: row.get(10)?,
                created_at: row.get(11)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    // Connected Accounts
    pub fn create_connected_account(&self, acc: &ConnectedAccountRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO connected_accounts (
                id, connector_id, account_identifier, display_name, status,
                status_message, granted_scopes_json, credential_key, config_json,
                created_at, updated_at, last_connected_at, last_error, expires_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ON CONFLICT(id) DO UPDATE SET
                account_identifier = ?3, display_name = ?4, status = ?5, status_message = ?6,
                granted_scopes_json = ?7, credential_key = ?8, config_json = ?9, updated_at = ?11,
                last_connected_at = ?12, last_error = ?13, expires_at = ?14",
            params![
                acc.id,
                acc.connector_id,
                acc.account_identifier,
                acc.display_name,
                acc.status,
                acc.status_message,
                acc.granted_scopes_json,
                acc.credential_key,
                acc.config_json,
                acc.created_at,
                acc.updated_at,
                acc.last_connected_at,
                acc.last_error,
                acc.expires_at
            ],
        )?;
        Ok(())
    }

    pub fn get_connected_account(&self, id: &str) -> Result<Option<ConnectedAccountRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, connector_id, account_identifier, display_name, status,
                    status_message, granted_scopes_json, credential_key, config_json,
                    created_at, updated_at, last_connected_at, last_error, expires_at
             FROM connected_accounts WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(ConnectedAccountRecord {
                id: row.get(0)?,
                connector_id: row.get(1)?,
                account_identifier: row.get(2)?,
                display_name: row.get(3)?,
                status: row.get(4)?,
                status_message: row.get(5)?,
                granted_scopes_json: row.get(6)?,
                credential_key: row.get(7)?,
                config_json: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                last_connected_at: row.get(11)?,
                last_error: row.get(12)?,
                expires_at: row.get(13)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_connected_accounts(&self) -> Result<Vec<ConnectedAccountRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, connector_id, account_identifier, display_name, status,
                    status_message, granted_scopes_json, credential_key, config_json,
                    created_at, updated_at, last_connected_at, last_error, expires_at
             FROM connected_accounts ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ConnectedAccountRecord {
                id: row.get(0)?,
                connector_id: row.get(1)?,
                account_identifier: row.get(2)?,
                display_name: row.get(3)?,
                status: row.get(4)?,
                status_message: row.get(5)?,
                granted_scopes_json: row.get(6)?,
                credential_key: row.get(7)?,
                config_json: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                last_connected_at: row.get(11)?,
                last_error: row.get(12)?,
                expires_at: row.get(13)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn list_accounts_for_connector(
        &self,
        connector_id: &str,
    ) -> Result<Vec<ConnectedAccountRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, connector_id, account_identifier, display_name, status,
                    status_message, granted_scopes_json, credential_key, config_json,
                    created_at, updated_at, last_connected_at, last_error, expires_at
             FROM connected_accounts WHERE connector_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![connector_id], |row| {
            Ok(ConnectedAccountRecord {
                id: row.get(0)?,
                connector_id: row.get(1)?,
                account_identifier: row.get(2)?,
                display_name: row.get(3)?,
                status: row.get(4)?,
                status_message: row.get(5)?,
                granted_scopes_json: row.get(6)?,
                credential_key: row.get(7)?,
                config_json: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
                last_connected_at: row.get(11)?,
                last_error: row.get(12)?,
                expires_at: row.get(13)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn update_connected_account_status(
        &self,
        id: &str,
        status: &str,
        status_message: Option<&str>,
        last_error: Option<&str>,
        last_connected_at: Option<i64>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE connected_accounts SET
                status = ?1,
                status_message = ?2,
                last_error = ?3,
                last_connected_at = COALESCE(?4, last_connected_at),
                updated_at = ?5
             WHERE id = ?6",
            params![
                status,
                status_message,
                last_error,
                last_connected_at,
                now,
                id
            ],
        )?;
        Ok(())
    }

    pub fn update_connected_account(&self, acc: &ConnectedAccountRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE connected_accounts SET
                account_identifier = ?1,
                display_name = ?2,
                status = ?3,
                status_message = ?4,
                granted_scopes_json = ?5,
                credential_key = ?6,
                config_json = ?7,
                updated_at = ?8,
                last_connected_at = ?9,
                last_error = ?10,
                expires_at = ?11
             WHERE id = ?12",
            params![
                acc.account_identifier,
                acc.display_name,
                acc.status,
                acc.status_message,
                acc.granted_scopes_json,
                acc.credential_key,
                acc.config_json,
                acc.updated_at,
                acc.last_connected_at,
                acc.last_error,
                acc.expires_at,
                acc.id
            ],
        )?;
        Ok(())
    }

    pub fn delete_connected_account(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM connected_accounts WHERE id = ?1", params![id])?;
        Ok(())
    }

    // Connector Tool Policies
    pub fn set_connector_tool_policy(&self, pol: &ConnectorToolPolicyRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO connector_tool_policies (id, account_id, tool_name, policy, risk_level, updated_at, schema_fingerprint)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(account_id, tool_name) DO UPDATE SET
                policy = ?4, risk_level = ?5, updated_at = ?6, schema_fingerprint = ?7",
            params![
                pol.id,
                pol.account_id,
                pol.tool_name,
                pol.policy,
                pol.risk_level,
                pol.updated_at,
                pol.schema_fingerprint
            ],
        )?;
        Ok(())
    }

    pub fn get_connector_tool_policy(
        &self,
        account_id: &str,
        tool_name: &str,
    ) -> Result<Option<ConnectorToolPolicyRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, account_id, tool_name, policy, risk_level, updated_at, schema_fingerprint
             FROM connector_tool_policies WHERE account_id = ?1 AND tool_name = ?2",
        )?;
        let mut rows = stmt.query(params![account_id, tool_name])?;
        if let Some(row) = rows.next()? {
            Ok(Some(ConnectorToolPolicyRecord {
                id: row.get(0)?,
                account_id: row.get(1)?,
                tool_name: row.get(2)?,
                policy: row.get(3)?,
                risk_level: row.get(4)?,
                updated_at: row.get(5)?,
                schema_fingerprint: row.get(6)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_connector_tool_policies(
        &self,
        account_id: &str,
    ) -> Result<Vec<ConnectorToolPolicyRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, account_id, tool_name, policy, risk_level, updated_at, schema_fingerprint
             FROM connector_tool_policies WHERE account_id = ?1 ORDER BY tool_name ASC",
        )?;
        let rows = stmt.query_map(params![account_id], |row| {
            Ok(ConnectorToolPolicyRecord {
                id: row.get(0)?,
                account_id: row.get(1)?,
                tool_name: row.get(2)?,
                policy: row.get(3)?,
                risk_level: row.get(4)?,
                updated_at: row.get(5)?,
                schema_fingerprint: row.get(6)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn delete_connector_tool_policies(&self, account_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM connector_tool_policies WHERE account_id = ?1",
            params![account_id],
        )?;
        Ok(())
    }

    // --- Authorized Folders ---

    pub fn list_authorized_folders(&self) -> Result<Vec<AuthorizedFolderRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, path, display_name, added_at FROM authorized_folders ORDER BY added_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AuthorizedFolderRecord {
                id: row.get(0)?,
                path: row.get(1)?,
                display_name: row.get(2)?,
                added_at: row.get(3)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn add_authorized_folder(&self, folder: &AuthorizedFolderRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO authorized_folders (id, path, display_name, added_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET display_name = excluded.display_name",
            params![folder.id, folder.path, folder.display_name, folder.added_at,],
        )?;
        Ok(())
    }

    pub fn remove_authorized_folder(&self, id_or_path: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(
            "DELETE FROM authorized_folders WHERE id = ?1 OR path = ?1",
            params![id_or_path],
        )?;
        Ok(affected > 0)
    }

    /// Atomically removes an authorized folder and immediately prunes documents that are no longer authorized.
    pub fn remove_authorized_folder_and_prune(&self, id_or_path: &str) -> Result<bool> {
        let removed = self.remove_authorized_folder(id_or_path)?;
        if removed {
            let remaining = self.list_authorized_folders()?;
            self.prune_unauthorized_indexed_documents(&remaining)?;
        }
        Ok(removed)
    }

    pub fn is_path_authorized(&self, path: &std::path::Path) -> Result<bool> {
        let folders = self.list_authorized_folders()?;
        Ok(Self::check_path_authorized_in_folders(&folders, path).is_ok())
    }

    pub fn validate_authorized_path(
        &self,
        path: &std::path::Path,
    ) -> Result<std::path::PathBuf, String> {
        let folders = self.list_authorized_folders().map_err(|e| e.to_string())?;
        Self::check_path_authorized_in_folders(&folders, path)
    }

    /// Validates directory containment using canonicalization and path-component semantics.
    /// Fails closed when authorized_folders is empty or path is outside authorized boundaries.
    pub fn check_path_authorized_in_folders(
        folders: &[AuthorizedFolderRecord],
        path: &std::path::Path,
    ) -> Result<std::path::PathBuf, String> {
        if folders.is_empty() {
            return Err("Access denied: No authorized folders configured. Please authorize a folder in Kyro Settings or Onboarding first.".to_string());
        }

        let raw_str = path.to_string_lossy();
        let trimmed = raw_str.trim();
        if trimmed.is_empty() {
            return Err("Access denied: Path cannot be empty.".to_string());
        }
        if trimmed.contains('\0') {
            return Err("Access denied: Path contains prohibited null byte.".to_string());
        }

        // Canonicalize target if exists, or canonicalize closest existing ancestor and preserve relative suffix
        let canonical_target = if path.exists() {
            std::fs::canonicalize(path)
                .map_err(|e| format!("Failed to canonicalize path '{}': {}", path.display(), e))?
        } else {
            let mut cur = path.to_path_buf();
            let mut trail = Vec::new();
            while !cur.exists() {
                if let Some(name) = cur.file_name() {
                    let name_str = name.to_string_lossy();
                    if name_str == ".." || name_str == "." {
                        return Err(
                            "Access denied: Path traversal characters prohibited.".to_string()
                        );
                    }
                    trail.push(name.to_os_string());
                } else {
                    break;
                }
                if !cur.pop() {
                    break;
                }
            }
            if !cur.exists() {
                return Err(format!(
                    "Access denied: Path '{}' has no accessible ancestor directory.",
                    path.display()
                ));
            }
            let canonical_ancestor = std::fs::canonicalize(&cur).map_err(|e| {
                format!("Failed to canonicalize ancestor '{}': {}", cur.display(), e)
            })?;
            let mut assembled = canonical_ancestor;
            for comp in trail.into_iter().rev() {
                assembled.push(comp);
            }
            assembled
        };

        for f in folders {
            let auth_path = std::path::Path::new(&f.path);
            if let Ok(canonical_auth) = std::fs::canonicalize(auth_path) {
                if canonical_target.starts_with(&canonical_auth) {
                    return Ok(canonical_target);
                }
            }
        }

        Err(format!(
            "Access denied: Path '{}' is outside authorized folder boundaries.",
            path.display()
        ))
    }

    // --- Document Search Index ---

    pub fn get_indexed_document(&self, path: &str) -> Result<Option<DocumentIndexRecord>> {
        // Fail-closed authorization check: never return cached extracted content if folder is not currently authorized
        let p = std::path::Path::new(path);
        let folders = self.list_authorized_folders().unwrap_or_default();
        if folders.is_empty() || Self::check_path_authorized_in_folders(&folders, p).is_err() {
            return Ok(None);
        }

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT path, file_name, format, size_bytes, modified_at, sha256, extracted_text, indexed_at
             FROM document_index WHERE path = ?1",
        )?;
        let mut rows = stmt.query(params![path])?;
        if let Some(row) = rows.next()? {
            let size_int: i64 = row.get(3)?;
            Ok(Some(DocumentIndexRecord {
                path: row.get(0)?,
                file_name: row.get(1)?,
                format: row.get(2)?,
                size_bytes: size_int as u64,
                modified_at: row.get(4)?,
                sha256: row.get(5)?,
                extracted_text: row.get(6)?,
                indexed_at: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn upsert_indexed_document(&self, record: &DocumentIndexRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO document_index (path, file_name, format, size_bytes, modified_at, sha256, extracted_text, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(path) DO UPDATE SET
                file_name = excluded.file_name,
                format = excluded.format,
                size_bytes = excluded.size_bytes,
                modified_at = excluded.modified_at,
                sha256 = excluded.sha256,
                extracted_text = excluded.extracted_text,
                indexed_at = excluded.indexed_at",
            params![
                record.path,
                record.file_name,
                record.format,
                record.size_bytes as i64,
                record.modified_at,
                record.sha256,
                record.extracted_text,
                record.indexed_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_indexed_document(&self, path: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute("DELETE FROM document_index WHERE path = ?1", params![path])?;
        Ok(count > 0)
    }

    pub fn list_indexed_documents(&self) -> Result<Vec<DocumentIndexRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT path, file_name, format, size_bytes, modified_at, sha256, extracted_text, indexed_at
             FROM document_index ORDER BY modified_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let size_int: i64 = row.get(3)?;
            Ok(DocumentIndexRecord {
                path: row.get(0)?,
                file_name: row.get(1)?,
                format: row.get(2)?,
                size_bytes: size_int as u64,
                modified_at: row.get(4)?,
                sha256: row.get(5)?,
                extracted_text: row.get(6)?,
                indexed_at: row.get(7)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn prune_unauthorized_indexed_documents(
        &self,
        authorized_folders: &[AuthorizedFolderRecord],
    ) -> Result<usize> {
        let all = self.list_indexed_documents()?;
        let mut removed = 0;
        for doc in all {
            let p = std::path::Path::new(&doc.path);
            let is_auth = Self::check_path_authorized_in_folders(authorized_folders, p).is_ok();
            if (!is_auth || !p.exists()) && self.delete_indexed_document(&doc.path)? {
                removed += 1;
            }
        }
        Ok(removed)
    }

    // --- Routine Runs ---

    pub fn record_routine_run(&self, run: &RoutineRunRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO routine_runs (id, routine_id, routine_name, status, started_at, completed_at, summary, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                run.id,
                run.routine_id,
                run.routine_name,
                run.status,
                run.started_at,
                run.completed_at,
                run.summary,
                run.error,
            ],
        )?;
        Ok(())
    }

    pub fn update_routine_run(
        &self,
        id: &str,
        status: &str,
        completed_at: i64,
        summary: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE routine_runs SET status = ?1, completed_at = ?2, summary = ?3, error = ?4 WHERE id = ?5",
            params![status, completed_at, summary, error, id],
        )?;
        Ok(())
    }

    pub fn get_routine_run(&self, id: &str) -> Result<Option<RoutineRunRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, routine_id, routine_name, status, started_at, completed_at, summary, error
             FROM routine_runs WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(RoutineRunRecord {
                id: row.get(0)?,
                routine_id: row.get(1)?,
                routine_name: row.get(2)?,
                status: row.get(3)?,
                started_at: row.get(4)?,
                completed_at: row.get(5)?,
                summary: row.get(6)?,
                error: row.get(7)?,
            })
        })?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn list_routine_runs(&self, limit: usize) -> Result<Vec<RoutineRunRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, routine_id, routine_name, status, started_at, completed_at, summary, error
             FROM routine_runs ORDER BY started_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(RoutineRunRecord {
                id: row.get(0)?,
                routine_id: row.get(1)?,
                routine_name: row.get(2)?,
                status: row.get(3)?,
                started_at: row.get(4)?,
                completed_at: row.get(5)?,
                summary: row.get(6)?,
                error: row.get(7)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn list_runs_for_routine(
        &self,
        routine_id: &str,
        limit: usize,
    ) -> Result<Vec<RoutineRunRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, routine_id, routine_name, status, started_at, completed_at, summary, error
             FROM routine_runs WHERE routine_id = ?1 ORDER BY started_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![routine_id, limit as i64], |row| {
            Ok(RoutineRunRecord {
                id: row.get(0)?,
                routine_id: row.get(1)?,
                routine_name: row.get(2)?,
                status: row.get(3)?,
                started_at: row.get(4)?,
                completed_at: row.get(5)?,
                summary: row.get(6)?,
                error: row.get(7)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    // --- File Transactions ---

    pub fn record_file_transaction(
        &self,
        tx: &crate::storage::organizer::OrganizationTransaction,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let payload = serde_json::to_string(tx).unwrap_or_default();
        conn.execute(
            "INSERT INTO file_transactions (transaction_id, target_directory, status, created_at, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(transaction_id) DO UPDATE SET status = excluded.status, payload_json = excluded.payload_json",
            params![
                tx.transaction_id,
                tx.target_directory,
                tx.status,
                tx.created_at,
                payload,
            ],
        )?;
        Ok(())
    }

    pub fn update_file_transaction_status(&self, transaction_id: &str, status: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE file_transactions SET status = ?1 WHERE transaction_id = ?2",
            params![status, transaction_id],
        )?;
        Ok(())
    }

    pub fn list_file_transactions(
        &self,
        limit: usize,
    ) -> Result<Vec<crate::storage::organizer::OrganizationTransaction>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT payload_json FROM file_transactions ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            let json_str: String = row.get(0)?;
            Ok(
                serde_json::from_str::<crate::storage::organizer::OrganizationTransaction>(
                    &json_str,
                )
                .ok(),
            )
        })?;
        let mut list = Vec::new();
        for r in rows {
            if let Some(tx) = r? {
                list.push(tx);
            }
        }
        Ok(list)
    }

    pub fn get_file_transaction(
        &self,
        transaction_id: &str,
    ) -> Result<Option<crate::storage::organizer::OrganizationTransaction>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT payload_json FROM file_transactions WHERE transaction_id = ?1")?;
        let mut rows = stmt.query(params![transaction_id])?;
        if let Some(row) = rows.next()? {
            let json_str: String = row.get(0)?;
            Ok(
                serde_json::from_str::<crate::storage::organizer::OrganizationTransaction>(
                    &json_str,
                )
                .ok(),
            )
        } else {
            Ok(None)
        }
    }

    // --- Habit Learning ---

    pub fn get_habit_settings(&self) -> Result<HabitSettingsRecord> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key, enabled, min_occurrences, updated_at FROM habit_learning_settings WHERE key = 'default'")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(HabitSettingsRecord {
                key: row.get(0)?,
                enabled: row.get::<_, i64>(1)? != 0,
                min_occurrences: row.get::<_, i64>(2)? as usize,
                updated_at: row.get(3)?,
            })
        } else {
            Ok(HabitSettingsRecord {
                key: "default".into(),
                enabled: false,
                min_occurrences: 3,
                updated_at: chrono::Utc::now().timestamp_millis(),
            })
        }
    }

    pub fn save_habit_settings(&self, settings: &HabitSettingsRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO habit_learning_settings (key, enabled, min_occurrences, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(key) DO UPDATE SET enabled = ?2, min_occurrences = ?3, updated_at = ?4",
            params![
                settings.key,
                if settings.enabled { 1 } else { 0 },
                settings.min_occurrences as i64,
                settings.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn upsert_habit_suggestion(&self, s: &HabitSuggestionRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO habit_suggestions (
                id, name, description, signature_hash, occurrences, confidence,
                proposed_action_type, proposed_payload_json, status, evidence_json,
                first_seen_at, last_seen_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(signature_hash) DO UPDATE SET
                occurrences = ?5,
                confidence = ?6,
                evidence_json = ?10,
                last_seen_at = ?12,
                updated_at = ?13",
            params![
                s.id,
                s.name,
                s.description,
                s.signature_hash,
                s.occurrences as i64,
                s.confidence,
                s.proposed_action_type,
                s.proposed_payload_json,
                s.status,
                s.evidence_json,
                s.first_seen_at,
                s.last_seen_at,
                s.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_habit_suggestion(&self, id: &str) -> Result<Option<HabitSuggestionRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, signature_hash, occurrences, confidence,
                    proposed_action_type, proposed_payload_json, status, evidence_json,
                    first_seen_at, last_seen_at, updated_at
             FROM habit_suggestions WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(HabitSuggestionRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                signature_hash: row.get(3)?,
                occurrences: row.get::<_, i64>(4)? as usize,
                confidence: row.get(5)?,
                proposed_action_type: row.get(6)?,
                proposed_payload_json: row.get(7)?,
                status: row.get(8)?,
                evidence_json: row.get(9)?,
                first_seen_at: row.get(10)?,
                last_seen_at: row.get(11)?,
                updated_at: row.get(12)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_habit_suggestions(
        &self,
        status_filter: Option<&str>,
    ) -> Result<Vec<HabitSuggestionRecord>> {
        let conn = self.conn.lock().unwrap();
        let map_row = |row: &rusqlite::Row| {
            Ok(HabitSuggestionRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                signature_hash: row.get(3)?,
                occurrences: row.get::<_, i64>(4)? as usize,
                confidence: row.get(5)?,
                proposed_action_type: row.get(6)?,
                proposed_payload_json: row.get(7)?,
                status: row.get(8)?,
                evidence_json: row.get(9)?,
                first_seen_at: row.get(10)?,
                last_seen_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        };
        let mut list = Vec::new();
        match status_filter {
            Some(st) if !st.is_empty() && st != "all" => {
                let mut stmt = conn.prepare(
                    "SELECT id, name, description, signature_hash, occurrences, confidence,
                            proposed_action_type, proposed_payload_json, status, evidence_json,
                            first_seen_at, last_seen_at, updated_at
                     FROM habit_suggestions WHERE status = ?1 ORDER BY confidence DESC, updated_at DESC",
                )?;
                let rows = stmt.query_map(params![st], map_row)?;
                for r in rows {
                    list.push(r?);
                }
            }
            _ => {
                let mut stmt = conn.prepare(
                    "SELECT id, name, description, signature_hash, occurrences, confidence,
                            proposed_action_type, proposed_payload_json, status, evidence_json,
                            first_seen_at, last_seen_at, updated_at
                     FROM habit_suggestions ORDER BY confidence DESC, updated_at DESC",
                )?;
                let rows = stmt.query_map([], map_row)?;
                for r in rows {
                    list.push(r?);
                }
            }
        }
        Ok(list)
    }

    pub fn update_habit_suggestion_status(&self, id: &str, status: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        let count = conn.execute(
            "UPDATE habit_suggestions SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status, now, id],
        )?;
        Ok(count > 0)
    }

    pub fn delete_habit_suggestion(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute("DELETE FROM habit_suggestions WHERE id = ?1", params![id])?;
        Ok(count > 0)
    }

    pub fn clear_all_habit_data(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM habit_suggestions", [])?;
        Ok(())
    }

    // --- Document Chunks & RAG ---

    pub fn insert_document_chunks(
        &self,
        doc_path: &str,
        chunks: &[DocumentChunkRecord],
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM document_chunks WHERE document_path = ?1",
            params![doc_path],
        )?;
        for c in chunks {
            tx.execute(
                "INSERT INTO document_chunks (
                    id, document_path, chunk_index, heading, start_line, end_line,
                    content, content_hash, format, chunk_version, indexed_at, embedding_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    c.id,
                    c.document_path,
                    c.chunk_index as i64,
                    c.heading,
                    c.start_line.map(|v| v as i64),
                    c.end_line.map(|v| v as i64),
                    c.content,
                    c.content_hash,
                    c.format,
                    c.chunk_version as i64,
                    c.indexed_at,
                    c.embedding_json,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_chunks_for_document(&self, doc_path: &str) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM document_chunks WHERE document_path = ?1",
            params![doc_path],
        )
    }

    pub fn get_chunks_for_document(&self, doc_path: &str) -> Result<Vec<DocumentChunkRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, document_path, chunk_index, heading, start_line, end_line,
                    content, content_hash, format, chunk_version, indexed_at, embedding_json
             FROM document_chunks WHERE document_path = ?1 ORDER BY chunk_index ASC",
        )?;
        let rows = stmt.query_map(params![doc_path], |row| {
            Ok(DocumentChunkRecord {
                id: row.get(0)?,
                document_path: row.get(1)?,
                chunk_index: row.get::<_, i64>(2)? as usize,
                heading: row.get(3)?,
                start_line: row.get::<_, Option<i64>>(4)?.map(|v| v as usize),
                end_line: row.get::<_, Option<i64>>(5)?.map(|v| v as usize),
                content: row.get(6)?,
                content_hash: row.get(7)?,
                format: row.get(8)?,
                chunk_version: row.get::<_, i64>(9)? as u32,
                indexed_at: row.get(10)?,
                embedding_json: row.get(11)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn list_all_chunks(&self) -> Result<Vec<DocumentChunkRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, document_path, chunk_index, heading, start_line, end_line,
                    content, content_hash, format, chunk_version, indexed_at, embedding_json
             FROM document_chunks ORDER BY indexed_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DocumentChunkRecord {
                id: row.get(0)?,
                document_path: row.get(1)?,
                chunk_index: row.get::<_, i64>(2)? as usize,
                heading: row.get(3)?,
                start_line: row.get::<_, Option<i64>>(4)?.map(|v| v as usize),
                end_line: row.get::<_, Option<i64>>(5)?.map(|v| v as usize),
                content: row.get(6)?,
                content_hash: row.get(7)?,
                format: row.get(8)?,
                chunk_version: row.get::<_, i64>(9)? as u32,
                indexed_at: row.get(10)?,
                embedding_json: row.get(11)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn clear_all_chunks(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM document_chunks", [])
    }

    pub fn get_rag_settings(&self) -> Result<RagSettingsRecord> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key, embedding_mode, embedding_endpoint, embedding_model, max_candidates, updated_at FROM rag_settings WHERE key = 'default'")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(RagSettingsRecord {
                key: row.get(0)?,
                embedding_mode: row.get(1)?,
                embedding_endpoint: row.get(2)?,
                embedding_model: row.get(3)?,
                max_candidates: row.get::<_, i64>(4)? as usize,
                updated_at: row.get(5)?,
            })
        } else {
            Ok(RagSettingsRecord {
                key: "default".into(),
                embedding_mode: "lexical_only".into(),
                embedding_endpoint: None,
                embedding_model: None,
                max_candidates: 5,
                updated_at: chrono::Utc::now().timestamp_millis(),
            })
        }
    }

    pub fn save_rag_settings(&self, settings: &RagSettingsRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO rag_settings (key, embedding_mode, embedding_endpoint, embedding_model, max_candidates, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(key) DO UPDATE SET
                embedding_mode = ?2,
                embedding_endpoint = ?3,
                embedding_model = ?4,
                max_candidates = ?5,
                updated_at = ?6",
            params![
                settings.key,
                settings.embedding_mode,
                settings.embedding_endpoint,
                settings.embedding_model,
                settings.max_candidates as i64,
                settings.updated_at
            ],
        )?;
        Ok(())
    }

    // --- Code Experiments ---

    pub fn create_code_experiment(&self, exp: &CodeExperimentRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO code_experiments (
                id, name, run_id, runtime, image, script, argv_json,
                workspace_path, output_dir, network_mode, cpu_limit,
                memory_limit_mb, timeout_seconds, status, exit_code,
                stdout_tail, stderr_tail, result_summary, artifact_ids_json,
                created_at, started_at, completed_at, error
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
            params![
                exp.id,
                exp.name,
                exp.run_id,
                exp.runtime,
                exp.image,
                exp.script,
                exp.argv_json,
                exp.workspace_path,
                exp.output_dir,
                exp.network_mode,
                exp.cpu_limit,
                exp.memory_limit_mb as i64,
                exp.timeout_seconds as i64,
                exp.status,
                exp.exit_code,
                exp.stdout_tail,
                exp.stderr_tail,
                exp.result_summary,
                exp.artifact_ids_json,
                exp.created_at,
                exp.started_at,
                exp.completed_at,
                exp.error,
            ],
        )?;
        Ok(())
    }

    pub fn update_code_experiment(&self, exp: &CodeExperimentRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE code_experiments SET
                name = ?2,
                status = ?3,
                exit_code = ?4,
                stdout_tail = ?5,
                stderr_tail = ?6,
                result_summary = ?7,
                artifact_ids_json = ?8,
                started_at = ?9,
                completed_at = ?10,
                error = ?11
             WHERE id = ?1",
            params![
                exp.id,
                exp.name,
                exp.status,
                exp.exit_code,
                exp.stdout_tail,
                exp.stderr_tail,
                exp.result_summary,
                exp.artifact_ids_json,
                exp.started_at,
                exp.completed_at,
                exp.error,
            ],
        )?;
        Ok(())
    }

    pub fn get_code_experiment(&self, id: &str) -> Result<Option<CodeExperimentRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, run_id, runtime, image, script, argv_json,
                    workspace_path, output_dir, network_mode, cpu_limit,
                    memory_limit_mb, timeout_seconds, status, exit_code,
                    stdout_tail, stderr_tail, result_summary, artifact_ids_json,
                    created_at, started_at, completed_at, error
             FROM code_experiments WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(CodeExperimentRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                run_id: row.get(2)?,
                runtime: row.get(3)?,
                image: row.get(4)?,
                script: row.get(5)?,
                argv_json: row.get(6)?,
                workspace_path: row.get(7)?,
                output_dir: row.get(8)?,
                network_mode: row.get(9)?,
                cpu_limit: row.get(10)?,
                memory_limit_mb: row.get::<_, i64>(11)? as u64,
                timeout_seconds: row.get::<_, i64>(12)? as u64,
                status: row.get(13)?,
                exit_code: row.get(14)?,
                stdout_tail: row.get(15)?,
                stderr_tail: row.get(16)?,
                result_summary: row.get(17)?,
                artifact_ids_json: row.get(18)?,
                created_at: row.get(19)?,
                started_at: row.get(20)?,
                completed_at: row.get(21)?,
                error: row.get(22)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_code_experiments(&self, limit: usize) -> Result<Vec<CodeExperimentRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, run_id, runtime, image, script, argv_json,
                    workspace_path, output_dir, network_mode, cpu_limit,
                    memory_limit_mb, timeout_seconds, status, exit_code,
                    stdout_tail, stderr_tail, result_summary, artifact_ids_json,
                    created_at, started_at, completed_at, error
             FROM code_experiments ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(CodeExperimentRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                run_id: row.get(2)?,
                runtime: row.get(3)?,
                image: row.get(4)?,
                script: row.get(5)?,
                argv_json: row.get(6)?,
                workspace_path: row.get(7)?,
                output_dir: row.get(8)?,
                network_mode: row.get(9)?,
                cpu_limit: row.get(10)?,
                memory_limit_mb: row.get::<_, i64>(11)? as u64,
                timeout_seconds: row.get::<_, i64>(12)? as u64,
                status: row.get(13)?,
                exit_code: row.get(14)?,
                stdout_tail: row.get(15)?,
                stderr_tail: row.get(16)?,
                result_summary: row.get(17)?,
                artifact_ids_json: row.get(18)?,
                created_at: row.get(19)?,
                started_at: row.get(20)?,
                completed_at: row.get(21)?,
                error: row.get(22)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn delete_code_experiment(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute("DELETE FROM code_experiments WHERE id = ?1", params![id])?;
        Ok(count > 0)
    }
}

/// Standalone path containment validator
pub fn check_path_authorized_in_folders(
    path: &std::path::Path,
    folders: &[AuthorizedFolderRecord],
) -> Result<std::path::PathBuf, String> {
    Database::check_path_authorized_in_folders(folders, path)
}

pub fn is_path_authorized(path: &std::path::Path, folders: &[AuthorizedFolderRecord]) -> bool {
    check_path_authorized_in_folders(path, folders).is_ok()
}

pub fn validate_authorized_path(
    db: &Database,
    path: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    db.validate_authorized_path(path)
}

/// Recursively canonicalize and compare JSON values for strict semantic equivalence,
/// ignoring key ordering in objects.
pub fn json_values_canonically_equal(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    fn canonicalize(val: &serde_json::Value) -> serde_json::Value {
        match val {
            serde_json::Value::Object(map) => {
                let mut btree = std::collections::BTreeMap::new();
                for (k, v) in map {
                    btree.insert(k.clone(), canonicalize(v));
                }
                serde_json::to_value(btree).unwrap_or(serde_json::Value::Null)
            }
            serde_json::Value::Array(arr) => {
                let canonical_arr: Vec<_> = arr.iter().map(canonicalize).collect();
                serde_json::Value::Array(canonical_arr)
            }
            other => other.clone(),
        }
    }
    canonicalize(a) == canonicalize(b)
}
