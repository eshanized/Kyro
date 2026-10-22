use kyro_lib::audit::findings::{
    detect_dead_control, detect_findings, finalise_findings, BrokenImage, Confidence,
    ConsoleObservation, FailedRequestObservation, FieldValidation, FindingCategory,
    HttpErrorObservation, PageObservations,
};
use kyro_lib::audit::regression::VisualRegressionEngine;
use kyro_lib::audit::report::{render_report, AuditContext};
use kyro_lib::audit::routes::{
    extract_routes_from_source, merge_routes, normalise_route, route_from_file, DiscoveredRoute,
    RouteInventory, RouteKind, RouteSource,
};
use kyro_lib::audit::workflow::{AuditOptions, UiAuditWorkflow};
use kyro_lib::browser::BrowserAutomationManager;
use kyro_lib::project::devserver::DevServerManager;
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

#[test]
fn test_audit_route_normalization_and_extraction() {
    // 1. normalise_route
    assert_eq!(normalise_route("/"), "/");
    assert_eq!(normalise_route(""), "/");
    assert_eq!(normalise_route("//dashboard//"), "/dashboard");
    assert_eq!(normalise_route("settings/profile"), "/settings/profile");

    // 2. route_from_file Next.js app & pages router
    let root = Path::new("/mock/project");

    let page1 = root.join("app/dashboard/page.tsx");
    let r1 = route_from_file(&page1, root).unwrap();
    assert_eq!(r1.path, "/dashboard");
    assert_eq!(r1.kind, RouteKind::Static);

    let page_dynamic = root.join("app/users/[id]/page.tsx");
    let r2 = route_from_file(&page_dynamic, root).unwrap();
    assert_eq!(r2.path, "/users/[id]");
    assert_eq!(r2.kind, RouteKind::Dynamic);

    let page_group = root.join("app/(marketing)/about/page.tsx");
    let r3 = route_from_file(&page_group, root).unwrap();
    assert_eq!(r3.path, "/about");

    let page_root = root.join("app/page.tsx");
    let r4 = route_from_file(&page_root, root).unwrap();
    assert_eq!(r4.path, "/");

    let outside_file = Path::new("/outside/components/Button.tsx");
    assert!(route_from_file(outside_file, root).is_none());

    // 3. extract_routes_from_source (React Router JSX)
    let source_code = r#"
        import { Route, Routes } from "react-router-dom";
        export function App() {
            return (
                <Routes>
                    <Route path="/home" element={<Home />} />
                    <Route path="/catalog/:category" element={<Catalog />} />
                    <Route path="/auth/login" element={<Login />} />
                </Routes>
            );
        }
    "#;
    let extracted = extract_routes_from_source(source_code, Path::new("App.tsx"));
    assert!(extracted.iter().any(|r| r.path == "/home"));
    assert!(extracted.iter().any(|r| r.path == "/catalog/:category"));
    assert!(extracted.iter().any(|r| r.path == "/auth/login"));

    // 4. merge_routes with static precedence
    let g1 = vec![DiscoveredRoute::new(
        "/items/:id".into(),
        RouteKind::Dynamic,
        RouteSource::RouteFile,
    )];
    let g2 = vec![
        DiscoveredRoute::new(
            "/items/:id".into(),
            RouteKind::Static,
            RouteSource::RouteFile,
        ),
        DiscoveredRoute::new(
            "/profile".into(),
            RouteKind::Static,
            RouteSource::RouterConfig,
        ),
    ];
    let merged = merge_routes(vec![g1, g2]);
    assert_eq!(merged.routes.len(), 2);
    let item_route = merged
        .routes
        .iter()
        .find(|r| r.path == "/items/:id")
        .unwrap();
    assert_eq!(
        item_route.kind,
        RouteKind::Static,
        "Static should take precedence over dynamic"
    );
}

