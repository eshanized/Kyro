//! Comprehensive integration tests for Kyro's three core capabilities:
//! 1. Local habit learning and routine suggestions
//! 2. Deeper local RAG over personal notes and codebases
//! 3. Sandboxed code-execution agents for safe experiments

use kyro_lib::agent::cancellation::CancellationToken;
use kyro_lib::sandbox::docker::{is_approved_sandbox_image, APPROVED_SANDBOX_IMAGES};
use kyro_lib::sandbox::experiment::{
    ExperimentRunner, ExperimentSpec, MockContainerRunner, ResourceLimits,
};
use kyro_lib::storage::chunking::{chunk_markdown, chunk_source_code};
use kyro_lib::storage::db::{
    AgentRun, AuthorizedFolderRecord, Database, DocumentChunkRecord, HabitSuggestionRecord, Message,
};
use kyro_lib::storage::retrieval::{
    validate_local_loopback_url, DeterministicTestEmbeddingProvider, EmbeddingProvider,
    RetrievalEngine,
};
use kyro_lib::teach_task::habit_learning::{contains_sensitive_info, HabitLearningEngine};
use std::path::{Path, PathBuf};
use std::sync::Arc;

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{}_{}", prefix, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn create_test_db() -> (Database, TestDir) {
    let td = TestDir::new("kyro_caps_test");
    let db = Database::init(td.path().to_path_buf()).expect("init database");
    (db, td)
}

// =========================================================================
// 1. Local Habit Learning and Routine Suggestions
// =========================================================================

#[test]
fn test_habit_learning_disabled_by_default() {
    let (db, _td) = create_test_db();
    let settings = db.get_habit_settings().expect("get settings");
    assert!(
        !settings.enabled,
        "Habit learning must be disabled by default"
    );

    let engine = HabitLearningEngine::new(db.clone());
    let suggestions = engine.analyze_recent_runs().expect("analyze");
    assert!(
        suggestions.is_empty(),
        "When habit learning is disabled, analysis must return 0 suggestions"
    );
}

#[test]
fn test_habit_learning_sensitive_data_filtering() {
    // Passwords, tokens, API keys, and private keys must be recognized as sensitive
    assert!(contains_sensitive_info(
        "export GITHUB_TOKEN=ghp_1234567890abcdef"
    ));
    assert!(contains_sensitive_info("password=super_secret_pass"));
    assert!(contains_sensitive_info(
        "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"
    ));
    assert!(contains_sensitive_info(
        "-----BEGIN OPENSSH PRIVATE KEY-----"
    ));
    assert!(contains_sensitive_info(
        "AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
    ));

    // Harmless operational commands must not be flagged
    assert!(!contains_sensitive_info("cargo test --workspace"));
    assert!(!contains_sensitive_info("git status --short"));
    assert!(!contains_sensitive_info("npm run build"));
}

