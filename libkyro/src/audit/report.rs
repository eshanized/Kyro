//! The Markdown bug report.
//!
//! The report separates confirmed defects from accessibility problems, design
//! inconsistencies, performance concerns and non-blocking warnings, and it
//! states the coverage that was actually achieved.

use crate::audit::findings::{Confidence, Finding, FindingCategory, Severity};
use crate::audit::routes::RouteInventory;
use serde::{Deserialize, Serialize};

/// The context of the audit that produced the report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditContext {
    /// The absolute path of the project that was audited.
    pub project_path: String,
    /// The command that was used to start the project.
    pub startup_command: String,
    /// The detected framework.
    pub framework: Option<String>,
    /// The detected package manager.
    pub package_manager: Option<String>,
    /// The browser that was used.
    pub browser: String,
    /// The browser version.
    pub browser_version: Option<String>,
    /// The URL that was verified as ready.
    pub preview_url: String,
    /// The number of routes that were discovered.
    pub routes_discovered: usize,
    /// The number of routes that were inspected successfully.
    pub routes_inspected: usize,
    /// The routes that were actually inspected.
    pub visited_routes: Vec<String>,
    /// The number of routes that were blocked or failed.
    pub routes_blocked_or_failed: usize,
    /// The number of console errors observed.
    pub console_errors: usize,
    /// The number of failed network requests observed.
    pub failed_requests: usize,
    /// The number of screenshots that were captured.
    pub screenshot_count: usize,
    /// Whether the development server is still running.
    pub dev_server_running: bool,
    /// The routes that could not be inspected, with the reason.
    pub blocked_routes: Vec<(String, String)>,
    /// The routes that failed to load.
    pub failed_routes: Vec<(String, String)>,
    /// The time the audit took.
    pub duration_ms: u64,
}

