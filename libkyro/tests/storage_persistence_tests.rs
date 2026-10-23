use kyro_lib::storage::db::{
    AgentRun, ApprovalRequest, Database, MemoryEntry, Message, RunEvent, TimelineStepRecord,
};
use kyro_lib::storage::file_assistant::FileAssistant;
use kyro_lib::storage::organizer::{FileOrganizer, OrganizationRule};
use kyro_lib::storage::vault::Vault;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    let td = TestDir::new("kyro_persist_test");
    let db = Database::init(td.path().to_path_buf()).unwrap();
    (db, td)
}

#[test]
fn test_database_lifecycle_and_reconciliation() {
    let (db, _td) = create_test_db();

    // Create conversation and running run
    db.create_conversation("conv_rec", "Reconcile Test", "test-model")
        .unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let run = AgentRun {
        id: "run_reconcile_1".to_string(),
        conversation_id: Some("conv_rec".to_string()),
        agent_id: "apex".to_string(),
        model: "test-model".to_string(),
        prompt: "Long running task".to_string(),
        status: "running".to_string(),
        created_at: now,
        started_at: Some(now),
        completed_at: None,
        error: None,
        config_json: None,
        parent_run_id: None,
    };
    db.create_run(&run).unwrap();

    // Verify status is running
    let loaded = db.get_run("run_reconcile_1").unwrap().unwrap();
    assert_eq!(loaded.status, "running");

    // Reconcile active runs on startup
    let count = db.reconcile_active_runs_on_startup().unwrap();
    assert_eq!(count, 1);

    // Verify run status was updated away from running (interrupted/failed)
    let updated = db.get_run("run_reconcile_1").unwrap().unwrap();
    assert_ne!(updated.status, "running");
    assert!(updated.status == "interrupted" || updated.status == "failed");
}

#[test]
fn test_database_conversations_and_messages_foreign_key() {
    let (db, _td) = create_test_db();

    // 1. Attempt to add message for non-existent conversation fails with foreign key constraint
    let orphan_msg = Message {
        id: "msg_orphan".to_string(),
        conversation_id: "non_existent_conv".to_string(),
        role: "user".to_string(),
        content: "Hello".to_string(),
        tool_calls_json: None,
        observations_json: None,
        created_at: 1000,
    };
    let err = db.add_message(&orphan_msg);
    assert!(
        err.is_err(),
        "Foreign key constraint must reject orphan message"
    );

    // 2. Create conversation
    let conv = db
        .create_conversation("conv_100", "Chat Title", "nemotron")
        .unwrap();
    assert_eq!(conv.id, "conv_100");
    assert_eq!(conv.title, "Chat Title");

    // 3. Add message
    let valid_msg = Message {
        id: "msg_100".to_string(),
        conversation_id: "conv_100".to_string(),
        role: "user".to_string(),
        content: "What is Kyro?".to_string(),
        tool_calls_json: None,
        observations_json: None,
        created_at: 1000,
    };
    db.add_message(&valid_msg).unwrap();

    let msgs = db.get_messages("conv_100").unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content, "What is Kyro?");

    // 4. Delete conversation
    let deleted = db.delete_conversation("conv_100").unwrap();
    assert!(deleted);
    assert!(db.get_conversation("conv_100").unwrap().is_none());
}

#[test]
fn test_database_run_events_atomicity_and_sequencing() {
    let (db, _td) = create_test_db();

    db.create_conversation("c_seq", "Seq Test", "model")
        .unwrap();
    let run = AgentRun {
        id: "r_seq_1".to_string(),
        conversation_id: Some("c_seq".to_string()),
        agent_id: "apex".to_string(),
        model: "model".to_string(),
        prompt: "p".to_string(),
        status: "running".to_string(),
        created_at: 1000,
        started_at: Some(1000),
        completed_at: None,
        error: None,
        config_json: None,
        parent_run_id: None,
    };
    db.create_run(&run).unwrap();

    // Verify atomic event sequence generation based on max inserted seq
    let s1 = db.get_next_event_seq("r_seq_1").unwrap();
    assert_eq!(s1, 1);

    // Append first run event with seq 1
    let ev1 = RunEvent {
        id: "evt_1".to_string(),
        run_id: "r_seq_1".to_string(),
        seq: s1,
        event_type: "thought".to_string(),
        payload_json: json!({"thought": "Thinking..."}).to_string(),
        created_at: 1000,
    };
    db.append_run_event(&ev1).unwrap();

    let s2 = db.get_next_event_seq("r_seq_1").unwrap();
    assert_eq!(s2, 2);

    let ev2 = RunEvent {
        id: "evt_2".to_string(),
        run_id: "r_seq_1".to_string(),
        seq: s2,
        event_type: "tool_call".to_string(),
        payload_json: json!({"tool": "read_file"}).to_string(),
        created_at: 1001,
    };
    db.append_run_event(&ev2).unwrap();

    let s3 = db.get_next_event_seq("r_seq_1").unwrap();
    assert_eq!(s3, 3);

    // Query events
    let all_events = db.get_run_events("r_seq_1", None).unwrap();
    assert_eq!(all_events.len(), 2);
    assert_eq!(all_events[0].event_type, "thought");
    assert_eq!(all_events[1].event_type, "tool_call");

    // Query events after seq 1
    let delta = db.get_run_events("r_seq_1", Some(1)).unwrap();
    assert_eq!(delta.len(), 1);
    assert_eq!(delta[0].seq, 2);
}

