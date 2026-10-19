//! Browser automation integration tests.
//!
//! These tests drive the real Firefox through Kyro's browser subsystem.
//! They need Firefox and Node.js, so they are marked `#[ignore]` and are run
//! deliberately:
//!
//! ```text
//! cargo test --test browser_tests -- --ignored --nocapture
//! ```

use kyro_lib::browser::{BrowserAutomationManager, ElementTarget};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

/// A tiny local HTTP server that serves the fixture page.
struct FixtureServer {
    port: u16,
    handle: Option<thread::JoinHandle<()>>,
}

impl FixtureServer {
    fn start(body: &'static str) -> FixtureServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream: TcpStream = match stream {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let mut buffer = [0u8; 2048];
                let _ = stream.read(&mut buffer);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        FixtureServer {
            port,
            handle: Some(handle),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }
}

impl Drop for FixtureServer {
    fn drop(&mut self) {
        // The listener thread is detached; the process exit ends it.
        self.handle.take();
    }
}

async fn prerequisites() -> Option<String> {
    if kyro_lib::browser::worker::find_node().is_none() {
        return Some(
            "Node.js is not installed; the browser automation worker cannot start.".to_string(),
        );
    }
    let manager = BrowserAutomationManager::new();
    match manager.capabilities().await {
        Ok(caps) => {
            if caps.browsers.iter().any(|b| b.available && b.automatable) {
                None
            } else {
                Some("No usable automatable browser binary was found. Install a supported browser and re-run.".to_string())
            }
        }
        Err(e) => Some(format!("Browser capabilities unavailable: {}", e)),
    }
}

/// The fixture page.
const FIXTURE: &str = r#"<html><head><title>Kyro Fixture</title></head><body>
<h1>Fixture heading</h1>
<button id=go onclick="document.getElementById('out').textContent='clicked'">Press me</button>
<a href="/next">Next page</a>
<input id=name aria-label="Your name">
<div id=out>idle</div>
<script>console.error('fixture console error')</script>
</body></html>"#;

#[tokio::test]
#[ignore = "requires Firefox and Node.js; run with --ignored"]
async fn browser_session_starts_and_reports_its_identity() {
    if let Some(reason) = prerequisites().await {
        eprintln!("SKIPPED: {}", reason);
        return;
    }

    let manager = BrowserAutomationManager::new();
    let session = manager
        .create_session("firefox", true, Some("run_browser_test"), &[])
        .await
        .expect("the browser session should start");

    assert_eq!(session.browser, "firefox");
    assert!(session.browser_version.is_some());
    assert!(session.profile_isolated, "the profile must be isolated");
    assert_eq!(session.driver, "webdriver-bidi");
    assert!(!session.browser_path.is_empty());

    // The session is registered and can be found again.
    let listed = manager.list_sessions().await;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].session_id, session.session_id);

    manager
        .close_session(&session.session_id)
        .await
        .expect("close");
    assert!(manager.list_sessions().await.is_empty());
}

#[tokio::test]
#[ignore = "requires Firefox and Node.js; run with --ignored"]
async fn browser_navigation_succeeds_and_reports_the_page() {
    if let Some(reason) = prerequisites().await {
        eprintln!("SKIPPED: {}", reason);
        return;
    }

    let server = FixtureServer::start(FIXTURE);
    let manager = BrowserAutomationManager::new();
    let session = manager
        .create_session("firefox", true, Some("run_browser_test"), &[])
        .await
        .expect("session");

    let nav = manager
        .navigate(&session.session_id, &server.url("/"), 30000, false)
        .await
        .expect("navigation");
    assert_eq!(nav.status, "success", "{:?}", nav);
    assert_eq!(nav.title.as_deref(), Some("Kyro Fixture"));
    assert_eq!(nav.navigation_status.as_deref(), Some("complete"));

    let info = manager.page_info(&session.session_id).await.expect("info");
    assert_eq!(info.status, "success");
    assert!(
        info.url
            .as_deref()
            .unwrap_or("")
            .starts_with("http://127.0.0.1:"),
        "the current URL must be reported: {:?}",
        info.url
    );

    manager.close_session(&session.session_id).await.ok();
}