#[test]
fn test_audit_findings_detection_and_deduplication() {
    let obs = PageObservations {
        route: "/checkout".into(),
        url: "http://localhost:3000/checkout".into(),
        viewport: "1440x900".into(),
        title: "Checkout Page".into(),
        console_entries: vec![
            ConsoleObservation {
                level: "error".into(),
                text: "TypeError: Cannot read properties of undefined (reading 'cart')".into(),
            },
            ConsoleObservation {
                level: "log".into(),
                text: "User session initialized".into(),
            },
        ],
        failed_requests: vec![FailedRequestObservation {
            url: "http://localhost:3000/api/cart/items".into(),
            error: "net::ERR_CONNECTION_REFUSED".into(),
        }],
        http_error_responses: vec![
            HttpErrorObservation {
                url: "http://localhost:3000/api/promotions".into(),
                status: 500,
            },
            HttpErrorObservation {
                url: "http://localhost:3000/api/protected_profile".into(),
                status: 401, // 401 should NOT be classified as bug
            },
        ],
        broken_images: vec![BrokenImage {
            src: Some("/logo.png".into()),
            alt: Some("".into()),
        }],
        horizontal_overflow: true,
        h1_count: 2, // Multiple H1 tags -> accessibility warning
        stuck_loading_indicator: None,
        external_links: vec!["https://external.com".into()],
        interactive_element_names: vec!["Submit".into()],
        unnamed_interactive_elements: 1, // Button without label
        declares_language: false,
        field_validations: vec![FieldValidation {
            name: "email".into(),
            field_type: "email".into(),
            declared_type: "text".into(),
            required: true,
            pattern: None,
            minlength: None,
            accepts_invalid: true,
            validation_message: None,
        }],
        fields_to_validate: Vec::new(),
    };

    let findings = detect_findings(&obs);
    assert!(findings.len() >= 4);

    // Verify console error bug
    assert!(findings
        .iter()
        .any(|f| f.category == FindingCategory::Bug && f.title.contains("TypeError")));

    // Verify network error bug
    assert!(findings
        .iter()
        .any(|f| f.category == FindingCategory::Bug && f.title.contains("Network request failed")));

    // Verify HTTP 500 bug
    assert!(findings
        .iter()
        .any(|f| f.category == FindingCategory::Bug && f.title.contains("HTTP 500")));

    // Verify 401 was omitted
    assert!(!findings.iter().any(|f| f.title.contains("HTTP 401")));

    // Verify horizontal overflow design inconsistency
    assert!(findings
        .iter()
        .any(|f| f.category == FindingCategory::DesignInconsistency));

    // Verify accessibility findings
    assert!(findings
        .iter()
        .any(|f| f.category == FindingCategory::Accessibility));

    // 2. Dead control detection
    let dead_finding = detect_dead_control(
        "/settings",
        "http://localhost:3000/settings",
        "1440x900",
        "Save Changes button",
        "http://localhost:3000/settings",
        "http://localhost:3000/settings",
        false, // dom did not change
    );
    assert_eq!(dead_finding.category, FindingCategory::Bug);
    assert_eq!(dead_finding.confidence, Confidence::Unverified);

    // 3. finalise_findings deduplication & grouping
    let finalised = finalise_findings(findings);
    assert!(!finalised.is_empty());
    assert!(finalised.iter().all(|f| !f.id.is_empty()));
}

#[test]
fn test_audit_report_rendering() {
    let obs = PageObservations {
        route: "/".into(),
        url: "http://localhost:3000/".into(),
        viewport: "1440x900".into(),
        title: "Home".into(),
        console_entries: vec![ConsoleObservation {
            level: "error".into(),
            text: "Uncaught ReferenceError: analytics is not defined".into(),
        }],
        ..Default::default()
    };

    let raw_findings = detect_findings(&obs);
    let findings = finalise_findings(raw_findings);

    let ctx = AuditContext {
        project_path: "/workspace/my-app".into(),
        startup_command: "npm run dev".into(),
        framework: Some("Next.js".into()),
        package_manager: Some("npm".into()),
        browser: "Chromium".into(),
        browser_version: Some("120.0".into()),
        preview_url: "http://localhost:3000".into(),
        routes_discovered: 5,
        routes_inspected: 3,
        visited_routes: vec!["/".into(), "/about".into(), "/contact".into()],
        routes_blocked_or_failed: 0,
        console_errors: 1,
        failed_requests: 0,
        screenshot_count: 3,
        dev_server_running: true,
        blocked_routes: Vec::new(),
        failed_routes: Vec::new(),
        duration_ms: 1500,
    };

    let inventory = RouteInventory {
        routes: vec![
            DiscoveredRoute::new("/".into(), RouteKind::Static, RouteSource::RouteFile),
            DiscoveredRoute::new("/about".into(), RouteKind::Static, RouteSource::RouteFile),
        ],
        visitable: 2,
        blocked: 0,
        summary: "All routes visitable".into(),
    };

    let markdown = render_report(&ctx, &findings, &inventory);

    assert!(markdown.contains("# UI Audit Report"));
    assert!(markdown.contains("analytics is not defined"));
    assert!(markdown.contains("## Route coverage"));
    assert!(markdown.contains("## Limitations"));
}

