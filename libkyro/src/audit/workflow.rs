//! The autonomous UI audit workflow.
//!
//! The workflow combines project discovery, managed development-server
//! management and browser automation into one auditable sequence.  Every
//! stage records what it did, and the workflow never claims that a route was
//! inspected when it was not.

use crate::audit::findings::{
    detect_dead_control, detect_findings, BrokenImage, ConsoleObservation,
    FailedRequestObservation, Finding, HttpErrorObservation, PageObservations, ValidationTarget,
};
use crate::audit::report::{render_report, AuditContext};
use crate::audit::routes::{merge_routes, DiscoveredRoute, RouteInventory, RouteSource};
use crate::browser::{BrowserAutomationManager, BrowserSession};
use crate::project::devserver::{DevCommand, DevServerManager, DevServerState};
use crate::project::discovery::{DiscoveryLimits, ProjectCandidate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

/// The options of an audit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditOptions {
    /// The name of the project to find.
    pub project_name: String,
    /// The directory to search.
    pub search_root: PathBuf,
    /// The browser to use.  Only `firefox` is supported.
    pub browser: String,
    /// Whether the browser runs headless.
    pub headless: bool,
    /// Whether the development server keeps running after the audit.
    pub keep_server_running: bool,
    /// The viewports that are inspected.
    pub viewports: Vec<(u32, u32)>,
    /// The maximum number of routes that are visited.
    pub max_routes: usize,
    /// How long to wait for the development server to become ready.
    pub ready_timeout_ms: u64,
    /// The run that owns the audit.
    pub run_id: Option<String>,
}

impl Default for AuditOptions {
    fn default() -> Self {
        Self {
            project_name: "Runstead".into(),
            search_root: PathBuf::from("."),
            browser: "firefox".into(),
            headless: true,
            keep_server_running: false,
            viewports: vec![(1280, 800), (768, 1024), (390, 844)],
            max_routes: 40,
            ready_timeout_ms: 120000,
            run_id: None,
        }
    }
}

/// The stages of the audit, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditStage {
    DiscoverProject,
    ValidateProject,
    StartDevServer,
    WaitForReady,
    OpenPreview,
    DiscoverRoutes,
    InspectRoutes,
    ExploreStates,
    CollectEvidence,
    VerifyFindings,
    GenerateReport,
    Cleanup,
}

impl AuditStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditStage::DiscoverProject => "Discover project",
            AuditStage::ValidateProject => "Validate project root",
            AuditStage::StartDevServer => "Start development server",
            AuditStage::WaitForReady => "Wait for confirmed readiness",
            AuditStage::OpenPreview => "Open the preview in the browser",
            AuditStage::DiscoverRoutes => "Discover application routes",
            AuditStage::InspectRoutes => "Inspect every accessible route",
            AuditStage::ExploreStates => "Explore relevant UI states",
            AuditStage::CollectEvidence => "Collect evidence",
            AuditStage::VerifyFindings => "Verify suspected bugs",
            AuditStage::GenerateReport => "Generate the bug report",
            AuditStage::Cleanup => "Clean up owned resources",
        }
    }
}

/// A progress notification emitted by the workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditProgress {
    pub stage: AuditStage,
    pub message: String,
    /// `true` when the stage completed.
    pub completed: bool,
}

/// The result of an audit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditOutcome {
    pub context: AuditContext,
    pub findings: Vec<Finding>,
    pub report_markdown: String,
    pub route_inventory: RouteInventory,
    /// The captured screenshots: (name, base64 PNG).
    pub screenshots: Vec<(String, String)>,
    /// The state of the development server, when one was started.
    pub dev_server: Option<DevServerState>,
    /// The command that was executed.
    pub dev_command: Option<DevCommand>,
    /// The project that was selected.
    pub project: Option<ProjectCandidate>,
}

/// The plan produced before an audit is executed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditPlan {
    /// The project that was selected.
    pub project: ProjectCandidate,
    /// The command that would start its development server.
    pub command: DevCommand,
    /// A summary of the discovery that produced the plan.
    pub discovery_summary: String,
}

