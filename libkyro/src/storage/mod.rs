pub mod db;
pub mod documents;
pub mod file_assistant;
pub mod organizer;
pub mod templates;
pub mod vault;

pub use documents::{
    ActionItem, DocumentAnalysisResult, DocumentAssistant, DocumentComparisonResult,
    DocumentMetadata, DocumentSection, ExtractedDate,
};
pub use file_assistant::{
    DocumentSearchResult, DuplicateFileItem, DuplicateGroup, FileAssistant, RevisionFlashcard,
    SafeFileOpResult, StudyNotesResult,
};
pub use organizer::{
    FileOrganizer, OrganizationOpRecord, OrganizationPlan, OrganizationRule,
    OrganizationTransaction, PlannedFileOp,
};

pub use db::{
    json_values_canonically_equal, AgentRun, ApprovalRequest, ArtifactRecord,
    AuthorizedFolderRecord, BotRecord, ConnectedAccountRecord, ConnectorDefinitionRecord,
    ConnectorToolPolicyRecord, Conversation, Database, FileTransactionRecord, McpServerRecord,
    MemoryEntry, Message, RoutineRecord, RoutineRunRecord, RunEvent, RunStatus, SearchResultItem,
    SkillRecord, TimelineStepRecord, VaultRecord,
};
pub use templates::{ImportSummary, TemplateEngine, TemplatePack};
pub use vault::{ApiKeys, ApiKeysMetadata, Vault, VAULT_SECRET_SENTINEL};
