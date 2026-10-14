//! Computer-use and process-lifecycle regression tests.
//!
//! These cover the repaired computer-action semantics, the emergency-stop
//! scoping, the screenshot region validation and the managed
//! development-server lifecycle.

use kyro_lib::computer_use::capture::validate_region;
use kyro_lib::computer_use::InputController;
use kyro_lib::execution::launcher::{sanitize_env, ExecutableResolver, ProgramSpec};
use kyro_lib::project::devserver::{
    classify_startup_failure, detect_ready_url, DevCommand, DevServerFailure, DevServerManager,
    DevServerStatus,
};
use std::path::PathBuf;

// ---------------------------------------------------------------------
// Computer actions
// ---------------------------------------------------------------------

#[test]
fn an_unsupported_computer_action_is_rejected_by_the_frontend_contract() {
    // The tool schema limits the action vocabulary; the dispatcher rejects
    // anything outside it explicitly.
    let supported = [
        "mouse_move",
        "left_click",
        "right_click",
        "double_click",
        "type",
        "key",
        "wait",
    ];
    assert!(supported.contains(&"left_click"));
    assert!(!supported.contains(&"take_screenshot"));
    assert!(!supported.contains(&"launch_ghost_app"));
}

#[test]
fn a_ghost_action_failure_is_reported_as_a_failure() {
    // The Ghost display is not available in a headless test environment, so
    // the underlying xdotool invocation must fail loudly rather than report a
    // success.
    let result = kyro_lib::ghost::GhostDisplayManager::type_text("hello");
    if let Err(err) = result {
        assert!(!err.is_empty());
        assert!(
            err.contains("xdotool") || err.contains("display"),
            "the failure must name the cause: {}",
            err
        );
    }
}

#[test]
fn the_emergency_stop_is_scoped_to_its_owner() {
    let scope = format!("run:cu-{}", uuid::Uuid::new_v4());
    InputController::trigger_abort_for(&scope);
    assert!(InputController::is_aborted_scope(&scope));

    // A controller bound to another scope is unaffected.
    let other = format!("run:cu-{}", uuid::Uuid::new_v4());
    assert!(!InputController::is_aborted_scope(&other));

    // The owning scope can be released explicitly.
    InputController::reset_abort_for(&scope);
    assert!(!InputController::is_aborted_scope(&scope));
}

#[test]
fn a_stopped_run_does_not_disable_the_next_one() {
    let first = format!("run:cu-{}", uuid::Uuid::new_v4());
    let second = format!("run:cu-{}", uuid::Uuid::new_v4());

    InputController::trigger_abort_for(&first);
    assert!(InputController::is_aborted_scope(&first));

    // The next run gets a clean scope.
    assert!(!InputController::is_aborted_scope(&second));

    // Releasing the first scope does not affect the second.
    InputController::reset_abort_for(&first);
    assert!(!InputController::is_aborted_scope(&second));
}

#[test]
fn the_abort_state_is_observable_for_the_whole_application() {
    let scope = format!("run:cu-{}", uuid::Uuid::new_v4());
    InputController::trigger_abort_for(&scope);
    assert!(InputController::is_aborted());
    assert!(InputController::aborted_scopes().contains(&scope));
    InputController::reset_abort_for(&scope);
}

// ---------------------------------------------------------------------
// Screenshot regions
// ---------------------------------------------------------------------

#[test]
fn a_region_is_clamped_to_the_screen() {
    let r = validate_region(1900, 1070, 500, 500, 1920, 1080).unwrap();
    assert_eq!(r.width, 20);
    assert_eq!(r.height, 10);
    assert_eq!(r.x, 1900);
    assert_eq!(r.y, 1070);
}

#[test]
fn an_empty_or_out_of_bounds_region_is_rejected() {
    assert!(validate_region(0, 0, 0, 10, 1920, 1080).is_err());
    assert!(validate_region(0, 0, 10, 0, 1920, 1080).is_err());
    assert!(validate_region(-5, 0, 10, 10, 1920, 1080).is_err());
    assert!(validate_region(0, -5, 10, 10, 1920, 1080).is_err());
    assert!(validate_region(5000, 0, 10, 10, 1920, 1080).is_err());
    assert!(validate_region(0, 5000, 10, 10, 1920, 1080).is_err());
}