/// A workflow that performs the whole audit.
pub struct UiAuditWorkflow {
    pub browser: Arc<BrowserAutomationManager>,
    pub dev_servers: Arc<DevServerManager>,
}

impl UiAuditWorkflow {
    pub fn new(browser: Arc<BrowserAutomationManager>, dev_servers: Arc<DevServerManager>) -> Self {
        Self {
            browser,
            dev_servers,
        }
    }

    /// Produces the plan for an audit: the project that was found and the
    /// command that would start it.
    ///
    /// No code is executed and no process is started; the plan exists so that
    /// the user can approve the exact command before it runs.
    pub async fn plan(&self, options: &AuditOptions) -> Result<AuditPlan, String> {
        let discovery = crate::project::discovery::discover_projects(
            &options.search_root,
            &options.project_name,
            &DiscoveryLimits::default(),
        )?;
        if discovery.candidates.is_empty() {
            return Err(format!(
                "{} No project was found, so nothing could be audited.",
                discovery.summary
            ));
        }
        if discovery.candidates.len() > 1 {
            let mut listing = String::new();
            for candidate in &discovery.candidates {
                listing.push_str(&format!(
                    "\n  - {} ({})",
                    candidate.path.display(),
                    candidate.kind.as_str()
                ));
            }
            return Err(format!(
                "Several projects match '{}'. Select the intended one before running the audit:{}",
                options.project_name, listing
            ));
        }
        let project = discovery.candidates[0].clone();
        let command = DevServerManager::choose_command(&project)?;
        Ok(AuditPlan {
            project,
            command,
            discovery_summary: discovery.summary,
        })
    }

    /// Executes a plan that the user has approved.
    pub async fn execute(
        &self,
        options: &AuditOptions,
        plan: AuditPlan,
        on_progress: &mut (dyn FnMut(AuditProgress) + Send),
    ) -> Result<AuditOutcome, String> {
        let started = std::time::Instant::now();
        let AuditPlan {
            project,
            command,
            discovery_summary,
        } = plan;

        on_progress(AuditProgress {
            stage: AuditStage::DiscoverProject,
            message: discovery_summary,
            completed: true,
        });
        on_progress(AuditProgress {
            stage: AuditStage::ValidateProject,
            message: format!(
                "Will run `{}` in `{}`",
                command.display(),
                command.working_directory.display()
            ),
            completed: true,
        });

        // ---- Start the development server and wait for readiness ---------
        on_progress(AuditProgress {
            stage: AuditStage::StartDevServer,
            message: format!("Starting `{}`", command.display()),
            completed: false,
        });
        let server_state = self
            .dev_servers
            .start(
                &command,
                options.run_id.as_deref(),
                options.ready_timeout_ms,
            )
            .await?;
        let preview_url = server_state
            .ready_url
            .clone()
            .ok_or_else(|| "The development server did not report a ready URL".to_string())?;
        on_progress(AuditProgress {
            stage: AuditStage::WaitForReady,
            message: format!("The development server is ready at {}", preview_url),
            completed: true,
        });

        // ---- Open the preview --------------------------------------------
        on_progress(AuditProgress {
            stage: AuditStage::OpenPreview,
            message: format!("Opening {} in {}", preview_url, options.browser),
            completed: false,
        });
        let session = self
            .browser
            .create_session(
                &options.browser,
                options.headless,
                options.run_id.as_deref(),
                &[],
            )
            .await?;
        if let Some(origin) = origin_of(&preview_url) {
            self.browser
                .set_origin_scope(&session.session_id, Some(origin))
                .await;
        }

        let mut outcome = match self
            .audit_session(
                &session,
                &project,
                &command,
                &server_state,
                options,
                &preview_url,
                started,
            )
            .await
        {
            Ok(o) => o,
            Err(e) => return Err(e),
        };

        // ---- Cleanup ------------------------------------------------------
        on_progress(AuditProgress {
            stage: AuditStage::Cleanup,
            message: "Releasing the browser session".into(),
            completed: false,
        });
        let _ = self.browser.close_session(&session.session_id).await;
        if !options.keep_server_running {
            let _ = self.dev_servers.stop(&server_state.id);
        }
        on_progress(AuditProgress {
            stage: AuditStage::Cleanup,
            message: if options.keep_server_running {
                "The development server was left running on request".into()
            } else {
                "The development server was stopped".into()
            },
            completed: true,
        });

        // The report is rendered before the cleanup, so the state of the
        // development server is corrected and the report regenerated.
        outcome.context.dev_server_running = options.keep_server_running;
        outcome.report_markdown = render_report(
            &outcome.context,
            &outcome.findings,
            &outcome.route_inventory,
        );

        Ok(outcome)
    }

