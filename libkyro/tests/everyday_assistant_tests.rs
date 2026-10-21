//! Tests for Everyday Assistant features:
//! - Authorized folders and path containment
//! - Safe file operations and duplicate detection
//! - Google Gmail & Calendar adapter tools & risk classification
//! - Routine execution history and natural language routine authoring
//! - Today dashboard aggregation

use kyro_lib::connector::native::google::GoogleAdapter;
use kyro_lib::connector::NativeConnectorAdapter;
use kyro_lib::execution::permissions::RiskLevel;
use kyro_lib::storage::file_assistant::FileAssistant;
use kyro_lib::storage::{
    AuthorizedFolderRecord, ConnectedAccountRecord, Database, RoutineRecord, RoutineRunRecord,
    Vault,
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
    let list_msgs = tools.iter().find(|t| t.name == "gmail_list_messages").unwrap();
    assert!(list_msgs.is_read_only);
    assert_eq!(list_msgs.risk_level, RiskLevel::Safe);

    let get_msg = tools.iter().find(|t| t.name == "gmail_get_message").unwrap();
    assert!(get_msg.is_read_only);
    assert_eq!(get_msg.risk_level, RiskLevel::Safe);

    let list_events = tools.iter().find(|t| t.name == "calendar_list_events").unwrap();
    assert!(list_events.is_read_only);
    assert_eq!(list_events.risk_level, RiskLevel::Safe);

    let find_times = tools.iter().find(|t| t.name == "calendar_find_meeting_times").unwrap();
    assert!(find_times.is_read_only);
    assert_eq!(find_times.risk_level, RiskLevel::Safe);

    // Consequential / write tools
    let create_draft = tools.iter().find(|t| t.name == "gmail_create_draft").unwrap();
    assert!(!create_draft.is_read_only);
    assert_eq!(create_draft.risk_level, RiskLevel::Moderate);

    let send_email = tools.iter().find(|t| t.name == "gmail_send_email").unwrap();
    assert!(!send_email.is_read_only);
    assert_eq!(send_email.risk_level, RiskLevel::Destructive);

    let create_event = tools.iter().find(|t| t.name == "calendar_create_event").unwrap();
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
    assert!(notes.key_dates_and_milestones.iter().any(|d| d.contains("2026-12-18")));

    let explanation = FileAssistant::explain_passage(&doc, "Grading Policy: 40% assignments, 60% exams.").unwrap();
    assert!(explanation.contains("Plain Language Explanation"));

    fs::remove_dir_all(&dir).ok();
}