#[test]
fn test_audit_visual_regression_baseline() {
    let td = TestDir::new("audit_reg_test");
    let project_dir = td.path();

    // 1. Sanitize component
    assert_eq!(
        VisualRegressionEngine::sanitize_component("header/nav:main"),
        "header_nav_main"
    );

    // 2. Baselines directory
    let b_dir = VisualRegressionEngine::baselines_dir(project_dir);
    assert!(b_dir.to_string_lossy().contains("baselines"));

    // 3. Save baseline
    let dummy_png_b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
    let saved = VisualRegressionEngine::save_baseline(
        project_dir,
        "hero_section",
        "desktop",
        dummy_png_b64,
        None,
    );
    assert!(saved.is_ok());

    // 4. Load baseline
    let loaded = VisualRegressionEngine::load_baseline(project_dir, "hero_section", "desktop");
    assert!(loaded.is_some());
    let baseline = loaded.unwrap();
    assert_eq!(baseline.route, "hero_section");
    assert_eq!(baseline.viewport, "desktop");

    // 5. Compare identical -> 0.0 diff
    let res_identical = VisualRegressionEngine::compare(
        dummy_png_b64,
        dummy_png_b64,
        "hero_section",
        "desktop",
        0.01,
    );
    assert!(res_identical.is_ok());
    let comp = res_identical.unwrap();
    assert!(!comp.has_regression);
    assert_eq!(comp.diff_percent, 0.0);
}

#[tokio::test]
async fn test_audit_workflow_planning() {
    let browser = Arc::new(BrowserAutomationManager::new());
    let dev_servers = Arc::new(DevServerManager::new());
    let workflow = UiAuditWorkflow::new(browser, dev_servers);

    let td = TestDir::new("audit_plan_test");
    let proj_dir = td.path();

    // Create a recognized project manifest
    let pkg = proj_dir.join("package.json");
    std::fs::write(
        &pkg,
        r#"{"name":"audit-test-app","scripts":{"dev":"vite"}}"#,
    )
    .unwrap();

    // Case 1: Plan with explicit project path
    let options = AuditOptions {
        project_name: "audit-test-app".into(),
        project_path: Some(proj_dir.to_path_buf()),
        run_id: Some("run_audit_1".into()),
        ..Default::default()
    };
    let plan = workflow.plan(&options).await;
    assert!(plan.is_ok(), "Expected plan ok, got: {:?}", plan);
    let p = plan.unwrap();
    assert_eq!(
        p.project.name,
        proj_dir.file_name().unwrap().to_str().unwrap()
    );
    assert!(
        p.command.executable.contains("npm")
            || p.command.executable.contains("npx")
            || p.command.executable.contains("vite")
    );

    // Case 2: Plan with empty name and path -> Err
    let bad_options = AuditOptions {
        project_name: "".into(),
        project_path: None,
        ..Default::default()
    };
    let err = workflow.plan(&bad_options).await;
    assert!(err.is_err());
    assert!(err.unwrap_err().contains("must be specified"));

    // Case 3: Plan with non-existent path -> Err
    let missing_path_options = AuditOptions {
        project_name: "non-existent".into(),
        project_path: Some(PathBuf::from("/non/existent/project/path/12345")),
        ..Default::default()
    };
    let err2 = workflow.plan(&missing_path_options).await;
    assert!(err2.is_err());
    assert!(err2.unwrap_err().contains("does not exist"));
}