    #[allow(clippy::too_many_arguments)]
    async fn audit_session(
        &self,
        session: &BrowserSession,
        project: &ProjectCandidate,
        command: &DevCommand,
        server_state: &DevServerState,
        options: &AuditOptions,
        preview_url: &str,
        started: std::time::Instant,
    ) -> Result<AuditOutcome, String> {
        // ---- Navigate to the preview -------------------------------------
        let nav = self
            .browser
            .navigate(&session.session_id, preview_url, 60000, false)
            .await?;
        if nav.status != "success" {
            return Err(format!(
                "Could not open the preview at {}: {}",
                preview_url,
                nav.error_message.unwrap_or_default()
            ));
        }

        // ---- Discover routes ----------------------------------------------
        let mut groups: Vec<Vec<DiscoveredRoute>> = Vec::new();
        groups.push(crate::audit::routes::discover_route_files(
            &project.path,
            8000,
        ));
        groups.push(crate::audit::routes::discover_router_config(
            &project.path,
            4000,
        ));

        // The rendered application contributes its own links.
        if let Ok(snapshot) = self.browser.snapshot(&session.session_id).await {
            if snapshot.status == "success" {
                let mut rendered: Vec<DiscoveredRoute> = Vec::new();
                if let Some(anchors) = snapshot.data.get("anchors").and_then(|a| a.as_array()) {
                    for anchor in anchors {
                        let resolved = anchor
                            .get("resolved")
                            .and_then(|r| r.as_str())
                            .unwrap_or_default();
                        if resolved.is_empty() {
                            continue;
                        }
                        if !resolved.starts_with(preview_url) && !resolved.starts_with('/') {
                            continue;
                        }
                        let path: String = if resolved.starts_with('/') {
                            resolved.to_string()
                        } else {
                            let base = origin_of(preview_url).unwrap_or_default();
                            resolved
                                .strip_prefix(&base)
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| resolved.to_string())
                        };
                        let path = path.split(['?', '#']).next().unwrap_or(&path).to_string();
                        let normalised = crate::audit::routes::normalise_route(&path);
                        let mut route = DiscoveredRoute::new(
                            normalised,
                            crate::audit::routes::RouteKind::Static,
                            RouteSource::RenderedLink,
                        );
                        route.visitable = true;
                        route.blocked_reason = None;
                        rendered.push(route);
                    }
                }
                groups.push(rendered);
            }
        }

        let inventory = merge_routes(groups);
        let visitable: Vec<DiscoveredRoute> = inventory
            .routes
            .iter()
            .filter(|r| r.visitable)
            .cloned()
            .collect();

        // ---- Inspect every visitable route -------------------------------
        let mut findings: Vec<Finding> = Vec::new();
        let mut screenshots: Vec<(String, String)> = Vec::new();
        let mut visited = 0usize;
        let mut visited_routes: Vec<String> = Vec::new();
        let mut failed_routes: Vec<(String, String)> = Vec::new();
        let mut total_console_errors = 0usize;
        let mut total_failed_requests = 0usize;
        let mut artifact_index = 0usize;

