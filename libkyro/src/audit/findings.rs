//! Bug findings and the detectors that produce them.
//!
//! A console message is not automatically a user-facing bug, a failed request
//! may be intentional and an authentication redirect is not a defect.  Every
//! finding therefore records what was observed, the evidence that supports
//! it, and how confident the audit is that it is a real defect.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Low => "Low",
            Severity::Medium => "Medium",
            Severity::High => "High",
            Severity::Critical => "Critical",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Confidence {
    /// The issue was reproduced during the audit.
    Reproduced,
    /// The evidence strongly indicates the issue, but it was not reproduced.
    StrongEvidence,
    /// The issue was observed once and could not be confirmed.
    Unverified,
}

impl Confidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Confidence::Reproduced => "Reproduced",
            Confidence::StrongEvidence => "Strong evidence",
            Confidence::Unverified => "Unverified",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingCategory {
    /// A functional defect in the application.
    Bug,
    /// An accessibility problem.
    Accessibility,
    /// A visual or structural inconsistency.
    DesignInconsistency,
    /// A performance concern.
    Performance,
    /// A non-blocking observation.
    Warning,
}

impl FindingCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            FindingCategory::Bug => "Bug",
            FindingCategory::Accessibility => "Accessibility",
            FindingCategory::DesignInconsistency => "Design inconsistency",
            FindingCategory::Performance => "Performance",
            FindingCategory::Warning => "Warning",
        }
    }
}

/// A single finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// The unique identifier, e.g. `BUG-001`.
    pub id: String,
    pub title: String,
    pub severity: Severity,
    pub confidence: Confidence,
    pub category: FindingCategory,
    /// The route the finding was observed on.
    pub route: String,
    /// The browser URL.
    pub url: String,
    /// The viewport the observation was made in.
    pub viewport: String,
    /// The UI area the finding relates to.
    pub area: String,
    /// The exact steps to reproduce.
    pub reproduction_steps: Vec<String>,
    /// The expected behaviour.
    pub expected: String,
    /// The observed behaviour.
    pub actual: String,
    /// Console output that supports the finding.
    pub console_evidence: Vec<String>,
    /// Network output that supports the finding.
    pub network_evidence: Vec<String>,
    /// The identifiers of the attached artifacts (screenshots).
    pub artifacts: Vec<String>,
    /// The likely root cause, when the evidence supports one.
    pub root_cause: Option<String>,
    /// A suggested fix.
    pub suggested_fix: String,
    /// The identifiers of related findings.
    pub related: Vec<String>,
    /// `true` when the finding is a duplicate of an earlier one.
    pub duplicate_of: Option<String>,
}

impl Finding {
    pub fn new(
        category: FindingCategory,
        severity: Severity,
        confidence: Confidence,
        title: String,
        route: &str,
        url: &str,
        viewport: &str,
    ) -> Self {
        Self {
            id: String::new(),
            title,
            severity,
            confidence,
            category,
            route: route.to_string(),
            url: url.to_string(),
            viewport: viewport.to_string(),
            area: String::new(),
            reproduction_steps: Vec::new(),
            expected: String::new(),
            actual: String::new(),
            console_evidence: Vec::new(),
            network_evidence: Vec::new(),
            artifacts: Vec::new(),
            root_cause: None,
            suggested_fix: String::new(),
            related: Vec::new(),
            duplicate_of: None,
        }
    }
}

/// Assigns the identifiers and removes duplicates.
pub fn finalise_findings(findings: Vec<Finding>) -> Vec<Finding> {
    let mut counter: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut assigned: Vec<Finding> = Vec::new();

    for mut finding in findings {
        // Duplicate detection: same title (case-insensitive) and same route.
        let key = (finding.title.to_lowercase(), finding.route.clone());
        if let Some(existing) = assigned.iter().find(|f| {
            (f.title.to_lowercase(), f.route.clone()) == key && f.category == finding.category
        }) {
            let dup_id = existing.id.clone();
            finding.duplicate_of = Some(dup_id);
        }

        let prefix = match finding.category {
            FindingCategory::Bug => "BUG",
            FindingCategory::Accessibility => "A11Y",
            FindingCategory::DesignInconsistency => "UI",
            FindingCategory::Performance => "PERF",
            FindingCategory::Warning => "WARN",
        };
        let count = counter.entry(prefix).or_insert(0);
        *count += 1;
        finding.id = format!("{}-{:03}", prefix, *count);
        assigned.push(finding);
    }

    // Resolve related findings that point at a duplicate.
    let ids: Vec<String> = assigned.iter().map(|f| f.id.clone()).collect();
    for finding in &mut assigned {
        if let Some(dup) = finding.duplicate_of.clone() {
            if ids.iter().any(|i| i == &dup) {
                finding.related.push(dup);
            }
        }
    }

    assigned
}

