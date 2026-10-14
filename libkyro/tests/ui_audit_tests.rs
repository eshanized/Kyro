//! End-to-end tests for the autonomous UI audit workflow.
//!
//! The tests run Kyro's real audit workflow against a deliberately defective
//! fixture application and check that the known defects are found with
//! reproducible evidence.
//!
//! These tests need Firefox and Node.js.  When they are not available the
//! test is skipped with an explicit message; the suite is marked `#[ignore]`
//! so that it is only executed deliberately:
//!
//! ```text
//! cargo test --test ui_audit_tests -- --ignored --nocapture
//! ```

use kyro_lib::audit::workflow::{AuditOptions, UiAuditWorkflow};
use kyro_lib::browser::BrowserAutomationManager;
use kyro_lib::project::devserver::DevServerManager;
use std::path::PathBuf;
use std::sync::Arc;

/// The fixture application that ships with the test suite.
fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("defect-app")
}

/// `true` when the browser and the worker runtime are available.
async fn prerequisites_available() -> (bool, String) {
    if kyro_lib::browser::worker::find_node().is_none() {
        return (
            false,
            "Node.js is not installed; the browser automation worker cannot start.".to_string(),
        );
    }
    let manager = BrowserAutomationManager::new();
    match manager.capabilities().await {
        Ok(caps) => {
            let has_firefox = caps.browsers.iter().any(|b| b.available);
            if has_firefox {
                (true, String::new())
            } else {
                (
                    false,
                    "No usable Firefox binary was found. Install Firefox (for example \
'sudo apt install firefox-esr') and re-run."
                        .to_string(),
                )
            }
        }
        Err(e) => (false, format!("Browser capabilities unavailable: {}", e)),
    }
}

