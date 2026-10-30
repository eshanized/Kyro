use kyro_lib::storage::{AgentRun, ApprovalRequest, Database, RunEvent};
use std::fs;
use std::path::{Path, PathBuf};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{}_{}", prefix, uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).expect("failed to create temp dir");
        Self { path }
    }
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn make_run(id: &str) -> AgentRun {
    AgentRun {
        id: id.to_string(),
        conversation_id: Some("conv_test".to_string()),
        agent_id: "apex".to_string(),
        model: "nemotron".to_string(),
        prompt: "test task".to_string(),
        status: "queued".to_string(),
        created_at: 1000,
        started_at: None,
        completed_at: None,
        error: None,
        config_json: None,
        parent_run_id: None,
    }
}

#[test]
fn test_unverified_status_persistence_and_retrieval() {
    let temp = TempDir::new("test_unverified_persistence");
    let db = Database::init(temp.path().to_path_buf()).expect("init db");
    let run_id = "run_unverified_test";

    db.create_conversation("conv_test", "Test Chat", "nemotron")
        .expect("create conv");
    db.create_run(&make_run(run_id)).expect("create run");

    // Transition to running first
    db.update_run_status(run_id, "running", Some(1000), None, None)
        .expect("update to running");

    // Persist unverified status
    let update_res = db.update_run_status(run_id, "unverified", None, Some(2000), None);
    assert!(
        update_res.is_ok(),
        "update_run_status must accept 'unverified' status: {:?}",
        update_res.err()
    );

    // Retrieve and verify persisted status
    let retrieved = db
        .get_run(run_id)
        .expect("get run")
        .expect("run must exist");
    assert_eq!(retrieved.status, "unverified");
    assert_eq!(retrieved.completed_at, Some(2000));

    // Strict rejection of genuinely invalid run statuses
    let invalid_res = db.update_run_status(run_id, "bogus_invalid_status", None, None, None);
    assert!(
        invalid_res.is_err(),
        "update_run_status must strictly reject invalid statuses"
    );
}

#[test]
fn test_whole_run_aggregation_single_action() {
    let temp = TempDir::new("test_single_action");
    let db = Database::init(temp.path().to_path_buf()).expect("init db");

    db.create_conversation("conv_test", "Test Chat", "nemotron")
        .expect("conv");

    // 1. One consequential action, one valid verification -> succeeded
    let run_v = "run_single_verified";
    db.create_run(&make_run(run_v)).expect("create run");
    let evt_v = RunEvent {
        id: "evt_v1".to_string(),
        run_id: run_v.to_string(),
        seq: 1,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_w1",
            "name": "write_file",
            "output": "Wrote 30 bytes",
            "is_error": false,
            "trusted_verification": {
                "status": "succeeded",
                "task_postcondition_verified": true,
                "trusted_verifier": true,
                "run_id": run_v,
                "tool_call_id": "call_w1",
                "action_name": "write_file",
                "condition_checked": "File exists on disk",
                "evidence": "File verified on disk (30 bytes)"
            }
        })
        .to_string(),
        created_at: 1000,
    };
    db.append_run_event(&evt_v).expect("append event");
    let outcome_v = db.get_run_outcome_from_events(run_v).expect("outcome");
    assert_eq!(
        outcome_v, "succeeded",
        "Single verified action must yield succeeded"
    );

    // 2. One consequential action, no verification -> unverified
    let run_u = "run_single_unverified";
    db.create_run(&make_run(run_u)).expect("create run");
    let evt_u = RunEvent {
        id: "evt_u1".to_string(),
        run_id: run_u.to_string(),
        seq: 1,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_w2",
            "name": "write_file",
            "output": "Wrote 30 bytes",
            "is_error": false
        })
        .to_string(),
        created_at: 1000,
    };
    db.append_run_event(&evt_u).expect("append event");
    let outcome_u = db.get_run_outcome_from_events(run_u).expect("outcome");
    assert_eq!(
        outcome_u, "unverified",
        "Single consequential action without verification must yield unverified"
    );
}