#[test]
fn test_habit_learning_repetition_detection_and_safe_acceptance() {
    let (db, _td) = create_test_db();

    // Enable habit learning explicitly
    let mut settings = db.get_habit_settings().unwrap();
    settings.enabled = true;
    db.save_habit_settings(&settings).unwrap();

    // Seed conversation and 4 identical completed runs
    db.create_conversation("conv_habit", "Habit Test Conv", "test-model")
        .unwrap();

    let now = chrono::Utc::now().timestamp_millis();
    for i in 1..=4 {
        let run_id = format!("run_check_health_{}", i);
        let run = AgentRun {
            id: run_id.clone(),
            conversation_id: Some("conv_habit".to_string()),
            agent_id: "apex".to_string(),
            model: "test-model".to_string(),
            prompt: "Please run cargo check and cargo test on Kyro".to_string(),
            status: "succeeded".to_string(),
            created_at: now - (i * 3600 * 1000),
            started_at: Some(now - (i * 3600 * 1000)),
            completed_at: Some(now - (i * 3600 * 1000) + 5000),
            error: None,
            config_json: None,
            parent_run_id: None,
        };
        db.create_run(&run).unwrap();

        // Add assistant messages with tool calls
        let msg = Message {
            id: format!("msg_tool_{}", i),
            conversation_id: "conv_habit".to_string(),
            role: "assistant".to_string(),
            content: "Running test suite".to_string(),
            tool_calls_json: Some(
                serde_json::to_string(&vec![
                    serde_json::json!({
                        "id": format!("call_1_{}", i),
                        "name": "run_command",
                        "arguments": { "command": "cargo check" }
                    }),
                    serde_json::json!({
                        "id": format!("call_2_{}", i),
                        "name": "run_command",
                        "arguments": { "command": "cargo test" }
                    }),
                ])
                .unwrap(),
            ),
            observations_json: None,
            created_at: now - (i * 3600 * 1000) + 1000,
        };
        db.add_message(&msg).unwrap();
    }

    let engine = HabitLearningEngine::new(db.clone());
    let suggestions = engine.analyze_recent_runs().unwrap();
    assert_eq!(
        suggestions.len(),
        1,
        "Should produce 1 suggestion from repeated workflow"
    );

    let sug = &suggestions[0];
    assert_eq!(sug.status, "pending");
    assert!(sug.occurrences >= 3);
    assert!(sug.confidence >= 0.6);

    // Accept the suggestion into routines
    let routine = engine
        .accept_suggestion(
            &sug.id,
            Some("Automated Cargo Health Check".to_string()),
            None,
        )
        .unwrap();

    // Critical security invariant: Newly learned routine MUST remain disabled
    assert!(
        !routine.enabled,
        "Newly learned routine must be disabled by default"
    );
    assert_eq!(routine.name, "Automated Cargo Health Check");

    // Suggestion status must now be "accepted"
    let updated_sug = db.get_habit_suggestion(&sug.id).unwrap().unwrap();
    assert_eq!(updated_sug.status, "accepted");

    // Dismiss & snooze lifecycle test
    let test_sug_id = "sug_temp_test";
    let test_sug = HabitSuggestionRecord {
        id: test_sug_id.to_string(),
        name: "Temporary suggestion".to_string(),
        description: "Testing lifecycle transitions".to_string(),
        signature_hash: "sig_temp".to_string(),
        occurrences: 3,
        confidence: 0.8,
        proposed_action_type: "routine".to_string(),
        proposed_payload_json: "{}".to_string(),
        status: "pending".to_string(),
        evidence_json: "[]".to_string(),
        first_seen_at: now,
        last_seen_at: now,
        updated_at: now,
    };
    db.upsert_habit_suggestion(&test_sug).unwrap();

    engine.snooze_suggestion(test_sug_id).unwrap();
    assert_eq!(
        db.get_habit_suggestion(test_sug_id)
            .unwrap()
            .unwrap()
            .status,
        "snoozed"
    );

    engine.dismiss_suggestion(test_sug_id).unwrap();
    assert_eq!(
        db.get_habit_suggestion(test_sug_id)
            .unwrap()
            .unwrap()
            .status,
        "dismissed"
    );

    // Clear habit data
    engine.clear_data().unwrap();
    assert!(db.list_habit_suggestions(None).unwrap().is_empty());
}

// =========================================================================
// 2. Deeper Local RAG over Personal Notes and Codebases
// =========================================================================

#[test]
fn test_rag_markdown_chunking_with_breadcrumbs() {
    let md = r#"# Project Kyro
Kyro is an autonomous desktop agent.

## Core Capabilities
Kyro integrates several core features.

### Local Habit Learning
Recognizes repeated patterns deterministically without sending data to cloud.

### Sandboxed Experiments
Runs experiments inside isolated Docker/Podman containers.
"#;

    let chunks = chunk_markdown("/docs/kyro.md", md);
    assert!(
        chunks.len() >= 3,
        "Markdown should produce at least 3 chunks"
    );

    // First chunk should have top-level heading
    assert_eq!(chunks[0].heading.as_deref(), Some("Project Kyro"));
    assert_eq!(chunks[0].start_line, Some(1));

    // Nested heading breadcrumbs
    let last_chunk = chunks
        .iter()
        .find(|c| c.content.contains("Sandboxed Experiments"))
        .unwrap();
    assert!(
        last_chunk
            .heading
            .as_deref()
            .unwrap()
            .contains("Sandboxed Experiments"),
        "Heading breadcrumbs should include section title"
    );
}