#[tokio::test]
#[ignore = "requires Firefox and Node.js; run with --ignored"]
async fn browser_navigation_to_a_blocked_destination_fails_with_a_reason() {
    if let Some(reason) = prerequisites().await {
        eprintln!("SKIPPED: {}", reason);
        return;
    }

    let manager = BrowserAutomationManager::new();
    let session = manager
        .create_session(
            "firefox",
            true,
            Some("run_browser_test"),
            &["paypal.com".to_string()],
        )
        .await
        .expect("session");

    let nav = manager
        .navigate(
            &session.session_id,
            "https://www.paypal.com/checkout",
            20000,
            false,
        )
        .await
        .expect("navigation");
    assert_eq!(nav.status, "failed");
    assert_eq!(nav.error_category.as_deref(), Some("navigation_blocked"));
    assert!(nav.error_message.unwrap_or_default().contains("paypal.com"));

    manager.close_session(&session.session_id).await.ok();
}

#[tokio::test]
#[ignore = "requires Firefox and Node.js; run with --ignored"]
async fn browser_snapshot_reports_semantic_elements() {
    if let Some(reason) = prerequisites().await {
        eprintln!("SKIPPED: {}", reason);
        return;
    }

    let server = FixtureServer::start(FIXTURE);
    let manager = BrowserAutomationManager::new();
    let session = manager
        .create_session("firefox", true, Some("run_browser_test"), &[])
        .await
        .expect("session");
    manager
        .navigate(&session.session_id, &server.url("/"), 30000, false)
        .await
        .expect("navigation");

    let snapshot = manager
        .snapshot(&session.session_id)
        .await
        .expect("snapshot");
    assert_eq!(snapshot.status, "success");

    let elements = snapshot.data["elements"].as_array().expect("elements");
    assert!(!elements.is_empty());

    let roles: Vec<String> = elements
        .iter()
        .filter_map(|e| e["role"].as_str().map(|s| s.to_string()))
        .collect();
    assert!(roles.iter().any(|r| r == "button"), "{:?}", roles);
    assert!(roles.iter().any(|r| r == "link"), "{:?}", roles);
    assert!(roles.iter().any(|r| r == "textbox"), "{:?}", roles);

    // The accessible name is reported, not just the tag.
    let names: Vec<String> = elements
        .iter()
        .filter_map(|e| e["name"].as_str().map(|s| s.to_string()))
        .collect();
    assert!(names.iter().any(|n| n == "Press me"), "{:?}", names);
    assert!(names.iter().any(|n| n == "Your name"), "{:?}", names);

    // The headings and links are listed separately.
    assert!(!snapshot.data["headings"].as_array().unwrap().is_empty());
    assert!(!snapshot.data["links"].as_array().unwrap().is_empty());

    manager.close_session(&session.session_id).await.ok();
}

#[tokio::test]
#[ignore = "requires Firefox and Node.js; run with --ignored"]
async fn browser_click_uses_a_semantic_locator_and_reports_the_result() {
    if let Some(reason) = prerequisites().await {
        eprintln!("SKIPPED: {}", reason);
        return;
    }

    let server = FixtureServer::start(FIXTURE);
    let manager = BrowserAutomationManager::new();
    let session = manager
        .create_session("firefox", true, Some("run_browser_test"), &[])
        .await
        .expect("session");
    manager
        .navigate(&session.session_id, &server.url("/"), 30000, false)
        .await
        .expect("navigation");

    let target = ElementTarget {
        role: Some("button".into()),
        name: Some("Press me".into()),
        ..Default::default()
    }
    .to_params()
    .expect("target");

    let click = manager
        .click(&session.session_id, target, 10000)
        .await
        .expect("click");
    assert_eq!(click.status, "success", "{:?}", click);
    assert_eq!(click.data["status"], "clicked");
    assert_eq!(click.data["element"]["name"], "Press me");

    // The click really happened: the page now reports the new state.
    let snapshot = manager
        .snapshot(&session.session_id)
        .await
        .expect("snapshot");
    let body_text = snapshot.data["body_text"].as_str().unwrap_or_default();
    assert!(
        body_text.contains("clicked"),
        "the button must have had an effect: {:?}",
        body_text
    );

    manager.close_session(&session.session_id).await.ok();
}