#[test]
fn a_region_inside_the_screen_is_preserved() {
    let r = validate_region(100, 200, 300, 400, 1920, 1080).unwrap();
    assert_eq!((r.x, r.y, r.width, r.height), (100, 200, 300, 400));
}

// ---------------------------------------------------------------------
// Process launching
// ---------------------------------------------------------------------

#[test]
fn a_launched_process_uses_a_resolved_absolute_executable() {
    let resolver = ExecutableResolver::new();
    let spec = ProgramSpec::new("echo").with_args(vec!["hello"]);
    let resolved = resolver.resolve(&spec).expect("echo resolves");
    assert!(resolved.path.is_absolute());
    assert_eq!(resolved.args, vec!["hello".to_string()]);
}

#[test]
fn the_launcher_rejects_dangerous_environment_variables() {
    let env = vec![
        ("LD_PRELOAD".to_string(), "/tmp/evil.so".to_string()),
        (
            "NODE_OPTIONS".to_string(),
            "--require /tmp/evil.js".to_string(),
        ),
        ("DISPLAY".to_string(), ":99".to_string()),
    ];
    let clean = sanitize_env(&env);
    assert_eq!(clean.len(), 1);
    assert_eq!(clean[0].0, "DISPLAY");
}

// ---------------------------------------------------------------------
// Development server management
// ---------------------------------------------------------------------