        for route in visitable.iter().take(options.max_routes) {
            let url = join_url(preview_url, &route.path);
            let _ = self.browser.reset_diagnostics(&session.session_id).await;
            let nav = self
                .browser
                .navigate(&session.session_id, &url, 45000, false)
                .await?;

            if nav.status != "success" {
                failed_routes.push((
                    route.path.clone(),
                    nav.error_message
                        .clone()
                        .unwrap_or_else(|| "navigation failed".to_string()),
                ));
                continue;
            }

            let snapshot = self.browser.snapshot(&session.session_id).await?;
            if snapshot.status != "success" {
                failed_routes.push((
                    route.path.clone(),
                    snapshot
                        .error_message
                        .clone()
                        .unwrap_or_else(|| "snapshot failed".to_string()),
                ));
                continue;
            }

            // Screenshot evidence.
            if let Ok(shot) = self.browser.screenshot(&session.session_id, false).await {
                if shot.status == "success" {
                    if let Some(png) = shot.data.get("base64_png").and_then(|d| d.as_str()) {
                        artifact_index += 1;
                        let name =
                            format!("route-{}-{:02}.png", sanitize(&route.path), artifact_index);
                        screenshots.push((name, png.to_string()));
                    }
                }
            }

            let console = self.browser.console(&session.session_id).await.ok();
            let network = self.browser.network(&session.session_id).await.ok();

            let mut observations = build_observations(
                &route.path,
                nav.url.as_deref().unwrap_or(&url),
                snapshot.data.as_object(),
                console.as_ref().and_then(|c| c.data.as_object()),
                network.as_ref().and_then(|n| n.data.as_object()),
            );

            total_console_errors += observations.console_entries.len();
            total_failed_requests += observations.failed_requests.len();

            // Check the form fields of this route for broken validation.
            for field in observations.fields_to_validate.clone() {
                if field.name.is_empty() {
                    continue;
                }
                let mut target = serde_json::Map::new();
                target.insert("role".into(), serde_json::Value::String("textbox".into()));
                target.insert("name".into(), serde_json::Value::String(field.name.clone()));
                let check = match self
                    .browser
                    .check_validation(&session.session_id, serde_json::Value::Object(target), 8000)
                    .await
                {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                if check.status != "success" {
                    continue;
                }
                if let Some(details) = check.data.get("field").and_then(|f| f.as_object()) {
                    if details.get("found").and_then(|f| f.as_bool()) != Some(true) {
                        continue;
                    }
                    observations
                        .field_validations
                        .push(crate::audit::findings::FieldValidation {
                            name: details
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or(&field.name)
                                .to_string(),
                            // The *effective* type is what the field is
                            // supposed to accept, which is derived from the
                            // name when the declared type does not enforce it.
                            field_type: details
                                .get("expects")
                                .and_then(|t| t.as_str())
                                .filter(|t| !t.is_empty())
                                .or_else(|| details.get("type").and_then(|t| t.as_str()))
                                .unwrap_or(&field.input_type)
                                .to_string(),
                            declared_type: details
                                .get("type")
                                .and_then(|t| t.as_str())
                                .unwrap_or(&field.input_type)
                                .to_string(),
                            required: details
                                .get("required")
                                .and_then(|r| r.as_bool())
                                .unwrap_or(false),
                            pattern: details
                                .get("pattern")
                                .and_then(|p| p.as_str())
                                .map(|s| s.to_string()),
                            minlength: details
                                .get("minlength")
                                .and_then(|p| p.as_str())
                                .map(|s| s.to_string()),
                            accepts_invalid: details
                                .get("accepts_invalid")
                                .and_then(|a| a.as_bool())
                                .unwrap_or(false),
                            validation_message: details
                                .get("validation_message")
                                .and_then(|m| m.as_str())
                                .filter(|m| !m.is_empty())
                                .map(|s| s.to_string()),
                        });
                }
            }

            let mut route_findings = detect_findings(&observations);

            // Exercise a few safe controls on this route.
            let (interaction_findings, interaction_shots) = self
                .explore_states(session, &route.path, snapshot.data.as_object())
                .await;
            route_findings.extend(interaction_findings);
            for shot in interaction_shots {
                artifact_index += 1;
                let name = format!(
                    "interaction-{}-{:02}.png",
                    sanitize(&route.path),
                    artifact_index
                );
                screenshots.push((name, shot));
            }

            for finding in &mut route_findings {
                finding.artifacts = screenshots
                    .iter()
                    .rev()
                    .take(1)
                    .map(|(name, _)| name.clone())
                    .collect();
            }

            findings.extend(route_findings);
            visited += 1;
            visited_routes.push(route.path.clone());
        }

        // ---- Responsive checks --------------------------------------------
        for route in visitable.iter().take(options.max_routes) {
            let url = join_url(preview_url, &route.path);
            for (width, height) in options.viewports.iter().skip(1) {
                let _ = self
                    .browser
                    .set_viewport(&session.session_id, *width, *height)
                    .await;
                let nav = match self
                    .browser
                    .navigate(&session.session_id, &url, 45000, false)
                    .await
                {
                    Ok(n) => n,
                    Err(_) => continue,
                };
                if nav.status != "success" {
                    continue;
                }
                let snapshot = match self.browser.snapshot(&session.session_id).await {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if snapshot.status != "success" {
                    continue;
                }
                let observations = build_observations(
                    &route.path,
                    nav.url.as_deref().unwrap_or(&url),
                    snapshot.data.as_object(),
                    None,
                    None,
                );
                let mut viewport_findings = detect_findings(&observations);
                // Only the layout-related findings are relevant for a
                // responsive check; the content findings were already
                // reported at the desktop viewport.
                viewport_findings.retain(|f| {
                    f.category == crate::audit::findings::FindingCategory::DesignInconsistency
                });
                for finding in &mut viewport_findings {
                    finding.viewport = format!("{}x{}", width, height);
                    finding.title = format!("Page scrolls horizontally at {}x{}", width, height);
                }
                findings.extend(viewport_findings);
            }
        }
        // Restore the primary viewport.
        if let Some((w, h)) = options.viewports.first() {
            let _ = self.browser.set_viewport(&session.session_id, *w, *h).await;
        }

        // ---- Finalise ------------------------------------------------------
        let mut findings = crate::audit::findings::finalise_findings(findings);
        // The findings are returned sorted by severity and then by confidence,
        // so that the most important defect is always first.
        findings.sort_by(|a, b| {
            b.severity
                .cmp(&a.severity)
                .then_with(|| confidence_rank(a.confidence).cmp(&confidence_rank(b.confidence)))
                .then_with(|| a.id.cmp(&b.id))
        });
        // The report states the *observed* totals, not the number of
        // findings, so that a console error that was not classified as a
        // defect is still counted.
        let console_errors = total_console_errors;
        let failed_requests = total_failed_requests;

        let blocked_routes: Vec<(String, String)> = inventory
            .routes
            .iter()
            .filter(|r| !r.visitable)
            .map(|r| (r.path.clone(), r.blocked_reason.clone().unwrap_or_default()))
            .collect();

        let context = AuditContext {
            project_path: project.path.to_string_lossy().to_string(),
            startup_command: command.display(),
            framework: server_state.framework.clone(),
            package_manager: Some(command.executable.clone()),
            browser: session.browser.clone(),
            browser_version: session.browser_version.clone(),
            preview_url: preview_url.to_string(),
            routes_discovered: inventory.routes.len(),
            routes_inspected: visited,
            visited_routes,
            routes_blocked_or_failed: blocked_routes.len() + failed_routes.len(),
            console_errors,
            failed_requests,
            screenshot_count: screenshots.len(),
            dev_server_running: true,
            blocked_routes,
            failed_routes,
            duration_ms: started.elapsed().as_millis() as u64,
        };

        let report = render_report(&context, &findings, &inventory);

        Ok(AuditOutcome {
            context,
            findings,
            report_markdown: report,
            route_inventory: inventory,
            screenshots,
            dev_server: Some(server_state.clone()),
            dev_command: Some(command.clone()),
            project: Some(project.clone()),
        })
    }

    /// Exercises a small set of safe controls on a page.
    ///
    /// Only controls whose accessible name suggests a non-destructive action
    /// are activated; anything that could submit data, delete records or
    /// change account state is left alone.
    async fn explore_states(
        &self,
        session: &BrowserSession,
        route: &str,
        snapshot: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> (Vec<Finding>, Vec<String>) {
        let mut findings = Vec::new();
        let mut screenshots = Vec::new();

        // The path of the page that is currently open, so that a link to the
        // current page is not treated as a broken interaction.
        let current_path = snapshot
            .and_then(|s| s.get("url"))
            .and_then(|u| u.as_str())
            .and_then(|u| url::Url::parse(u).ok())
            .map(|u| u.path().to_string());

        let elements = match snapshot
            .and_then(|s| s.get("elements"))
            .and_then(|e| e.as_array())
        {
            Some(e) => e,
            None => return (findings, screenshots),
        };

        const SAFE_HINTS: &[&str] = &[
            "close", "dismiss", "menu", "toggle", "expand", "collapse", "tab", "more", "open",
            "show", "hide", "next", "previous", "back", "details", "settings", "filter", "sort",
            "view", "preview",
        ];
        const UNSAFE_HINTS: &[&str] = &[
            "delete",
            "remove",
            "submit",
            "send",
            "purchase",
            "buy",
            "pay",
            "confirm",
            "deploy",
            "publish",
            "save",
            "sign out",
            "logout",
            "invite",
            "upgrade",
            "subscribe",
            "charge",
        ];

        let mut tested: BTreeSet<String> = BTreeSet::new();
        let mut candidates: Vec<(String, serde_json::Value)> = Vec::new();

        for element in elements {
            let role = element.get("role").and_then(|r| r.as_str()).unwrap_or("");
            let name = element.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let lower = name.to_lowercase();

            let mut target = serde_json::Map::new();
            #[allow(clippy::needless_late_init)]
            let label: String;

            if role == "link" {
                // Following an internal link is safe: it only navigates.
                // External links are skipped so that the audit stays inside
                // the application.
                let href = element.get("href").and_then(|h| h.as_str()).unwrap_or("");
                if href.is_empty() || !href.starts_with('/') {
                    continue;
                }
                // A link to the page that is already open cannot change
                // anything, so it is not a meaningful interaction.
                if current_path.as_deref() == Some(href) {
                    continue;
                }
                if name.is_empty() {
                    continue;
                }
                if UNSAFE_HINTS.iter().any(|h| lower.contains(h)) {
                    continue;
                }
                if !tested.insert(format!("link:{}", lower)) {
                    continue;
                }
                target.insert("role".into(), serde_json::Value::String("link".into()));
                target.insert("name".into(), serde_json::Value::String(name.to_string()));
                label = format!("the link '{}'", name);
            } else if role == "button" || role == "tab" {
                if name.is_empty() {
                    continue;
                }
                if UNSAFE_HINTS.iter().any(|h| lower.contains(h)) {
                    continue;
                }
                if !SAFE_HINTS.iter().any(|h| lower.contains(h)) {
                    continue;
                }
                if !tested.insert(lower.clone()) {
                    continue;
                }
                target.insert("role".into(), serde_json::Value::String(role.to_string()));
                target.insert("name".into(), serde_json::Value::String(name.to_string()));
                label = name.to_string();
            } else {
                continue;
            }

            candidates.push((label, serde_json::Value::Object(target)));
            if candidates.len() >= 5 {
                break;
            }
        }

        for (name, target) in candidates {
            let before = self.browser.page_info(&session.session_id).await.ok();
            let url_before = before
                .as_ref()
                .and_then(|b| b.url.clone())
                .unwrap_or_default();

            let click = match self.browser.click(&session.session_id, target, 8000).await {
                Ok(c) => c,
                Err(_) => continue,
            };
            if click.status != "success" {
                continue;
            }

            if let Ok(shot) = self.browser.screenshot(&session.session_id, false).await {
                if shot.status == "success" {
                    if let Some(png) = shot.data.get("base64_png").and_then(|d| d.as_str()) {
                        screenshots.push(png.to_string());
                    }
                }
            }

            let after = self.browser.page_info(&session.session_id).await.ok();
            let url_after = after
                .as_ref()
                .and_then(|a| a.url.clone())
                .unwrap_or_default();
            let dom_changed = url_before != url_after;

            if !dom_changed {
                // The interaction produced no observable change.  Report it
                // as unverified: the control may still have a purpose that is
                // not visible in the URL.
                findings.push(detect_dead_control(
                    route,
                    &url_after,
                    "1280x800",
                    &name,
                    &url_before,
                    &url_after,
                    false,
                ));
            }

            // Return to the route so the next interaction starts from a
            // known state.
            if url_after != url_before && !url_after.is_empty() {
                let _ = self
                    .browser
                    .navigate(&session.session_id, &url_before, 30000, false)
                    .await;
            }
        }

        (findings, screenshots)
    }
}

/// Builds the page observations from the collected browser data.
pub fn build_observations(
    route: &str,
    url: &str,
    snapshot: Option<&serde_json::Map<String, serde_json::Value>>,
    console: Option<&serde_json::Map<String, serde_json::Value>>,
    network: Option<&serde_json::Map<String, serde_json::Value>>,
) -> PageObservations {
    let mut observations = PageObservations {
        route: route.to_string(),
        url: url.to_string(),
        ..Default::default()
    };

    if let Some(snapshot) = snapshot {
        observations.title = snapshot
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .to_string();
        if let Some(viewport) = snapshot.get("viewport").and_then(|v| v.as_object()) {
            observations.viewport = format!(
                "{}x{}",
                viewport.get("width").and_then(|w| w.as_u64()).unwrap_or(0),
                viewport.get("height").and_then(|h| h.as_u64()).unwrap_or(0)
            );
        }
        observations.horizontal_overflow = snapshot
            .get("overflowX")
            .and_then(|o| o.as_bool())
            .unwrap_or(false);
        if let Some(images) = snapshot.get("images").and_then(|i| i.as_array()) {
            for image in images {
                let natural_width = image
                    .get("naturalWidth")
                    .and_then(|w| w.as_u64())
                    .unwrap_or(0);
                let complete = image
                    .get("complete")
                    .and_then(|c| c.as_bool())
                    .unwrap_or(false);
                if natural_width == 0 && complete {
                    observations.broken_images.push(BrokenImage {
                        src: image
                            .get("src")
                            .and_then(|s| s.as_str())
                            .map(|s| s.to_string()),
                        alt: image
                            .get("alt")
                            .and_then(|s| s.as_str())
                            .map(|s| s.to_string()),
                    });
                }
            }
        }
        if let Some(elements) = snapshot.get("elements").and_then(|e| e.as_array()) {
            let mut h1 = 0usize;
            let mut unnamed = 0usize;
            for element in elements {
                let role = element.get("role").and_then(|r| r.as_str()).unwrap_or("");
                if role == "heading" {
                    let level = element
                        .get("headingLevel")
                        .and_then(|l| l.as_u64())
                        .unwrap_or(0);
                    if level == 1 {
                        h1 += 1;
                    }
                }
                let interactive = element
                    .get("interactive")
                    .and_then(|i| i.as_bool())
                    .unwrap_or(false);
                if interactive {
                    let name = element.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    if name.trim().is_empty() {
                        unnamed += 1;
                    } else {
                        observations
                            .interactive_element_names
                            .push(name.to_string());
                    }
                }
                if role == "progressbar" {
                    observations.stuck_loading_indicator = Some(
                        element
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("progress indicator")
                            .to_string(),
                    );
                }
            }
            observations.h1_count = h1;
            observations.unnamed_interactive_elements = unnamed;
        }
        observations.declares_language = snapshot
            .get("declaresLanguage")
            .and_then(|d| d.as_bool())
            .unwrap_or(true);
        // Form fields whose type claims a constraint that the browser can
        // verify.  They are checked individually so that a broken field is
        // reported precisely.
        if let Some(elements) = snapshot.get("elements").and_then(|e| e.as_array()) {
            for element in elements {
                let role = element.get("role").and_then(|r| r.as_str()).unwrap_or("");
                if role != "textbox" && role != "searchbox" && role != "spinbutton" {
                    continue;
                }
                let input_type = element.get("type").and_then(|t| t.as_str()).unwrap_or("");
                let name = element
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let placeholder = element
                    .get("placeholder")
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .to_string();
                // A field is worth checking when it declares a constrained
                // type, or when its name claims a constrained format that the
                // declared type does not enforce.
                let declares_constrained = matches!(input_type, "email" | "url" | "number" | "tel");
                let hint = format!("{} {}", name, placeholder).to_lowercase();
                let name_suggests_constrained = hint.contains("email")
                    || hint.contains("e-mail")
                    || hint.contains("url")
                    || hint.contains("website")
                    || hint.contains("phone")
                    || hint.contains("amount")
                    || hint.contains("price")
                    || hint.contains("quantity");
                if !declares_constrained && !name_suggests_constrained {
                    continue;
                }
                observations.fields_to_validate.push(ValidationTarget {
                    name,
                    input_type: input_type.to_string(),
                });
            }
        }
    }

    if let Some(console) = console {
        if let Some(entries) = console.get("entries").and_then(|e| e.as_array()) {
            for entry in entries {
                observations.console_entries.push(ConsoleObservation {
                    level: entry
                        .get("level")
                        .and_then(|l| l.as_str())
                        .unwrap_or("log")
                        .to_string(),
                    text: entry
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string(),
                });
            }
        }
    }

    if let Some(network) = network {
        if let Some(failed) = network.get("failed").and_then(|f| f.as_array()) {
            for entry in failed {
                observations.failed_requests.push(FailedRequestObservation {
                    url: entry
                        .get("url")
                        .and_then(|u| u.as_str())
                        .unwrap_or("")
                        .to_string(),
                    error: entry
                        .get("error")
                        .and_then(|e| e.as_str())
                        .unwrap_or("request failed")
                        .to_string(),
                });
            }
        }
        if let Some(requests) = network.get("requests").and_then(|r| r.as_array()) {
            for entry in requests {
                let status = entry.get("status").and_then(|s| s.as_u64());
                if let Some(status) = status {
                    if status >= 400 {
                        let url = entry.get("url").and_then(|u| u.as_str()).unwrap_or("");
                        // The browser's automatic favicon request is not an
                        // application defect and is not reported as one.
                        if url.ends_with("/favicon.ico") {
                            continue;
                        }
                        observations
                            .http_error_responses
                            .push(HttpErrorObservation {
                                url: url.to_string(),
                                status: status as u16,
                            });
                    }
                }
            }
        }
    }

    observations
}

/// The ordering rank of a confidence level (lower is stronger).
fn confidence_rank(c: crate::audit::findings::Confidence) -> u8 {
    match c {
        crate::audit::findings::Confidence::Reproduced => 0,
        crate::audit::findings::Confidence::StrongEvidence => 1,
        crate::audit::findings::Confidence::Unverified => 2,
    }
}

/// Returns the origin of a URL, if it has one.
pub fn origin_of(url: &str) -> Option<String> {
    crate::browser::safety::origin_of(url)
}

/// Joins a base URL and a path.
pub fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    if path.is_empty() || path == "/" {
        return format!("{}/", base);
    }
    if path.starts_with('/') {
        format!("{}{}", base, path)
    } else {
        format!("{}/{}", base, path)
    }
}

fn sanitize(path: &str) -> String {
    path.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_urls() {
        assert_eq!(
            join_url("http://localhost:3000/", "/about"),
            "http://localhost:3000/about"
        );
        assert_eq!(
            join_url("http://localhost:3000", "/"),
            "http://localhost:3000/"
        );
        assert_eq!(
            join_url("http://localhost:3000/", "about"),
            "http://localhost:3000/about"
        );
    }

    #[test]
    fn extracts_the_origin() {
        assert_eq!(
            origin_of("http://localhost:3000/dashboard"),
            Some("http://localhost:3000".to_string())
        );
    }

    #[test]
    fn builds_observations_from_a_snapshot() {
        let snapshot = serde_json::json!({
            "title": "Dashboard",
            "viewport": { "width": 1280, "height": 800 },
            "overflowX": true,
            "elements": [
                { "role": "heading", "headingLevel": 1, "name": "Dashboard" },
                { "role": "button", "name": "", "interactive": true },
                { "role": "button", "name": "Save", "interactive": true }
            ],
            "images": [
                { "src": "/logo.png", "alt": "Logo", "complete": true, "naturalWidth": 0 }
            ]
        });
        let obs = build_observations(
            "/dashboard",
            "http://localhost:3000/dashboard",
            snapshot.as_object(),
            None,
            None,
        );
        assert_eq!(obs.title, "Dashboard");
        assert_eq!(obs.viewport, "1280x800");
        assert!(obs.horizontal_overflow);
        assert_eq!(obs.h1_count, 1);
        assert_eq!(obs.unnamed_interactive_elements, 1);
        assert_eq!(obs.broken_images.len(), 1);
    }
}