#[tokio::test]
#[ignore = "requires Firefox and Node.js; run with --ignored"]
async fn browser_click_on_a_missing_element_fails_with_a_reason() {
    if let Some(reason) = prerequisites().await {
        eprintln!("SKIPPED: {}", reason);
        return;
    }

    let server = FixtureServer::start(FIXTURE);
    let manager = BrowserAutomationManager::new();
    let session = manager
        .create_session("firefox", true, Some("run_browser_test"), &[])
        .await
        .expect("session");
    manager
        .navigate(&session.session_id, &server.url("/"), 30000, false)
        .await
        .expect("navigation");

    let target = ElementTarget {
        role: Some("button".into()),
        name: Some("there is no such button".into()),
        ..Default::default()
    }
    .to_params()
    .expect("target");

    let click = manager
        .click(&session.session_id, target, 3000)
        .await
        .expect("click");
    assert_eq!(click.status, "failed");
    assert_eq!(click.error_category.as_deref(), Some("element_not_found"));
    assert!(click
        .error_message
        .unwrap_or_default()
        .contains("Could not find"));

    manager.close_session(&session.session_id).await.ok();
}

#[tokio::test]
#[ignore = "requires Firefox and Node.js; run with --ignored"]
async fn browser_fill_enters_a_value_into_a_field() {
    if let Some(reason) = prerequisites().await {
        eprintln!("SKIPPED: {}", reason);
        return;
    }

    let server = FixtureServer::start(FIXTURE);
    let manager = BrowserAutomationManager::new();
    let session = manager
        .create_session("firefox", true, Some("run_browser_test"), &[])
        .await
        .expect("session");
    manager
        .navigate(&session.session_id, &server.url("/"), 30000, false)
        .await
        .expect("navigation");

    let target = ElementTarget {
        role: Some("textbox".into()),
        ..Default::default()
    }
    .to_params()
    .expect("target");

    let fill = manager
        .fill(&session.session_id, target, "Kyro".to_string(), 10000)
        .await
        .expect("fill");
    assert_eq!(fill.status, "success", "{:?}", fill);
    assert_eq!(fill.data["value_after"], "Kyro");

    manager.close_session(&session.session_id).await.ok();
}

#[tokio::test]
#[ignore = "requires Firefox and Node.js; run with --ignored"]
async fn browser_reports_console_errors() {
    if let Some(reason) = prerequisites().await {
        eprintln!("SKIPPED: {}", reason);
        return;
    }

    let server = FixtureServer::start(FIXTURE);
    let manager = BrowserAutomationManager::new();
    let session = manager
        .create_session("firefox", true, Some("run_browser_test"), &[])
        .await
        .expect("session");
    manager
        .navigate(&session.session_id, &server.url("/"), 30000, false)
        .await
        .expect("navigation");
    // Give the page a moment to report the error.
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    let console = manager.console(&session.session_id).await.expect("console");
    assert_eq!(console.status, "success");
    assert!(
        console.console_error_count >= 1,
        "the console error must be observed: {:?}",
        console.data
    );
    let entries = console.data["entries"].as_array().unwrap();
    assert!(entries.iter().any(|e| e["text"]
        .as_str()
        .unwrap_or("")
        .contains("fixture console error")));

    manager.close_session(&session.session_id).await.ok();
}

#[tokio::test]
#[ignore = "requires Firefox and Node.js; run with --ignored"]
async fn browser_screenshot_returns_an_image() {
    if let Some(reason) = prerequisites().await {
        eprintln!("SKIPPED: {}", reason);
        return;
    }

    let server = FixtureServer::start(FIXTURE);
    let manager = BrowserAutomationManager::new();
    let session = manager
        .create_session("firefox", true, Some("run_browser_test"), &[])
        .await
        .expect("session");
    manager
        .navigate(&session.session_id, &server.url("/"), 30000, false)
        .await
        .expect("navigation");

    let shot = manager
        .screenshot(&session.session_id, false)
        .await
        .expect("screenshot");
    assert_eq!(shot.status, "success");
    let png = shot.data["base64_png"].as_str().expect("png");
    assert!(png.len() > 1000, "the screenshot must contain an image");
    assert!(png.starts_with("iVBOR"), "the screenshot must be a PNG");

    manager.close_session(&session.session_id).await.ok();
}