#[test]
fn test_whole_run_aggregation_two_actions() {
    let temp = TempDir::new("test_two_actions");
    let db = Database::init(temp.path().to_path_buf()).expect("init db");
    db.create_conversation("conv_test", "Test Chat", "nemotron")
        .expect("conv");

    // Case A: Two actions, both verified -> succeeded
    let run_vv = "run_both_verified";
    db.create_run(&make_run(run_vv)).expect("create run");
    db.append_run_event(&RunEvent {
        id: "evt_vv1".to_string(),
        run_id: run_vv.to_string(),
        seq: 1,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_1",
            "name": "write_file",
            "output": "Wrote file 1",
            "is_error": false,
            "trusted_verification": {
                "status": "succeeded",
                "task_postcondition_verified": true,
                "trusted_verifier": true,
                "run_id": run_vv,
                "tool_call_id": "call_1",
                "action_name": "write_file",
                "evidence": "File 1 verified"
            }
        })
        .to_string(),
        created_at: 1000,
    })
    .expect("append 1");
    db.append_run_event(&RunEvent {
        id: "evt_vv2".to_string(),
        run_id: run_vv.to_string(),
        seq: 2,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_2",
            "name": "write_file",
            "output": "Wrote file 2",
            "is_error": false,
            "trusted_verification": {
                "status": "succeeded",
                "task_postcondition_verified": true,
                "trusted_verifier": true,
                "run_id": run_vv,
                "tool_call_id": "call_2",
                "action_name": "write_file",
                "evidence": "File 2 verified"
            }
        })
        .to_string(),
        created_at: 1001,
    })
    .expect("append 2");
    assert_eq!(
        db.get_run_outcome_from_events(run_vv).expect("outcome"),
        "succeeded"
    );

    // Case B: Two actions, ONLY ONE verified -> UNVERIFIED! (CRITICAL DEFECT FIX)
    let run_vu = "run_one_verified_one_unverified";
    db.create_run(&make_run(run_vu)).expect("create run");
    db.append_run_event(&RunEvent {
        id: "evt_vu1".to_string(),
        run_id: run_vu.to_string(),
        seq: 1,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_1",
            "name": "write_file",
            "output": "Wrote file 1",
            "is_error": false,
            "trusted_verification": {
                "status": "succeeded",
                "task_postcondition_verified": true,
                "trusted_verifier": true,
                "run_id": run_vu,
                "tool_call_id": "call_1",
                "action_name": "write_file",
                "evidence": "File 1 verified"
            }
        })
        .to_string(),
        created_at: 1000,
    })
    .expect("append 1");
    // Action 2 lacks verification evidence
    db.append_run_event(&RunEvent {
        id: "evt_vu2".to_string(),
        run_id: run_vu.to_string(),
        seq: 2,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_2",
            "name": "write_file",
            "output": "Wrote file 2 without verification",
            "is_error": false
        })
        .to_string(),
        created_at: 1001,
    })
    .expect("append 2");
    assert_eq!(
        db.get_run_outcome_from_events(run_vu).expect("outcome"),
        "unverified",
        "A run with one verified and one unverified action must be unverified, never succeeded"
    );

    // Case C: Two actions, one verified and one failed -> failed
    let run_vf = "run_one_verified_one_failed";
    db.create_run(&make_run(run_vf)).expect("create run");
    db.append_run_event(&RunEvent {
        id: "evt_vf1".to_string(),
        run_id: run_vf.to_string(),
        seq: 1,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_1",
            "name": "write_file",
            "output": "Wrote file 1",
            "is_error": false,
            "trusted_verification": {
                "status": "succeeded",
                "task_postcondition_verified": true,
                "trusted_verifier": true,
                "run_id": run_vf,
                "tool_call_id": "call_1",
                "action_name": "write_file",
                "evidence": "File 1 verified"
            }
        })
        .to_string(),
        created_at: 1000,
    })
    .expect("append 1");
    db.append_run_event(&RunEvent {
        id: "evt_vf2".to_string(),
        run_id: run_vf.to_string(),
        seq: 2,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_2",
            "name": "write_file",
            "output": "Disk write failed: permission denied",
            "is_error": true
        })
        .to_string(),
        created_at: 1001,
    })
    .expect("append 2");
    assert_eq!(
        db.get_run_outcome_from_events(run_vf).expect("outcome"),
        "failed",
        "Failure must take precedence over verified action"
    );

    // Case D: Two actions, one verified and one uncertain -> uncertain
    let run_vunc = "run_one_verified_one_uncertain";
    db.create_run(&make_run(run_vunc)).expect("create run");
    db.append_run_event(&RunEvent {
        id: "evt_vunc1".to_string(),
        run_id: run_vunc.to_string(),
        seq: 1,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_1",
            "name": "write_file",
            "output": "Wrote file 1",
            "is_error": false,
            "trusted_verification": {
                "status": "succeeded",
                "task_postcondition_verified": true,
                "trusted_verifier": true,
                "run_id": run_vunc,
                "tool_call_id": "call_1",
                "action_name": "write_file",
                "evidence": "File 1 verified"
            }
        })
        .to_string(),
        created_at: 1000,
    })
    .expect("append 1");
    db.append_run_event(&RunEvent {
        id: "evt_vunc2".to_string(),
        run_id: run_vunc.to_string(),
        seq: 2,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_2",
            "name": "computer_action",
            "output": serde_json::json!({
                "status": "uncertain",
                "verification": {
                    "status": "uncertain",
                    "evidence": "Visual change 1.2% <= 5%"
                }
            }).to_string(),
            "is_error": false
        })
        .to_string(),
        created_at: 1001,
    })
    .expect("append 2");
    assert_eq!(
        db.get_run_outcome_from_events(run_vunc).expect("outcome"),
        "uncertain",
        "Uncertain outcome must take precedence over success, never succeeded"
    );
}

