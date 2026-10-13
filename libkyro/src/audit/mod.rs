//! The autonomous UI audit workflow.

pub mod findings;
pub mod report;
pub mod routes;
pub mod workflow;

pub use findings::{
    detect_dead_control, detect_findings, Confidence, Finding, FindingCategory, PageObservations,
    Severity,
};
pub use report::{render_report, AuditContext};
pub use routes::{DiscoveredRoute, RouteInventory, RouteKind, RouteSource};
pub use workflow::{AuditOptions, AuditOutcome, AuditProgress, AuditStage, UiAuditWorkflow};