#[tokio::test]
#[ignore = "requires Firefox and Node.js; run with --ignored"]
async fn test_ui_audit_finds_the_known_defects() {
    let (available, reason) = prerequisites_available().await;
    if !available {
        eprintln!("SKIPPED: {}", reason);
        return;
    }

    let fixture = fixture_dir();
    assert!(
        fixture.is_dir(),
        "The fixture application is missing: {}",
        fixture.display()
    );

    let browser = Arc::new(BrowserAutomationManager::new());
    let dev_servers = Arc::new(DevServerManager::new());
    let workflow = UiAuditWorkflow::new(browser.clone(), dev_servers.clone());

    let options = AuditOptions {
        project_name: "defect-app".to_string(),
        search_root: fixture.parent().unwrap().to_path_buf(),
        browser: "firefox".to_string(),
        headless: true,
        keep_server_running: false,
        viewports: vec![(1280, 800), (390, 844)],
        max_routes: 20,
        ready_timeout_ms: 60000,
        run_id: Some("run_ui_audit_test".to_string()),
    };

    // ---- Plan ------------------------------------------------------------
    let plan = workflow
        .plan(&options)
        .await
        .expect("the plan should be produced");
    assert_eq!(
        plan.project.path.file_name().unwrap().to_string_lossy(),
        "defect-app"
    );
    assert!(plan.command.display().contains("dev"), "{:?}", plan.command);
    assert_eq!(plan.command.working_directory, fixture);

    // ---- Execute ---------------------------------------------------------
    let mut progress: Vec<String> = Vec::new();
    let mut on_progress = |p: kyro_lib::audit::workflow::AuditProgress| {
        progress.push(format!("[{}] {}", p.stage.as_str(), p.message));
    };
    let outcome = workflow
        .execute(&options, plan, &mut on_progress)
        .await
        .expect("the audit should complete");

    // The development server must have been verified as ready, not guessed.
    let ready_url = outcome
        .dev_server
        .as_ref()
        .and_then(|s| s.ready_url.clone())
        .expect("the development server must report a verified ready URL");
    assert!(
        ready_url.starts_with("http://127.0.0.1:") || ready_url.starts_with("http://localhost:"),
        "unexpected ready URL: {}",
        ready_url
    );
    assert!(outcome
        .dev_server
        .as_ref()
        .map(|s| s.port.is_some())
        .unwrap_or(false));

    // The routes must have been discovered.
    let discovered_paths: Vec<String> = outcome
        .route_inventory
        .routes
        .iter()
        .map(|r| r.path.clone())
        .collect();
    assert!(
        discovered_paths.iter().any(|p| p == "/"),
        "the home route must be discovered: {:?}",
        discovered_paths
    );
    assert!(
        discovered_paths.iter().any(|p| p == "/about"),
        "the about route must be discovered: {:?}",
        discovered_paths
    );

    // The home route and at least the other visitable routes must have been
    // inspected.
    assert!(
        outcome.context.routes_inspected >= 2,
        "at least two routes should have been inspected, got {}",
        outcome.context.routes_inspected
    );

    // ---- The known defects ----------------------------------------------
    let findings = &outcome.findings;
    let titles: Vec<String> = findings.iter().map(|f| f.title.clone()).collect();

    // 1. The visible JavaScript error.
    let console_finding = findings
        .iter()
        .find(|f| f.title.contains("JavaScript error"));
    assert!(
        console_finding.is_some(),
        "the JavaScript error must be reported; findings were: {:#?}",
        titles
    );
    let console_finding = console_finding.unwrap();
    assert_eq!(console_finding.route, "/");
    assert!(
        !console_finding.console_evidence.is_empty(),
        "the console evidence must be attached"
    );
    assert_eq!(
        console_finding.confidence,
        kyro_lib::audit::Confidence::Reproduced
    );

    // 2. The missing image.
    assert!(
        findings
            .iter()
            .any(|f| f.title.contains("Image failed to load")),
        "the missing image must be reported; findings were: {:#?}",
        titles
    );

    // 3. The broken internal route.
    let broken_route = findings
        .iter()
        .find(|f| f.title.contains("HTTP 404") || f.route == "/missing-route");
    assert!(
        broken_route.is_some(),
        "the broken internal route must be reported; findings were: {:#?}",
        titles
    );

    // 4. The form with broken validation.
    let validation = findings
        .iter()
        .find(|f| f.title.contains("accepts an invalid"));
    assert!(
        validation.is_some(),
        "the broken form validation must be reported; findings were: {:#?}",
        titles
    );

    // 5. The narrow-viewport overflow.
    let overflow = findings
        .iter()
        .find(|f| f.title.contains("scrolls horizontally"));
    assert!(
        overflow.is_some(),
        "the narrow viewport overflow must be reported; findings were: {:#?}",
        titles
    );
    let overflow = overflow.unwrap();
    assert!(
        overflow.viewport.starts_with("390"),
        "the overflow must be reported for the narrow viewport, got {}",
        overflow.viewport
    );

    // ---- The report ------------------------------------------------------
    let report = &outcome.report_markdown;
    assert!(report.contains("# UI Audit Report"));
    assert!(report.contains("## Confirmed bugs"));
    assert!(report.contains("## Route coverage"));
    assert!(report.contains("## Limitations"));
    assert!(report.contains(&outcome.context.project_path));
    assert!(report.contains("firefox"));
    assert!(report.contains("Verified preview URL"));
    // The report must not claim complete coverage when routes were blocked.
    assert!(
        report.contains("was **not** inspected") || report.contains("Routes discovered |"),
        "the report must state the coverage it achieved"
    );
    // The screenshots must have been collected.
    assert!(
        !outcome.screenshots.is_empty(),
        "the audit must capture screenshots as evidence"
    );

    // The findings must be sorted by severity, then confidence.
    let confirmed: Vec<&kyro_lib::audit::Finding> = findings
        .iter()
        .filter(|f| f.category == kyro_lib::audit::FindingCategory::Bug && f.duplicate_of.is_none())
        .collect();
    for pair in confirmed.windows(2) {
        let a = pair[0].severity;
        let b = pair[1].severity;
        assert!(
            a >= b,
            "findings must be sorted by severity: {:?} then {:?}",
            a,
            b
        );
    }

    // Persist the report so that it can be inspected after the run.
    let report_path = std::env::temp_dir().join("kyro-ui-audit-report.md");
    std::fs::write(&report_path, report).expect("the report should be written");
    eprintln!(
        "\n--- REPORT (also written to {}) ---\n{}",
        report_path.display(),
        report
    );

    // ---- Cleanup ---------------------------------------------------------
    // The browser session must be released and the server stopped.
    assert!(
        browser.list_sessions().await.is_empty(),
        "the browser session must be closed after the audit"
    );
    let states = dev_servers.list();
    assert_eq!(states.len(), 1);
    assert_eq!(
        states[0].status,
        kyro_lib::project::DevServerStatus::Stopped,
        "the development server must be stopped when the audit finishes"
    );
}

#[tokio::test]
#[ignore = "requires Firefox and Node.js; run with --ignored"]
async fn test_ui_audit_reports_missing_projects_honestly() {
    let (available, reason) = prerequisites_available().await;
    if !available {
        eprintln!("SKIPPED: {}", reason);
        return;
    }

    let browser = Arc::new(BrowserAutomationManager::new());
    let dev_servers = Arc::new(DevServerManager::new());
    let workflow = UiAuditWorkflow::new(browser.clone(), dev_servers.clone());

    let options = AuditOptions {
        project_name: "a-project-that-does-not-exist".to_string(),
        search_root: fixture_dir(),
        browser: "firefox".to_string(),
        headless: true,
        ..Default::default()
    };

    let err = workflow
        .plan(&options)
        .await
        .expect_err("the plan must fail");
    assert!(
        err.contains("No project was found"),
        "the error must explain that nothing was found: {}",
        err
    );
}