#[test]
fn test_verification_binding_integrity_and_anti_forgery() {
    let temp = TempDir::new("test_binding_integrity");
    let db = Database::init(temp.path().to_path_buf()).expect("init db");
    db.create_conversation("conv_test", "Test Chat", "nemotron")
        .expect("conv");

    let run_id = "run_binding_target";
    db.create_run(&make_run(run_id)).expect("create run");

    // Attack 1: Genuine verification record referring to ANOTHER run
    let evt_wrong_run = RunEvent {
        id: "evt_alien_run".to_string(),
        run_id: run_id.to_string(),
        seq: 1,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_1",
            "name": "write_file",
            "output": "Wrote file",
            "is_error": false,
            "trusted_verification": {
                "status": "succeeded",
                "task_postcondition_verified": true,
                "trusted_verifier": true,
                "run_id": "run_DIFFERENT_ALIEN_RUN",
                "tool_call_id": "call_1",
                "action_name": "write_file",
                "evidence": "Stolen verification proof from run_DIFFERENT_ALIEN_RUN"
            }
        })
        .to_string(),
        created_at: 1000,
    };
    db.append_run_event(&evt_wrong_run).expect("append event");
    assert_ne!(
        db.get_run_outcome_from_events(run_id).expect("outcome"),
        "succeeded",
        "Verification record bound to a different run must NOT authorize success"
    );

    // Attack 2: Evidence bound to the WRONG action ID
    let run_wrong_tc = "run_binding_wrong_action";
    db.create_run(&make_run(run_wrong_tc)).expect("create run");
    let evt_wrong_action = RunEvent {
        id: "evt_wrong_tc".to_string(),
        run_id: run_wrong_tc.to_string(),
        seq: 1,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_action_ACTUAL",
            "name": "write_file",
            "output": "Wrote file",
            "is_error": false,
            "trusted_verification": {
                "status": "succeeded",
                "task_postcondition_verified": true,
                "trusted_verifier": true,
                "run_id": run_wrong_tc,
                "tool_call_id": "call_action_OTHER_MISMATCH",
                "action_name": "write_file",
                "evidence": "Evidence intended for another action"
            }
        })
        .to_string(),
        created_at: 1000,
    };
    db.append_run_event(&evt_wrong_action)
        .expect("append event");
    assert_ne!(
        db.get_run_outcome_from_events(run_wrong_tc)
            .expect("outcome"),
        "succeeded",
        "Evidence bound to wrong action ID must NOT authorize success"
    );

    // Attack 3: Failed/uncertain verification cannot later be replaced by a forged success
    let run_unforgeable = "run_unforgeable_failure";
    db.create_run(&make_run(run_unforgeable))
        .expect("create run");
    // Action fails first
    db.append_run_event(&RunEvent {
        id: "evt_fail_1".to_string(),
        run_id: run_unforgeable.to_string(),
        seq: 1,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_target",
            "name": "write_file",
            "output": "Disk error",
            "is_error": true
        })
        .to_string(),
        created_at: 1000,
    })
    .expect("append fail");
    // Followed by a late attempt to report success for the same tool_call_id
    db.append_run_event(&RunEvent {
        id: "evt_late_success".to_string(),
        run_id: run_unforgeable.to_string(),
        seq: 2,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_target",
            "name": "write_file",
            "output": "I am actually successful!",
            "is_error": false,
            "trusted_verification": {
                "status": "succeeded",
                "task_postcondition_verified": true,
                "trusted_verifier": true,
                "run_id": run_unforgeable,
                "tool_call_id": "call_target",
                "action_name": "write_file",
                "evidence": "Forged late success"
            }
        })
        .to_string(),
        created_at: 1001,
    })
    .expect("append late success");
    assert_eq!(
        db.get_run_outcome_from_events(run_unforgeable)
            .expect("outcome"),
        "failed",
        "A recorded action failure cannot be overwritten by a later event"
    );
}

