//! Project discovery and development-server management.

pub mod checkpoint;
pub mod devserver;
pub mod discovery;

pub use checkpoint::{
    BuildDoctor, BuildDoctorDiagnostic, CheckpointManager, CodeCheckpoint, FileDiff,
    GitWorkspaceState, RollbackResult,
};

pub use devserver::{
    DevCommand, DevServerFailure, DevServerManager, DevServerState, DevServerStatus, LogBuffer,
};
pub use discovery::{
    desktop_directory, discover_projects, home_directory, inspect_project_root,
    is_excluded_directory, DiscoveryLimits, DiscoveryResult, PackageManager, ProjectCandidate,
    ProjectKind, EXCLUDED_DIRS,
};