/// Builds the Markdown report.
pub fn render_report(
    context: &AuditContext,
    findings: &[Finding],
    inventory: &RouteInventory,
) -> String {
    let mut out = String::new();

    let mut confirmed: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.category == FindingCategory::Bug && f.duplicate_of.is_none())
        .collect();
    let mut accessibility: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.category == FindingCategory::Accessibility && f.duplicate_of.is_none())
        .collect();
    let mut design: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.category == FindingCategory::DesignInconsistency && f.duplicate_of.is_none())
        .collect();
    let mut performance: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.category == FindingCategory::Performance && f.duplicate_of.is_none())
        .collect();
    let mut warnings: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.category == FindingCategory::Warning && f.duplicate_of.is_none())
        .collect();

    let by_severity = |list: &mut Vec<&Finding>| {
        list.sort_by(|a, b| {
            b.severity
                .cmp(&a.severity)
                .then(confidence_rank(a.confidence).cmp(&confidence_rank(b.confidence)))
                .then(a.id.cmp(&b.id))
        });
    };
    by_severity(&mut confirmed);
    by_severity(&mut accessibility);
    by_severity(&mut design);
    by_severity(&mut performance);
    by_severity(&mut warnings);

    out.push_str("# UI Audit Report\n\n");

    // --- Summary -----------------------------------------------------------
    out.push_str("## Summary\n\n");
    out.push_str("| Item | Value |\n|---|---|\n");
    out.push_str(&format!("| Project path | `{}` |\n", context.project_path));
    out.push_str(&format!(
        "| Startup command | `{}` |\n",
        context.startup_command
    ));
    out.push_str(&format!(
        "| Framework | {} |\n",
        context.framework.as_deref().unwrap_or("not detected")
    ));
    out.push_str(&format!(
        "| Package manager | {} |\n",
        context
            .package_manager
            .as_deref()
            .unwrap_or("not applicable")
    ));
    out.push_str(&format!(
        "| Browser | {}{} |\n",
        context.browser,
        context
            .browser_version
            .as_ref()
            .map(|v| format!(" {}", v))
            .unwrap_or_default()
    ));
    out.push_str(&format!(
        "| Verified preview URL | `{}` |\n",
        context.preview_url
    ));
    out.push_str(&format!(
        "| Routes discovered | {} |\n",
        context.routes_discovered
    ));
    out.push_str(&format!(
        "| Routes inspected | {} |\n",
        context.routes_inspected
    ));
    out.push_str(&format!(
        "| Routes blocked or failed | {} |\n",
        context.routes_blocked_or_failed
    ));
    out.push_str(&format!(
        "| Console errors | {} |\n",
        context.console_errors
    ));
    out.push_str(&format!(
        "| Failed requests | {} |\n",
        context.failed_requests
    ));
    out.push_str(&format!(
        "| Screenshots captured | {} |\n",
        context.screenshot_count
    ));
    out.push_str(&format!(
        "| Development server | {} |\n",
        if context.dev_server_running {
            "still running"
        } else {
            "stopped"
        }
    ));
    out.push_str(&format!(
        "| Audit duration | {:.1}s |\n",
        context.duration_ms as f64 / 1000.0
    ));
    out.push('\n');

    // Counts by severity and confidence.
    out.push_str("### Reproducible bugs by severity\n\n");
    out.push_str(
        "| Severity | Reproduced | Strong evidence | Unverified | Total |\n|---|---|---|---|---|\n",
    );
    for severity in [
        Severity::Critical,
        Severity::High,
        Severity::Medium,
        Severity::Low,
    ] {
        let slice: Vec<&&Finding> = confirmed
            .iter()
            .filter(|f| f.severity == severity)
            .collect();
        let reproduced = slice
            .iter()
            .filter(|f| f.confidence == Confidence::Reproduced)
            .count();
        let strong = slice
            .iter()
            .filter(|f| f.confidence == Confidence::StrongEvidence)
            .count();
        let unverified = slice
            .iter()
            .filter(|f| f.confidence == Confidence::Unverified)
            .count();
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            severity.as_str(),
            reproduced,
            strong,
            unverified,
            slice.len()
        ));
    }
    let unverified_total = findings
        .iter()
        .filter(|f| f.confidence == Confidence::Unverified)
        .count();
    out.push_str(&format!(
        "\nTotal unverified findings: {}\n\n",
        unverified_total
    ));

    // --- Findings ----------------------------------------------------------
    out.push_str("## Confirmed bugs\n\n");
    if confirmed.is_empty() {
        out.push_str("No confirmed bugs were found.\n\n");
    } else {
        for finding in &confirmed {
            render_finding(&mut out, finding);
        }
    }

    if !accessibility.is_empty() {
        out.push_str("## Accessibility issues\n\n");
        for finding in &accessibility {
            render_finding(&mut out, finding);
        }
    }

    if !design.is_empty() {
        out.push_str("## Design inconsistencies\n\n");
        for finding in &design {
            render_finding(&mut out, finding);
        }
    }

    if !performance.is_empty() {
        out.push_str("## Performance concerns\n\n");
        for finding in &performance {
            render_finding(&mut out, finding);
        }
    }

    if !warnings.is_empty() {
        out.push_str("## Non-blocking warnings\n\n");
        for finding in &warnings {
            render_finding(&mut out, finding);
        }
    }

    // --- Duplicates --------------------------------------------------------
    let duplicates: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.duplicate_of.is_some())
        .collect();
    if !duplicates.is_empty() {
        out.push_str("## Duplicate and related findings\n\n");
        out.push_str("| Finding | Duplicate of | Title |\n|---|---|---|\n");
        for finding in &duplicates {
            out.push_str(&format!(
                "| {} | {} | {} |\n",
                finding.id,
                finding.duplicate_of.as_deref().unwrap_or("—"),
                finding.title
            ));
        }
        out.push('\n');
    }

    // --- Route coverage ----------------------------------------------------
    out.push_str("## Route coverage\n\n");
    out.push_str("| Route | Kind | Status | Source | Note |\n|---|---|---|---|---|\n");
    for route in &inventory.routes {
        let status = if !route.visitable {
            "blocked"
        } else if context.visited_routes.iter().any(|v| v == &route.path) {
            "inspected"
        } else {
            "not inspected"
        };
        let sources = route
            .sources
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} |\n",
            route.path,
            route.kind.as_str(),
            status,
            sources,
            route.blocked_reason.as_deref().unwrap_or("—")
        ));
    }
    out.push('\n');

    if !context.blocked_routes.is_empty() {
        out.push_str("### Routes that could not be inspected\n\n");
        for (route, reason) in &context.blocked_routes {
            out.push_str(&format!("- `{}` — {}\n", route, reason));
        }
        out.push('\n');
    }

    if !context.failed_routes.is_empty() {
        out.push_str("### Routes that failed to load\n\n");
        for (route, reason) in &context.failed_routes {
            out.push_str(&format!("- `{}` — {}\n", route, reason));
        }
        out.push('\n');
    }

    out.push_str("## Limitations\n\n");
    if context.routes_inspected >= context.routes_discovered && context.routes_discovered > 0 {
        out.push_str(&format!(
            "- All {} discovered routes were inspected.\n",
            context.routes_discovered
        ));
    } else {
        out.push_str(&format!(
            "- {} of {} discovered routes were inspected. The complete UI was **not** inspected.\n",
            context.routes_inspected, context.routes_discovered
        ));
    }
    if !context.blocked_routes.is_empty() || !context.failed_routes.is_empty() {
        out.push_str(
            "- Routes listed as blocked or failed were not exercised; the corresponding behaviour is unknown.\n",
        );
    }
    out.push_str(
        "- Findings marked *Unverified* were observed once and could not be reproduced; treat them as leads rather than confirmed defects.\n",
    );
    out.push_str(
        "- The audit exercises the interface with synthetic, non-destructive input only. Data-destructive or transactional flows were not tested.\n",
    );

    out
}

fn confidence_rank(c: Confidence) -> u8 {
    match c {
        Confidence::Reproduced => 0,
        Confidence::StrongEvidence => 1,
        Confidence::Unverified => 2,
    }
}