#[tokio::test]
#[ignore = "requires Firefox and Node.js; run with --ignored"]
async fn browser_sessions_are_isolated_from_each_other() {
    if let Some(reason) = prerequisites().await {
        eprintln!("SKIPPED: {}", reason);
        return;
    }

    let server = FixtureServer::start(FIXTURE);
    let manager = BrowserAutomationManager::new();
    let first = manager
        .create_session("firefox", true, Some("run_a"), &[])
        .await
        .expect("session a");
    let second = manager
        .create_session("firefox", true, Some("run_b"), &[])
        .await
        .expect("session b");
    assert_ne!(first.session_id, second.session_id);

    manager
        .navigate(&first.session_id, &server.url("/"), 30000, false)
        .await
        .expect("navigation a");

    // The second session has its own page.
    let info = manager.page_info(&second.session_id).await.expect("info b");
    assert_ne!(
        info.url.as_deref().unwrap_or(""),
        "Kyro Fixture",
        "the sessions must not share a page"
    );

    // Closing one session leaves the other one alive.
    manager.close_session(&first.session_id).await.ok();
    assert_eq!(manager.list_sessions().await.len(), 1);
    let still = manager.page_info(&second.session_id).await.expect("info b");
    assert_eq!(still.status, "success");

    manager.close_session(&second.session_id).await.ok();
}

#[tokio::test]
#[ignore = "requires Firefox and Node.js; run with --ignored"]
async fn a_closed_session_cannot_be_used() {
    if let Some(reason) = prerequisites().await {
        eprintln!("SKIPPED: {}", reason);
        return;
    }

    let manager = BrowserAutomationManager::new();
    let session = manager
        .create_session("firefox", true, Some("run_browser_test"), &[])
        .await
        .expect("session");
    manager.close_session(&session.session_id).await.ok();

    let info = manager.page_info(&session.session_id).await;
    assert!(info.is_err(), "a closed session must not be usable");
}

#[tokio::test]
#[ignore = "requires Node.js and an automatable browser; run with --ignored"]
async fn kyro_does_not_silently_substitute_another_browser() {
    if let Some(reason) = prerequisites().await {
        eprintln!("SKIPPED: {}", reason);
        return;
    }

    let manager = BrowserAutomationManager::new();
    let err = manager
        .create_session("safari", true, Some("run_browser_test"), &[])
        .await
        .expect_err("an unsupported browser must be refused");
    assert!(
        err.contains("safari") && (err.contains("Available supported alternatives") || err.contains("Available supported browsers")),
        "the error must explain why the requested browser cannot be automated and list alternatives: {}",
        err
    );
}

#[tokio::test]
#[ignore = "requires Chromium and Node.js; run with --ignored"]
async fn browser_session_starts_with_chromium_and_navigates() {
    if kyro_lib::browser::worker::find_node().is_none() {
        return;
    }
    let server = FixtureServer::start(FIXTURE);
    let manager = BrowserAutomationManager::new();
    let caps = manager.capabilities().await.expect("caps");
    if !caps
        .browsers
        .iter()
        .any(|b| b.id == "chromium" && b.available && b.automatable)
    {
        eprintln!("SKIPPED: chromium is not available or automatable");
        return;
    }

    let session = manager
        .create_session("chromium", true, Some("run_chromium_test"), &[])
        .await
        .expect("chromium session should start");

    assert_eq!(session.browser, "chromium");
    assert_eq!(session.driver, "cdp");
    assert!(session.profile_isolated);

    let nav = manager
        .navigate(&session.session_id, &server.url("/"), 30000, false)
        .await
        .expect("navigation");
    assert_eq!(nav.status, "success");
    assert_eq!(nav.title.as_deref(), Some("Kyro Fixture"));

    let snapshot = manager
        .snapshot(&session.session_id)
        .await
        .expect("snapshot");
    assert_eq!(snapshot.status, "success");

    let elements = snapshot.data["elements"].as_array().expect("elements");
    assert!(!elements.is_empty());

    manager.close_session(&session.session_id).await.ok();
}