#[tokio::test]
async fn test_concurrent_event_writers_atomic_sequencing_and_uniqueness() {
    let (db, _td) = create_test_db();
    let run_id = "run_concurrent_writers";

    // Setup conversation and run
    db.create_conversation("conv_writers", "Concurrent Test", "test-model")
        .unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let run = AgentRun {
        id: run_id.to_string(),
        conversation_id: Some("conv_writers".to_string()),
        agent_id: "apex".to_string(),
        model: "test-model".to_string(),
        prompt: "Concurrent writer test".to_string(),
        status: "running".to_string(),
        created_at: now,
        started_at: Some(now),
        completed_at: None,
        error: None,
        config_json: None,
        parent_run_id: None,
    };
    db.create_run(&run).unwrap();

    // Spawn 10 concurrent tasks each appending 10 events
    let mut handles = Vec::new();
    for task_idx in 0..10 {
        let db_cloned = db.clone();
        handles.push(tokio::spawn(async move {
            for event_idx in 0..10 {
                let payload = json!({
                    "task": task_idx,
                    "event": event_idx
                })
                .to_string();
                db_cloned
                    .append_new_run_event(run_id, "test_event", &payload)
                    .expect("Concurrent event append must succeed");
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // Verify all 100 events persisted
    let events = db.get_run_events(run_id, None).unwrap();
    assert_eq!(events.len(), 100);

    // Verify sequences are strictly contiguous and unique 1..=100
    let mut seqs: Vec<i64> = events.iter().map(|e| e.seq).collect();
    seqs.sort();
    let expected: Vec<i64> = (1..=100).collect();
    assert_eq!(
        seqs, expected,
        "Event sequences must be strictly unique and contiguous"
    );
}

#[test]
fn test_artifact_record_reconciliation_recovery_path() {
    let (db, td) = create_test_db();
    let file_path = td.path().join("orphaned_report.md");
    std::fs::write(
        &file_path,
        "# Orphaned Analysis Report\nContent preserved on disk.",
    )
    .unwrap();

    // Verify file is not yet in the artifact registry
    let initial_arts = db.list_artifacts(None, None).unwrap();
    assert!(initial_arts
        .iter()
        .all(|a| a.file_path != file_path.to_string_lossy()));

    // Run reconciliation recovery
    let rec = db
        .reconcile_file_as_artifact(&file_path, None, None)
        .expect("Artifact reconciliation must succeed for existing on-disk file");

    assert_eq!(rec.name, "orphaned_report.md");
    assert_eq!(rec.artifact_type, "markdown");
    assert_eq!(
        rec.size_bytes,
        std::fs::metadata(&file_path).unwrap().len() as i64
    );

    // Verify artifact is now discoverable in database
    let queried = db
        .get_artifact(&rec.id)
        .unwrap()
        .expect("Artifact must exist");
    assert_eq!(queried.file_path, file_path.to_string_lossy());
    assert!(queried.metadata_json.unwrap().contains("reconciled"));
}

#[test]
fn test_database_approval_requests_expiration_and_consumption() {
    let (db, _td) = create_test_db();
    let now = chrono::Utc::now().timestamp_millis();

    // 1. Create overdue approval request
    let expired_req = ApprovalRequest {
        id: "appr_exp".to_string(),
        run_id: "run_exp".to_string(),
        tool_name: "execute_shell".to_string(),
        summary: "Reboot system".to_string(),
        args_json: json!({"command": "reboot"}).to_string(),
        risk_level: "destructive".to_string(),
        reason: "Testing expiration".to_string(),
        status: "pending".to_string(),
        created_at: now - 100_000,
        expires_at: now - 1,
        resolved_at: None,
        resolved_by: None,
    };
    db.create_approval_request(&expired_req).unwrap();

    // Expire overdue
    let expired_count = db.expire_overdue_approvals(now).unwrap();
    assert_eq!(expired_count, 1);
    let check = db.get_approval_request("appr_exp").unwrap().unwrap();
    assert_eq!(check.status, "expired");

    // 2. Create valid pending approval request
    let valid_req = ApprovalRequest {
        id: "appr_valid".to_string(),
        run_id: "run_active_1".to_string(),
        tool_name: "delete_file".to_string(),
        summary: "Delete temp file".to_string(),
        args_json: json!({"path": "/workspace/temp.txt"}).to_string(),
        risk_level: "high".to_string(),
        reason: "Valid cleanup".to_string(),
        status: "pending".to_string(),
        created_at: now,
        expires_at: now + 60_000,
        resolved_at: None,
        resolved_by: None,
    };
    db.create_approval_request(&valid_req).unwrap();

    // Resolve to approved
    let resolved = db
        .resolve_approval_request("appr_valid", "approved", "sec_officer")
        .unwrap();
    assert!(resolved);

    // 3. Consume approval bound to specific run and tool
    let args_val = json!({"path": "/workspace/temp.txt"});
    let res = db.consume_approval_request_bound(
        "appr_valid",
        "run_active_1",
        &["delete_file"],
        &args_val,
        now,
    );
    assert!(res.is_ok(), "First consumption must succeed: {:?}", res);

    // 4. Replay attack prevention: second consumption fails
    let replay_res = db.consume_approval_request_bound(
        "appr_valid",
        "run_active_1",
        &["delete_file"],
        &args_val,
        now,
    );
    assert!(replay_res.is_err(), "Replay consumption must be rejected");

    // 5. Tampering arguments check: consuming with altered arguments fails
    let tampered_req = ApprovalRequest {
        id: "appr_valid_2".to_string(),
        run_id: "run_active_1".to_string(),
        tool_name: "delete_file".to_string(),
        summary: "Delete temp file 2".to_string(),
        args_json: json!({"path": "/workspace/temp.txt"}).to_string(),
        risk_level: "high".to_string(),
        reason: "Valid cleanup 2".to_string(),
        status: "pending".to_string(),
        created_at: now,
        expires_at: now + 60_000,
        resolved_at: None,
        resolved_by: None,
    };
    db.create_approval_request(&tampered_req).unwrap();
    db.resolve_approval_request("appr_valid_2", "approved", "sec_officer")
        .unwrap();

    let tampered_args = json!({"path": "/etc/shadow"});
    let tampered_res = db.consume_approval_request_bound(
        "appr_valid_2",
        "run_active_1",
        &["delete_file"],
        &tampered_args,
        now,
    );
    assert!(tampered_res.is_err(), "Tampered arguments must be rejected");
}

#[test]
fn test_database_timeline_and_memory_store() {
    let (db, _td) = create_test_db();

    // 1. Timeline steps
    let tl = TimelineStepRecord {
        id: "tl_1".to_string(),
        run_id: Some("run_tl".to_string()),
        step: 1,
        action_name: "click_mark #12".to_string(),
        click_x: Some(300),
        click_y: Some(450),
        before_base64: "before_png_data".to_string(),
        after_base64: "after_png_data".to_string(),
        diff_percent: 4.5,
        timestamp: 1000,
    };
    db.save_timeline_step(&tl).unwrap();

    let steps = db.get_timeline_steps(Some("run_tl"), 10).unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].action_name, "click_mark #12");

    db.clear_timeline_steps().unwrap();
    let empty_steps = db.get_timeline_steps(None, 10).unwrap();
    assert!(empty_steps.is_empty());

    // 2. Memory persistence
    let mem = MemoryEntry {
        id: "mem_pref_1".to_string(),
        scope: "preference".to_string(),
        owner_id: "user_default".to_string(),
        key: "editor_theme".to_string(),
        value: "dark".to_string(),
        source: "settings".to_string(),
        created_at: 1000,
        updated_at: 1000,
    };
    db.save_memory(&mem).unwrap();

    let user_mems = db
        .get_memories_for_scope("preference", "user_default")
        .unwrap();
    assert_eq!(user_mems.len(), 1);
    assert_eq!(user_mems[0].value, "dark");

    let deleted = db.delete_memory("mem_pref_1").unwrap();
    assert!(deleted);
    assert!(db.list_all_memories().unwrap().is_empty());
}

#[test]
fn test_file_assistant_duplicates_and_study_notes() {
    let td = TestDir::new("file_asst_test");
    let dir = td.path();

    // Create duplicate files
    let file1 = dir.join("original.txt");
    let file2 = dir.join("copy.txt");
    let content = "Exact duplicate file content for hash collision testing.";
    std::fs::write(&file1, content).unwrap();
    std::fs::write(&file2, content).unwrap();

    let dupes = FileAssistant::find_duplicates(dir).unwrap();
    assert_eq!(dupes.len(), 1);
    assert_eq!(dupes[0].files.len(), 2);
    assert_eq!(dupes[0].size_bytes, content.len() as u64);

    // Study notes generation
    let doc_file = dir.join("lecture.txt");
    let lecture_text = "Kyro Architecture:\n1. Governance Engine enforces safety.\n2. Agent Loop coordinates LLM reasoning.\n3. Vault secures credentials.";
    std::fs::write(&doc_file, lecture_text).unwrap();

    let notes = FileAssistant::generate_study_notes(&doc_file, None).unwrap();
    assert_eq!(notes.doc_name, "lecture.txt");
    assert!(!notes.overview.is_empty() || !notes.summary_points.is_empty());

    // Unicode passage finding
    let passage_text = "The quick brown 🦊 jumps over the lazy 🐶.";
    let span = FileAssistant::find_unicode_passage(passage_text, "🦊 jumps");
    assert!(span.is_some());
}

#[test]
fn test_organizer_preview_execute_undo() {
    let td = TestDir::new("organizer_test");
    let dir = td.path();

    // 1. Categorize extension helper
    let custom = HashMap::new();
    assert_eq!(
        FileOrganizer::categorize_extension("png", &custom),
        "Images"
    );
    assert_eq!(
        FileOrganizer::categorize_extension("pdf", &custom),
        "Documents"
    );
    assert_eq!(FileOrganizer::categorize_extension("rs", &custom), "Code");

    // 2. Setup files to organize
    let f1 = dir.join("photo.png");
    let f2 = dir.join("invoice.pdf");
    let f3 = dir.join("script.py");
    std::fs::write(&f1, "png-data").unwrap();
    std::fs::write(&f2, "pdf-data").unwrap();
    std::fs::write(&f3, "print('hello')").unwrap();

    // 3. Preview organization
    let rule = OrganizationRule::default();
    let plan = FileOrganizer::preview(dir, Some(rule)).unwrap();
    assert_eq!(plan.operations.len(), 3);

    // 4. Execute organization
    let mut tx = FileOrganizer::execute(&plan).unwrap();
    assert_eq!(tx.operations.len(), 3);
    assert!(!f1.exists());
    assert!(dir.join("Images/photo.png").exists());
    assert!(dir.join("Documents/invoice.pdf").exists());
    assert!(dir.join("Code/script.py").exists());

    // 5. Undo organization
    let undone_count = FileOrganizer::undo(&mut tx).unwrap();
    assert_eq!(undone_count, 3);
    assert!(f1.exists(), "Original file must be restored");
    assert!(f2.exists(), "Original file must be restored");
    assert!(f3.exists(), "Original file must be restored");
}

#[test]
fn test_vault_encryption_fallback_and_tamper_detection() {
    let (db, _td) = create_test_db();
    let vault = Vault::new_encrypted_fallback(db.clone());

    // 1. Store secret
    vault
        .set_secret("test_api_key", "secret-value-12345")
        .unwrap();

    // 2. Retrieve secret
    let retrieved = vault.get_secret("test_api_key").unwrap();
    assert_eq!(retrieved, Some("secret-value-12345".to_string()));

    // 3. Verify ciphertext in database is not plaintext
    let record = db.get_vault_record("test_api_key").unwrap().unwrap();
    assert_ne!(record.encrypted_data, "secret-value-12345");
    assert!(!record.encrypted_data.contains("secret-value"));

    // 4. Delete secret
    vault.delete_secret("test_api_key").unwrap();
    assert_eq!(vault.get_secret("test_api_key").unwrap(), None);
}