fn render_finding(out: &mut String, finding: &Finding) {
    out.push_str(&format!("### {} — {}\n\n", finding.id, finding.title));
    out.push_str(&format!("- **Severity:** {}\n", finding.severity.as_str()));
    out.push_str(&format!(
        "- **Confidence:** {}\n",
        finding.confidence.as_str()
    ));
    out.push_str(&format!("- **Category:** {}\n", finding.category.as_str()));
    out.push_str(&format!("- **Route:** `{}`\n", finding.route));
    out.push_str(&format!("- **URL:** `{}`\n", finding.url));
    out.push_str(&format!("- **Viewport:** {}\n", finding.viewport));
    out.push_str(&format!("- **Area:** {}\n", finding.area));
    out.push('\n');
    out.push_str("**Reproduction steps**\n\n");
    for (i, step) in finding.reproduction_steps.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", i + 1, step));
    }
    out.push('\n');
    out.push_str(&format!("**Expected:** {}\n\n", finding.expected));
    out.push_str(&format!("**Actual:** {}\n\n", finding.actual));
    if !finding.console_evidence.is_empty() {
        out.push_str("**Console evidence**\n\n```\n");
        for line in &finding.console_evidence {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str("```\n\n");
    }
    if !finding.network_evidence.is_empty() {
        out.push_str("**Network evidence**\n\n```\n");
        for line in &finding.network_evidence {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str("```\n\n");
    }
    if !finding.artifacts.is_empty() {
        out.push_str(&format!(
            "**Evidence artifacts:** {}\n\n",
            finding.artifacts.join(", ")
        ));
    }
    if let Some(root_cause) = &finding.root_cause {
        out.push_str(&format!("**Likely root cause:** {}\n\n", root_cause));
    }
    out.push_str(&format!("**Suggested fix:** {}\n\n", finding.suggested_fix));
    if !finding.related.is_empty() {
        out.push_str(&format!(
            "**Related findings:** {}\n\n",
            finding.related.join(", ")
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::findings::{Finding, FindingCategory};

    fn context() -> AuditContext {
        AuditContext {
            project_path: "/home/u/Desktop/Runstead".into(),
            startup_command: "npm run dev".into(),
            framework: Some("nextjs".into()),
            package_manager: Some("npm".into()),
            browser: "firefox".into(),
            browser_version: Some("158.0.1".into()),
            preview_url: "http://localhost:3000/".into(),
            routes_discovered: 3,
            routes_inspected: 2,
            visited_routes: vec!["/".into(), "/about".into()],
            routes_blocked_or_failed: 1,
            console_errors: 1,
            failed_requests: 0,
            screenshot_count: 4,
            dev_server_running: false,
            blocked_routes: vec![("/blog/[slug]".into(), "dynamic route".into())],
            failed_routes: vec![],
            duration_ms: 42000,
        }
    }

    fn finding() -> Finding {
        let mut f = Finding::new(
            FindingCategory::Bug,
            Severity::High,
            Confidence::Reproduced,
            "Button does nothing".into(),
            "/dashboard",
            "http://localhost:3000/dashboard",
            "1280x800",
        );
        f.area = "toolbar".into();
        f.reproduction_steps = vec!["Open /dashboard".into(), "Click Export".into()];
        f.expected = "The export should start.".into();
        f.actual = "Nothing happened.".into();
        f.console_evidence = vec!["TypeError: x is not a function".into()];
        f.suggested_fix = "Wire the handler.".into();
        f
    }

    #[test]
    fn report_contains_the_summary_fields() {
        let report = render_report(
            &context(),
            &[finding()],
            &RouteInventory {
                routes: vec![],
                visitable: 0,
                blocked: 0,
                summary: String::new(),
            },
        );
        assert!(report.contains("Project path"));
        assert!(report.contains("/home/u/Desktop/Runstead"));
        assert!(report.contains("npm run dev"));
        assert!(report.contains("firefox"));
        assert!(report.contains("http://localhost:3000/"));
    }

    #[test]
    fn report_lists_findings_with_evidence() {
        let report = render_report(
            &context(),
            &[finding()],
            &RouteInventory {
                routes: vec![],
                visitable: 0,
                blocked: 0,
                summary: String::new(),
            },
        );
        assert!(report.contains("## Confirmed bugs"));
        assert!(report.contains("Button does nothing"));
        assert!(report.contains("TypeError: x is not a function"));
        assert!(report.contains("**Severity:** High"));
    }

    #[test]
    fn report_states_the_coverage_limitations() {
        let report = render_report(
            &context(),
            &[],
            &RouteInventory {
                routes: vec![],
                visitable: 0,
                blocked: 0,
                summary: String::new(),
            },
        );
        assert!(report.contains("was **not** inspected"));
        assert!(report.contains("2 of 3 discovered routes"));
    }

    #[test]
    fn empty_report_is_still_valid() {
        let report = render_report(
            &context(),
            &[],
            &RouteInventory {
                routes: vec![],
                visitable: 0,
                blocked: 0,
                summary: String::new(),
            },
        );
        assert!(report.contains("No confirmed bugs were found."));
    }
}