#[test]
fn test_rag_source_code_chunking_symbol_boundaries() {
    let rust_code = r#"// Kyro Sandbox Runner
use std::path::PathBuf;

pub struct SandboxConfig {
    pub image: String,
    pub timeout_seconds: u64,
}

impl SandboxConfig {
    pub fn new(image: &str) -> Self {
        Self {
            image: image.to_string(),
            timeout_seconds: 60,
        }
    }
}

pub fn run_isolated_task(cfg: &SandboxConfig) -> Result<String, String> {
    println!("Running task in container {}", cfg.image);
    Ok("success".to_string())
}
"#;

    let chunks = chunk_source_code("/src/sandbox.rs", rust_code);
    assert!(
        chunks.len() >= 2,
        "Source code should split along struct/fn definitions"
    );

    let struct_chunk = chunks
        .iter()
        .find(|c| c.content.contains("pub struct SandboxConfig"));
    assert!(
        struct_chunk.is_some(),
        "Should extract SandboxConfig struct chunk"
    );
    assert!(struct_chunk
        .unwrap()
        .heading
        .as_deref()
        .unwrap()
        .contains("struct SandboxConfig"));
}

#[test]
fn test_rag_loopback_security_endpoint_validation() {
    // Loopback endpoints must succeed
    assert!(validate_local_loopback_url("http://127.0.0.1:11434/api/embeddings").is_ok());
    assert!(validate_local_loopback_url("http://localhost:8080/v1/embeddings").is_ok());
    assert!(validate_local_loopback_url("http://[::1]:11434/api/embeddings").is_ok());

    // Remote endpoints must be rejected to prevent silent cloud data leaks
    assert!(validate_local_loopback_url("https://api.openai.com/v1/embeddings").is_err());
    assert!(validate_local_loopback_url("http://192.168.1.100:11434/api/embeddings").is_err());
    assert!(validate_local_loopback_url("http://example.com/embed").is_err());
}