#[test]
fn test_terminal_state_immutability_and_pending_approval() {
    let temp = TempDir::new("test_terminal_guards");
    let db = Database::init(temp.path().to_path_buf()).expect("init db");
    db.create_conversation("conv_test", "Test Chat", "nemotron")
        .expect("conv");

    // 1. Cancelled run followed by late success events stays cancelled
    let run_canc = "run_cancelled_late_events";
    db.create_run(&make_run(run_canc)).expect("create run");
    db.update_run_status(run_canc, "cancelled", Some(1000), Some(1005), None)
        .expect("mark cancelled");
    db.append_run_event(&RunEvent {
        id: "evt_late_v".to_string(),
        run_id: run_canc.to_string(),
        seq: 1,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_canc_1",
            "name": "write_file",
            "output": "late success",
            "is_error": false,
            "trusted_verification": {
                "status": "succeeded",
                "task_postcondition_verified": true,
                "trusted_verifier": true,
                "run_id": run_canc,
                "tool_call_id": "call_canc_1",
                "action_name": "write_file",
                "evidence": "late verified event"
            }
        })
        .to_string(),
        created_at: 2000,
    })
    .expect("append late event");
    assert_eq!(
        db.get_run_outcome_from_events(run_canc).expect("outcome"),
        "cancelled",
        "Cancelled terminal status must be immutable against late events"
    );

    // 2. Interrupted run recovered after restart stays interrupted
    let run_int = "run_interrupted_recovery";
    db.create_run(&make_run(run_int)).expect("create run");
    db.update_run_status(run_int, "running", Some(1000), None, None)
        .expect("start run");
    db.reconcile_active_runs_on_startup()
        .expect("reconcile startup");
    assert_eq!(
        db.get_run_outcome_from_events(run_int).expect("outcome"),
        "interrupted",
        "Interrupted run from startup reconciliation must retain interrupted outcome"
    );

    // 3. A run with pending approval at run termination cannot be marked successful
    let run_pending = "run_with_pending_approval";
    db.create_run(&make_run(run_pending)).expect("create run");
    db.create_approval_request(&ApprovalRequest {
        id: "app_req_test".to_string(),
        run_id: run_pending.to_string(),
        tool_name: "write_file".to_string(),
        summary: "Modifying system config".to_string(),
        args_json: "{\"path\":\"/etc/hosts\"}".to_string(),
        risk_level: "moderate".to_string(),
        reason: "Security policy".to_string(),
        status: "pending".to_string(),
        created_at: 1000,
        expires_at: 9999999999,
        resolved_at: None,
        resolved_by: None,
    })
    .expect("create approval");
    assert_eq!(
        db.get_run_outcome_from_events(run_pending)
            .expect("outcome"),
        "waiting_approval",
        "Run with pending approval cannot be resolved to succeeded"
    );
}

