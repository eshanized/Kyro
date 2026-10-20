pub mod bot;
pub mod group;
pub mod workspace;

pub use bot::{BotProfile, BotStatus};
pub use group::{DelegatedTaskRequest, DelegationResult, MultiBotOrchestrator};
pub use workspace::{
    MultiAgentOrchestrator, MultiAgentWorkspace, ReconciliationOutcome, SpecialistRole,
    SpecialistTask,
};