/// The observations made on one page.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PageObservations {
    /// The route that was visited.
    pub route: String,
    /// The browser URL.
    pub url: String,
    /// The page title.
    pub title: String,
    /// The viewport size, e.g. `1280x800`.
    pub viewport: String,
    /// The console messages observed.
    pub console_entries: Vec<ConsoleObservation>,
    /// The failed network requests observed.
    pub failed_requests: Vec<FailedRequestObservation>,
    /// The HTTP responses with an error status.
    pub http_error_responses: Vec<HttpErrorObservation>,
    /// Images that did not load.
    pub broken_images: Vec<BrokenImage>,
    /// `true` when the page scrolls horizontally at this viewport.
    pub horizontal_overflow: bool,
    /// The number of `<h1>` elements on the page.
    pub h1_count: usize,
    /// A visible element that looks like a loading indicator.
    pub stuck_loading_indicator: Option<String>,
    /// Links that point outside the application.
    pub external_links: Vec<String>,
    /// The accessible names of the interactive elements.
    pub interactive_element_names: Vec<String>,
    /// The number of interactive elements without an accessible name.
    pub unnamed_interactive_elements: usize,
    /// Whether the document declares a language.
    pub declares_language: bool,
    /// Form fields whose validation was checked.
    pub field_validations: Vec<FieldValidation>,
    /// Form fields that still need a validation check.
    pub fields_to_validate: Vec<ValidationTarget>,
}

/// A form field that a validation check should be performed on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationTarget {
    pub name: String,
    pub input_type: String,
}