#[tokio::test]
async fn test_rag_authorized_folder_scoping_and_hybrid_retrieval() {
    let (db, td) = create_test_db();
    let auth_folder = td.path().join("authorized_project");
    std::fs::create_dir_all(&auth_folder).unwrap();

    // Authorize folder in DB
    db.add_authorized_folder(&AuthorizedFolderRecord {
        id: "folder_1".to_string(),
        path: auth_folder.to_string_lossy().to_string(),
        display_name: "Project Root".to_string(),
        added_at: chrono::Utc::now().timestamp(),
    })
    .unwrap();

    let doc_path = auth_folder.join("notes.md");
    let unauthorized_path = td.path().join("secret").join("passwords.txt");

    // Index documents first to satisfy foreign key constraint on document_chunks
    let doc_record_auth = kyro_lib::storage::db::DocumentIndexRecord {
        path: doc_path.to_string_lossy().to_string(),
        file_name: "notes.md".to_string(),
        format: "markdown".to_string(),
        size_bytes: 1024,
        modified_at: chrono::Utc::now().timestamp(),
        sha256: "hash123".to_string(),
        extracted_text: Some("Kyro uses local-first deterministic pattern extraction".to_string()),
        indexed_at: chrono::Utc::now().timestamp(),
    };
    db.upsert_indexed_document(&doc_record_auth).unwrap();

    let doc_record_unauth = kyro_lib::storage::db::DocumentIndexRecord {
        path: unauthorized_path.to_string_lossy().to_string(),
        file_name: "passwords.txt".to_string(),
        format: "text".to_string(),
        size_bytes: 512,
        modified_at: chrono::Utc::now().timestamp(),
        sha256: "hash999".to_string(),
        extracted_text: Some("Unauthorized secret file containing passwords".to_string()),
        indexed_at: chrono::Utc::now().timestamp(),
    };
    db.upsert_indexed_document(&doc_record_unauth).unwrap();

    // Insert chunks
    let chunk_auth = DocumentChunkRecord {
        id: "chunk_auth_1".to_string(),
        document_path: doc_path.to_string_lossy().to_string(),
        chunk_index: 0,
        heading: Some("Architecture Notes".to_string()),
        start_line: Some(1),
        end_line: Some(20),
        content: "Kyro uses local-first deterministic pattern extraction for habit learning and Docker sandboxing."
            .to_string(),
        content_hash: "hash123".to_string(),
        format: "markdown".to_string(),
        chunk_version: 1,
        indexed_at: chrono::Utc::now().timestamp(),
        embedding_json: Some(serde_json::to_string(&vec![0.5f32, 0.5, 0.5, 0.5]).unwrap()),
    };
    db.insert_document_chunks(&doc_path.to_string_lossy(), &[chunk_auth])
        .unwrap();

    let chunk_unauth = DocumentChunkRecord {
        id: "chunk_unauth_1".to_string(),
        document_path: unauthorized_path.to_string_lossy().to_string(),
        chunk_index: 0,
        heading: Some("Secrets".to_string()),
        start_line: Some(1),
        end_line: Some(5),
        content: "Unauthorized secret file containing passwords and credentials.".to_string(),
        content_hash: "hash999".to_string(),
        format: "text".to_string(),
        chunk_version: 1,
        indexed_at: chrono::Utc::now().timestamp(),
        embedding_json: Some(serde_json::to_string(&vec![0.5f32, 0.5, 0.5, 0.5]).unwrap()),
    };
    db.insert_document_chunks(&unauthorized_path.to_string_lossy(), &[chunk_unauth])
        .unwrap();

    let embedding_provider: Arc<dyn EmbeddingProvider> =
        Arc::new(DeterministicTestEmbeddingProvider::with_dimensions(4));
    let engine = RetrievalEngine::with_provider(db.clone(), embedding_provider);

    // Search query for "habit learning"
    let results = engine.search("habit learning", 5).await.unwrap();
    assert_eq!(results.len(), 1, "Should only return authorized chunks");
    assert_eq!(results[0].chunk_id, "chunk_auth_1");

    // Verify search context injection format
    let context_block = RetrievalEngine::format_context_block(&results);
    assert!(context_block.contains("Architecture Notes"));
    assert!(context_block.contains("Kyro uses local-first"));
    assert!(!context_block.contains("Unauthorized secret"));

    // Revoking / deleting document chunks
    db.delete_chunks_for_document(&doc_path.to_string_lossy())
        .unwrap();
    let remaining = db.list_all_chunks().unwrap();
    assert_eq!(remaining.len(), 1); // Only unauth remains
}

// =========================================================================
// 3. Sandboxed Code-Execution Agents for Safe Experiments
// =========================================================================

#[test]
fn test_experiment_image_allowlist_validation() {
    // Approved images with pinned tags
    for img in APPROVED_SANDBOX_IMAGES {
        assert!(
            is_approved_sandbox_image(img),
            "Approved image '{}' must be valid",
            img
        );
    }

    // Mutable tags and unapproved registries must be rejected
    assert!(!is_approved_sandbox_image("python:latest"));
    assert!(!is_approved_sandbox_image("node:latest"));
    assert!(!is_approved_sandbox_image("ubuntu:latest"));
    assert!(!is_approved_sandbox_image("ghcr.io/malicious/agent:v1"));
    assert!(!is_approved_sandbox_image("alpine:latest"));
    assert!(!is_approved_sandbox_image("debian:latest"));
}