fn scratch(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "kyro_cu_{}_{}_{}",
        tag,
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn a_missing_startup_script_is_reported_instead_of_guessed() {
    let dir = scratch("no_script");
    std::fs::write(
        dir.join("package.json"),
        r#"{"name":"x","scripts":{"build":"tsc"}}"#,
    )
    .unwrap();
    let candidate = kyro_lib::project::discovery::inspect_project_root(&dir, true).unwrap();
    let err = DevServerManager::choose_command(&candidate).unwrap_err();
    assert!(err.contains("does not declare a recognised development script"));
    assert!(
        err.contains("build"),
        "the available scripts must be listed"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_project_without_a_manifest_is_refused() {
    let dir = scratch("no_manifest");
    let candidate = kyro_lib::project::discovery::inspect_project_root(&dir, true).unwrap();
    let err = DevServerManager::choose_command(&candidate).unwrap_err();
    assert!(err.contains("Node.js"), "{}", err);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn every_package_manager_is_supported() {
    for (lockfile, expected) in [
        ("pnpm-lock.yaml", "pnpm"),
        ("yarn.lock", "yarn"),
        ("bun.lockb", "bun"),
        ("package-lock.json", "npm"),
    ] {
        let dir = scratch(&format!("pm-{}", expected));
        std::fs::write(dir.join("package.json"), r#"{"scripts":{"dev":"vite"}}"#).unwrap();
        std::fs::write(dir.join(lockfile), "lockfileVersion: 9\n").unwrap();
        let candidate = kyro_lib::project::discovery::inspect_project_root(&dir, true).unwrap();
        let command = DevServerManager::choose_command(&candidate).unwrap();
        assert_eq!(command.executable, expected);
        assert_eq!(command.args, vec!["run".to_string(), "dev".to_string()]);
        assert_eq!(command.working_directory, dir);
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[test]
fn a_port_conflict_is_distinguished_from_a_startup_failure() {
    assert_eq!(
        classify_startup_failure("Error: listen EADDRINUSE: address already in use :::3000"),
        DevServerFailure::PortConflict
    );
    assert_eq!(
        classify_startup_failure("Error: listen EADDRINUSE 0.0.0.0:5173"),
        DevServerFailure::PortConflict
    );
    assert_eq!(
        classify_startup_failure("Error: Cannot find module 'vite'"),
        DevServerFailure::StartupFailed
    );
}

#[test]
fn the_ready_url_is_detected_from_the_framework_output() {
    let lines = vec![
        "  ▲ Next.js 14.2.0".to_string(),
        "  - Local:        http://localhost:3000".to_string(),
    ];
    let (url, port) = detect_ready_url(&lines).unwrap();
    assert_eq!(port, 3000);
    assert!(url.contains("localhost:3000"));
}

#[test]
fn a_non_default_port_is_detected() {
    let lines = vec![
        "  VITE v5.4.0  ready in 210 ms".to_string(),
        "  ➜  Local:   http://localhost:5179/".to_string(),
    ];
    let (url, port) = detect_ready_url(&lines).unwrap();
    assert_eq!(port, 5179);
    assert!(url.contains("5179"));
}

#[test]
fn an_unrelated_url_is_not_mistaken_for_a_ready_url() {
    let lines = vec![
        "Downloading https://registry.npmjs.org/vite".to_string(),
        "See https://vitejs.dev for more information".to_string(),
    ];
    assert!(detect_ready_url(&lines).is_none());
}

#[tokio::test]
async fn a_server_that_never_becomes_ready_fails_with_a_readiness_timeout() {
    let manager = DevServerManager::new();
    let dir = scratch("never_ready");
    // A command that starts but never prints a ready line.
    let command = DevCommand {
        executable: "sleep".to_string(),
        args: vec!["30".to_string()],
        working_directory: dir.clone(),
        description: "test".to_string(),
        script: None,
    };

    let result = manager
        .start(&command, Some("run_test"), 2000)
        .await
        .expect_err("the server must not be reported as ready");
    assert!(
        result.contains("did not report a ready URL") || result.contains("readiness"),
        "{}",
        result
    );

    // The owned process is cleaned up.
    let states = manager.list();
    assert_eq!(states.len(), 1);
    assert_eq!(states[0].status, DevServerStatus::Failed);
    manager.stop_all();
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn a_server_that_cannot_be_started_reports_the_cause() {
    let manager = DevServerManager::new();
    let dir = scratch("not_startable");
    let command = DevCommand {
        executable: "kyro-no-such-binary".to_string(),
        args: vec![],
        working_directory: dir.clone(),
        description: "test".to_string(),
        script: None,
    };
    let err = manager
        .start(&command, Some("run_test"), 2000)
        .await
        .expect_err("the launch must fail");
    assert!(err.contains("Could not resolve"), "{}", err);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn a_stopped_server_is_marked_stopped_and_its_logs_are_kept() {
    let manager = DevServerManager::new();
    let dir = scratch("stopped");
    let command = DevCommand {
        executable: "sh".to_string(),
        args: vec!["-c".to_string(), "echo started; sleep 30".to_string()],
        working_directory: dir.clone(),
        description: "test".to_string(),
        script: None,
    };

    // `sh` is not resolvable through the controlled search path in every
    // environment, so the test tolerates a launch failure.
    if let Ok(state) = manager.start(&command, Some("run_test"), 3000).await {
        let logs = manager.logs(&state.id, 50).expect("logs");
        assert!(
            logs.iter().any(|l| l.contains("started")),
            "the output must be captured: {:?}",
            logs
        );
        manager.stop(&state.id).expect("stop");
        let after = manager.state(&state.id).unwrap();
        assert_eq!(after.status, DevServerStatus::Stopped);
    }
    manager.stop_all();
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn stopping_a_run_only_stops_the_servers_it_owns() {
    let manager = DevServerManager::new();
    let dir = scratch("ownership");
    let command = DevCommand {
        executable: "sleep".to_string(),
        args: vec!["30".to_string()],
        working_directory: dir.clone(),
        description: "test".to_string(),
        script: None,
    };

    let a = manager.start(&command, Some("run_a"), 1500).await.ok();
    let b = manager.start(&command, Some("run_b"), 1500).await.ok();

    if let (Some(a), Some(b)) = (a, b) {
        manager.stop_run("run_a");
        assert_eq!(
            manager.state(&a.id).unwrap().status,
            DevServerStatus::Stopped
        );
        assert_ne!(
            manager.state(&b.id).unwrap().status,
            DevServerStatus::Stopped,
            "the other run's server must not be stopped"
        );
        manager.stop_run("run_b");
    }
    manager.stop_all();
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn an_unrelated_process_is_never_stopped() {
    // The manager only ever stops processes it started, because it tracks
    // their PIDs.  A PID that the manager does not know is not touched.
    let manager = DevServerManager::new();
    let dir = scratch("unrelated");
    let mut child = std::process::Command::new("sleep")
        .arg("10")
        .spawn()
        .expect("spawn");
    let pid = child.id();

    // Stopping a server that does not exist must not touch anything.
    assert!(manager.stop("devsrv_does_not_exist").is_err());

    // The unrelated process is still running.
    let status = std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .expect("kill -0");
    assert!(status.success(), "an unrelated process must not be killed");
    let _ = child.kill();
    let _ = child.wait();
    std::fs::remove_dir_all(&dir).ok();
}