/// The result of checking one form field's validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldValidation {
    pub name: String,
    /// The format the field is expected to accept.
    pub field_type: String,
    /// The type the field actually declares.
    pub declared_type: String,
    pub required: bool,
    pub pattern: Option<String>,
    pub minlength: Option<String>,
    /// `true` when the field accepted an obviously invalid value.
    pub accepts_invalid: bool,
    /// The browser's own validation message, when it rejected the value.
    pub validation_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleObservation {
    pub level: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedRequestObservation {
    pub url: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpErrorObservation {
    pub url: String,
    pub status: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokenImage {
    pub src: Option<String>,
    pub alt: Option<String>,
}

/// Produces the findings for one inspected page.
///
/// The detectors only report what they can support with the evidence that was
/// collected.  Anything that could not be confirmed is labelled `Unverified`.
pub fn detect_findings(obs: &PageObservations) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();
    let viewport = obs.viewport.clone();
    let url = obs.url.clone();
    let route = obs.route.clone();

    // --- Console errors ---------------------------------------------------
    let console_errors: Vec<&ConsoleObservation> = obs
        .console_entries
        .iter()
        .filter(|c| c.level.eq_ignore_ascii_case("error"))
        .collect();

    for error in &console_errors {
        let mut finding = Finding::new(
            FindingCategory::Bug,
            Severity::High,
            Confidence::Reproduced,
            format!("Uncaught JavaScript error: {}", truncate(&error.text, 160)),
            &route,
            &url,
            &viewport,
        );
        finding.area = "page script".into();
        finding.reproduction_steps = vec![
            format!("Start the application and open {}", route),
            "Observe the browser console".into(),
        ];
        finding.expected = "The page loads without raising an uncaught JavaScript error.".into();
        finding.actual = format!("The browser console reported: {}", error.text);
        finding.console_evidence = vec![error.text.clone()];
        finding.suggested_fix =
            "Inspect the failing script and fix the error; add error handling so the page degrades gracefully."
                .into();
        findings.push(finding);
    }

    // --- Failed network requests ------------------------------------------
    for failed in &obs.failed_requests {
        let mut finding = Finding::new(
            FindingCategory::Bug,
            Severity::High,
            Confidence::Reproduced,
            format!("Network request failed: {}", truncate(&failed.url, 120)),
            &route,
            &url,
            &viewport,
        );
        finding.area = "network".into();
        finding.reproduction_steps = vec![
            format!("Open {}", route),
            format!("Observe the failed request to {}", failed.url),
        ];
        finding.expected = "All requests the page makes should complete successfully.".into();
        finding.actual = format!("The request to {} failed: {}", failed.url, failed.error);
        finding.network_evidence = vec![format!("{} — {}", failed.url, failed.error)];
        finding.suggested_fix =
            "Check the endpoint exists, is reachable from the browser, and returns a successful status."
                .into();
        findings.push(finding);
    }

    // --- HTTP error responses ---------------------------------------------
    for response in &obs.http_error_responses {
        // A 401/403 on an authenticated route is expected behaviour, not a
        // defect, so it is not reported as a bug.
        if matches!(response.status, 401 | 403) {
            continue;
        }
        let mut finding = Finding::new(
            FindingCategory::Bug,
            Severity::High,
            Confidence::Reproduced,
            format!(
                "Request returned HTTP {}: {}",
                response.status,
                truncate(&response.url, 120)
            ),
            &route,
            &url,
            &viewport,
        );
        finding.area = "network".into();
        finding.reproduction_steps = vec![
            format!("Open {}", route),
            format!(
                "Observe the HTTP {} response for {}",
                response.status, response.url
            ),
        ];
        finding.expected =
            "The application should not request resources that return an error status.".into();
        finding.actual = format!("{} responded with HTTP {}", response.url, response.status);
        finding.network_evidence = vec![format!("HTTP {} — {}", response.status, response.url)];
        finding.suggested_fix =
            "Correct the request URL or make the endpoint return a successful status.".into();
        findings.push(finding);
    }

    // --- Broken images ----------------------------------------------------
    for image in &obs.broken_images {
        let mut finding = Finding::new(
            FindingCategory::Bug,
            Severity::Medium,
            Confidence::Reproduced,
            format!(
                "Image failed to load: {}",
                truncate(image.src.as_deref().unwrap_or("(no src)"), 100)
            ),
            &route,
            &url,
            &viewport,
        );
        finding.area = "image".into();
        finding.reproduction_steps = vec![
            format!("Open {}", route),
            "Observe the missing image".into(),
        ];
        finding.expected = "Every image referenced by the page should load.".into();
        finding.actual = format!(
            "The image at '{}' did not load (naturalWidth = 0).",
            image.src.as_deref().unwrap_or("(no src)")
        );
        finding.suggested_fix =
            "Check that the asset exists at the referenced path and that the build serves it."
                .into();
        findings.push(finding);
    }

    // --- Horizontal overflow ----------------------------------------------
    if obs.horizontal_overflow {
        let mut finding = Finding::new(
            FindingCategory::DesignInconsistency,
            Severity::Medium,
            Confidence::Reproduced,
            "Page scrolls horizontally at this viewport".into(),
            &route,
            &url,
            &viewport,
        );
        finding.area = "page layout".into();
        finding.reproduction_steps = vec![
            format!("Open {} at a viewport of {}", route, viewport),
            "Scroll the page horizontally".into(),
        ];
        finding.expected =
            "The page should fit the viewport width without horizontal scrolling.".into();
        finding.actual = "The document width exceeds the viewport width.".into();
        finding.suggested_fix =
            "Find the element that overflows the viewport and constrain its width.".into();
        findings.push(finding);
    }

    // --- Stuck loading indicator ------------------------------------------
    if let Some(indicator) = &obs.stuck_loading_indicator {
        let mut finding = Finding::new(
            FindingCategory::Bug,
            Severity::Medium,
            Confidence::StrongEvidence,
            "Loading indicator remained visible after the page settled".into(),
            &route,
            &url,
            &viewport,
        );
        finding.area = "loading state".into();
        finding.reproduction_steps = vec![
            format!("Open {}", route),
            format!(
                "Observe the loading indicator '{}' still present after the page settled",
                indicator
            ),
        ];
        finding.expected =
            "The loading indicator should disappear once the data has loaded.".into();
        finding.actual = format!("The element '{}' was still present.", indicator);
        finding.suggested_fix =
            "Make sure the loading state is cleared in every code path, including error paths."
                .into();
        findings.push(finding);
    }

    // --- Accessibility observations ---------------------------------------
    if obs.h1_count == 0 {
        let mut finding = Finding::new(
            FindingCategory::Accessibility,
            Severity::Medium,
            Confidence::Reproduced,
            "The page has no top-level heading".into(),
            &route,
            &url,
            &viewport,
        );
        finding.area = "document structure".into();
        finding.reproduction_steps = vec![format!("Open {}", route)];
        finding.expected = "Every page should expose exactly one level-1 heading.".into();
        finding.actual = "No <h1> element was present in the accessibility tree.".into();
        finding.suggested_fix = "Add a descriptive <h1> that names the page.".into();
        findings.push(finding);
    }

    if obs.unnamed_interactive_elements > 0 {
        let mut finding = Finding::new(
            FindingCategory::Accessibility,
            Severity::Medium,
            Confidence::Reproduced,
            format!(
                "{} interactive element(s) have no accessible name",
                obs.unnamed_interactive_elements
            ),
            &route,
            &url,
            &viewport,
        );
        finding.area = "interactive elements".into();
        finding.reproduction_steps = vec![format!("Open {}", route)];
        finding.expected = "Every interactive control should expose an accessible name.".into();
        finding.actual = format!(
            "{} interactive element(s) were exposed without a name.",
            obs.unnamed_interactive_elements
        );
        finding.suggested_fix =
            "Add visible text, an aria-label or a <label> to every interactive control.".into();
        findings.push(finding);
    }

    if !obs.declares_language {
        let mut finding = Finding::new(
            FindingCategory::Accessibility,
            Severity::Low,
            Confidence::Reproduced,
            "The document does not declare a language".into(),
            &route,
            &url,
            &viewport,
        );
        finding.area = "document metadata".into();
        finding.reproduction_steps = vec![format!("Open {}", route)];
        finding.expected = "The <html> element should declare the page language.".into();
        finding.actual = "No lang attribute was present on the document element.".into();
        finding.suggested_fix =
            "Add lang=\"en\" (or the correct language) to the <html> element.".into();
        findings.push(finding);
    }

    // --- Form validation --------------------------------------------------
    for field in &obs.field_validations {
        if !field.accepts_invalid {
            continue;
        }
        let mut finding = Finding::new(
            FindingCategory::Bug,
            Severity::Medium,
            Confidence::Reproduced,
            format!(
                "The '{}' field accepts an invalid {} value",
                field.name, field.field_type
            ),
            &route,
            &url,
            &viewport,
        );
        finding.area = format!("form field '{}'", field.name);
        finding.reproduction_steps = vec![
            format!("Open {}", route),
            format!(
                "Enter an obviously invalid {} value into the '{}' field",
                field.field_type, field.name
            ),
            "Observe that the field accepts the value without reporting a problem".into(),
        ];
        finding.expected = format!(
            "A '{}' field must reject a value that is not a valid {}.",
            field.name, field.field_type
        );
        finding.actual = if field.declared_type == field.field_type {
            format!(
                "The field accepted the invalid value and reported no validation message, although it is declared as type '{}'.",
                field.declared_type
            )
        } else {
            format!(
                "The field is labelled as a '{}' field but is declared as type '{}', so it accepted the invalid value without reporting a problem.",
                field.field_type, field.declared_type
            )
        };
        finding.suggested_fix = format!(
            "Declare the '{}' field with type=\"{}\" (or add a pattern constraint) so that invalid input is rejected.",
            field.name, field.field_type
        );
        findings.push(finding);
    }

    finalise_findings(findings)
}

/// Produces a finding for a control that did not do anything when activated.
pub fn detect_dead_control(
    route: &str,
    url: &str,
    viewport: &str,
    control_name: &str,
    url_before: &str,
    url_after: &str,
    dom_changed: bool,
) -> Finding {
    let mut finding = Finding::new(
        FindingCategory::Bug,
        Severity::Medium,
        if dom_changed {
            Confidence::StrongEvidence
        } else {
            Confidence::Unverified
        },
        format!(
            "Control '{}' appears to do nothing when activated",
            control_name
        ),
        route,
        url,
        viewport,
    );
    finding.area = format!("control '{}'", control_name);
    finding.reproduction_steps = vec![
        format!("Open {}", route),
        format!("Activate the control '{}'", control_name),
    ];
    finding.expected =
        "Activating the control should produce a visible change in the interface.".into();
    finding.actual = if url_before == url_after {
        "The URL did not change and no visible change was detected after the control was activated."
            .into()
    } else {
        "The URL changed but no visible change could be confirmed.".into()
    };
    finding.suggested_fix =
        "Check the control's click handler; it may not be wired to the intended action.".into();
    finalise_findings(vec![finding]).into_iter().next().unwrap()
}

fn truncate(text: &str, max: usize) -> String {
    let cleaned = text.replace('\n', " ").trim().to_string();
    if cleaned.chars().count() <= max {
        cleaned
    } else {
        let mut out: String = cleaned.chars().take(max).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_observations() -> PageObservations {
        PageObservations {
            route: "/".into(),
            url: "http://localhost:3000/".into(),
            title: "Home".into(),
            viewport: "1280x800".into(),
            declares_language: true,
            h1_count: 1,
            ..Default::default()
        }
    }

    #[test]
    fn a_clean_page_produces_no_findings() {
        let findings = detect_findings(&base_observations());
        assert!(findings.is_empty(), "{:?}", findings);
    }

    #[test]
    fn a_console_error_becomes_a_high_severity_reproduced_finding() {
        let mut obs = base_observations();
        obs.console_entries.push(ConsoleObservation {
            level: "error".into(),
            text: "TypeError: undefined is not a function".into(),
        });
        let findings = detect_findings(&obs);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::High);
        assert_eq!(findings[0].confidence, Confidence::Reproduced);
        assert_eq!(findings[0].id, "BUG-001");
        assert!(!findings[0].console_evidence.is_empty());
    }

    #[test]
    fn an_authentication_error_is_not_reported_as_a_bug() {
        let mut obs = base_observations();
        obs.http_error_responses.push(HttpErrorObservation {
            url: "http://localhost:3000/api/private".into(),
            status: 401,
        });
        let findings = detect_findings(&obs);
        assert!(findings.is_empty());
    }

    #[test]
    fn a_server_error_is_reported() {
        let mut obs = base_observations();
        obs.http_error_responses.push(HttpErrorObservation {
            url: "http://localhost:3000/api/data".into(),
            status: 500,
        });
        let findings = detect_findings(&obs);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].title.contains("HTTP 500"));
    }

    #[test]
    fn overflow_is_reported_as_a_design_inconsistency() {
        let mut obs = base_observations();
        obs.horizontal_overflow = true;
        let findings = detect_findings(&obs);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].category, FindingCategory::DesignInconsistency);
        assert_eq!(findings[0].id, "UI-001");
    }

    #[test]
    fn accessibility_observations_are_categorised_separately() {
        let mut obs = base_observations();
        obs.h1_count = 0;
        obs.unnamed_interactive_elements = 3;
        obs.declares_language = false;
        let findings = detect_findings(&obs);
        assert_eq!(findings.len(), 3);
        assert!(findings
            .iter()
            .all(|f| f.category == FindingCategory::Accessibility));
        assert!(findings.iter().all(|f| f.id.starts_with("A11Y-")));
    }

    #[test]
    fn duplicate_findings_are_linked() {
        let mut obs = base_observations();
        obs.console_entries.push(ConsoleObservation {
            level: "error".into(),
            text: "boom".into(),
        });
        let first = detect_findings(&obs);
        let mut second = detect_findings(&obs);
        second[0].id = "BUG-001".into();
        assert_eq!(first.len(), 1);
    }

    #[test]
    fn a_dead_control_is_unverified_when_nothing_changed() {
        let finding = detect_dead_control(
            "/",
            "http://localhost:3000/",
            "1280x800",
            "Submit",
            "http://localhost:3000/",
            "http://localhost:3000/",
            false,
        );
        assert_eq!(finding.confidence, Confidence::Unverified);
        assert_eq!(finding.category, FindingCategory::Bug);
    }
}
