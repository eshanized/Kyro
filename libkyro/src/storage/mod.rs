pub mod chunking;
pub mod db;
pub mod documents;
pub mod file_assistant;
pub mod organizer;
pub mod retrieval;
pub mod templates;
pub mod vault;

pub use chunking::{chunk_document, chunk_markdown, chunk_plain_text, chunk_source_code};
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
pub use retrieval::{
    cosine_similarity, validate_local_loopback_url, DeterministicTestEmbeddingProvider,
    EmbeddingProvider, LocalHttpEmbeddingProvider, NoneEmbeddingProvider, RagStatus,
    RetrievalEngine, RetrievalResult, MAX_RAG_CONTEXT_CHARS,
};

pub use db::{
    check_path_authorized_in_folders, is_path_authorized, json_values_canonically_equal,
    validate_authorized_path, AgentRun, ApprovalRequest, ArtifactRecord, AuthorizedFolderRecord,
    BotRecord, CodeExperimentRecord, ConnectedAccountRecord, ConnectorDefinitionRecord,
    ConnectorToolPolicyRecord, Conversation, Database, DocumentChunkRecord, DocumentIndexRecord,
    FileTransactionRecord, HabitSettingsRecord, HabitSuggestionRecord, McpServerRecord,
    MemoryEntry, Message, RagSettingsRecord, RoutineRecord, RoutineRunRecord, RunEvent, RunStatus,
    SearchResultItem, SkillRecord, TimelineStepRecord, VaultRecord,
};
pub use templates::{ImportSummary, TemplateEngine, TemplatePack};
pub use vault::{ApiKeys, ApiKeysMetadata, Vault, VAULT_SECRET_SENTINEL};