#[tokio::test]
#[ignore = "requires Brave and Node.js; run with --ignored"]
async fn browser_session_starts_with_brave_and_navigates() {
    if kyro_lib::browser::worker::find_node().is_none() {
        return;
    }
    let server = FixtureServer::start(FIXTURE);
    let manager = BrowserAutomationManager::new();
    let caps = manager.capabilities().await.expect("caps");
    if !caps
        .browsers
        .iter()
        .any(|b| b.id == "brave" && b.available && b.automatable)
    {
        eprintln!("SKIPPED: brave is not available or automatable");
        return;
    }

    let session = manager
        .create_session("brave", true, Some("run_brave_test"), &[])
        .await
        .expect("brave session should start");

    assert_eq!(session.browser, "brave");
    assert_eq!(session.driver, "cdp");
    assert!(session.profile_isolated);

    let nav = manager
        .navigate(&session.session_id, &server.url("/"), 30000, false)
        .await
        .expect("navigation");
    assert_eq!(nav.status, "success");
    assert_eq!(nav.title.as_deref(), Some("Kyro Fixture"));

    let snapshot = manager
        .snapshot(&session.session_id)
        .await
        .expect("snapshot");
    assert_eq!(snapshot.status, "success");

    manager.close_session(&session.session_id).await.ok();
}

#[tokio::test]
#[ignore = "requires Firefox and Node.js; run with --ignored"]
async fn browser_element_inspection_returns_details() {
    if let Some(reason) = prerequisites().await {
        eprintln!("SKIPPED: {}", reason);
        return;
    }

    let server = FixtureServer::start(FIXTURE);
    let manager = BrowserAutomationManager::new();
    let session = manager
        .create_session("firefox", true, Some("run_inspect_test"), &[])
        .await
        .expect("session should start");

    manager
        .navigate(&session.session_id, &server.url("/"), 30000, false)
        .await
        .expect("navigation");

    let snapshot = manager
        .snapshot(&session.session_id)
        .await
        .expect("snapshot");
    assert_eq!(snapshot.status, "success");

    let target = ElementTarget {
        role: Some("button".to_string()),
        name: Some("Press me".to_string()),
        ..Default::default()
    };
    let inspect_res = manager
        .inspect_element(&session.session_id, target, 5000)
        .await
        .expect("inspect_element");

    assert_eq!(inspect_res.tag, "button");
    assert_eq!(inspect_res.role, "button");
    assert!(inspect_res.interactable, "element must be interactable");

    manager.close_session(&session.session_id).await.ok();
}

#[tokio::test]
#[ignore = "requires Firefox and Node.js; run with --ignored"]
async fn browser_viewport_resizing_reports_actual_dimensions() {
    if let Some(reason) = prerequisites().await {
        eprintln!("SKIPPED: {}", reason);
        return;
    }

    let manager = BrowserAutomationManager::new();
    let session = manager
        .create_session("firefox", true, Some("run_vp_test"), &[])
        .await
        .expect("session should start");

    let vp_res = manager
        .set_viewport(&session.session_id, 768, 1024)
        .await
        .expect("set_viewport");

    assert_eq!(vp_res.status, "success");
    let w = vp_res.data.get("width").and_then(|v| v.as_u64());
    let h = vp_res.data.get("height").and_then(|v| v.as_u64());
    assert_eq!(w, Some(768));
    assert_eq!(h, Some(1024));

    manager.close_session(&session.session_id).await.ok();
}

#[tokio::test]
#[ignore = "requires Firefox and Node.js; run with --ignored"]
async fn browser_stale_element_reference_rejection() {
    if let Some(reason) = prerequisites().await {
        eprintln!("SKIPPED: {}", reason);
        return;
    }

    let server = FixtureServer::start(FIXTURE);
    let manager = BrowserAutomationManager::new();
    let session = manager
        .create_session("firefox", true, Some("run_stale_test"), &[])
        .await
        .expect("session should start");

    manager
        .navigate(&session.session_id, &server.url("/"), 30000, false)
        .await
        .expect("navigation");

    // Pass a fake/stale ref ID with a snapshot_id
    let stale_target = ElementTarget {
        ref_id: Some("e99999_stale".to_string()),
        snapshot_id: Some("snap_old_111".to_string()),
        ..Default::default()
    };

    let target_params = stale_target.to_params().unwrap();
    let click_res = manager
        .click(&session.session_id, target_params, 5000)
        .await;
    match click_res {
        Ok(res) => {
            assert!(
                res.status != "success"
                    || res.error_message.as_deref().unwrap_or("").contains("stale")
                    || res
                        .error_message
                        .as_deref()
                        .unwrap_or("")
                        .contains("not found")
            );
        }
        Err(e) => {
            assert!(e.contains("stale") || e.contains("not found") || e.contains("target"));
        }
    }

    manager.close_session(&session.session_id).await.ok();
}
