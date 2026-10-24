//! Tests for Everyday Assistant features:
//! - Authorized folders and path containment
//! - Safe file operations and duplicate detection
//! - Google Gmail & Calendar adapter tools & risk classification
//! - Routine execution history and natural language routine authoring
//! - Today dashboard aggregation

use kyro_lib::commands::natural_language_to_routine;
use kyro_lib::connector::manager::ConnectorManager;
use kyro_lib::connector::native::google::GoogleAdapter;
use kyro_lib::connector::NativeConnectorAdapter;
use kyro_lib::execution::permissions::RiskLevel;
use kyro_lib::storage::file_assistant::FileAssistant;
use kyro_lib::storage::organizer::{
    FileOrganizer, OrganizationOpRecord, OrganizationPlan, OrganizationTransaction, PlannedFileOp,
};
use kyro_lib::storage::{
    AuthorizedFolderRecord, ConnectedAccountRecord, Database, DocumentIndexRecord, RoutineRecord,
    RoutineRunRecord, Vault,
};
use std::fs;
use std::path::{Path, PathBuf};

fn temp_db(tag: &str) -> (PathBuf, Database, Vault) {
    let dir = std::env::temp_dir().join(format!("kyro_everyday_{}_{}", tag, uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let db = Database::init(dir.clone()).expect("database init");
    let vault = Vault::new(db.clone());
    (dir, db, vault)
}

#[test]
fn test_authorized_folders_db_lifecycle() {
    let (dir, db, _vault) = temp_db("auth_folders");

    // Initially empty
    let list = db.list_authorized_folders().unwrap();
    assert!(list.is_empty());

    let folder1 = AuthorizedFolderRecord {
        id: "auth_1".to_string(),
        path: dir.to_string_lossy().to_string(),
        display_name: "Test Dir".to_string(),
        added_at: 1000,
    };
    db.add_authorized_folder(&folder1).unwrap();

    let list_after = db.list_authorized_folders().unwrap();
    assert_eq!(list_after.len(), 1);
    assert_eq!(list_after[0].display_name, "Test Dir");

    // Check path authorization
    let subfile = dir.join("nested/doc.txt");
    assert!(db.is_path_authorized(&subfile).unwrap());

    let outside = Path::new("/etc/shadow");
    assert!(!db.is_path_authorized(outside).unwrap());

    // Removal
    let removed = db.remove_authorized_folder("auth_1").unwrap();
    assert!(removed);
    assert!(db.list_authorized_folders().unwrap().is_empty());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_routine_runs_db_lifecycle() {
    let (dir, db, _vault) = temp_db("routine_runs");

    // Create routine
    let routine = RoutineRecord {
        id: "rt_1".to_string(),
        name: "Morning Briefing".to_string(),
        cron_expression: "0 9 * * *".to_string(),
        action_type: "prompt".to_string(),
        target_id: "Summarize calendar and unread emails".to_string(),
        enabled: true,
        last_run: None,
        next_run: None,
    };
    db.create_routine(&routine).unwrap();

    // Record run
    let run = RoutineRunRecord {
        id: "run_1".to_string(),
        routine_id: "rt_1".to_string(),
        routine_name: "Morning Briefing".to_string(),
        status: "running".to_string(),
        started_at: 1000,
        completed_at: None,
        summary: None,
        error: None,
    };
    db.record_routine_run(&run).unwrap();

    let runs = db.list_routine_runs(10).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, "running");

    // Update run
    db.update_routine_run("run_1", "succeeded", 2000, Some("All clear"), None)
        .unwrap();

    let updated_runs = db.list_runs_for_routine("rt_1", 10).unwrap();
    assert_eq!(updated_runs.len(), 1);
    assert_eq!(updated_runs[0].status, "succeeded");
    assert_eq!(updated_runs[0].summary.as_deref(), Some("All clear"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_google_adapter_tools_and_risk_boundaries() {
    let adapter = GoogleAdapter::new();
    assert_eq!(adapter.provider_id(), "google");

    let account = ConnectedAccountRecord {
        id: "conn_google_test".to_string(),
        connector_id: "google".to_string(),
        account_identifier: "user@gmail.com".to_string(),
        display_name: "Google (User)".to_string(),
        status: "connected".to_string(),
        status_message: None,
        granted_scopes_json: "[]".to_string(),
        credential_key: "cred_test".to_string(),
        config_json: "{}".to_string(),
        created_at: 0,
        updated_at: 0,
        last_connected_at: None,
        last_error: None,
        expires_at: None,
    };

    let tools = adapter.list_tools(&account);
    assert!(tools.len() >= 7);

    // Read-only tools
    let list_msgs = tools
        .iter()
        .find(|t| t.name == "gmail_list_messages")
        .unwrap();
    assert!(list_msgs.is_read_only);
    assert_eq!(list_msgs.risk_level, RiskLevel::Safe);

    let get_msg = tools
        .iter()
        .find(|t| t.name == "gmail_get_message")
        .unwrap();
    assert!(get_msg.is_read_only);
    assert_eq!(get_msg.risk_level, RiskLevel::Safe);

    let list_events = tools
        .iter()
        .find(|t| t.name == "calendar_list_events")
        .unwrap();
    assert!(list_events.is_read_only);
    assert_eq!(list_events.risk_level, RiskLevel::Safe);

    let find_times = tools
        .iter()
        .find(|t| t.name == "calendar_find_meeting_times")
        .unwrap();
    assert!(find_times.is_read_only);
    assert_eq!(find_times.risk_level, RiskLevel::Safe);

    // Consequential / write tools
    let create_draft = tools
        .iter()
        .find(|t| t.name == "gmail_create_draft")
        .unwrap();
    assert!(!create_draft.is_read_only);
    assert_eq!(create_draft.risk_level, RiskLevel::Moderate);

    let send_email = tools.iter().find(|t| t.name == "gmail_send_email").unwrap();
    assert!(!send_email.is_read_only);
    assert_eq!(send_email.risk_level, RiskLevel::Destructive);

    let create_event = tools
        .iter()
        .find(|t| t.name == "calendar_create_event")
        .unwrap();
    assert!(!create_event.is_read_only);
    assert_eq!(create_event.risk_level, RiskLevel::Moderate);
}

#[test]
fn test_file_assistant_study_notes_and_explanation() {
    let dir = std::env::temp_dir().join(format!("kyro_notes_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();

    let doc = dir.join("syllabus.txt");
    fs::write(
        &doc,
        "COURSE SYLLABUS: Computer Science 101\n\
         Instructor: Prof. Smith\n\
         Final Exam Date: 2026-12-18\n\
         Homework #1 Deadline: 2026-10-30\n\
         Students must submit assignments via portal before midnight.\n\
         Grading Policy: 40% assignments, 60% exams.",
    )
    .unwrap();

    let notes = FileAssistant::generate_study_notes(&doc, None).unwrap();
    assert_eq!(notes.doc_name, "syllabus.txt");
    assert!(!notes.revision_flashcards.is_empty());
    assert!(notes
        .key_dates_and_milestones
        .iter()
        .any(|d| d.contains("2026-12-18")));

    let explanation =
        FileAssistant::explain_passage(&doc, "Grading Policy: 40% assignments, 60% exams.")
            .unwrap();
    assert!(explanation.contains("Plain Language Explanation"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_fail_closed_authorized_folders_and_symlink_escape() {
    let (temp_dir, db, _vault) = temp_db("fail_closed_symlink");

    let allowed_dir = temp_dir.join("allowed_zone");
    let forbidden_dir = temp_dir.join("forbidden_zone");
    let spoof_prefix_dir = temp_dir.join("allowed_zone_spoofed");
    fs::create_dir_all(&allowed_dir).unwrap();
    fs::create_dir_all(&forbidden_dir).unwrap();
    fs::create_dir_all(&spoof_prefix_dir).unwrap();

    let secret_file = forbidden_dir.join("secret_credentials.txt");
    fs::write(&secret_file, "API_KEY=supersecret").unwrap();

    let normal_file = allowed_dir.join("project_plan.txt");
    fs::write(&normal_file, "Kyro project plan").unwrap();

    let spoof_file = spoof_prefix_dir.join("spoof.txt");
    fs::write(&spoof_file, "malicious prefix file").unwrap();

    // 1. Fail closed when NO folders are authorized
    assert!(db.list_authorized_folders().unwrap().is_empty());
    let err_empty = db.validate_authorized_path(&normal_file);
    assert!(
        err_empty.is_err(),
        "Must fail closed when no folders authorized"
    );
    assert!(err_empty
        .unwrap_err()
        .contains("No authorized folders configured"));

    // 2. Authorize ONLY allowed_dir
    let auth_record = AuthorizedFolderRecord {
        id: "auth_allowed".to_string(),
        path: allowed_dir.to_string_lossy().to_string(),
        display_name: "Allowed Zone".to_string(),
        added_at: 1000,
    };
    db.add_authorized_folder(&auth_record).unwrap();

    // Normal file in authorized folder succeeds
    let valid_path = db.validate_authorized_path(&normal_file);
    assert!(valid_path.is_ok());
    assert_eq!(
        valid_path.unwrap(),
        std::fs::canonicalize(&normal_file).unwrap()
    );

    // File in forbidden folder fails closed
    let err_forbidden = db.validate_authorized_path(&secret_file);
    assert!(err_forbidden.is_err());
    assert!(err_forbidden
        .unwrap_err()
        .contains("outside authorized folder boundaries"));

    // String prefix spoof directory fails closed (component boundary enforcement)
    let err_spoof = db.validate_authorized_path(&spoof_file);
    assert!(err_spoof.is_err());
    assert!(err_spoof
        .unwrap_err()
        .contains("outside authorized folder boundaries"));

    // Path traversal attempt (allowed_dir/../forbidden_zone/secret_credentials.txt)
    let traversal_path = allowed_dir.join("../forbidden_zone/secret_credentials.txt");
    let err_traversal = db.validate_authorized_path(&traversal_path);
    assert!(err_traversal.is_err());
    assert!(err_traversal
        .unwrap_err()
        .contains("outside authorized folder boundaries"));

    // Symlink escape attempt: symlink inside allowed pointing to forbidden
    #[cfg(unix)]
    {
        let symlink_path = allowed_dir.join("escaped_symlink");
        std::os::unix::fs::symlink(&forbidden_dir, &symlink_path).unwrap();
        let target_through_symlink = symlink_path.join("secret_credentials.txt");
        let err_symlink = db.validate_authorized_path(&target_through_symlink);
        assert!(
            err_symlink.is_err(),
            "Symlink escape out of authorized root must fail closed"
        );
        assert!(err_symlink
            .unwrap_err()
            .contains("outside authorized folder boundaries"));
    }

    // Non-existent target inside allowed directory (e.g. creating a destination file)
    let non_existent_dest = allowed_dir.join("new_output_dir/summary.pdf");
    let valid_dest = db.validate_authorized_path(&non_existent_dest);
    assert!(
        valid_dest.is_ok(),
        "Non-existent path inside authorized directory should validate successfully"
    );

    fs::remove_dir_all(&temp_dir).ok();
}

#[test]
fn test_natural_language_date_and_format_parser() {
    let now = chrono::Utc::now().timestamp_millis();

    // Test relative date parsing: "from last month in pdf format"
    let parsed_pdf =
        FileAssistant::parse_natural_language_query("find receipts from last month in pdf format");
    assert_eq!(parsed_pdf.format_hint.as_deref(), Some("pdf"));
    assert!(parsed_pdf.date_start.is_some());
    assert!(parsed_pdf.date_end.is_some());
    assert!(parsed_pdf.date_start.unwrap() < parsed_pdf.date_end.unwrap());
    assert!(parsed_pdf.date_end.unwrap() < now);
    assert!(parsed_pdf.query_tokens.contains(&"receipts".to_string()));
    assert!(!parsed_pdf.query_tokens.contains(&"from".to_string()));

    // Test "yesterday"
    let parsed_yesterday =
        FileAssistant::parse_natural_language_query("notes from yesterday in markdown");
    assert_eq!(parsed_yesterday.format_hint.as_deref(), Some("md"));
    assert!(parsed_yesterday.date_start.is_some());
    assert!(parsed_yesterday.date_end.is_some());
    assert!(parsed_yesterday.date_start.unwrap() < parsed_yesterday.date_end.unwrap());
    assert!(parsed_yesterday.date_end.unwrap() < now);

    // Test "docx"
    let parsed_docx = FileAssistant::parse_natural_language_query("contract in word document");
    assert_eq!(parsed_docx.format_hint.as_deref(), Some("docx"));
}

#[test]
fn test_facts_extraction_from_document() {
    let text = "INVOICE DETAILS\n\
                Invoice Number: INV-2026-90412\n\
                Date: 2026-10-15\n\
                Due Date: 2026-11-15\n\
                Total Amount Due: $4,250.75\n\
                Payment received: €500.00 discount applied.\n\
                PO Reference: PO-88319.";

    let facts = FileAssistant::extract_facts_from_text(text);
    assert!(facts
        .iter()
        .any(|(k, v)| k.contains("Amount") && v.contains("4,250.75")));
    assert!(facts
        .iter()
        .any(|(k, v)| k.contains("Due Date") && v.contains("2026-11-15")));
    assert!(facts.iter().any(|(k, v)| k.contains("Reference")
        && (v.contains("INV-2026-90412") || v.contains("PO-88319"))));
}

#[test]
fn test_grounded_passage_explanation_and_filtered_study_notes() {
    let dir = std::env::temp_dir().join(format!("kyro_notes_filtered_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();

    let doc = dir.join("physics.txt");
    fs::write(
        &doc,
        "PHYSICS 201: THERMODYNAMICS\n\
         First Law of Thermodynamics: Energy cannot be created or destroyed, only transformed.\n\
         Second Law: The entropy of any isolated system always increases.\n\
         Exam Schedule: 2026-12-10.\n\
         Calculus prerequisite required.",
    )
    .unwrap();

    // Grounding check: passage not in document must fail
    let err_ungrounded =
        FileAssistant::explain_passage(&doc, "Quantum mechanics is about wave functions.");
    assert!(
        err_ungrounded.is_err(),
        "Must reject explanation for passage absent from document"
    );
    assert!(err_ungrounded
        .unwrap_err()
        .contains("Passage not found in document"));

    // Grounded passage succeeds
    let grounded_ok = FileAssistant::explain_passage(
        &doc,
        "First Law of Thermodynamics: Energy cannot be created or destroyed",
    );
    assert!(grounded_ok.is_ok());
    let expl = grounded_ok.unwrap();
    assert!(expl.contains("Plain Language Explanation"));
    assert!(expl.contains("Energy cannot be created or destroyed"));

    // Study notes with topic filter
    let notes_all = FileAssistant::generate_study_notes(&doc, None).unwrap();
    assert!(!notes_all.revision_flashcards.is_empty());

    let notes_filtered = FileAssistant::generate_study_notes(&doc, Some("Entropy")).unwrap();
    for card in &notes_filtered.revision_flashcards {
        assert!(
            card.question.to_lowercase().contains("entropy")
                || card.answer.to_lowercase().contains("entropy")
        );
    }

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_document_index_caching_and_queries() {
    let (dir, db, _vault) = temp_db("doc_index");

    let invoices_dir = dir.join("invoices");
    let notes_dir = dir.join("notes");
    fs::create_dir_all(&invoices_dir).unwrap();
    fs::create_dir_all(&notes_dir).unwrap();

    let doc1_path = invoices_dir.join("inv_october.pdf");
    let doc2_path = notes_dir.join("meeting.md");
    fs::write(
        &doc1_path,
        "Acme Corp October invoice for software engineering services total $1500",
    )
    .unwrap();
    fs::write(
        &doc2_path,
        "Standup notes discussing sprint goals and timeline",
    )
    .unwrap();

    let doc1 = kyro_lib::storage::DocumentIndexRecord {
        path: doc1_path.to_string_lossy().to_string(),
        file_name: "inv_october.pdf".to_string(),
        format: "pdf".to_string(),
        size_bytes: 10240,
        modified_at: 1729000000000,
        sha256: "hash1".to_string(),
        extracted_text: Some(
            "Acme Corp October invoice for software engineering services total $1500".to_string(),
        ),
        indexed_at: 1729000500000,
    };
    let doc2 = kyro_lib::storage::DocumentIndexRecord {
        path: doc2_path.to_string_lossy().to_string(),
        file_name: "meeting.md".to_string(),
        format: "md".to_string(),
        size_bytes: 2048,
        modified_at: 1729100000000,
        sha256: "hash2".to_string(),
        extracted_text: Some("Standup notes discussing sprint goals and timeline".to_string()),
        indexed_at: 1729100500000,
    };

    db.upsert_indexed_document(&doc1).unwrap();
    db.upsert_indexed_document(&doc2).unwrap();

    let all_docs = db.list_indexed_documents().unwrap();
    assert_eq!(all_docs.len(), 2);

    // Before authorizing folder, get_indexed_document fails closed
    let unauth_retrieved = db.get_indexed_document(&doc1.path).unwrap();
    assert!(
        unauth_retrieved.is_none(),
        "Must fail closed when no authorized folder covers document path"
    );

    // Authorize folder covering doc1
    let auth_invoices = AuthorizedFolderRecord {
        id: "auth_inv".to_string(),
        path: invoices_dir.to_string_lossy().to_string(),
        display_name: "Invoices".to_string(),
        added_at: 1000,
    };
    db.add_authorized_folder(&auth_invoices).unwrap();

    // Verify retrieval by path now succeeds
    let retrieved = db.get_indexed_document(&doc1.path).unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().sha256, "hash1");

    // doc2 is still unauthorized
    assert!(db.get_indexed_document(&doc2.path).unwrap().is_none());

    // Prune unauthorized: only notes_dir is passed, so doc1 (in invoices) is pruned
    let auth_notes = AuthorizedFolderRecord {
        id: "auth1".to_string(),
        path: notes_dir.to_string_lossy().to_string(),
        display_name: "Notes".to_string(),
        added_at: 1000,
    };
    let pruned = db
        .prune_unauthorized_indexed_documents(&[auth_notes])
        .unwrap();
    assert_eq!(pruned, 1);

    fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn test_routine_unified_execution_lifecycle_and_concurrency() {
    let (dir, db, vault) = temp_db("unified_routine");
    let engine = std::sync::Arc::new(kyro_lib::agent::loop_engine::AgentLoopEngine::new());

    let routine = RoutineRecord {
        id: "rt_test_exec".to_string(),
        name: "Test Routine".to_string(),
        cron_expression: "0 17 * * 5".to_string(),
        action_type: "prompt".to_string(),
        target_id: "echo test".to_string(),
        enabled: true,
        last_run: None,
        next_run: None,
    };
    db.create_routine(&routine).unwrap();

    // Execute unified routine
    let result = kyro_lib::teach_task::scheduler::RoutineScheduler::execute_routine_unified(
        routine.clone(),
        db.clone(),
        vault.clone(),
        engine.clone(),
        None,
        None,
        true,
    )
    .await;

    assert!(
        result.is_ok(),
        "execute_routine_unified should succeed or record truthful outcome"
    );
    let run_record = result.unwrap();
    assert_eq!(run_record.routine_id, "rt_test_exec");
    assert!(run_record.started_at > 0);
    assert!(run_record.completed_at.is_some());
    // Terminal status should be either succeeded or failed (never stuck at running)
    assert!(run_record.status == "succeeded" || run_record.status == "failed");

    // Check DB record
    let persisted_runs = db.list_runs_for_routine("rt_test_exec", 10).unwrap();
    assert_eq!(persisted_runs.len(), 1);
    assert_eq!(persisted_runs[0].id, run_record.id);

    // Routine last_run should be updated in DB
    let updated_routine = db.get_routine("rt_test_exec").unwrap().unwrap();
    assert!(updated_routine.last_run.is_some());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_remove_authorized_folder_by_id_or_path() {
    let (dir, db, _vault) = temp_db("remove_folder");

    let f1 = AuthorizedFolderRecord {
        id: "folder_1".to_string(),
        path: "/path/to/downloads".to_string(),
        display_name: "Downloads".to_string(),
        added_at: 1000,
    };
    let f2 = AuthorizedFolderRecord {
        id: "folder_2".to_string(),
        path: "/path/to/documents".to_string(),
        display_name: "Documents".to_string(),
        added_at: 1001,
    };
    db.add_authorized_folder(&f1).unwrap();
    db.add_authorized_folder(&f2).unwrap();
    assert_eq!(db.list_authorized_folders().unwrap().len(), 2);

    // Remove by id
    let removed_by_id = db.remove_authorized_folder("folder_1").unwrap();
    assert!(removed_by_id);
    assert_eq!(db.list_authorized_folders().unwrap().len(), 1);

    // Remove by path
    let removed_by_path = db.remove_authorized_folder("/path/to/documents").unwrap();
    assert!(removed_by_path);
    assert_eq!(db.list_authorized_folders().unwrap().len(), 0);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_google_mime_and_html_decoding() {
    // 1. html_to_plain_text
    let html = "<html><head><style>.test{color:red;}</style></head><body>\
                <h1>Meeting Briefing</h1>\
                <script>console.log('strip me');</script>\
                <p>Hello &amp; welcome,<br/>Please review the attached invoice &lt;INV-101&gt;.</p>\
                </body></html>";
    let plain = GoogleAdapter::html_to_plain_text(html);
    assert!(!plain.contains("<style>"));
    assert!(!plain.contains("console.log"));
    assert!(plain.contains("Hello & welcome"));
    assert!(plain.contains("attached invoice <INV-101>"));

    // 2. decode_base64url
    let raw = "Hello World!";
    let encoded = "SGVsbG8gV29ybGQh"; // Standard base64 without padding
    let decoded = GoogleAdapter::decode_base64url(encoded).unwrap();
    assert_eq!(String::from_utf8(decoded).unwrap(), raw);

    // 3. extract_mime_content
    let body_text = "Important update: routine completed.";
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    let b64_body = URL_SAFE_NO_PAD.encode(body_text.as_bytes());

    let mime_tree = serde_json::json!({
        "mimeType": "multipart/alternative",
        "parts": [
            {
                "mimeType": "text/plain",
                "body": {
                    "data": b64_body
                }
            },
            {
                "mimeType": "application/pdf",
                "filename": "summary.pdf",
                "body": {
                    "size": 4096
                }
            }
        ]
    });

    let mut text_plain = None;
    let mut text_html = None;
    let mut attachments = Vec::new();
    GoogleAdapter::extract_mime_content(
        &mime_tree,
        &mut text_plain,
        &mut text_html,
        &mut attachments,
    );

    assert_eq!(text_plain.as_deref(), Some(body_text));
    assert_eq!(attachments.len(), 1);
    assert!(attachments[0].contains("summary.pdf"));
}

#[test]
fn test_rfc2822_crlf_header_injection_rejection() {
    // 1. CRLF in recipient
    let r1 = GoogleAdapter::encode_rfc2822_message(
        "alice@example.com\r\nBcc: evil@attacker.com",
        "Hello",
        "Body content",
        None,
    );
    assert!(r1.is_err(), "CRLF in recipient must be rejected");
    assert!(r1.unwrap_err().contains("injection"));

    // 2. LF-only in recipient
    let r2 = GoogleAdapter::encode_rfc2822_message(
        "alice@example.com\nBcc: evil@attacker.com",
        "Hello",
        "Body content",
        None,
    );
    assert!(r2.is_err(), "LF in recipient must be rejected");

    // 3. CRLF in subject
    let r3 = GoogleAdapter::encode_rfc2822_message(
        "alice@example.com",
        "Status Update\r\nBcc: evil@attacker.com",
        "Body content",
        None,
    );
    assert!(r3.is_err(), "CRLF in subject must be rejected");
    assert!(r3.unwrap_err().contains("injection"));

    // 4. Content-Type injection in subject
    let r4 = GoogleAdapter::encode_rfc2822_message(
        "alice@example.com",
        "Quarterly Report\r\nContent-Type: text/html",
        "Body content",
        None,
    );
    assert!(
        r4.is_err(),
        "Content-Type injection in subject must be rejected"
    );

    // 5. CRLF in in_reply_to
    let r5 = GoogleAdapter::encode_rfc2822_message(
        "alice@example.com",
        "Re: Status",
        "Body content",
        Some("<msg123>\r\nBcc: evil@attacker.com"),
    );
    assert!(r5.is_err(), "CRLF in in_reply_to must be rejected");

    // 6. Header injection via colons in in_reply_to
    let r6 = GoogleAdapter::encode_rfc2822_message(
        "alice@example.com",
        "Re: Status",
        "Body content",
        Some("msg123: injection"),
    );
    assert!(
        r6.is_err(),
        "Colon header injection in in_reply_to must be rejected"
    );
}

#[test]
fn test_rfc2822_malformed_email_address_rejection() {
    // Missing @
    assert!(GoogleAdapter::encode_rfc2822_message("alice", "Test", "Body", None).is_err());
    // Missing local part
    assert!(GoogleAdapter::encode_rfc2822_message("@example.com", "Test", "Body", None).is_err());
    // Missing domain part
    assert!(GoogleAdapter::encode_rfc2822_message("alice@", "Test", "Body", None).is_err());
    // Missing top-level domain dot
    assert!(
        GoogleAdapter::encode_rfc2822_message("alice@localhost", "Test", "Body", None).is_err()
    );
    // Spaces inside raw address
    assert!(
        GoogleAdapter::encode_rfc2822_message("alice smith@example.com", "Test", "Body", None)
            .is_err()
    );
    // Double dot in domain
    assert!(
        GoogleAdapter::encode_rfc2822_message("alice@example..com", "Test", "Body", None).is_err()
    );
    // Empty recipient
    assert!(GoogleAdapter::encode_rfc2822_message("", "Test", "Body", None).is_err());
}

#[test]
fn test_rfc2822_unicode_subjects_and_rfc2047_encoding() {
    let to = "Recipient <user@example.com>";
    let subject = "Actualización de Seguridad 🚀";
    let body = "Este es un mensaje con caracteres UTF-8.\r\nLínea 2.";
    let in_reply_to = Some("<thread123@kyro.local>");

    let encoded_res = GoogleAdapter::encode_rfc2822_message(to, subject, body, in_reply_to);
    assert!(
        encoded_res.is_ok(),
        "Valid Unicode email must be encoded successfully"
    );

    let b64 = encoded_res.unwrap();
    let decoded_bytes = GoogleAdapter::decode_base64url(&b64).expect("Base64url decoding");
    let mime_str = String::from_utf8(decoded_bytes).expect("Valid UTF-8 RFC 2822 text");

    // Verify headers structure
    assert!(mime_str.contains("To: Recipient <user@example.com>\r\n"));
    assert!(mime_str.contains("MIME-Version: 1.0\r\n"));
    assert!(mime_str.contains("Content-Type: text/plain; charset=UTF-8\r\n"));
    // Subject should be RFC 2047 encoded
    assert!(mime_str.contains("Subject: =?UTF-8?B?"));
    // Threading headers
    assert!(mime_str.contains("In-Reply-To: <thread123@kyro.local>\r\n"));
    assert!(mime_str.contains("References: <thread123@kyro.local>\r\n"));
    // Header section must end with \r\n\r\n before body
    assert!(mime_str.contains("\r\n\r\nEste es un mensaje con caracteres UTF-8."));
    // Must NOT contain unencoded raw injected headers
    assert!(!mime_str.contains("Bcc:"));
}

#[test]
fn test_google_token_extraction_raw_and_vault_json() {
    let adapter = GoogleAdapter::new();

    // 1. Raw token string
    let raw = "ya29.a0AfH6_SAMPLE_BEARER_TOKEN";
    assert_eq!(adapter.extract_token(raw).unwrap(), raw);

    // 2. JSON vault secret payload
    let json_secret = r#"{"access_token": "ya29.extracted_token_123", "refresh_token": "1//rot_abc", "client_secret": null}"#;
    assert_eq!(
        adapter.extract_token(json_secret).unwrap(),
        "ya29.extracted_token_123"
    );

    // 3. Fallback token field
    let json_token = r#"{"token": "ya29.token_field_val"}"#;
    assert_eq!(
        adapter.extract_token(json_token).unwrap(),
        "ya29.token_field_val"
    );

    // 4. Empty secret error
    assert!(adapter.extract_token("   ").is_err());
}

#[test]
fn test_revoked_folder_pruning_removes_unauthorized_indexed_docs() {
    let (dir, db, _vault) = temp_db("prune_revoked");
    let folder_a = dir.join("folder_a");
    let folder_b = dir.join("folder_b");
    fs::create_dir_all(&folder_a).unwrap();
    fs::create_dir_all(&folder_b).unwrap();

    let doc_a = folder_a.join("report_a.txt");
    let doc_b = folder_b.join("report_b.txt");
    fs::write(&doc_a, "Sensitive internal notes from folder A").unwrap();
    fs::write(&doc_b, "Public syllabus from folder B").unwrap();

    let auth_a = AuthorizedFolderRecord {
        id: "auth_a".to_string(),
        path: folder_a.to_string_lossy().to_string(),
        display_name: "Folder A".to_string(),
        added_at: 100,
    };
    let auth_b = AuthorizedFolderRecord {
        id: "auth_b".to_string(),
        path: folder_b.to_string_lossy().to_string(),
        display_name: "Folder B".to_string(),
        added_at: 101,
    };
    db.add_authorized_folder(&auth_a).unwrap();
    db.add_authorized_folder(&auth_b).unwrap();

    // Index both documents
    let rec_a = DocumentIndexRecord {
        path: doc_a.to_string_lossy().to_string(),
        file_name: "report_a.txt".to_string(),
        format: "txt".to_string(),
        size_bytes: 40,
        modified_at: 1000,
        sha256: "hash_a".to_string(),
        extracted_text: Some("Sensitive internal notes from folder A".to_string()),
        indexed_at: 1000,
    };
    let rec_b = DocumentIndexRecord {
        path: doc_b.to_string_lossy().to_string(),
        file_name: "report_b.txt".to_string(),
        format: "txt".to_string(),
        size_bytes: 30,
        modified_at: 1000,
        sha256: "hash_b".to_string(),
        extracted_text: Some("Public syllabus from folder B".to_string()),
        indexed_at: 1000,
    };
    db.upsert_indexed_document(&rec_a).unwrap();
    db.upsert_indexed_document(&rec_b).unwrap();

    assert_eq!(db.list_indexed_documents().unwrap().len(), 2);

    // Revoke folder A
    db.remove_authorized_folder("auth_a").unwrap();
    let remaining_folders = db.list_authorized_folders().unwrap();
    assert_eq!(remaining_folders.len(), 1);

    // Prune unauthorized indexed documents
    let pruned_count = db
        .prune_unauthorized_indexed_documents(&remaining_folders)
        .unwrap();
    assert_eq!(
        pruned_count, 1,
        "Exactly one unauthorized document must be pruned"
    );

    let remaining_docs = db.list_indexed_documents().unwrap();
    assert_eq!(remaining_docs.len(), 1);
    assert_eq!(remaining_docs[0].path, doc_b.to_string_lossy().to_string());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_file_organizer_toctou_symlink_traversal_rejection() {
    let (dir, _db, _vault) = temp_db("toctou_symlink");
    let authorized_root = dir.join("authorized");
    let outside_dir = dir.join("outside_forbidden");
    fs::create_dir_all(&authorized_root).unwrap();
    fs::create_dir_all(&outside_dir).unwrap();

    let outside_secret = outside_dir.join("etc_shadow_mock.txt");
    fs::write(&outside_secret, "root:x:0:0:root").unwrap();

    // Create a symlink inside authorized root pointing to forbidden outside file
    let malicious_symlink = authorized_root.join("innocent_looking_link.txt");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside_secret, &malicious_symlink).unwrap();

    let plan = OrganizationPlan {
        plan_id: "test_toctou".to_string(),
        target_directory: authorized_root.clone(),
        total_scanned: 1,
        duplicates_count: 0,
        conflicts_count: 0,
        operations: vec![PlannedFileOp {
            source_path: malicious_symlink.clone(),
            destination_path: authorized_root.join("organized/innocent.txt"),
            category: "Documents".to_string(),
            reason: "Test organizing".to_string(),
            file_size: 15,
            file_hash: None,
            is_duplicate: false,
            duplicate_of: None,
            conflict: None,
        }],
        ..Default::default()
    };

    let result = FileOrganizer::execute(&plan).unwrap();
    assert_eq!(result.operations.len(), 1);
    assert_eq!(result.operations[0].status, "failed");
    assert!(result.operations[0]
        .error
        .as_ref()
        .unwrap()
        .contains("Security rejection"));

    // Verify outside file was NOT moved
    assert!(outside_secret.exists());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_filesystem_move_and_copy_destination_collision_and_confirm() {
    let dir = std::env::temp_dir().join(format!("kyro_fs_collision_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();

    let src = dir.join("source.txt");
    let dest = dir.join("existing_dest.txt");
    fs::write(&src, "Source Content v1").unwrap();
    fs::write(&dest, "Original Destination Content").unwrap();

    // 1. Copy collision without confirmation -> fails closed with requires_confirmation
    let copy_unconfirmed =
        FileAssistant::execute_safe_op("copy", &src, Some(&dest), false).unwrap();
    assert!(!copy_unconfirmed.success);
    assert!(copy_unconfirmed.requires_confirmation);
    assert!(copy_unconfirmed.message.contains("already exists"));
    assert_eq!(
        fs::read_to_string(&dest).unwrap(),
        "Original Destination Content"
    );

    // 2. Copy collision with confirmation -> succeeds and replaces
    let copy_confirmed = FileAssistant::execute_safe_op("copy", &src, Some(&dest), true).unwrap();
    assert!(copy_confirmed.success);
    assert!(!copy_confirmed.requires_confirmation);
    assert_eq!(fs::read_to_string(&dest).unwrap(), "Source Content v1");
    assert!(src.exists());

    // 3. Move collision without confirmation -> fails closed with requires_confirmation
    fs::write(&dest, "Destination Overwrite Guard").unwrap();
    let move_unconfirmed =
        FileAssistant::execute_safe_op("move", &src, Some(&dest), false).unwrap();
    assert!(!move_unconfirmed.success);
    assert!(move_unconfirmed.requires_confirmation);
    assert!(move_unconfirmed.message.contains("already exists"));
    assert_eq!(
        fs::read_to_string(&dest).unwrap(),
        "Destination Overwrite Guard"
    );
    assert!(src.exists());

    // 4. Move collision with confirmation -> succeeds, moves and replaces
    let move_confirmed = FileAssistant::execute_safe_op("move", &src, Some(&dest), true).unwrap();
    assert!(move_confirmed.success);
    assert_eq!(fs::read_to_string(&dest).unwrap(), "Source Content v1");
    assert!(!src.exists());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_dynamic_toctou_path_race_rejection_and_documentation() {
    // Audit Note on TOCTOU Protection:
    // Full race-free TOCTOU resistance on standard POSIX without openat2 / O_BENEATH relies on:
    // 1. Immediate canonicalization and verification of path containment at every execution step.
    // 2. Safe directory pre-removal and symlink-metadata checks preventing symlink dereferencing.
    // 3. Re-checking source and destination containment within authorized root immediately before rename/copy.
    let (dir, _db, _vault) = temp_db("dynamic_toctou");
    let authorized_root = dir.join("authorized");
    let outside_forbidden = dir.join("forbidden");
    fs::create_dir_all(&authorized_root).unwrap();
    fs::create_dir_all(&outside_forbidden).unwrap();

    let outside_secret = outside_forbidden.join("secret_keys.pem");
    fs::write(&outside_secret, "PRIVATE KEY DATA").unwrap();

    let valid_source = authorized_root.join("notes.txt");
    fs::write(&valid_source, "My public notes").unwrap();

    let planned_dest = authorized_root.join("subfolder/notes.txt");

    let plan = OrganizationPlan {
        plan_id: "plan_race_test".to_string(),
        target_directory: authorized_root.clone(),
        total_scanned: 1,
        duplicates_count: 0,
        conflicts_count: 0,
        operations: vec![PlannedFileOp {
            source_path: valid_source.clone(),
            destination_path: planned_dest.clone(),
            category: "Documents".to_string(),
            reason: "Organize notes".to_string(),
            file_size: 15,
            file_hash: None,
            is_duplicate: false,
            duplicate_of: None,
            conflict: None,
        }],
        ..Default::default()
    };

    // Simulate TOCTOU race: an attacker creates a symlink at the destination parent pointing to outside_forbidden
    #[cfg(unix)]
    {
        let subfolder_link = authorized_root.join("subfolder");
        std::os::unix::fs::symlink(&outside_forbidden, &subfolder_link).unwrap();

        // Execution must detect that the destination resolves through a symlink outside authorized root!
        let result = FileOrganizer::execute(&plan).unwrap();
        assert_eq!(result.operations.len(), 1);
        assert_eq!(result.operations[0].status, "failed");
        assert!(result.operations[0]
            .error
            .as_ref()
            .unwrap()
            .contains("Security rejection"));

        // Verify the forbidden directory did not receive the file
        assert!(!outside_forbidden.join("notes.txt").exists());
        // Verify original source still exists
        assert!(valid_source.exists());
    }

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_file_organizer_undo_containment_and_overwrite_safety() {
    let (dir, _db, _vault) = temp_db("undo_containment");
    let root = dir.join("root");
    let outside = dir.join("outside");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();

    let organized_file = root.join("category/file.txt");
    fs::create_dir_all(organized_file.parent().unwrap()).unwrap();
    fs::write(&organized_file, "Organized text").unwrap();

    // 1. Transaction trying to rollback to an outside directory escapes authorized root -> rejected
    let mut bad_tx = OrganizationTransaction {
        transaction_id: "tx_bad".to_string(),
        created_at: 1000,
        target_directory: root.to_string_lossy().to_string(),
        status: "completed".to_string(),
        operations: vec![OrganizationOpRecord {
            source_path: outside.join("file.txt").to_string_lossy().to_string(), // tries to escape root
            destination_path: organized_file.to_string_lossy().to_string(),
            file_size: 14,
            file_hash: String::new(),
            status: "completed".to_string(),
            error: None,
        }],
    };
    let reversed = FileOrganizer::undo(&mut bad_tx).unwrap();
    assert_eq!(reversed, 0);
    assert!(bad_tx.operations[0]
        .error
        .as_ref()
        .unwrap()
        .contains("escapes authorized target directory"));

    // 2. Transaction where original location already has a file -> overwrite guard prevents undo clobbering
    let orig_in_root = root.join("file.txt");
    fs::write(
        &orig_in_root,
        "Existing original file that shouldn't be overwritten",
    )
    .unwrap();
    let mut clobber_tx = OrganizationTransaction {
        transaction_id: "tx_clobber".to_string(),
        created_at: 1000,
        target_directory: root.to_string_lossy().to_string(),
        status: "completed".to_string(),
        operations: vec![OrganizationOpRecord {
            source_path: orig_in_root.to_string_lossy().to_string(),
            destination_path: organized_file.to_string_lossy().to_string(),
            file_size: 14,
            file_hash: String::new(),
            status: "completed".to_string(),
            error: None,
        }],
    };
    let reversed_clobber = FileOrganizer::undo(&mut clobber_tx).unwrap();
    assert_eq!(reversed_clobber, 0);
    assert!(clobber_tx.operations[0]
        .error
        .as_ref()
        .unwrap()
        .contains("occupied"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_unicode_multibyte_document_explanation_safety() {
    let dir = std::env::temp_dir().join(format!("kyro_unicode_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();

    let doc_path = dir.join("multilingual_contract.txt");
    let content = "KYRO GLOBAL AGREEMENT 🚀\n\
                   Café de la Paix: Résumé des engagements financiers.\n\
                   Russian section: Привет мир! Безопасность данных гарантирована.\n\
                   Japanese section: 日本語の文書データとAI解析.\n\
                   German: Übergröße für besondere Anforderungen.\n\
                   Key Clause: Action required before 2026-11-15.";
    fs::write(&doc_path, content).unwrap();

    // 1. Emoji passage
    let expl_emoji = FileAssistant::explain_passage(&doc_path, "KYRO GLOBAL AGREEMENT 🚀");
    assert!(expl_emoji.is_ok(), "Multibyte emoji passage must not panic");
    assert!(expl_emoji.unwrap().contains("Plain Language Explanation"));

    // 2. Accented passage
    let expl_accents = FileAssistant::explain_passage(&doc_path, "Café de la Paix: Résumé");
    assert!(expl_accents.is_ok(), "Accented passage must not panic");

    // 3. Cyrillic passage
    let expl_cyrillic = FileAssistant::explain_passage(&doc_path, "Привет мир!");
    assert!(expl_cyrillic.is_ok(), "Cyrillic passage must not panic");

    // 4. Japanese CJK passage
    let expl_cjk = FileAssistant::explain_passage(&doc_path, "日本語の文書データ");
    assert!(expl_cjk.is_ok(), "CJK passage must not panic");

    // 5. Umlaut passage
    let expl_umlaut = FileAssistant::explain_passage(&doc_path, "Übergröße");
    assert!(expl_umlaut.is_ok(), "German umlaut passage must not panic");

    // 6. Grounding rejection on missing excerpt
    let err_missing = FileAssistant::explain_passage(&doc_path, "Non-existent clause xyz 123");
    assert!(err_missing.is_err());
    assert!(err_missing
        .unwrap_err()
        .contains("Passage not found in document"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_startup_reconciliation_cleans_running_routine_runs() {
    let (dir, db, _vault) = temp_db("startup_reconcile");

    let routine = RoutineRecord {
        id: "rt_recon".to_string(),
        name: "Backup Routine".to_string(),
        cron_expression: "0 0 * * *".to_string(),
        action_type: "prompt".to_string(),
        target_id: "backup".to_string(),
        enabled: true,
        last_run: None,
        next_run: None,
    };
    db.create_routine(&routine).unwrap();

    let running_run = RoutineRunRecord {
        id: "run_stuck".to_string(),
        routine_id: "rt_recon".to_string(),
        routine_name: "Backup Routine".to_string(),
        status: "running".to_string(),
        started_at: 1000,
        completed_at: None,
        summary: None,
        error: None,
    };
    let completed_run = RoutineRunRecord {
        id: "run_done".to_string(),
        routine_id: "rt_recon".to_string(),
        routine_name: "Backup Routine".to_string(),
        status: "succeeded".to_string(),
        started_at: 500,
        completed_at: Some(600),
        summary: Some("Done".to_string()),
        error: None,
    };

    db.record_routine_run(&running_run).unwrap();
    db.record_routine_run(&completed_run).unwrap();

    // Reconcile on startup
    let count = db.reconcile_active_runs_on_startup().unwrap();
    assert!(count >= 1, "Must reconcile at least 1 interrupted run");

    let runs = db.list_runs_for_routine("rt_recon", 10).unwrap();
    let stuck = runs.iter().find(|r| r.id == "run_stuck").unwrap();
    assert_eq!(stuck.status, "interrupted");
    assert!(stuck.completed_at.is_some());
    assert!(stuck
        .error
        .as_ref()
        .unwrap()
        .contains("Process terminated unexpectedly"));

    let done = runs.iter().find(|r| r.id == "run_done").unwrap();
    assert_eq!(done.status, "succeeded");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_oauth_refresh_transient_vs_permanent_classification() {
    // Transient errors -> must NOT trigger needs_reauth
    let transient_cases = [
        "Connection refused (os error 111)",
        "dns error: failed to lookup address information",
        "operation timed out after 30000ms",
        "HTTP 429 Too Many Requests: Rate limit exceeded",
        "HTTP 502 Bad Gateway",
        "HTTP 503 Service Unavailable",
        "HTTP 504 Gateway Timeout",
        "network unreachable",
    ];

    for err in transient_cases {
        assert!(
            !ConnectorManager::is_permanent_refresh_error(err),
            "Error '{}' should be classified as transient, not permanent",
            err
        );
    }

    // Permanent errors -> must trigger needs_reauth
    let permanent_cases = [
        "HTTP 400 Bad Request: invalid_grant: Token has been expired or revoked",
        "unauthorized_client: The OAuth client is not authorized",
        "access_denied: The user has revoked access to this application",
        "HTTP 401 Unauthorized: token has been expired",
        "HTTP 403 Forbidden: invalid_scope",
        "status 400: invalid_grant",
        "status 401: unauthorized",
    ];

    for err in permanent_cases {
        assert!(
            ConnectorManager::is_permanent_refresh_error(err),
            "Error '{}' should be classified as permanent reauth requirement",
            err
        );
    }
}

#[tokio::test]
async fn test_calendar_invalid_timezone_rejection_and_working_hours() {
    let (_dir, _db, vault) = temp_db("cal_tz");
    let adapter = GoogleAdapter::new();
    let account = ConnectedAccountRecord {
        id: "conn_cal_tz".to_string(),
        connector_id: "google".to_string(),
        account_identifier: "user@example.com".to_string(),
        display_name: "Google (User)".to_string(),
        status: "connected".to_string(),
        status_message: None,
        granted_scopes_json: "[]".to_string(),
        credential_key: "cred_test".to_string(),
        config_json: "{}".to_string(),
        created_at: 0,
        updated_at: 0,
        last_connected_at: None,
        last_error: None,
        expires_at: None,
    };
    vault.set_secret("cred_test", "mock_bearer_token").unwrap();

    // 1. Invalid timezone string -> rejected before any network call
    let bad_tz_args = serde_json::json!({
        "date": "2026-10-30",
        "timezone": "Not/A_Real_Timezone_Name",
    });
    let err_tz = adapter
        .execute_tool(&account, "calendar_find_meeting_times", bad_tz_args, &vault)
        .await;
    assert!(err_tz.is_err());
    assert!(err_tz.unwrap_err().contains("Invalid timezone"));

    // 2. Inverted working hours (start >= end) -> rejected
    let bad_hours_args = serde_json::json!({
        "date": "2026-10-30",
        "timezone": "America/New_York",
        "work_start_hour": 18,
        "work_end_hour": 9,
    });
    let err_hours = adapter
        .execute_tool(
            &account,
            "calendar_find_meeting_times",
            bad_hours_args,
            &vault,
        )
        .await;
    assert!(err_hours.is_err());
    assert!(err_hours
        .unwrap_err()
        .contains("work_start_hour (18) must be less than work_end_hour (9)"));
}

#[test]
fn test_nl_routine_clarification_for_ambiguous_prompts() {
    // 1. Time only without frequency -> needs clarification
    let res_time_only =
        natural_language_to_routine("At 9am find invoices in Downloads".to_string()).unwrap();
    assert!(res_time_only.needs_clarification);
    assert!(res_time_only.requires_confirmation);
    assert!(res_time_only
        .clarification_prompt
        .as_ref()
        .unwrap()
        .contains("without indicating recurrence"));

    // 2. Monthly without specific day -> needs clarification
    let res_monthly =
        natural_language_to_routine("Every month summarize my expenses".to_string()).unwrap();
    assert!(res_monthly.needs_clarification);
    assert!(res_monthly.requires_confirmation);
    assert!(res_monthly
        .clarification_prompt
        .as_ref()
        .unwrap()
        .contains("day of the month"));

    // 3. Complete explicit prompt -> no clarification needed
    let res_complete =
        natural_language_to_routine("Every Friday at 5pm organize my downloads".to_string())
            .unwrap();
    assert!(!res_complete.needs_clarification);
    assert_eq!(res_complete.cron_expression, "0 17 * * 5");
}

#[test]
fn test_revoked_folder_and_prune_removes_authorization_and_indexed_data() {
    let (dir, db, _vault) = temp_db("revoke_and_prune");
    let folder = dir.join("personal_docs");
    fs::create_dir_all(&folder).unwrap();

    let doc = folder.join("tax_return.pdf");
    fs::write(&doc, "Tax Return 2026 Sensitive Financial Data").unwrap();

    let auth = AuthorizedFolderRecord {
        id: "auth_tax".to_string(),
        path: folder.to_string_lossy().to_string(),
        display_name: "Tax Folder".to_string(),
        added_at: 1000,
    };
    db.add_authorized_folder(&auth).unwrap();

    let index_rec = DocumentIndexRecord {
        path: doc.to_string_lossy().to_string(),
        file_name: "tax_return.pdf".to_string(),
        format: "pdf".to_string(),
        size_bytes: 40,
        modified_at: 1000,
        sha256: "taxhash".to_string(),
        extracted_text: Some("Tax Return 2026 Sensitive Financial Data".to_string()),
        indexed_at: 1000,
    };
    db.upsert_indexed_document(&index_rec).unwrap();

    // Verification: document is accessible
    assert!(db
        .get_indexed_document(&doc.to_string_lossy())
        .unwrap()
        .is_some());

    // Revoke and prune atomically
    let removed = db.remove_authorized_folder_and_prune(&auth.id).unwrap();
    assert!(removed, "Tax return document and folder must be pruned");

    // Verification: folder authorization is revoked
    assert!(db.list_authorized_folders().unwrap().is_empty());

    // Verification: get_indexed_document fails closed
    assert!(db
        .get_indexed_document(&doc.to_string_lossy())
        .unwrap()
        .is_none());

    // Verification: document_index has 0 records
    assert!(db.list_indexed_documents().unwrap().is_empty());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_calendar_timezone_dst_deterministic_resolution() {
    use chrono::{NaiveDate, Offset};

    // 1. Normal unambiguous time
    let tz_ny: chrono_tz::Tz = "America/New_York".parse().unwrap();
    let dt_normal = NaiveDate::from_ymd_opt(2026, 6, 15)
        .unwrap()
        .and_hms_opt(9, 0, 0)
        .unwrap();
    let res_normal =
        GoogleAdapter::resolve_local_datetime_deterministic(tz_ny, &dt_normal).unwrap();
    assert_eq!(res_normal.to_rfc3339(), "2026-06-15T09:00:00-04:00");

    // 2. Spring-forward gap in America/New_York (2026-03-08: clocks jump 02:00 -> 03:00)
    // 02:30 does not exist locally; advancing in 15-min increments finds 03:00
    let dt_gap_ny = NaiveDate::from_ymd_opt(2026, 3, 8)
        .unwrap()
        .and_hms_opt(2, 30, 0)
        .unwrap();
    let res_gap_ny =
        GoogleAdapter::resolve_local_datetime_deterministic(tz_ny, &dt_gap_ny).unwrap();
    assert_eq!(res_gap_ny.to_rfc3339(), "2026-03-08T03:00:00-04:00");

    // 3. Fall-back ambiguous in America/New_York (2026-11-01: clocks repeat 01:00 -> 02:00)
    // 01:30 occurs twice (-04:00 and -05:00); earliest (-04:00) must be selected deterministically
    let dt_ambig_ny = NaiveDate::from_ymd_opt(2026, 11, 1)
        .unwrap()
        .and_hms_opt(1, 30, 0)
        .unwrap();
    let res_ambig_ny =
        GoogleAdapter::resolve_local_datetime_deterministic(tz_ny, &dt_ambig_ny).unwrap();
    assert_eq!(res_ambig_ny.to_rfc3339(), "2026-11-01T01:30:00-04:00");
    assert_eq!(res_ambig_ny.offset().fix().local_minus_utc(), -4 * 3600);

    // 4. Spring-forward gap in Europe/London (2026-03-29: clocks jump 01:00 -> 02:00)
    let tz_london: chrono_tz::Tz = "Europe/London".parse().unwrap();
    let dt_gap_london = NaiveDate::from_ymd_opt(2026, 3, 29)
        .unwrap()
        .and_hms_opt(1, 30, 0)
        .unwrap();
    let res_gap_london =
        GoogleAdapter::resolve_local_datetime_deterministic(tz_london, &dt_gap_london).unwrap();
    assert_eq!(res_gap_london.to_rfc3339(), "2026-03-29T02:00:00+01:00");

    // 5. Spring-forward gap in Australia/Sydney (2026-10-04: clocks jump 02:00 -> 03:00)
    let tz_sydney: chrono_tz::Tz = "Australia/Sydney".parse().unwrap();
    let dt_gap_sydney = NaiveDate::from_ymd_opt(2026, 10, 4)
        .unwrap()
        .and_hms_opt(2, 30, 0)
        .unwrap();
    let res_gap_sydney =
        GoogleAdapter::resolve_local_datetime_deterministic(tz_sydney, &dt_gap_sydney).unwrap();
    assert_eq!(res_gap_sydney.to_rfc3339(), "2026-10-04T03:00:00+11:00");
}

#[tokio::test]
async fn test_calendar_list_events_multi_page_pagination() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let mock_base = format!("http://127.0.0.1:{}", port);

    tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(conn) => conn,
                Err(_) => break,
            };

            let mut buf = [0u8; 8192];
            let n = match socket.read(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => continue,
            };

            let req_str = String::from_utf8_lossy(&buf[..n]);
            let first_line = req_str.lines().next().unwrap_or_default();

            let (status, body) = if first_line.contains("pageToken=tok_page2") {
                // Page 2: final page
                (
                    "200 OK",
                    serde_json::json!({
                        "items": [
                            {
                                "id": "event_3",
                                "summary": "Design Sync",
                                "start": { "dateTime": "2026-10-23T15:00:00Z" },
                                "end": { "dateTime": "2026-10-23T16:00:00Z" }
                            }
                        ]
                    })
                    .to_string(),
                )
            } else {
                // Page 1: returns 2 items + nextPageToken
                (
                    "200 OK",
                    serde_json::json!({
                        "items": [
                            {
                                "id": "event_1",
                                "summary": "Standup",
                                "start": { "dateTime": "2026-10-23T09:00:00Z" },
                                "end": { "dateTime": "2026-10-23T09:30:00Z" }
                            },
                            {
                                "id": "event_2",
                                "summary": "Planning",
                                "start": { "dateTime": "2026-10-23T10:00:00Z" },
                                "end": { "dateTime": "2026-10-23T11:00:00Z" }
                            }
                        ],
                        "nextPageToken": "tok_page2"
                    })
                    .to_string(),
                )
            };

            let response = format!(
                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status,
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    let (_dir, _db, vault) = temp_db("cal_paged");
    let adapter = GoogleAdapter::new();
    let account = ConnectedAccountRecord {
        id: "conn_cal_paged".to_string(),
        connector_id: "google".to_string(),
        account_identifier: "user@example.com".to_string(),
        display_name: "Google (User)".to_string(),
        status: "connected".to_string(),
        status_message: None,
        granted_scopes_json: "[]".to_string(),
        credential_key: "cred_cal_paged".to_string(),
        config_json: serde_json::json!({ "base_url": mock_base }).to_string(),
        created_at: 0,
        updated_at: 0,
        last_connected_at: None,
        last_error: None,
        expires_at: None,
    };
    vault
        .set_secret("cred_cal_paged", "mock_bearer_token")
        .unwrap();

    let res_raw = adapter
        .execute_tool(
            &account,
            "calendar_list_events",
            serde_json::json!({ "max_results": 10 }),
            &vault,
        )
        .await
        .expect("calendar_list_events failed");

    let val: serde_json::Value = serde_json::from_str(&res_raw).unwrap();
    assert_eq!(val["total_events"].as_u64().unwrap(), 3);
    assert_eq!(val["pages_retrieved"].as_u64().unwrap(), 2);
    assert!(!val["has_more"].as_bool().unwrap());
    assert!(val.get("warning").is_none());

    let events = val["events"].as_array().unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0]["summary"], "Standup");
    assert_eq!(events[1]["summary"], "Planning");
    assert_eq!(events[2]["summary"], "Design Sync");
}

#[tokio::test]
async fn test_calendar_list_events_pagination_safety_limit_warning() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let mock_base = format!("http://127.0.0.1:{}", port);

    // Infinite pagination server
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(conn) => conn,
                Err(_) => break,
            };

            let mut buf = [0u8; 8192];
            let _n = match socket.read(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => continue,
            };

            let body = serde_json::json!({
                "items": [
                    {
                        "id": "event_repeat",
                        "summary": "Infinite Event",
                        "start": { "dateTime": "2026-10-23T09:00:00Z" },
                        "end": { "dateTime": "2026-10-23T09:30:00Z" }
                    }
                ],
                "nextPageToken": "keep_going_forever"
            })
            .to_string();

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    let (_dir, _db, vault) = temp_db("cal_limit");
    let adapter = GoogleAdapter::new();
    let account = ConnectedAccountRecord {
        id: "conn_cal_limit".to_string(),
        connector_id: "google".to_string(),
        account_identifier: "user@example.com".to_string(),
        display_name: "Google (User)".to_string(),
        status: "connected".to_string(),
        status_message: None,
        granted_scopes_json: "[]".to_string(),
        credential_key: "cred_cal_limit".to_string(),
        config_json: serde_json::json!({ "base_url": mock_base }).to_string(),
        created_at: 0,
        updated_at: 0,
        last_connected_at: None,
        last_error: None,
        expires_at: None,
    };
    vault
        .set_secret("cred_cal_limit", "mock_bearer_token")
        .unwrap();

    let res_raw = adapter
        .execute_tool(
            &account,
            "calendar_list_events",
            serde_json::json!({ "max_results": 1 }),
            &vault,
        )
        .await
        .expect("calendar_list_events should succeed with truncated warning");

    let val: serde_json::Value = serde_json::from_str(&res_raw).unwrap();
    assert_eq!(val["pages_retrieved"].as_u64().unwrap(), 10);
    assert!(val["has_more"].as_bool().unwrap());
    assert!(val["warning"]
        .as_str()
        .unwrap()
        .contains("Calendar events list safety limit reached (10 pages)"));
    assert_eq!(val["total_events"].as_u64().unwrap(), 10);
}

#[tokio::test]
async fn test_calendar_find_meeting_times_all_day_event_under_dst() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let mock_base = format!("http://127.0.0.1:{}", port);

    // Mock server returns an all-day conference on 2026-03-08 (DST change day in America/New_York)
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(conn) => conn,
                Err(_) => break,
            };

            let mut buf = [0u8; 8192];
            let _n = match socket.read(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => continue,
            };

            let body = serde_json::json!({
                "items": [
                    {
                        "id": "conf_all_day",
                        "summary": "Annual Offsite",
                        "start": { "date": "2026-03-08" },
                        "end": { "date": "2026-03-09" }
                    }
                ]
            })
            .to_string();

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });

    let (_dir, _db, vault) = temp_db("cal_allday_dst");
    let adapter = GoogleAdapter::new();
    let account = ConnectedAccountRecord {
        id: "conn_cal_allday".to_string(),
        connector_id: "google".to_string(),
        account_identifier: "user@example.com".to_string(),
        display_name: "Google (User)".to_string(),
        status: "connected".to_string(),
        status_message: None,
        granted_scopes_json: "[]".to_string(),
        credential_key: "cred_cal_allday".to_string(),
        config_json: serde_json::json!({ "base_url": mock_base }).to_string(),
        created_at: 0,
        updated_at: 0,
        last_connected_at: None,
        last_error: None,
        expires_at: None,
    };
    vault
        .set_secret("cred_cal_allday", "mock_bearer_token")
        .unwrap();

    let res_raw = adapter
        .execute_tool(
            &account,
            "calendar_find_meeting_times",
            serde_json::json!({
                "date": "2026-03-08",
                "timezone": "America/New_York",
                "work_start_hour": 9,
                "work_end_hour": 17,
                "duration_minutes": 30
            }),
            &vault,
        )
        .await
        .expect("find meeting times should succeed across DST transition");

    let val: serde_json::Value = serde_json::from_str(&res_raw).unwrap();
    // The all-day event covers 09:00 - 17:00, so busy_slots has it and has_availability is false
    assert!(!val["has_availability"].as_bool().unwrap());
    let busy = val["busy_slots"].as_array().unwrap();
    assert_eq!(busy.len(), 1);
    assert_eq!(busy[0]["summary"], "Annual Offsite");
}