#[tokio::test]
async fn test_experiment_mock_runner_execution_and_bounded_logs() {
    let (db, _td) = create_test_db();

    // Create a mock container runner simulating output
    let mock = MockContainerRunner::new().with_exit(
        0,
        "Calculation completed successfully\nResult: 42\n",
        "",
    );

    let runner = ExperimentRunner::with_runner(db.clone(), Arc::new(mock));

    let spec = ExperimentSpec {
        id: None,
        name: "Mock Python Benchmark".to_string(),
        runtime: "python".to_string(),
        custom_image: None,
        script: Some("print('Result: 42')".to_string()),
        argv: None,
        workspace_path: None,
        network_mode: Some("none".to_string()),
        limits: Some(ResourceLimits::default()),
    };

    let cancel = CancellationToken::new();
    let result = runner.execute(spec, None, cancel).await.unwrap();

    assert_eq!(result.status, "succeeded");
    assert_eq!(result.exit_code, Some(0));
    assert!(result.stdout_tail.unwrap().contains("Result: 42"));

    // Verify artifact creation in Kyro's database
    let artifacts = db.list_artifacts(None, None).unwrap();
    assert!(
        artifacts.iter().any(|a| a.name.contains("report")),
        "Experiment report should be registered as Kyro artifact"
    );
}

#[tokio::test]
async fn test_experiment_cancellation_handling() {
    let (db, _td) = create_test_db();

    let mock = MockContainerRunner::new().with_exit(137, "", "killed");
    let runner = ExperimentRunner::with_runner(db.clone(), Arc::new(mock));

    let spec = ExperimentSpec {
        id: None,
        name: "Cancelled Experiment".to_string(),
        runtime: "bash".to_string(),
        custom_image: None,
        script: Some("sleep 100".to_string()),
        argv: None,
        workspace_path: None,
        network_mode: Some("none".to_string()),
        limits: Some(ResourceLimits::default()),
    };

    let cancel = CancellationToken::new();
    cancel.cancel(); // Pre-cancelled token

    let result = runner.execute(spec, None, cancel).await.unwrap();
    assert_eq!(result.status, "cancelled");
}

#[test]
fn test_experiment_comparison_diff() {
    let (db, _td) = create_test_db();

    let exp_a = kyro_lib::storage::db::CodeExperimentRecord {
        id: "exp_a".to_string(),
        name: "Run Baseline".to_string(),
        run_id: None,
        runtime: "python".to_string(),
        image: "python:3.11-slim".to_string(),
        script: Some("print('A')".to_string()),
        argv_json: "[]".to_string(),
        workspace_path: None,
        output_dir: None,
        network_mode: "none".to_string(),
        cpu_limit: 1.0,
        memory_limit_mb: 512,
        timeout_seconds: 60,
        status: "succeeded".to_string(),
        exit_code: Some(0),
        stdout_tail: Some("Baseline output\n".to_string()),
        stderr_tail: None,
        result_summary: Some("Baseline finished in 1.2s".to_string()),
        artifact_ids_json: None,
        created_at: 1000,
        started_at: Some(1001),
        completed_at: Some(1003),
        error: None,
    };
    db.create_code_experiment(&exp_a).unwrap();

    let exp_b = kyro_lib::storage::db::CodeExperimentRecord {
        id: "exp_b".to_string(),
        name: "Run Optimized".to_string(),
        run_id: None,
        runtime: "python".to_string(),
        image: "python:3.11-slim".to_string(),
        script: Some("print('B')".to_string()),
        argv_json: "[]".to_string(),
        workspace_path: None,
        output_dir: None,
        network_mode: "none".to_string(),
        cpu_limit: 1.0,
        memory_limit_mb: 512,
        timeout_seconds: 60,
        status: "succeeded".to_string(),
        exit_code: Some(0),
        stdout_tail: Some("Optimized output\n".to_string()),
        stderr_tail: None,
        result_summary: Some("Optimized finished in 0.4s".to_string()),
        artifact_ids_json: None,
        created_at: 2000,
        started_at: Some(2001),
        completed_at: Some(2002),
        error: None,
    };
    db.create_code_experiment(&exp_b).unwrap();

    let cmp = ExperimentRunner::compare(&exp_a, &exp_b);
    assert!(cmp.exit_codes_match);
    assert!(!cmp.stdout_match); // outputs differ
    assert!(cmp.summary_diff.contains("Baseline finished"));
    assert!(cmp.summary_diff.contains("Optimized finished"));
}