#[test]
fn test_read_only_tasks_policy() {
    let temp = TempDir::new("test_read_only_policy");
    let db = Database::init(temp.path().to_path_buf()).expect("init db");
    db.create_conversation("conv_test", "Test Chat", "nemotron")
        .expect("conv");

    // Pure read-only inspection tasks complete as succeeded without meaningless verifiers
    let run_ro = "run_read_only_inspection";
    db.create_run(&make_run(run_ro)).expect("create run");
    db.append_run_event(&RunEvent {
        id: "evt_ro1".to_string(),
        run_id: run_ro.to_string(),
        seq: 1,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_read1",
            "name": "read_file",
            "output": "contents of /tmp/file.txt",
            "is_error": false
        })
        .to_string(),
        created_at: 1000,
    })
    .expect("append ro1");
    db.append_run_event(&RunEvent {
        id: "evt_ro2".to_string(),
        run_id: run_ro.to_string(),
        seq: 2,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_list1",
            "name": "list_directory",
            "output": "file1.txt, file2.txt",
            "is_error": false
        })
        .to_string(),
        created_at: 1001,
    })
    .expect("append ro2");

    assert_eq!(
        db.get_run_outcome_from_events(run_ro).expect("outcome"),
        "succeeded",
        "Read-only tasks without consequential postconditions must complete successfully"
    );

    // Read-only task that encounters an error fails
    let run_ro_err = "run_read_only_error";
    db.create_run(&make_run(run_ro_err)).expect("create run");
    db.append_run_event(&RunEvent {
        id: "evt_ro_err".to_string(),
        run_id: run_ro_err.to_string(),
        seq: 1,
        event_type: "tool_result".to_string(),
        payload_json: serde_json::json!({
            "tool_call_id": "call_read2",
            "name": "read_file",
            "output": "File not found",
            "is_error": true
        })
        .to_string(),
        created_at: 1000,
    })
    .expect("append ro error");
    assert_eq!(
        db.get_run_outcome_from_events(run_ro_err).expect("outcome"),
        "failed"
    );
}

#[test]
fn test_malformed_persisted_events_fail_closed() {
    let temp = TempDir::new("test_malformed_events");
    let db = Database::init(temp.path().to_path_buf()).expect("init db");
    db.create_conversation("conv_test", "Test Chat", "nemotron")
        .expect("conv");

    let run_malformed = "run_malformed_payload";
    db.create_run(&make_run(run_malformed)).expect("create run");
    db.append_run_event(&RunEvent {
        id: "evt_malformed".to_string(),
        run_id: run_malformed.to_string(),
        seq: 1,
        event_type: "tool_result".to_string(),
        payload_json: "{ this is invalid json".to_string(),
        created_at: 1000,
    })
    .expect("append malformed");

    assert_eq!(
        db.get_run_outcome_from_events(run_malformed)
            .expect("outcome"),
        "failed",
        "Malformed persisted event must fail closed to failed"
    );
}
